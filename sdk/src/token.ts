import {
    AccountRole,
    getAddressEncoder,
    getProgramDerivedAddress,
    type Address,
    type Instruction,
} from "@solana/kit";

export const TOKEN_PROGRAM_ADDRESS =
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" as Address;
export const TOKEN_2022_PROGRAM_ADDRESS =
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" as Address;
export const ASSOCIATED_TOKEN_PROGRAM_ADDRESS =
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL" as Address;
const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;

/** Canonical wrapped-SOL mint (classic SPL Token). */
export const NATIVE_MINT =
    "So11111111111111111111111111111111111111112" as Address;

const IX_TRANSFER = 3;
const IX_APPROVE = 4;
const IX_REVOKE = 5;
const IX_CLOSE_ACCOUNT = 9;
const IX_SYNC_NATIVE = 17;
const IX_ATA_CREATE_IDEMPOTENT = 1;
const SYSTEM_IX_TRANSFER = 2;

const U64_MAX = 0xff_ff_ff_ff_ff_ff_ff_ffn;

function tagAndU64(tag: number, amount: bigint): Uint8Array {
    if (amount < 0n || amount > U64_MAX) {
        throw new RangeError(`token amount out of u64 range: ${amount}`);
    }
    const data = new Uint8Array(9);
    data[0] = tag;
    new DataView(data.buffer).setBigUint64(1, amount, true);
    return data;
}

export interface ApproveArgs {
    source: Address;
    delegate: Address;
    owner: Address;
    amount: bigint;
    tokenProgram?: Address;
}

export function buildApproveIx(args: ApproveArgs): Instruction {
    return {
        programAddress: args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.source, role: AccountRole.WRITABLE },
            { address: args.delegate, role: AccountRole.READONLY },
            { address: args.owner, role: AccountRole.READONLY_SIGNER },
        ],
        data: tagAndU64(IX_APPROVE, args.amount),
    };
}

export interface RevokeArgs {
    source: Address;
    owner: Address;
    tokenProgram?: Address;
}

export function buildRevokeIx(args: RevokeArgs): Instruction {
    return {
        programAddress: args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.source, role: AccountRole.WRITABLE },
            { address: args.owner, role: AccountRole.READONLY_SIGNER },
        ],
        data: new Uint8Array([IX_REVOKE]),
    };
}

export interface TokenTransferArgs {
    source: Address;
    dest: Address;
    authority: Address;
    amount: bigint;
    tokenProgram?: Address;
}

export function buildTokenTransferIx(args: TokenTransferArgs): Instruction {
    return {
        programAddress: args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.source, role: AccountRole.WRITABLE },
            { address: args.dest, role: AccountRole.WRITABLE },
            { address: args.authority, role: AccountRole.READONLY_SIGNER },
        ],
        data: tagAndU64(IX_TRANSFER, args.amount),
    };
}

export interface AtaArgs {
    owner: Address;
    mint: Address;
    tokenProgram?: Address;
}

export async function associatedTokenAddress(args: AtaArgs): Promise<Address> {
    const tokenProgram = args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS;
    const enc = getAddressEncoder();
    const [ata] = await getProgramDerivedAddress({
        programAddress: ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
        seeds: [
            enc.encode(args.owner),
            enc.encode(tokenProgram),
            enc.encode(args.mint),
        ],
    });
    return ata;
}

export interface CreateAtaArgs {
    payer: Address;
    ata: Address;
    owner: Address;
    mint: Address;
    tokenProgram?: Address;
}

export function buildCreateAtaIdempotentIx(args: CreateAtaArgs): Instruction {
    return {
        programAddress: ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.payer, role: AccountRole.WRITABLE_SIGNER },
            { address: args.ata, role: AccountRole.WRITABLE },
            { address: args.owner, role: AccountRole.READONLY },
            { address: args.mint, role: AccountRole.READONLY },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            {
                address: args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS,
                role: AccountRole.READONLY,
            },
        ],
        data: new Uint8Array([IX_ATA_CREATE_IDEMPOTENT]),
    };
}

export interface SyncNativeArgs {
    account: Address;
    tokenProgram?: Address;
}

export function buildSyncNativeIx(args: SyncNativeArgs): Instruction {
    return {
        programAddress: args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS,
        accounts: [{ address: args.account, role: AccountRole.WRITABLE }],
        data: new Uint8Array([IX_SYNC_NATIVE]),
    };
}

export interface CloseAccountArgs {
    account: Address;
    destination: Address;
    owner: Address;
    tokenProgram?: Address;
}

export function buildCloseAccountIx(args: CloseAccountArgs): Instruction {
    return {
        programAddress: args.tokenProgram ?? TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.account, role: AccountRole.WRITABLE },
            { address: args.destination, role: AccountRole.WRITABLE },
            { address: args.owner, role: AccountRole.READONLY_SIGNER },
        ],
        data: new Uint8Array([IX_CLOSE_ACCOUNT]),
    };
}

function buildSystemTransferIx(
    from: Address,
    to: Address,
    lamports: bigint
): Instruction {
    if (lamports < 0n || lamports > U64_MAX) {
        throw new RangeError(`lamports out of u64 range: ${lamports}`);
    }
    const data = new Uint8Array(12);
    const view = new DataView(data.buffer);
    view.setUint32(0, SYSTEM_IX_TRANSFER, true);
    view.setBigUint64(4, lamports, true);
    return {
        programAddress: SYSTEM_PROGRAM_ADDRESS,
        accounts: [
            { address: from, role: AccountRole.WRITABLE_SIGNER },
            { address: to, role: AccountRole.WRITABLE },
        ],
        data,
    };
}

export interface WrapSolAllowanceArgs {
    owner: Address;
    delegate: Address;
    amount: bigint;
    payer?: Address;
}

export interface WrapSolAllowanceResult {
    ata: Address;
    instructions: Instruction[];
}

/**
 * Build the holder-side instruction sequence that puts SOL under Bastion as an
 * **allowance** rather than a vault. SOL has no native allowance, so
 * we wrap it: fund the owner's wSOL ATA and approve the delegate as its SPL
 * spender. The lamports stay in an owner-owned account — the delegate never
 * custodies native SOL — and a `SpendCap` on `Asset::SplToken(NATIVE_MINT)`
 * gates spends via the standard pre/post balance delta.
 *
 * Order: create ATA (idempotent) → fund it → `SyncNative` → `Approve(delegate)`.
 * All instructions are owner-signed; pair with `unwrapSolIxs` to reclaim.
 */
export async function wrapSolAllowanceIxs(
    args: WrapSolAllowanceArgs
): Promise<WrapSolAllowanceResult> {
    const payer = args.payer ?? args.owner;
    const ata = await associatedTokenAddress({
        owner: args.owner,
        mint: NATIVE_MINT,
    });
    const instructions: Instruction[] = [
        buildCreateAtaIdempotentIx({
            payer,
            ata,
            owner: args.owner,
            mint: NATIVE_MINT,
        }),
        buildSystemTransferIx(args.owner, ata, args.amount),
        buildSyncNativeIx({ account: ata }),
        buildApproveIx({
            source: ata,
            delegate: args.delegate,
            owner: args.owner,
            amount: args.amount,
        }),
    ];
    return { ata, instructions };
}

export interface UnwrapSolArgs {
    owner: Address;
    /** Where reclaimed lamports land. Defaults to `owner`. */
    destination?: Address;
}

export interface UnwrapSolResult {
    ata: Address;
    instructions: Instruction[];
}

/**
 * Reverse {@link wrapSolAllowanceIxs}: revoke the delegate's approval and close
 * the wSOL ATA, returning the wrapped lamports to `destination` (default owner).
 */
export async function unwrapSolIxs(
    args: UnwrapSolArgs
): Promise<UnwrapSolResult> {
    const ata = await associatedTokenAddress({
        owner: args.owner,
        mint: NATIVE_MINT,
    });
    const destination = args.destination ?? args.owner;
    return {
        ata,
        instructions: [
            buildRevokeIx({ source: ata, owner: args.owner }),
            buildCloseAccountIx({
                account: ata,
                destination,
                owner: args.owner,
            }),
        ],
    };
}
