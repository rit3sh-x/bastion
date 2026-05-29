import {
    AccountRole,
    appendTransactionMessageInstructions,
    assertIsTransactionMessageWithinSizeLimit,
    assertIsTransactionWithBlockhashLifetime,
    createTransactionMessage,
    getSignatureFromTransaction,
    isSignerRole,
    isWritableRole,
    pipe,
    sendAndConfirmTransactionFactory,
    setTransactionMessageFeePayerSigner,
    setTransactionMessageLifetimeUsingBlockhash,
    signTransactionMessageWithSigners,
    type AccountMeta,
    type Address,
    type Commitment,
    type Instruction,
    type ReadonlyUint8Array,
    type Rpc,
    type RpcSubscriptions,
    type Signature,
    type SolanaRpcApi,
    type SolanaRpcSubscriptionsApi,
    type TransactionSigner,
} from "@solana/kit";
import {
    fetchAllPolicy,
    type CompactAccountMeta,
    type Policy,
} from "./generated";
import { wrapSendError } from "./errors";

const BASTION_FLAG_SIGNER = 0b01;
const BASTION_FLAG_WRITABLE = 0b10;

const COMPUTE_BUDGET_PROGRAM_ADDRESS =
    "ComputeBudget111111111111111111111111111111" as Address;

export interface WrappedInner {
    programId: Address;
    accounts: CompactAccountMeta[];
    data: ReadonlyUint8Array;
    innerMetas: AccountMeta[];
}

export function wrapInner(inner: Instruction): WrappedInner {
    const idxByKey = new Map<Address, number>();
    const innerMetas: AccountMeta[] = [];
    const compact: CompactAccountMeta[] = [];
    const innerAccounts = inner.accounts ?? [];

    for (const meta of innerAccounts) {
        const existing = idxByKey.get(meta.address);
        if (existing === undefined) {
            idxByKey.set(meta.address, innerMetas.length);
            innerMetas.push({ address: meta.address, role: meta.role });
        } else {
            const prev = innerMetas[existing]!;
            innerMetas[existing] = {
                address: prev.address,
                role: mergeAccountRole(prev.role, meta.role),
            };
        }
    }

    for (const meta of innerAccounts) {
        const idx = idxByKey.get(meta.address)!;
        compact.push({
            index: idx,
            flags: roleToBastionFlags(innerMetas[idx]!.role),
        });
    }

    return {
        programId: inner.programAddress,
        accounts: compact,
        data: inner.data ?? new Uint8Array(0),
        innerMetas,
    };
}

function mergeAccountRole(a: AccountRole, b: AccountRole): AccountRole {
    const signer = isSignerRole(a) || isSignerRole(b);
    const writable = isWritableRole(a) || isWritableRole(b);
    if (signer && writable) return AccountRole.WRITABLE_SIGNER;
    if (signer) return AccountRole.READONLY_SIGNER;
    if (writable) return AccountRole.WRITABLE;
    return AccountRole.READONLY;
}

function roleToBastionFlags(role: AccountRole): number {
    let flags = 0;
    if (isSignerRole(role)) flags |= BASTION_FLAG_SIGNER;
    if (isWritableRole(role)) flags |= BASTION_FLAG_WRITABLE;
    return flags;
}

export interface OuterIxArgs {
    computeUnitLimit?: number;
    computeUnitPrice?: bigint;
    memo?: { program: Address; data: Uint8Array };
}

export interface ExecutionPlan {
    policies: { address: Address; account: Policy }[];
    outerIxs: Instruction[];
}

function setComputeUnitLimitIx(limit: number): Instruction {
    if (!Number.isInteger(limit) || limit < 0 || limit > 0xff_ff_ff_ff) {
        throw new RangeError(
            `computeUnitLimit must be a u32 (0..4294967295): ${limit}`
        );
    }
    const data = new Uint8Array(5);
    data[0] = 2;
    new DataView(data.buffer).setUint32(1, limit, true);
    return {
        programAddress: COMPUTE_BUDGET_PROGRAM_ADDRESS,
        accounts: [],
        data,
    };
}

function setComputeUnitPriceIx(microLamports: bigint): Instruction {
    if (microLamports < 0n || microLamports > 0xff_ff_ff_ff_ff_ff_ff_ffn) {
        throw new RangeError(
            `computeUnitPrice must be a u64: ${microLamports}`
        );
    }
    const data = new Uint8Array(9);
    data[0] = 3;
    new DataView(data.buffer).setBigUint64(1, microLamports, true);
    return {
        programAddress: COMPUTE_BUDGET_PROGRAM_ADDRESS,
        accounts: [],
        data,
    };
}

function memoIx(program: Address, data: Uint8Array): Instruction {
    return { programAddress: program, accounts: [], data };
}

export async function planExecution(
    rpc: Rpc<SolanaRpcApi>,
    policyAddresses: readonly Address[],
    args: OuterIxArgs
): Promise<ExecutionPlan> {
    const decoded = await fetchAllPolicy(rpc, [...policyAddresses]);
    const pairs = decoded
        .map((acc) => ({ address: acc.address, account: acc.data }))
        .sort((a, b) =>
            a.address < b.address ? -1 : a.address > b.address ? 1 : 0
        );

    let hasCuPolicy = false;
    let hasPricePolicy = false;
    for (const { account } of pairs) {
        const kind = account.data.__kind;
        if (kind === "MaxComputeUnits") hasCuPolicy = true;
        if (kind === "MaxPriorityFee") hasPricePolicy = true;
    }

    const outerIxs: Instruction[] = [];
    if (args.computeUnitLimit !== undefined) {
        outerIxs.push(setComputeUnitLimitIx(args.computeUnitLimit));
    } else if (hasCuPolicy) {
        outerIxs.push(setComputeUnitLimitIx(400_000));
    }
    if (args.computeUnitPrice !== undefined) {
        outerIxs.push(setComputeUnitPriceIx(args.computeUnitPrice));
    } else if (hasPricePolicy) {
        outerIxs.push(setComputeUnitPriceIx(0n));
    }
    if (args.memo !== undefined) {
        outerIxs.push(memoIx(args.memo.program, args.memo.data));
    }

    return { policies: pairs, outerIxs };
}

export interface SendArgs {
    rpc: Rpc<SolanaRpcApi>;
    rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
    feePayer: TransactionSigner;
    instructions: readonly Instruction[];
    commitment?: Commitment;
}

export async function sendTx(args: SendArgs): Promise<Signature> {
    try {
        const { value: latest } = await args.rpc
            .getLatestBlockhash({ commitment: args.commitment ?? "confirmed" })
            .send();

        const message = pipe(
            createTransactionMessage({ version: 0 }),
            (m) => setTransactionMessageFeePayerSigner(args.feePayer, m),
            (m) => setTransactionMessageLifetimeUsingBlockhash(latest, m),
            (m) =>
                appendTransactionMessageInstructions([...args.instructions], m)
        );
        assertIsTransactionMessageWithinSizeLimit(message);

        const signed = await signTransactionMessageWithSigners(message);
        assertIsTransactionWithBlockhashLifetime(signed);
        const send = sendAndConfirmTransactionFactory({
            rpc: args.rpc,
            rpcSubscriptions: args.rpcSubscriptions,
        });
        await send(signed, { commitment: args.commitment ?? "confirmed" });

        return getSignatureFromTransaction(signed);
    } catch (err) {
        throw wrapSendError(err);
    }
}
