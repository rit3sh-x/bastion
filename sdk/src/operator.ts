import {
    AccountRole,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    fetchAddressesForLookupTables,
    getBase58Encoder,
    type Account,
    type Address,
    type Commitment,
    type Instruction,
    type Rpc,
    type RpcSubscriptions,
    type Signature,
    type SolanaRpcApi,
    type SolanaRpcSubscriptionsApi,
    type TransactionSigner,
} from "@solana/kit";
import {
    BASTION_PROGRAM_ADDRESS,
    fetchAllMaybePolicy,
    fetchSession,
    getExecuteInstruction,
    type Policy,
    type Session,
} from "./generated";
import {
    planExecution,
    sendTx,
    wrapInnerBatch,
    type OuterIxArgs,
} from "./execute";
import { BastionSdkError } from "./errors";
import {
    buildEd25519Instruction,
    publicKeyBytes,
    type SignedManifest,
} from "./manifest";
import { pda } from "./session";
import { sessionKeyFromSecret } from "./wallet";

export interface OperatorCredential {
    sessionSecret: string;
    sessionPda: Address;
    owner: Address;
    programId: Address;
    policies?: Address[];
    rpcUrl: string;
    wsUrl?: string;
    lookupTable?: Address;
}

export function serializeOperatorCredential(cred: OperatorCredential): string {
    return JSON.stringify(cred);
}

export function parseOperatorCredential(json: string): OperatorCredential {
    const c = JSON.parse(json) as Partial<OperatorCredential>;
    for (const k of [
        "sessionSecret",
        "sessionPda",
        "owner",
        "programId",
        "rpcUrl",
    ] as const) {
        if (!c[k]) {
            throw new BastionSdkError({
                code: "InvalidConfig",
                message: `OperatorCredential.${k} is required`,
            });
        }
    }
    return { ...(c as OperatorCredential), policies: c.policies ?? [] };
}

export interface OperatorExecuteArgs extends OuterIxArgs {
    inner: Instruction;
    /** Override the policy set (defaults to the credential's). */
    policies?: readonly Address[];
    expectedNonce?: bigint;
    /** Holder-signed stateless manifest to enforce alongside on-chain policies. */
    manifest?: SignedManifest;
}

export interface OperatorBatchArgs extends OuterIxArgs {
    inners: readonly Instruction[];
    policies?: readonly Address[];
    expectedNonce?: bigint;
    manifest?: SignedManifest;
}

export interface OperatorTxOpts {
    feePayer?: TransactionSigner<Address>;
    commitment?: Commitment;
}

export interface SequenceStep {
    index: number;
    signature: Signature;
}

export interface SequenceResult {
    completed: SequenceStep[];
    /** Index of the leg that failed, or null if all succeeded. */
    failedAt: number | null;
    error?: unknown;
}

export interface OperatorClient {
    readonly sessionPda: Address;
    readonly owner: Address;
    readonly sessionKey: Address;

    /** Single atomic action (one wrapped leg). */
    execute(
        args: OperatorExecuteArgs,
        opts?: OperatorTxOpts
    ): Promise<Signature>;
    /** Many legs, one atomic transaction (all-or-nothing). */
    executeBatch(
        args: OperatorBatchArgs,
        opts?: OperatorTxOpts
    ): Promise<Signature>;
    /** Ordered, resumable multi-tx sequence (NOT atomic across txns). */
    executeSequence(
        inners: readonly Instruction[],
        opts?: OperatorTxOpts & OuterIxArgs
    ): Promise<SequenceResult>;

    state(): Promise<Session>;
    policies(): Promise<readonly Address[]>;
    delegateBalance(): Promise<bigint>;
    isExpired(now?: bigint): Promise<boolean>;
}

export interface CreateOperatorClientOptions {
    rpc?: Rpc<SolanaRpcApi>;
    rpcSubscriptions?: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
    commitment?: Commitment;
}

export async function createOperatorClient(
    cred: OperatorCredential,
    options: CreateOperatorClientOptions = {}
): Promise<OperatorClient> {
    const sessionKey = await sessionKeyFromSecret(
        new Uint8Array(getBase58Encoder().encode(cred.sessionSecret))
    );
    const rpc = options.rpc ?? createSolanaRpc(cred.rpcUrl);
    const rpcSubscriptions =
        options.rpcSubscriptions ??
        createSolanaRpcSubscriptions(cred.wsUrl ?? deriveWs(cred.rpcUrl));
    const commitment: Commitment = options.commitment ?? "confirmed";
    const programId = cred.programId ?? BASTION_PROGRAM_ADDRESS;
    const { sessionPda, owner } = cred;

    const altMapPromise = cred.lookupTable
        ? fetchAddressesForLookupTables([cred.lookupTable], rpc)
        : null;

    const staticPolicies =
        cred.policies && cred.policies.length > 0 ? cred.policies : null;

    const resolvePolicyAddresses = async (
        override?: readonly Address[]
    ): Promise<readonly Address[]> => {
        if (override) return override;
        if (staticPolicies) return staticPolicies;

        const session = await fetchSession(rpc, sessionPda);
        const nextSeed = Number(session.data.nextSeed);

        const derived: Address[] = [];
        for (let i = 0; i < nextSeed; i++) {
            const [policyPda] = await pda.policy(sessionPda, BigInt(i));
            derived.push(policyPda);
        }
        const maybe = await fetchAllMaybePolicy(rpc, derived);
        return maybe.filter((a) => a.exists).map((a) => a.address);
    };

    const send = async (instructions: Instruction[], opts?: OperatorTxOpts) =>
        sendTx({
            rpc,
            rpcSubscriptions,
            feePayer: opts?.feePayer ?? sessionKey,
            instructions,
            commitment: opts?.commitment ?? commitment,
            ...(altMapPromise
                ? { addressLookupTables: await altMapPromise }
                : {}),
        });

    const buildExecuteIxs = async (
        inners: readonly Instruction[],
        outer: OuterIxArgs,
        policyAddrs: readonly Address[],
        expectedNonce: bigint | null,
        manifest?: SignedManifest
    ): Promise<Instruction[]> => {
        const wrapped = wrapInnerBatch(inners);
        const plan = await planExecution(
            rpc,
            policyAddrs,
            outer,
            inners.length
        );
        const [delegatePda] = await pda.delegate(owner, sessionKey.address);
        const ix = getExecuteInstruction(
            {
                sessionKey,
                session: sessionPda,
                wrappedIxs: wrapped.legs.map((l) => ({
                    programId: l.programId,
                    accounts: l.accounts,
                    data: l.data,
                })),
                policyCount: plan.policies.length,
                expectedNonce,
                manifest: manifest ? manifest.policies : null,
            },
            { programAddress: programId }
        );
        const full: Instruction = {
            ...ix,
            accounts: [
                ...ix.accounts,
                ...plan.policies.map(({ address }) => ({
                    address,
                    role: AccountRole.WRITABLE,
                })),
                { address: delegatePda, role: AccountRole.WRITABLE },
                ...wrapped.innerMetas.map((m) =>
                    m.address === delegatePda
                        ? { address: m.address, role: AccountRole.WRITABLE }
                        : m
                ),
                ...wrapped.programIds.map((pid) => ({
                    address: pid,
                    role: AccountRole.READONLY,
                })),
            ],
        };
        if (manifest) {
            const ed = buildEd25519Instruction({
                publicKey: publicKeyBytes(owner),
                signature: manifest.signature,
                message: manifest.manifestHash,
            });
            return [ed, ...plan.outerIxs, full];
        }
        return [...plan.outerIxs, full];
    };

    const execOne = async (
        inners: readonly Instruction[],
        args: OuterIxArgs & {
            policies?: readonly Address[];
            expectedNonce?: bigint;
            manifest?: SignedManifest;
        },
        opts?: OperatorTxOpts
    ) => {
        const policyAddrs = await resolvePolicyAddresses(args.policies);
        const ixs = await buildExecuteIxs(
            inners,
            args,
            policyAddrs,
            args.expectedNonce ?? null,
            args.manifest
        );
        return send(ixs, opts);
    };

    return {
        sessionPda,
        owner,
        sessionKey: sessionKey.address,

        execute(args, opts) {
            return execOne([args.inner], args, opts);
        },

        executeBatch(args, opts) {
            if (args.inners.length === 0) {
                throw new BastionSdkError({
                    code: "InvalidConfig",
                    message:
                        "executeBatch requires at least one inner instruction",
                });
            }
            return execOne(args.inners, args, opts);
        },

        async executeSequence(inners, opts) {
            const completed: SequenceStep[] = [];
            const session = await fetchSession(rpc, sessionPda);
            let nonce = session.data.actionNonce;
            for (let i = 0; i < inners.length; i++) {
                try {
                    const sig = await execOne(
                        [inners[i]!],
                        { ...(opts ?? {}), expectedNonce: nonce },
                        opts
                    );
                    completed.push({ index: i, signature: sig });
                    nonce = nonce + 1n;
                } catch (error) {
                    return { completed, failedAt: i, error };
                }
            }
            return { completed, failedAt: null };
        },

        async state() {
            return (await fetchSession(rpc, sessionPda)).data;
        },

        async policies() {
            return resolvePolicyAddresses();
        },

        async delegateBalance() {
            const [delegatePda] = await pda.delegate(owner, sessionKey.address);
            const { value } = await rpc.getBalance(delegatePda).send();
            return value;
        },

        async isExpired(now) {
            const session = await fetchSession(rpc, sessionPda);
            const ts = now ?? BigInt(Math.floor(Date.now() / 1000));
            return ts > session.data.expiry;
        },
    };
}

function deriveWs(httpUrl: string): string {
    if (httpUrl.startsWith("https://")) return "wss://" + httpUrl.slice(8);
    if (httpUrl.startsWith("http://")) return "ws://" + httpUrl.slice(7);
    throw new BastionSdkError({
        code: "InvalidConfig",
        message: `Cannot derive ws URL from "${httpUrl}". Supply cred.wsUrl.`,
    });
}

export type { Account, Policy };
