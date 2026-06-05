import { describe, expect, it } from "vitest";
import { AccountRole, type Address } from "@solana/kit";
import {
    ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
    NATIVE_MINT,
    TOKEN_2022_PROGRAM_ADDRESS,
    TOKEN_PROGRAM_ADDRESS,
    associatedTokenAddress,
    buildCloseAccountIx,
    buildSyncNativeIx,
    unwrapSolIxs,
    wrapSolAllowanceIxs,
} from "@/token";

const OWNER = "Vote111111111111111111111111111111111111111" as Address;
const DELEGATE = "Stake11111111111111111111111111111111111111" as Address;
const SYSTEM = "11111111111111111111111111111111" as Address;

function u64le(data: Uint8Array, offset: number): bigint {
    return new DataView(data.buffer, data.byteOffset + offset, 8).getBigUint64(
        0,
        true
    );
}

describe("wrapSolAllowanceIxs", () => {
    it("builds create-ATA -> fund -> syncNative -> approve(delegate), allowance not vault", async () => {
        const amount = 5_000_000n;
        const { ata, instructions } = await wrapSolAllowanceIxs({
            owner: OWNER,
            delegate: DELEGATE,
            amount,
        });

        expect(ata).toBe(
            await associatedTokenAddress({ owner: OWNER, mint: NATIVE_MINT })
        );
        expect(instructions).toHaveLength(4);

        const [createAta, fund, sync, approve] = instructions;

        expect(createAta!.programAddress).toBe(
            ASSOCIATED_TOKEN_PROGRAM_ADDRESS
        );
        expect(createAta!.data).toEqual(new Uint8Array([1]));

        expect(fund!.programAddress).toBe(SYSTEM);
        expect(fund!.data![0]).toBe(2);
        expect(u64le(fund!.data as Uint8Array, 4)).toBe(amount);
        expect(fund!.accounts).toEqual([
            { address: OWNER, role: AccountRole.WRITABLE_SIGNER },
            { address: ata, role: AccountRole.WRITABLE },
        ]);

        expect(sync!.programAddress).toBe(TOKEN_PROGRAM_ADDRESS);
        expect(sync!.data).toEqual(new Uint8Array([17]));

        expect(approve!.data![0]).toBe(4);
        expect(u64le(approve!.data as Uint8Array, 1)).toBe(amount);
        expect(approve!.accounts).toEqual([
            { address: ata, role: AccountRole.WRITABLE },
            { address: DELEGATE, role: AccountRole.READONLY },
            { address: OWNER, role: AccountRole.READONLY_SIGNER },
        ]);
    });

    it("defaults payer to owner; honours a distinct payer", async () => {
        const def = await wrapSolAllowanceIxs({
            owner: OWNER,
            delegate: DELEGATE,
            amount: 1n,
        });
        expect(def.instructions[0]!.accounts![0]!.address).toBe(OWNER);

        const withPayer = await wrapSolAllowanceIxs({
            owner: OWNER,
            delegate: DELEGATE,
            amount: 1n,
            payer: DELEGATE,
        });
        expect(withPayer.instructions[0]!.accounts![0]!.address).toBe(DELEGATE);
    });

    it("rejects an out-of-range amount before encoding", async () => {
        await expect(
            wrapSolAllowanceIxs({
                owner: OWNER,
                delegate: DELEGATE,
                amount: 2n ** 64n,
            })
        ).rejects.toThrow(RangeError);
    });
});

describe("unwrapSolIxs", () => {
    it("revokes then closes the wSOL ata to the owner by default", async () => {
        const { ata, instructions } = await unwrapSolIxs({ owner: OWNER });
        expect(ata).toBe(
            await associatedTokenAddress({ owner: OWNER, mint: NATIVE_MINT })
        );
        expect(instructions).toHaveLength(2);

        const [revoke, close] = instructions;
        expect(revoke!.data).toEqual(new Uint8Array([5]));
        expect(close!.data).toEqual(new Uint8Array([9]));
        expect(close!.accounts).toEqual([
            { address: ata, role: AccountRole.WRITABLE },
            { address: OWNER, role: AccountRole.WRITABLE },
            { address: OWNER, role: AccountRole.READONLY_SIGNER },
        ]);
    });

    it("routes reclaimed lamports to an explicit destination", async () => {
        const { instructions } = await unwrapSolIxs({
            owner: OWNER,
            destination: DELEGATE,
        });
        expect(instructions[1]!.accounts![1]!.address).toBe(DELEGATE);
    });
});

describe("buildSyncNativeIx", () => {
    const ACCT = "Vote111111111111111111111111111111111111111" as Address;

    it("encodes tag 17 on the default token program", () => {
        const ix = buildSyncNativeIx({ account: ACCT });
        expect(ix.programAddress).toBe(TOKEN_PROGRAM_ADDRESS);
        expect(ix.data).toEqual(new Uint8Array([17]));
        expect(ix.accounts).toEqual([
            { address: ACCT, role: AccountRole.WRITABLE },
        ]);
    });

    it("honours a tokenProgram override", () => {
        const ix = buildSyncNativeIx({
            account: ACCT,
            tokenProgram: TOKEN_2022_PROGRAM_ADDRESS,
        });
        expect(ix.programAddress).toBe(TOKEN_2022_PROGRAM_ADDRESS);
    });
});

describe("buildCloseAccountIx", () => {
    const ACCT = "Vote111111111111111111111111111111111111111" as Address;
    const DEST = "Stake11111111111111111111111111111111111111" as Address;
    const AUTH = "11111111111111111111111111111111" as Address;

    it("encodes tag 9 with account/destination/owner-signer on the default program", () => {
        const ix = buildCloseAccountIx({
            account: ACCT,
            destination: DEST,
            owner: AUTH,
        });
        expect(ix.programAddress).toBe(TOKEN_PROGRAM_ADDRESS);
        expect(ix.data).toEqual(new Uint8Array([9]));
        expect(ix.accounts).toEqual([
            { address: ACCT, role: AccountRole.WRITABLE },
            { address: DEST, role: AccountRole.WRITABLE },
            { address: AUTH, role: AccountRole.READONLY_SIGNER },
        ]);
    });

    it("honours a tokenProgram override", () => {
        const ix = buildCloseAccountIx({
            account: ACCT,
            destination: DEST,
            owner: AUTH,
            tokenProgram: TOKEN_2022_PROGRAM_ADDRESS,
        });
        expect(ix.programAddress).toBe(TOKEN_2022_PROGRAM_ADDRESS);
    });
});
