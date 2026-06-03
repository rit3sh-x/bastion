import { describe, expect, it } from "vitest";
import { address } from "@solana/kit";
import {
    ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS,
    buildCreateLookupTableInstruction,
    buildExtendLookupTableInstruction,
    deriveLookupTableAddress,
} from "@/alt";

const AUTH = address("So11111111111111111111111111111111111111112");
const A = address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const B = address("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

describe("deriveLookupTableAddress", () => {
    it("is deterministic and returns a canonical bump", async () => {
        const [addr1, bump] = await deriveLookupTableAddress(AUTH, 123n);
        const [addr2] = await deriveLookupTableAddress(AUTH, 123n);
        expect(addr1).toBe(addr2);
        expect(bump).toBeGreaterThanOrEqual(0);
        expect(bump).toBeLessThanOrEqual(255);
        const [other] = await deriveLookupTableAddress(AUTH, 124n);
        expect(other).not.toBe(addr1);
    });
});

describe("buildCreateLookupTableInstruction", () => {
    it("encodes discriminator 0, slot, bump and the 4 accounts", () => {
        const lut = A;
        const ix = buildCreateLookupTableInstruction({
            lookupTable: lut,
            authority: AUTH,
            payer: AUTH,
            recentSlot: 0x0102n,
            bump: 254,
        });
        expect(ix.programAddress).toBe(ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS);
        const d = ix.data as Uint8Array;
        const view = new DataView(d.buffer, d.byteOffset, d.byteLength);
        expect(view.getUint32(0, true)).toBe(0); // CreateLookupTable
        expect(view.getBigUint64(4, true)).toBe(0x0102n);
        expect(d[12]).toBe(254); // bump
        expect(ix.accounts).toHaveLength(4);
    });
});

describe("buildExtendLookupTableInstruction", () => {
    it("encodes discriminator 2, u64 count, and the addresses", () => {
        const ix = buildExtendLookupTableInstruction({
            lookupTable: A,
            authority: AUTH,
            payer: AUTH,
            addresses: [A, B],
        });
        const d = ix.data as Uint8Array;
        const view = new DataView(d.buffer, d.byteOffset, d.byteLength);
        expect(view.getUint32(0, true)).toBe(2); // ExtendLookupTable
        expect(view.getBigUint64(4, true)).toBe(2n); // count
        expect(d).toHaveLength(4 + 8 + 32 * 2);
    });
});
