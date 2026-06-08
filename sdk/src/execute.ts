import {
    AccountRole,
    appendTransactionMessageInstructions,
    assertIsTransactionMessageWithinSizeLimit,
    compressTransactionMessageUsingAddressLookupTables,
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
    COMPUTE_BUDGET_ID,
} from "./generated";
import { wrapSendError } from "./errors";

const BASTION_FLAG_SIGNER = 0b01;
const BASTION_FLAG_WRITABLE = 0b10;

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

export interface WrappedLeg {
    programId: Address;
    accounts: CompactAccountMeta[];
    data: ReadonlyUint8Array;
}

export interface WrappedBatch {
    legs: WrappedLeg[];
    innerMetas: AccountMeta[];
    programIds: Address[];
}

/**
 * Wrap N inner instructions into one batch sharing a single deduplicated account
 * pool — mirrors the program's `wrapped_ixs: Vec<WrappedInstruction>` over a
 * shared `ix_accounts` slice. Each leg references the pool by index.
 */
export function wrapInnerBatch(inners: readonly Instruction[]): WrappedBatch {
    const idxByKey = new Map<Address, number>();
    const innerMetas: AccountMeta[] = [];

    for (const inner of inners) {
        for (const meta of inner.accounts ?? []) {
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
    }

    const legs: WrappedLeg[] = inners.map((inner) => ({
        programId: inner.programAddress,
        accounts: (inner.accounts ?? []).map((meta) => {
            const idx = idxByKey.get(meta.address)!;
            return {
                index: idx,
                flags: roleToBastionFlags(innerMetas[idx]!.role),
            };
        }),
        data: inner.data ?? new Uint8Array(0),
    }));

    const programIds = [...new Set(inners.map((i) => i.programAddress))];

    return { legs, innerMetas, programIds };
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
        programAddress: COMPUTE_BUDGET_ID,
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
        programAddress: COMPUTE_BUDGET_ID,
        accounts: [],
        data,
    };
}

function memoIx(program: Address, data: Uint8Array): Instruction {
    return { programAddress: program, accounts: [], data };
}

/** Rough CU cost of validating + charging one policy of the given kind. */
function policyCuWeight(kind: string): number {
    switch (kind) {
        case "NftCollectionAllowlist":
        case "NftCollectionBlocklist":
        case "NftCreatorAllowlist":
            return 25_000;
        case "SpendCap":
        case "AmountPerCall":
        case "PerCounterpartyCap":
        case "PerProgramSpendCap":
        case "MinDelegateBalance":
            return 12_000;
        default:
            return 6_000;
    }
}

/** Dynamic CU estimate: base + legs × (Σ per-policy weight + per-leg CPI). */
export function estimateComputeUnits(
    kinds: readonly string[],
    legCount: number
): number {
    const perLeg = kinds.reduce((s, k) => s + policyCuWeight(k), 0) + 10_000;
    const est = 30_000 + Math.max(1, legCount) * perLeg;
    return Math.min(Math.max(est, 50_000), 1_400_000);
}

export async function planExecution(
    rpc: Rpc<SolanaRpcApi>,
    policyAddresses: readonly Address[],
    args: OuterIxArgs,
    legCount = 1
): Promise<ExecutionPlan> {
    const decoded = await fetchAllPolicy(rpc, [...policyAddresses]);
    const pairs = decoded
        .map((acc) => ({ address: acc.address, account: acc.data }))
        .sort((a, b) =>
            a.address < b.address ? -1 : a.address > b.address ? 1 : 0
        );

    let hasCuPolicy = false;
    let hasPricePolicy = false;
    let cuPolicyMax: number | undefined;
    const kinds: string[] = [];
    for (const { account } of pairs) {
        kinds.push(account.data.__kind);
        if (account.data.__kind === "MaxComputeUnits") {
            hasCuPolicy = true;
            cuPolicyMax = account.data.max;
        } else if (account.data.__kind === "MaxPriorityFee") {
            hasPricePolicy = true;
        }
    }

    const outerIxs: Instruction[] = [];
    if (args.computeUnitLimit !== undefined) {
        outerIxs.push(setComputeUnitLimitIx(args.computeUnitLimit));
    } else if (hasCuPolicy || legCount > 1) {
        let cu = estimateComputeUnits(kinds, legCount);
        if (cuPolicyMax !== undefined) cu = Math.min(cu, cuPolicyMax);
        outerIxs.push(setComputeUnitLimitIx(cu));
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
    addressLookupTables?: Record<Address, Address[]>;
}

export async function sendTx(args: SendArgs): Promise<Signature> {
    try {
        const { value: latest } = await args.rpc
            .getLatestBlockhash({ commitment: args.commitment ?? "confirmed" })
            .send();

        const base = pipe(
            createTransactionMessage({ version: 0 }),
            (m) => setTransactionMessageFeePayerSigner(args.feePayer, m),
            (m) => setTransactionMessageLifetimeUsingBlockhash(latest, m),
            (m) =>
                appendTransactionMessageInstructions([...args.instructions], m)
        );
        const message =
            args.addressLookupTables &&
            Object.keys(args.addressLookupTables).length > 0
                ? compressTransactionMessageUsingAddressLookupTables(
                      base,
                      args.addressLookupTables
                  )
                : base;
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
