import { describe, expect, it } from "vitest";
import {
    COMPUTE_UNITS_TOO_HIGH,
    MAX_CALLS_EXCEEDED,
    SESSION_EXPIRED,
    SESSION_REVOKED,
    SPEND_CAP_EXCEEDED,
} from "../src/generated";
import {
    BastionSdkError,
    parseProgramError,
    wrapSendError,
} from "../src/errors";

describe("parseProgramError — Anchor format", () => {
    it("parses standard Anchor error line → number code", () => {
        const logs = [
            "Program log: Instruction: Execute",
            "Program log: AnchorError thrown in programs/bastion/src/policies/spend_cap.rs:42. Error Code: SpendCapExceeded. Error Number: 6017. Error Message: ...",
            "Program ... failed: custom program error: 0x1781",
        ];
        const err = parseProgramError(logs);
        expect(err).not.toBeNull();
        expect(err!.code).toBe(SPEND_CAP_EXCEEDED);
        expect(err!.onChainCode).toBe(6017);
    });

    it("returns null when no Bastion error code matches", () => {
        const err = parseProgramError([
            "Program log: regular line",
            "no errors",
        ]);
        expect(err).toBeNull();
    });

    it("parses ComputeUnitsTooHigh (6043)", () => {
        const err = parseProgramError([
            "Program log: AnchorError thrown in ... Error Number: 6043. ...",
        ]);
        expect(err!.code).toBe(COMPUTE_UNITS_TOO_HIGH);
        expect(err!.onChainCode).toBe(6043);
    });

    it("parses MaxCallsExceeded (6033)", () => {
        const err = parseProgramError([
            "Program log: AnchorError thrown in ... Error Number: 6033. ...",
        ]);
        expect(err!.code).toBe(MAX_CALLS_EXCEEDED);
    });
});

describe("parseProgramError — custom-program-error hex format", () => {
    it("parses 'custom program error: 0xNNN'", () => {
        const err = parseProgramError([
            "Program failed: custom program error: 0x1771",
        ]); // 6001
        expect(err).not.toBeNull();
        expect(err!.code).toBe(SESSION_EXPIRED);
        expect(err!.onChainCode).toBe(6001);
    });

    it("returns null for hex codes outside the Bastion set", () => {
        const err = parseProgramError([
            "Program failed: custom program error: 0x9999",
        ]);
        expect(err).toBeNull();
    });
});

describe("wrapSendError", () => {
    it("passes through an existing BastionSdkError unchanged", () => {
        const orig = new BastionSdkError({
            code: "InvalidConfig",
            message: "x",
        });
        expect(wrapSendError(orig)).toBe(orig);
    });

    it("parses logs off an error object with .logs", () => {
        const wrapped = wrapSendError({
            message: "tx failed",
            logs: [
                "Program log: AnchorError thrown in ... Error Number: 6000. ...",
            ],
        });
        expect(wrapped.code).toBe(SESSION_REVOKED);
        expect(wrapped.onChainCode).toBe(6000);
    });

    it("falls back to UnknownProgramError when no parseable code", () => {
        const err = new Error("RPC timeout");
        const wrapped = wrapSendError(err);
        expect(wrapped.code).toBe("UnknownProgramError");
        expect(wrapped.cause).toBe(err);
    });

    it("preserves logs even without a recognized code", () => {
        const wrapped = wrapSendError({
            message: "x",
            logs: ["Program log: random", "another"],
        });
        expect(wrapped.code).toBe("UnknownProgramError");
        expect(wrapped.logs).toHaveLength(2);
    });

    it("handles non-Error throwables (string)", () => {
        const wrapped = wrapSendError("plain string error");
        expect(wrapped.code).toBe("UnknownProgramError");
        expect(wrapped.message).toBe("plain string error");
    });
});

describe("BastionSdkError construction", () => {
    it("requires only code; message defaults to String(code)", () => {
        const e = new BastionSdkError({ code: "InvalidConfig" });
        expect(e.code).toBe("InvalidConfig");
        expect(e.message).toBe("InvalidConfig");
    });
    it("name is 'BastionSdkError' and instanceof Error", () => {
        const e = new BastionSdkError({ code: "AccountNotFound" });
        expect(e.name).toBe("BastionSdkError");
        expect(e).toBeInstanceOf(Error);
    });
});

describe("matchers — codama number constants + SDK reason strings", () => {
    it("static is() matches on a codama constant", () => {
        const err = new BastionSdkError({ code: SPEND_CAP_EXCEEDED });
        expect(BastionSdkError.is(err, SPEND_CAP_EXCEEDED)).toBe(true);
        expect(BastionSdkError.is(err, SESSION_REVOKED)).toBe(false);
    });

    it("static is() rejects non-BastionSdkError", () => {
        expect(BastionSdkError.is(new Error("plain"), SPEND_CAP_EXCEEDED)).toBe(
            false
        );
        expect(BastionSdkError.is("string", SPEND_CAP_EXCEEDED)).toBe(false);
        expect(BastionSdkError.is(undefined, SPEND_CAP_EXCEEDED)).toBe(false);
    });

    it("isAny() matches any of N codes", () => {
        const err = new BastionSdkError({ code: SESSION_REVOKED });
        expect(
            BastionSdkError.isAny(err, [SESSION_EXPIRED, SESSION_REVOKED])
        ).toBe(true);
        expect(
            BastionSdkError.isAny(err, [SPEND_CAP_EXCEEDED, MAX_CALLS_EXCEEDED])
        ).toBe(false);
        expect(BastionSdkError.isAny(err, [])).toBe(false);
    });

    it("instance is() matches on SDK reason string", () => {
        const err = new BastionSdkError({ code: "InvalidConfig" });
        expect(err.is("InvalidConfig")).toBe(true);
        expect(err.is("AccountNotFound")).toBe(false);
    });

    it("realistic typed catch pattern", async () => {
        const op = async (): Promise<string> => {
            throw new BastionSdkError({
                code: SPEND_CAP_EXCEEDED,
                onChainCode: 6017,
            });
        };
        let outcome = "unattempted";
        try {
            await op();
        } catch (err) {
            if (BastionSdkError.is(err, SPEND_CAP_EXCEEDED)) {
                outcome = `backOff:${err.onChainCode}`;
            } else if (
                BastionSdkError.isAny(err, [SESSION_REVOKED, SESSION_EXPIRED])
            ) {
                outcome = "dead";
            } else {
                outcome = "unknown";
            }
        }
        expect(outcome).toBe("backOff:6017");
    });
});
