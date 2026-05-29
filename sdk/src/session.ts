import {
    AccountRole,
    getAddressEncoder,
    getProgramDerivedAddress,
    type Account,
    type Address,
    type Base58EncodedBytes,
    type Commitment,
    type Instruction,
    type ProgramDerivedAddress,
    type RpcSubscriptions,
    type Signature,
    type SolanaRpcSubscriptionsApi,
    type TransactionSigner,
} from "@solana/kit";
import {
    BASTION_PROGRAM_ADDRESS,
    fetchAllMaybePolicy,
    fetchSession,
    findPolicyPda,
    getAttachPolicyInstruction,
    getCloseSessionInstruction,
    getDetachPolicyInstruction,
    getExecuteInstruction,
    getExtendSessionInstruction,
    getInitSessionInstruction,
    getRevokeSessionInstruction,
    getSweepDelegateInstruction,
    getUpdatePolicyInstruction,
    type Policy,
    type PolicyDataArgs,
    type Session,
} from "./generated";
import type { ResolvedBastionConfig } from "./config";
import { wrapSendError } from "./errors";
import { planExecution, sendTx, wrapInner } from "./execute";
import { withHooks } from "./hooks";
import type { Logger } from "./logger";
import { generateSessionKey, type SessionSigner } from "./wallet";
import { associatedTokenAddress, buildApproveIx, buildRevokeIx } from "./token";

export const MPL_TOKEN_METADATA_ADDRESS =
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s" as Address;

const SEED_SESSION = new TextEncoder().encode("session");
const SEED_DELEGATE = new TextEncoder().encode("delegate");
const SEED_METADATA = new TextEncoder().encode("metadata");

export interface PdaDerivation {
    session(
        owner: Address,
        sessionKey: Address
    ): Promise<ProgramDerivedAddress>;
    policy(
        session: Address,
        seed: number | bigint
    ): Promise<ProgramDerivedAddress>;
    delegate(
        owner: Address,
        sessionKey: Address
    ): Promise<ProgramDerivedAddress>;
    metadata(mint: Address): Promise<ProgramDerivedAddress>;
}

export const pda: PdaDerivation = {
    async session(owner, sessionKey) {
        return getProgramDerivedAddress({
            programAddress: BASTION_PROGRAM_ADDRESS,
            seeds: [
                SEED_SESSION,
                getAddressEncoder().encode(owner),
                getAddressEncoder().encode(sessionKey),
            ],
        });
    },
    async policy(session, seed) {
        return findPolicyPda({ session, seed });
    },
    async delegate(owner, sessionKey) {
        return getProgramDerivedAddress({
            programAddress: BASTION_PROGRAM_ADDRESS,
            seeds: [
                SEED_DELEGATE,
                getAddressEncoder().encode(owner),
                getAddressEncoder().encode(sessionKey),
            ],
        });
    },
    async metadata(mint) {
        return getProgramDerivedAddress({
            programAddress: MPL_TOKEN_METADATA_ADDRESS,
            seeds: [
                SEED_METADATA,
                getAddressEncoder().encode(MPL_TOKEN_METADATA_ADDRESS),
                getAddressEncoder().encode(mint),
            ],
        });
    },
};

export function resolveExpiry(
    expiry: bigint | Date | { secsFromNow: number }
): bigint {
    if (typeof expiry === "bigint") return expiry;
    if (expiry instanceof Date)
        return BigInt(Math.floor(expiry.getTime() / 1000));
    return BigInt(Math.floor(Date.now() / 1000) + expiry.secsFromNow);
}

export interface AttachResult {
    signature: Signature;
    policyPda: Address;
    seed: bigint;
}

export interface ExecuteArgs {
    inner: Instruction;
    policies?: readonly Address[];
    computeUnitLimit?: number;
    computeUnitPrice?: bigint;
    memo?: { program: Address; data: Uint8Array };
}

export interface TxOpts {
    feePayer?: TransactionSigner<Address>;
    commitment?: Commitment;
}

export interface SessionHandle {
    readonly pubkey: Address;
    readonly owner: Address;
    readonly sessionKey: SessionSigner;

    attach(data: PolicyDataArgs, opts?: TxOpts): Promise<AttachResult>;
    attachMany(
        data: readonly PolicyDataArgs[],
        opts?: TxOpts
    ): Promise<readonly AttachResult[]>;
    update(
        seed: bigint,
        data: PolicyDataArgs,
        opts?: TxOpts
    ): Promise<Signature>;
    detach(seed: bigint, opts?: TxOpts): Promise<Signature>;
    extend(newExpiry: bigint, opts?: TxOpts): Promise<Signature>;
    revoke(opts?: TxOpts): Promise<Signature>;
    close(opts?: TxOpts): Promise<Signature>;
    sweep(destination: Address, opts?: TxOpts): Promise<Signature>;
    execute(args: ExecuteArgs, opts?: TxOpts): Promise<Signature>;
    approveAllowance(
        args: { mint: Address; amount: bigint; tokenProgram?: Address },
        opts?: TxOpts
    ): Promise<{ signature: Signature; source: Address; delegate: Address }>;
    revokeAllowance(
        args: { mint: Address; tokenProgram?: Address },
        opts?: TxOpts
    ): Promise<Signature>;
    allowanceSource(mint: Address, tokenProgram?: Address): Promise<Address>;

    state(): Promise<Session>;
    policies(): Promise<readonly Account<Policy>[]>;
    delegateBalance(): Promise<bigint>;
    isExpired(now?: bigint): Promise<boolean>;
}

interface SessionHandleArgs {
    config: ResolvedBastionConfig;
    rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
    pubkey: Address;
    sessionKey: SessionSigner;
    logger: Logger;
}

function createSessionHandle(args: SessionHandleArgs): SessionHandle {
    const { config, rpcSubscriptions, pubkey, sessionKey, logger } = args;
    const owner = config.wallet.address;
    const hooks = config.hooks;

    const send = (instructions: Instruction[], opts?: TxOpts) =>
        sendTx({
            rpc: config.rpc,
            rpcSubscriptions,
            feePayer: opts?.feePayer ?? config.wallet,
            instructions,
            commitment: opts?.commitment ?? config.commitment,
        });

    const base = () => ({ sessionPda: pubkey, owner, startedAt: Date.now() });

    const fetchPolicyAccounts = async (): Promise<Account<Policy>[]> => {
        const session = await fetchSession(config.rpc, pubkey);
        const nextSeed = Number(session.data.nextSeed);
        const addresses: Address[] = [];
        for (let i = 0; i < nextSeed; i++) {
            const [policyPda] = await pda.policy(pubkey, BigInt(i));
            addresses.push(policyPda);
        }
        const maybe = await fetchAllMaybePolicy(config.rpc, addresses);
        const existing: Account<Policy>[] = [];
        for (const a of maybe) {
            if (a.exists) existing.push(a);
        }
        return existing;
    };

    return {
        pubkey,
        owner,
        sessionKey,

        async attach(data, opts) {
            const session = await fetchSession(config.rpc, pubkey);
            const seed = session.data.nextSeed;
            const [policyPda] = await pda.policy(pubkey, seed);
            const existing = await fetchPolicyAccounts();
            return withHooks(
                hooks,
                { op: "attach", ...base(), data },
                async () => {
                    const ix = getAttachPolicyInstruction(
                        {
                            owner: config.wallet,
                            session: pubkey,
                            policy: policyPda,
                            data,
                        },
                        { programAddress: config.programId }
                    );
                    const ixWithExisting: Instruction = {
                        ...ix,
                        accounts: [
                            ...ix.accounts,
                            ...existing.map((p) => ({
                                address: p.address,
                                role: AccountRole.READONLY,
                            })),
                        ],
                    };
                    const signature = await send([ixWithExisting], opts);
                    logger.info("attach.confirmed", {
                        op: "attach",
                        sessionPda: pubkey,
                        policyPda,
                        seed: seed.toString(),
                    });
                    return { signature, policyPda, seed };
                },
                (r) => r,
                wrapSendError
            );
        },

        async attachMany(data, opts) {
            const results: AttachResult[] = [];
            for (const d of data) results.push(await this.attach(d, opts));
            return results;
        },

        async update(seed, data, opts) {
            return withHooks(
                hooks,
                { op: "update", ...base(), seed, data },
                async () => {
                    const [policyPda] = await pda.policy(pubkey, seed);
                    const ix = getUpdatePolicyInstruction(
                        {
                            owner: config.wallet,
                            session: pubkey,
                            policy: policyPda,
                            seed,
                            newData: data,
                        },
                        { programAddress: config.programId }
                    );
                    return send([ix], opts);
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async detach(seed, opts) {
            const [policyPda] = await pda.policy(pubkey, seed);
            const others = (await fetchPolicyAccounts()).filter(
                (p) => p.address !== policyPda
            );
            return withHooks(
                hooks,
                { op: "detach", ...base(), seed },
                async () => {
                    const ix = getDetachPolicyInstruction(
                        {
                            owner: config.wallet,
                            session: pubkey,
                            policy: policyPda,
                            seed,
                        },
                        { programAddress: config.programId }
                    );
                    const ixWithOthers: Instruction = {
                        ...ix,
                        accounts: [
                            ...ix.accounts,
                            ...others.map((p) => ({
                                address: p.address,
                                role: AccountRole.READONLY,
                            })),
                        ],
                    };
                    return send([ixWithOthers], opts);
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async extend(newExpiry, opts) {
            return withHooks(
                hooks,
                { op: "extend", ...base(), newExpiry },
                async () => {
                    const ix = getExtendSessionInstruction(
                        { owner: config.wallet, session: pubkey, newExpiry },
                        { programAddress: config.programId }
                    );
                    return send([ix], opts);
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async revoke(opts) {
            return withHooks(
                hooks,
                { op: "revoke", ...base() },
                async () => {
                    const ix = getRevokeSessionInstruction(
                        { owner: config.wallet, session: pubkey },
                        { programAddress: config.programId }
                    );
                    const sig = await send([ix], opts);
                    logger.info("revoke.confirmed", {
                        op: "revoke",
                        sessionPda: pubkey,
                    });
                    return sig;
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async close(opts) {
            return withHooks(
                hooks,
                { op: "close", ...base() },
                async () => {
                    const children = await fetchPolicyAccounts();
                    const ix = getCloseSessionInstruction(
                        { owner: config.wallet, session: pubkey },
                        { programAddress: config.programId }
                    );
                    const withChildren: Instruction = {
                        ...ix,
                        accounts: [
                            ...ix.accounts,
                            ...children.map((c) => ({
                                address: c.address,
                                role: AccountRole.WRITABLE,
                            })),
                        ],
                    };
                    return send([withChildren], opts);
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async sweep(destination, opts) {
            return withHooks(
                hooks,
                { op: "sweep", ...base(), destination },
                async () => {
                    const [delegatePda] = await pda.delegate(
                        owner,
                        sessionKey.address
                    );
                    const ix = getSweepDelegateInstruction(
                        {
                            owner: config.wallet,
                            session: pubkey,
                            delegate: delegatePda,
                            destination,
                        },
                        { programAddress: config.programId }
                    );
                    return send([ix], opts);
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async execute(execArgs, opts) {
            const policyAddresses =
                execArgs.policies ??
                (await fetchPolicyAccounts()).map((a) => a.address);
            const plan = await planExecution(
                config.rpc,
                policyAddresses,
                execArgs
            );
            return withHooks(
                hooks,
                {
                    op: "execute",
                    ...base(),
                    innerIx: execArgs.inner,
                    policies: plan.policies,
                    outerIxs: plan.outerIxs,
                },
                async () => {
                    const wrapped = wrapInner(execArgs.inner);
                    const [delegatePda] = await pda.delegate(
                        owner,
                        sessionKey.address
                    );
                    const ix = getExecuteInstruction(
                        {
                            sessionKey,
                            session: pubkey,
                            programId: wrapped.programId,
                            accounts: wrapped.accounts,
                            data: wrapped.data,
                            policyCount: plan.policies.length,
                        },
                        { programAddress: config.programId }
                    );
                    const ixs: Instruction[] = [
                        ...plan.outerIxs,
                        {
                            ...ix,
                            accounts: [
                                ...ix.accounts,
                                ...plan.policies.map(({ address }) => ({
                                    address,
                                    role: AccountRole.WRITABLE,
                                })),
                                {
                                    address: delegatePda,
                                    role: AccountRole.WRITABLE,
                                },
                                ...wrapped.innerMetas.map((m) =>
                                    m.address === delegatePda
                                        ? {
                                              address: m.address,
                                              role: AccountRole.WRITABLE,
                                          }
                                        : m
                                ),
                                {
                                    address: wrapped.programId,
                                    role: AccountRole.READONLY,
                                },
                            ],
                        },
                    ];
                    const sig = await send(ixs, opts);
                    logger.info("execute.confirmed", {
                        op: "execute",
                        sessionPda: pubkey,
                        signature: sig,
                    });
                    return sig;
                },
                (signature) => ({ signature }),
                wrapSendError
            );
        },

        async allowanceSource(mint, tokenProgram) {
            return associatedTokenAddress({
                owner,
                mint,
                ...(tokenProgram ? { tokenProgram } : {}),
            });
        },

        async approveAllowance(args, opts) {
            const [delegatePda] = await pda.delegate(owner, sessionKey.address);
            const source = await associatedTokenAddress({
                owner,
                mint: args.mint,
                ...(args.tokenProgram
                    ? { tokenProgram: args.tokenProgram }
                    : {}),
            });
            const ix = buildApproveIx({
                source,
                delegate: delegatePda,
                owner,
                amount: args.amount,
                ...(args.tokenProgram
                    ? { tokenProgram: args.tokenProgram }
                    : {}),
            });
            const signature = await send([ix], opts);
            logger.info("approveAllowance.confirmed", {
                op: "approveAllowance",
                sessionPda: pubkey,
                source,
                delegate: delegatePda,
                amount: args.amount.toString(),
            });
            return { signature, source, delegate: delegatePda };
        },

        async revokeAllowance(args, opts) {
            const source = await associatedTokenAddress({
                owner,
                mint: args.mint,
                ...(args.tokenProgram
                    ? { tokenProgram: args.tokenProgram }
                    : {}),
            });
            const ix = buildRevokeIx({
                source,
                owner,
                ...(args.tokenProgram
                    ? { tokenProgram: args.tokenProgram }
                    : {}),
            });
            return send([ix], opts);
        },

        async state() {
            const result = await fetchSession(config.rpc, pubkey);
            return result.data;
        },

        async policies() {
            return fetchPolicyAccounts();
        },

        async delegateBalance() {
            const [delegatePda] = await pda.delegate(owner, sessionKey.address);
            const { value } = await config.rpc.getBalance(delegatePda).send();
            return value;
        },

        async isExpired(now) {
            const session = await fetchSession(config.rpc, pubkey);
            const ts = now ?? BigInt(Math.floor(Date.now() / 1000));
            return ts > session.data.expiry;
        },
    };
}

export interface OpenSessionArgs {
    expiry: bigint | Date | { secsFromNow: number };
    sessionKey?: SessionSigner;
}

export interface HydrateSessionArgs {
    pubkey: Address;
    sessionKey: SessionSigner;
}

export interface BastionSessionManager {
    open(args: OpenSessionArgs): Promise<SessionHandle>;
    hydrate(args: HydrateSessionArgs): SessionHandle;
    listMine(): Promise<readonly Address[]>;
}

export function createSessionManager(
    config: ResolvedBastionConfig,
    rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi> | undefined,
    logger: Logger
): BastionSessionManager {
    const requireSubs = (): RpcSubscriptions<SolanaRpcSubscriptionsApi> => {
        if (!rpcSubscriptions) {
            throw new Error(
                "rpcSubscriptions required: pass `url`/`wsUrl` or `rpcSubscriptions` to createBastion"
            );
        }
        return rpcSubscriptions;
    };

    return {
        async open(args) {
            const subs = requireSubs();
            const sessionKey = args.sessionKey ?? (await generateSessionKey());
            const owner = config.wallet.address;
            const [sessionPda] = await pda.session(owner, sessionKey.address);
            const expiry = resolveExpiry(args.expiry);
            const ix = getInitSessionInstruction(
                {
                    owner: config.wallet,
                    session: sessionPda,
                    sessionKey: sessionKey.address,
                    expiry,
                },
                { programAddress: config.programId }
            );
            await sendTx({
                rpc: config.rpc,
                rpcSubscriptions: subs,
                feePayer: config.wallet,
                instructions: [ix],
                commitment: config.commitment,
            });
            logger.info("session.opened", {
                op: "open",
                sessionPda,
                sessionKey: sessionKey.address,
            });
            return createSessionHandle({
                config,
                rpcSubscriptions: subs,
                pubkey: sessionPda,
                sessionKey,
                logger,
            });
        },

        hydrate(args) {
            return createSessionHandle({
                config,
                rpcSubscriptions: requireSubs(),
                pubkey: args.pubkey,
                sessionKey: args.sessionKey,
                logger,
            });
        },

        async listMine() {
            const owner = config.wallet.address;
            const result = await config.rpc
                .getProgramAccounts(config.programId, {
                    encoding: "base64",
                    filters: [
                        {
                            memcmp: {
                                offset: 8n,
                                bytes: owner as unknown as Base58EncodedBytes,
                                encoding: "base58",
                            },
                        },
                    ],
                    withContext: false,
                })
                .send();
            return (result as ReadonlyArray<{ pubkey: Address }>).map(
                (r) => r.pubkey
            );
        },
    };
}
