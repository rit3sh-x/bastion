import { address } from "@solana/kit";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
    fetchAllMaybePolicy: vi.fn(),
    fetchSession: vi.fn(),
}));

vi.mock("@/generated", async () => {
    const actual = await vi.importActual("@/generated");
    return {
        ...actual,
        fetchAllMaybePolicy: mocks.fetchAllMaybePolicy,
        fetchSession: mocks.fetchSession,
    };
});

import { createOperatorClient } from "@/operator";

const OLD_POLICY = address("11111111111111111111111111111111");
const FIRST_POLICY = address("So11111111111111111111111111111111111111112");
const SECOND_POLICY = address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SESSION_PDA = address("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const OWNER = address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA6iy1");
const PROGRAM_ID = address("ComputeBudget111111111111111111111111111111");

describe("createOperatorClient policy resolution", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mocks.fetchSession.mockResolvedValue({ data: { nextSeed: 1n } });
    });

    it("re-reads the current policy accounts from chain instead of using the credential snapshot", async () => {
        mocks.fetchAllMaybePolicy.mockResolvedValueOnce([
            { address: FIRST_POLICY, exists: true },
        ]);

        const client = await createOperatorClient({
            sessionSecret: "11111111111111111111111111111111",
            sessionPda: SESSION_PDA,
            owner: OWNER,
            programId: PROGRAM_ID,
            policies: [OLD_POLICY],
            rpcUrl: "http://127.0.0.1:8899",
        });

        await expect(client.policies()).resolves.toEqual([FIRST_POLICY]);
        expect(mocks.fetchSession).toHaveBeenCalledTimes(1);
        expect(mocks.fetchAllMaybePolicy).toHaveBeenCalledTimes(1);

        mocks.fetchAllMaybePolicy.mockResolvedValueOnce([
            { address: SECOND_POLICY, exists: true },
        ]);

        await expect(client.policies()).resolves.toEqual([SECOND_POLICY]);
        expect(mocks.fetchSession).toHaveBeenCalledTimes(2);
        expect(mocks.fetchAllMaybePolicy).toHaveBeenCalledTimes(2);
    });
});
