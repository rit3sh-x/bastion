import { describe, expect, it } from "vitest";
import { AccountRole, type Address } from "@solana/kit";
import {
    ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
    TOKEN_2022_PROGRAM_ADDRESS,
    TOKEN_PROGRAM_ADDRESS,
    associatedTokenAddress,
    buildApproveIx,
    buildCreateAtaIdempotentIx,
    buildRevokeIx,
    buildTokenTransferIx,
} from "../src/token";

const OWNER = "So11111111111111111111111111111111111111112" as Address;
const MINT = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" as Address;
const DELEGATE = "11111111111111111111111111111111" as Address;
const SOURCE = "Vote111111111111111111111111111111111111111" as Address;
const DEST = "Stake11111111111111111111111111111111111111" as Address;

function u64le(data: Uint8Array, offset: number): bigint {
    return new DataView(data.buffer, data.byteOffset + offset, 8).getBigUint64(
        0,
        true
    );
}

describe("buildApproveIx", () => {
    it("encodes tag 4 + LE u64 amount and correct roles", () => {
        const amount = 123_456_789n;
        const ix = buildApproveIx({
            source: SOURCE,
            delegate: DELEGATE,
            owner: OWNER,
            amount,
        });
        expect(ix.programAddress).toBe(TOKEN_PROGRAM_ADDRESS);
        expect(ix.data![0]).toBe(4);
        expect(u64le(ix.data as Uint8Array, 1)).toBe(amount);
        expect(ix.accounts).toEqual([
            { address: SOURCE, role: AccountRole.WRITABLE },
            { address: DELEGATE, role: AccountRole.READONLY },
            { address: OWNER, role: AccountRole.READONLY_SIGNER },
        ]);
    });

    it("honours a custom token program", () => {
        const ix = buildApproveIx({
            source: SOURCE,
            delegate: DELEGATE,
            owner: OWNER,
            amount: 1n,
            tokenProgram: TOKEN_2022_PROGRAM_ADDRESS,
        });
        expect(ix.programAddress).toBe(TOKEN_2022_PROGRAM_ADDRESS);
    });
});

describe("buildRevokeIx", () => {
    it("encodes tag 5 with source + owner-signer", () => {
        const ix = buildRevokeIx({ source: SOURCE, owner: OWNER });
        expect(ix.data).toEqual(new Uint8Array([5]));
        expect(ix.accounts).toEqual([
            { address: SOURCE, role: AccountRole.WRITABLE },
            { address: OWNER, role: AccountRole.READONLY_SIGNER },
        ]);
    });
});

describe("buildTokenTransferIx", () => {
    it("encodes tag 3 + amount; authority is a (read-only) signer", () => {
        const amount = 7_000n;
        const ix = buildTokenTransferIx({
            source: SOURCE,
            dest: DEST,
            authority: DELEGATE,
            amount,
        });
        expect(ix.data![0]).toBe(3);
        expect(u64le(ix.data as Uint8Array, 1)).toBe(amount);
        expect(ix.accounts).toEqual([
            { address: SOURCE, role: AccountRole.WRITABLE },
            { address: DEST, role: AccountRole.WRITABLE },
            { address: DELEGATE, role: AccountRole.READONLY_SIGNER },
        ]);
        expect(ix.accounts![2]!.role).toBe(AccountRole.READONLY_SIGNER);
    });
});

describe("buildCreateAtaIdempotentIx", () => {
    it("encodes tag 1 with the 6 ATA-program accounts", () => {
        const ata = SOURCE;
        const ix = buildCreateAtaIdempotentIx({
            payer: OWNER,
            ata,
            owner: OWNER,
            mint: MINT,
        });
        expect(ix.programAddress).toBe(ASSOCIATED_TOKEN_PROGRAM_ADDRESS);
        expect(ix.data).toEqual(new Uint8Array([1]));
        expect(ix.accounts).toHaveLength(6);
        expect(ix.accounts![0]).toEqual({
            address: OWNER,
            role: AccountRole.WRITABLE_SIGNER,
        });
        expect(ix.accounts![1]).toEqual({
            address: ata,
            role: AccountRole.WRITABLE,
        });
    });
});

describe("associatedTokenAddress", () => {
    it("is deterministic for the same inputs", async () => {
        const a = await associatedTokenAddress({ owner: OWNER, mint: MINT });
        const b = await associatedTokenAddress({ owner: OWNER, mint: MINT });
        expect(a).toBe(b);
        expect(typeof a).toBe("string");
        expect((a as string).length).toBeGreaterThanOrEqual(32);
    });

    it("differs by owner and by token program", async () => {
        const base = await associatedTokenAddress({ owner: OWNER, mint: MINT });
        const otherOwner = await associatedTokenAddress({
            owner: DELEGATE,
            mint: MINT,
        });
        const t22 = await associatedTokenAddress({
            owner: OWNER,
            mint: MINT,
            tokenProgram: TOKEN_2022_PROGRAM_ADDRESS,
        });
        expect(otherOwner).not.toBe(base);
        expect(t22).not.toBe(base);
    });
});

describe("u64 amount guard", () => {
    it("rejects negative and >u64 amounts before encoding", () => {
        expect(() =>
            buildTokenTransferIx({
                source: SOURCE,
                dest: DEST,
                authority: DELEGATE,
                amount: -1n,
            })
        ).toThrow(RangeError);
        expect(() =>
            buildApproveIx({
                source: SOURCE,
                delegate: DELEGATE,
                owner: OWNER,
                amount: 2n ** 64n,
            })
        ).toThrow(RangeError);
    });
    it("accepts the u64 maximum", () => {
        const ix = buildTokenTransferIx({
            source: SOURCE,
            dest: DEST,
            authority: DELEGATE,
            amount: 2n ** 64n - 1n,
        });
        expect(ix.data).toHaveLength(9);
    });
});
