import { describe, expect, it, vi } from "vitest";
import type { Address, Signature } from "@solana/kit";
import { withHooks, type BastionHooks } from "../src/hooks";
import { BastionSdkError } from "../src/errors";
import { wrapSendError } from "../src/errors";

const SESSION = "11111111111111111111111111111111" as Address;
const OWNER = "So11111111111111111111111111111111111111112" as Address;

const baseCtx = {
    op: "revoke" as const,
    sessionPda: SESSION,
    owner: OWNER,
    startedAt: 1_700_000_000_000,
};

describe("withHooks — happy path", () => {
    it("calls before → fn → after in order", async () => {
        const calls: string[] = [];
        const hooks: BastionHooks = {
            before: () => {
                calls.push("before");
            },
            after: () => {
                calls.push("after");
            },
        };
        await withHooks(
            hooks,
            baseCtx,
            async () => {
                calls.push("fn");
                return "sig-123" as Signature;
            },
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(calls).toEqual(["before", "fn", "after"]);
    });

    it("after-hook ctx includes result", async () => {
        let captured: { signature: Signature } | undefined;
        const hooks: BastionHooks = {
            after: (ctx) => {
                if (ctx.op === "revoke") captured = ctx.result;
            },
        };
        await withHooks(
            hooks,
            baseCtx,
            async () => "sig-abc" as Signature,
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(captured?.signature).toBe("sig-abc");
    });

    it("works with no hooks at all (undefined)", async () => {
        const result = await withHooks(
            undefined,
            baseCtx,
            async () => "sig" as Signature,
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(result).toBe("sig");
    });

    it("works with empty hooks object", async () => {
        const result = await withHooks(
            {},
            baseCtx,
            async () => "ok" as Signature,
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(result).toBe("ok");
    });
});

describe("withHooks — error path", () => {
    it("error-hook fires on fn throw; error still propagates", async () => {
        const errorSpy = vi.fn();
        const hooks: BastionHooks = { error: errorSpy };
        await expect(
            withHooks(
                hooks,
                baseCtx,
                async () => {
                    throw new Error("rpc timeout");
                },
                (sig: Signature) => ({ signature: sig }),
                wrapSendError
            )
        ).rejects.toBeInstanceOf(BastionSdkError);
        expect(errorSpy).toHaveBeenCalledOnce();
        const errCtx = errorSpy.mock.calls[0]![0];
        expect(errCtx.op).toBe("revoke");
        expect(errCtx.error).toBeInstanceOf(BastionSdkError);
        expect(errCtx.error.code).toBe("UnknownProgramError");
    });

    it("before-hook throw aborts; error-hook fires; after-hook does NOT", async () => {
        const calls: string[] = [];
        const hooks: BastionHooks = {
            before: () => {
                calls.push("before");
                throw new Error("policy denied by gate");
            },
            after: () => {
                calls.push("after");
            },
            error: () => {
                calls.push("error");
            },
        };
        const fnSpy = vi.fn(async () => "sig" as Signature);
        await expect(
            withHooks(
                hooks,
                baseCtx,
                fnSpy,
                (sig) => ({ signature: sig }),
                wrapSendError
            )
        ).rejects.toThrow();
        expect(fnSpy).not.toHaveBeenCalled();
        expect(calls).toEqual(["before"]);
    });

    it("error-hook can re-throw to replace the propagated error", async () => {
        const hooks: BastionHooks = {
            error: () => {
                throw new BastionSdkError({
                    code: "WalletRejected",
                    message: "user cancelled",
                });
            },
        };
        const err = await withHooks(
            hooks,
            baseCtx,
            async () => {
                throw new Error("original failure");
            },
            (sig: Signature) => ({ signature: sig }),
            wrapSendError
        ).catch((e: unknown) => e);
        expect(err).toBeInstanceOf(BastionSdkError);
        expect((err as BastionSdkError).code).toBe("WalletRejected");
    });

    it("after-hook throw propagates to caller (no error-hook re-fire)", async () => {
        const errorSpy = vi.fn();
        const hooks: BastionHooks = {
            after: () => {
                throw new Error("after barfed");
            },
            error: errorSpy,
        };
        await expect(
            withHooks(
                hooks,
                baseCtx,
                async () => "sig" as Signature,
                (sig) => ({ signature: sig }),
                wrapSendError
            )
        ).rejects.toThrow("after barfed");
        expect(errorSpy).not.toHaveBeenCalled();
    });
});

describe("withHooks — async semantics", () => {
    it("awaits async before-hook", async () => {
        const calls: string[] = [];
        const hooks: BastionHooks = {
            before: async () => {
                await new Promise((r) => setTimeout(r, 5));
                calls.push("before-done");
            },
        };
        await withHooks(
            hooks,
            baseCtx,
            async () => {
                calls.push("fn");
                return "sig" as Signature;
            },
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(calls).toEqual(["before-done", "fn"]);
    });

    it("awaits async after-hook", async () => {
        const calls: string[] = [];
        const hooks: BastionHooks = {
            after: async () => {
                await new Promise((r) => setTimeout(r, 5));
                calls.push("after-done");
            },
        };
        await withHooks(
            hooks,
            baseCtx,
            async () => {
                calls.push("fn");
                return "sig" as Signature;
            },
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(calls).toEqual(["fn", "after-done"]);
    });
});

describe("withHooks — ctx discrimination", () => {
    it("op discriminator narrows ctx for after-hook", async () => {
        let signatureSeen = "";
        const hooks: BastionHooks = {
            after: (ctx) => {
                switch (ctx.op) {
                    case "revoke":
                        signatureSeen = ctx.result.signature;
                        break;
                    case "attach":
                        signatureSeen = `attach:${ctx.result.policyPda}`;
                        break;
                    default:
                        signatureSeen = "other";
                }
            },
        };
        await withHooks(
            hooks,
            baseCtx,
            async () => "rev-sig" as Signature,
            (sig) => ({ signature: sig }),
            wrapSendError
        );
        expect(signatureSeen).toBe("rev-sig");
    });
});
