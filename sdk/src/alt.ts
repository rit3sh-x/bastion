import {
    AccountRole,
    getAddressEncoder,
    getProgramDerivedAddress,
    type Address,
    type Instruction,
    type ProgramDerivedAddress,
} from "@solana/kit";

export const ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS =
    "AddressLookupTab1e1111111111111111111111111" as Address;

const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;

/**
 * Derive a lookup-table address. The on-chain program derives it as
 * `PDA([authority, recent_slot_le_u64])`, so the SDK must do the same and pass
 * the resulting bump into CreateLookupTable.
 */
export function deriveLookupTableAddress(
    authority: Address,
    recentSlot: bigint
): Promise<ProgramDerivedAddress> {
    const slot = new Uint8Array(8);
    new DataView(slot.buffer).setBigUint64(0, recentSlot, true);
    return getProgramDerivedAddress({
        programAddress: ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS,
        seeds: [getAddressEncoder().encode(authority), slot],
    });
}

/** CreateLookupTable (bincode enum index 0). authority == payer == owner. */
export function buildCreateLookupTableInstruction(args: {
    lookupTable: Address;
    authority: Address;
    payer: Address;
    recentSlot: bigint;
    bump: number;
}): Instruction {
    const data = new Uint8Array(4 + 8 + 1);
    const view = new DataView(data.buffer);
    view.setUint32(0, 0, true); // CreateLookupTable
    view.setBigUint64(4, args.recentSlot, true);
    data[12] = args.bump;
    return {
        programAddress: ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS,
        accounts: [
            { address: args.lookupTable, role: AccountRole.WRITABLE },
            { address: args.authority, role: AccountRole.READONLY_SIGNER },
            { address: args.payer, role: AccountRole.WRITABLE_SIGNER },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}

/** ExtendLookupTable (bincode enum index 2) — appends addresses. */
export function buildExtendLookupTableInstruction(args: {
    lookupTable: Address;
    authority: Address;
    payer: Address;
    addresses: readonly Address[];
}): Instruction {
    const n = args.addresses.length;
    const data = new Uint8Array(4 + 8 + 32 * n);
    const view = new DataView(data.buffer);
    view.setUint32(0, 2, true); // ExtendLookupTable
    view.setBigUint64(4, BigInt(n), true); // bincode Vec length = u64
    let off = 12;
    const enc = getAddressEncoder();
    for (const addr of args.addresses) {
        data.set(enc.encode(addr), off);
        off += 32;
    }
    return {
        programAddress: ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS,
        accounts: [
            { address: args.lookupTable, role: AccountRole.WRITABLE },
            { address: args.authority, role: AccountRole.READONLY_SIGNER },
            { address: args.payer, role: AccountRole.WRITABLE_SIGNER },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}
