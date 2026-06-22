import type { Address } from "@solana/kit";
import {
    createOperatorClient,
    pda,
    type OperatorClient,
    type SessionHandle,
} from "bastion";
import type { PolicyDataArgs } from "bastion/policies";
import { describe, expect } from "vitest";

import {
    DEVNET_E2E_ENABLED,
    createDevnetContext,
    type DevnetContext,
} from "./env";
import { fundDelegate, solBalance } from "./tx";

type SuiteRunner = (name: string, fn: () => void) => void;

export const run: SuiteRunner = DEVNET_E2E_ENABLED
    ? (name, fn) => describe(name, { concurrent: false }, fn)
    : describe.skip;

export async function bootstrap(minSol: bigint): Promise<DevnetContext> {
    const ctx = await createDevnetContext();
    expect(await solBalance(ctx, ctx.owner.address)).toBeGreaterThan(minSol);
    return ctx;
}

export interface OpenedSession {
    handle: SessionHandle;
    operator: OperatorClient;
    delegate: Address;
}

export async function startSession(
    ctx: DevnetContext,
    policies: readonly PolicyDataArgs[],
    opts: {
        fund?: bigint;
        expirySecs?: number;
        allowance?: { mint: Address; amount: bigint };
    } = {}
): Promise<OpenedSession> {
    const opened = await ctx.holder.openSession({
        expiry: { secsFromNow: opts.expirySecs ?? 3_600 },
        policies,
        ...(opts.allowance ? { allowance: opts.allowance } : {}),
    });
    const operator = await createOperatorClient(opened.operator);
    const [delegate] = await pda.delegate(
        ctx.owner.address,
        operator.sessionKey
    );
    if (opts.fund) await fundDelegate(ctx, delegate, opts.fund);
    return { handle: opened.handle, operator, delegate };
}
