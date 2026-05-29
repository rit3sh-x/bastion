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

const IX_TRANSFER = 3;
const IX_APPROVE = 4;
const IX_REVOKE = 5;
const IX_ATA_CREATE_IDEMPOTENT = 1;

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
