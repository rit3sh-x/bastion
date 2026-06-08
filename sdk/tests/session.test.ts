import { describe, expect, it } from "vitest";
import type { Address } from "@solana/kit";
import { pda } from "@/session";
import { MPL_TOKEN_METADATA_ID } from "@/generated";

const OWNER = "So11111111111111111111111111111111111111112" as Address;
const SESSION_KEY = "11111111111111111111111111111111" as Address;

describe("pda.session", () => {
    it("derives deterministically", async () => {
        const [a] = await pda.session(OWNER, SESSION_KEY);
        const [b] = await pda.session(OWNER, SESSION_KEY);
        expect(a).toBe(b);
    });
});

describe("pda.policy", () => {
    it("derives via codama findPolicyPda", async () => {
        const session = "11111111111111111111111111111111" as Address;
        const [addr] = await pda.policy(session, 0n);
        expect(addr).toBeDefined();
    });
});

describe("pda.delegate", () => {
    it("derives deterministically", async () => {
        const [a] = await pda.delegate(OWNER, SESSION_KEY);
        const [b] = await pda.delegate(OWNER, SESSION_KEY);
        expect(a).toBe(b);
    });
});

describe("pda.metadata", () => {
    it("derives Metaplex metadata PDA", async () => {
        const [addr] = await pda.metadata(OWNER);
        expect(addr).toBeDefined();
    });
    it("MPL_TOKEN_METADATA_ADDRESS is the canonical program id", () => {
        expect(MPL_TOKEN_METADATA_ID).toBe(
            "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
        );
    });
});
