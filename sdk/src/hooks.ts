import type { Address, Instruction, Signature } from "@solana/kit";
import type { PolicyDataArgs } from "./generated";
import type { BastionSdkError } from "./errors";

export interface BaseHookContext {
    sessionPda: Address;
    owner: Address;
    startedAt: number;
}

export interface BeforeOpenContext extends BaseHookContext {
    op: "open";
    expiry: bigint;
}
export interface BeforeAttachContext extends BaseHookContext {
    op: "attach";
    data: PolicyDataArgs;
}
export interface BeforeUpdateContext extends BaseHookContext {
    op: "update";
    seed: bigint;
    data: PolicyDataArgs;
}
export interface BeforeDetachContext extends BaseHookContext {
    op: "detach";
    seed: bigint;
}
export interface BeforeExtendContext extends BaseHookContext {
    op: "extend";
    newExpiry: bigint;
}
export interface BeforeRevokeContext extends BaseHookContext {
    op: "revoke";
}
export interface BeforeCloseContext extends BaseHookContext {
    op: "close";
}
export interface BeforeSweepContext extends BaseHookContext {
    op: "sweep";
    destination: Address;
}
export interface BeforeExecuteContext extends BaseHookContext {
    op: "execute";
    innerIx: Instruction;
    policies: readonly { address: Address }[];
    outerIxs: readonly Instruction[];
}

export type BeforeContext =
    | BeforeOpenContext
    | BeforeAttachContext
    | BeforeUpdateContext
    | BeforeDetachContext
    | BeforeExtendContext
    | BeforeRevokeContext
    | BeforeCloseContext
    | BeforeSweepContext
    | BeforeExecuteContext;

export interface AfterOpenContext extends BeforeOpenContext {
    result: { signature: Signature; sessionPda: Address; sessionKey: Address };
}
export interface AfterAttachContext extends BeforeAttachContext {
    result: { signature: Signature; policyPda: Address; seed: bigint };
}
export interface AfterUpdateContext extends BeforeUpdateContext {
    result: { signature: Signature };
}
export interface AfterDetachContext extends BeforeDetachContext {
    result: { signature: Signature };
}
export interface AfterExtendContext extends BeforeExtendContext {
    result: { signature: Signature };
}
export interface AfterRevokeContext extends BeforeRevokeContext {
    result: { signature: Signature };
}
export interface AfterCloseContext extends BeforeCloseContext {
    result: { signature: Signature };
}
export interface AfterSweepContext extends BeforeSweepContext {
    result: { signature: Signature };
}
export interface AfterExecuteContext extends BeforeExecuteContext {
    result: { signature: Signature };
}

export type AfterContext =
    | AfterOpenContext
    | AfterAttachContext
    | AfterUpdateContext
    | AfterDetachContext
    | AfterExtendContext
    | AfterRevokeContext
    | AfterCloseContext
    | AfterSweepContext
    | AfterExecuteContext;

export type ErrorContext = BeforeContext & { error: BastionSdkError };

export interface BastionHooks {
    before?: (ctx: BeforeContext) => void | Promise<void>;
    after?: (ctx: AfterContext) => void | Promise<void>;
    error?: (ctx: ErrorContext) => void | Promise<void>;
}

export async function withHooks<TBefore extends BeforeContext, TResult>(
    hooks: BastionHooks | undefined,
    beforeCtx: TBefore,
    run: () => Promise<TResult>,
    toAfterResult: (result: TResult) => AfterContext["result"],
    wrapError: (err: unknown) => BastionSdkError
): Promise<TResult> {
    if (hooks?.before) {
        await hooks.before(beforeCtx);
    }
    let result: TResult;
    try {
        result = await run();
    } catch (err) {
        const wrapped = wrapError(err);
        if (hooks?.error) {
            await hooks.error({ ...beforeCtx, error: wrapped } as ErrorContext);
        }
        throw wrapped;
    }
    if (hooks?.after) {
        await hooks.after({
            ...beforeCtx,
            result: toAfterResult(result),
        } as AfterContext);
    }
    return result;
}
