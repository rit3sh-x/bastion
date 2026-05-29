import { describe, expect, it } from "vitest";
import { AccountRole, type Address, type Instruction } from "@solana/kit";
import { wrapInner } from "../src/execute";

const P1 = "11111111111111111111111111111111" as Address;
const A1 = "So11111111111111111111111111111111111111112" as Address;
const A2 = "Stake11111111111111111111111111111111111111" as Address;

describe("wrapInner — basic shape", () => {
    it("maps programAddress + data", () => {
        const inner: Instruction = {
            programAddress: P1,
            accounts: [],
            data: new Uint8Array([1, 2, 3]),
        };
        const w = wrapInner(inner);
        expect(w.programId).toBe(P1);
        expect(w.data).toEqual(new Uint8Array([1, 2, 3]));
        expect(w.accounts).toEqual([]);
        expect(w.innerMetas).toEqual([]);
    });
});

describe("wrapInner — full kit AccountRole → Bastion flag matrix", () => {
    const one = (role: AccountRole) =>
        wrapInner({
            programAddress: P1,
            accounts: [{ address: A1, role }],
            data: new Uint8Array(),
        }).accounts[0]!.flags;

    it("READONLY → 0b00", () => {
        expect(one(AccountRole.READONLY)).toBe(0b00);
    });
    it("WRITABLE → 0b10 (writable bit only)", () => {
        expect(one(AccountRole.WRITABLE)).toBe(0b10);
    });
    it("READONLY_SIGNER → 0b01 (signer bit only)", () => {
        expect(one(AccountRole.READONLY_SIGNER)).toBe(0b01);
    });
    it("WRITABLE_SIGNER → 0b11 (both bits)", () => {
        expect(one(AccountRole.WRITABLE_SIGNER)).toBe(0b11);
    });
});

describe("wrapInner — account dedup + flag merge", () => {
    it("dedups repeated address, merges flags", () => {
        const inner: Instruction = {
            programAddress: P1,
            accounts: [
                { address: A1, role: AccountRole.READONLY },
                { address: A1, role: AccountRole.WRITABLE },
            ],
            data: new Uint8Array(),
        };
        const w = wrapInner(inner);
        expect(w.innerMetas).toHaveLength(1);
        expect(w.accounts).toHaveLength(2);
        expect(w.accounts[0]!.index).toBe(0);
        expect(w.accounts[1]!.index).toBe(0);
        expect(w.accounts[0]!.flags & 0b10).toBeTruthy();
    });

    it("translates AccountRole → Bastion flags (signer + writable)", () => {
        const inner: Instruction = {
            programAddress: P1,
            accounts: [
                { address: A1, role: AccountRole.WRITABLE_SIGNER },
                { address: A2, role: AccountRole.READONLY_SIGNER },
            ],
            data: new Uint8Array(),
        };
        const w = wrapInner(inner);
        expect(w.accounts[0]!.flags).toBe(0b11);
        expect(w.accounts[1]!.flags).toBe(0b01);
    });
});

describe("wrapInner — preserves inner metas for outer tx", () => {
    it("innerMetas keep kit AccountRole semantics", () => {
        const inner: Instruction = {
            programAddress: P1,
            accounts: [{ address: A1, role: AccountRole.WRITABLE }],
            data: new Uint8Array([9]),
        };
        const w = wrapInner(inner);
        expect(w.innerMetas).toEqual([
            { address: A1, role: AccountRole.WRITABLE },
        ]);
    });
});
