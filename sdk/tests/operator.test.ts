import { describe, expect, it } from "vitest";
import {
    AccountRole,
    address,
    type Address,
    type Instruction,
} from "@solana/kit";
import { estimateComputeUnits, wrapInnerBatch } from "@/execute";
import {
    parseOperatorCredential,
    serializeOperatorCredential,
    type OperatorCredential,
} from "@/operator";
import { generateExtractableSessionKey, sessionKeyFromSecret } from "@/wallet";
import { COMPUTE_BUDGET_ID } from "@/generated";

const SYS = address("11111111111111111111111111111111");
const TOK = address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const WSOL = address("So11111111111111111111111111111111111111112");
const MEMO = address("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

function ix(
    program: Address,
    accs: { address: Address; role: AccountRole }[]
): Instruction {
    return {
        programAddress: program,
        accounts: accs,
        data: new Uint8Array([1, 2, 3]),
    };
}

describe("wrapInnerBatch", () => {
    it("dedups a shared account across legs and merges to the strongest role", () => {
        const leg1 = ix(SYS, [
            { address: WSOL, role: AccountRole.WRITABLE },
            { address: COMPUTE_BUDGET_ID, role: AccountRole.READONLY },
        ]);
        const leg2 = ix(TOK, [
            { address: WSOL, role: AccountRole.READONLY },
            { address: MEMO, role: AccountRole.WRITABLE },
        ]);

        const batch = wrapInnerBatch([leg1, leg2]);

        // shared pool: WSOL, COMPUTE_BUDGET_ID, MEMO (WSOL appears once, merged writable)
        expect(batch.innerMetas.map((m) => m.address)).toEqual([
            WSOL,
            COMPUTE_BUDGET_ID,
            MEMO,
        ]);
        expect(batch.innerMetas[0]!.role).toBe(AccountRole.WRITABLE);

        // leg-1 indexes [WSOL=0, COMPUTE_BUDGET_ID=1]; leg-2 indexes [WSOL=0, MEMO=2]
        expect(batch.legs[0]!.accounts.map((a) => a.index)).toEqual([0, 1]);
        expect(batch.legs[1]!.accounts.map((a) => a.index)).toEqual([0, 2]);

        // WSOL flag is writable (0b10) in both legs after merge
        expect(batch.legs[1]!.accounts[0]!.flags).toBe(0b10);

        expect(batch.programIds).toEqual([SYS, TOK]);
    });

    it("preserves single-leg shape", () => {
        const batch = wrapInnerBatch([
            ix(SYS, [{ address: WSOL, role: AccountRole.WRITABLE }]),
        ]);
        expect(batch.legs).toHaveLength(1);
        expect(batch.programIds).toEqual([SYS]);
        expect(batch.legs[0]!.accounts).toEqual([{ index: 0, flags: 0b10 }]);
    });
});

describe("operator credential", () => {
    const cred: OperatorCredential = {
        sessionSecret: "ZsT2t7v8n2tQ9wptH7vN4Z6m1c5k3J9pQ2rW8xY7bV3",
        sessionPda: WSOL,
        owner: SYS,
        programId: TOK,
        policies: [COMPUTE_BUDGET_ID, MEMO],
        rpcUrl: "http://127.0.0.1:8899",
    };

    it("round-trips through serialize/parse", () => {
        const parsed = parseOperatorCredential(
            serializeOperatorCredential(cred)
        );
        expect(parsed).toEqual(cred);
    });

    it("defaults policies to [] when absent", () => {
        const { policies: _omit, ...without } = cred;
        const parsed = parseOperatorCredential(JSON.stringify(without));
        expect(parsed.policies).toEqual([]);
    });

    it("rejects a credential missing a required field", () => {
        const { sessionSecret: _omit, ...broken } = cred;
        expect(() => parseOperatorCredential(JSON.stringify(broken))).toThrow();
    });

    it("V1: credential carries only the session secret + owner PUBKEY, never an owner secret", () => {
        const json = serializeOperatorCredential(cred);
        const keys = Object.keys(JSON.parse(json));
        // owner is present only as its address; there is no field that could hold an owner private key
        expect(keys).not.toContain("wallet");
        expect(keys).not.toContain("ownerSecret");
        expect(keys).not.toContain("ownerSecretKey");
        expect(cred.owner).toBe(SYS); // a pubkey, branded Address
    });
});

describe("estimateComputeUnits", () => {
    it("scales with leg count and weights heavy policies more", () => {
        const oneLeg = estimateComputeUnits(["SpendCap"], 1);
        const twoLegs = estimateComputeUnits(["SpendCap"], 2);
        expect(twoLegs).toBeGreaterThan(oneLeg);

        const cheap = estimateComputeUnits(["ProgramAllowlist"], 1);
        const heavy = estimateComputeUnits(["NftCollectionAllowlist"], 1);
        expect(heavy).toBeGreaterThan(cheap);
    });

    it("clamps to the 1.4M CU ceiling", () => {
        const many = Array.from({ length: 32 }, () => "NftCollectionAllowlist");
        expect(estimateComputeUnits(many, 8)).toBe(1_400_000);
    });
});

describe("extractable session key", () => {
    it("derives the same address from the retained 32-byte seed", async () => {
        const { signer, secretKey } = await generateExtractableSessionKey();
        expect(secretKey).toHaveLength(32);
        const restored = await sessionKeyFromSecret(secretKey);
        expect(restored.address).toBe(signer.address);
    });
});
