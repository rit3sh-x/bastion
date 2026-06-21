import { generateKeyPairSigner, type Address } from "@solana/kit";
import {
    createOperatorClient,
    pda,
    type OperatorClient,
    type SessionHandle,
} from "bastion";
import {
    AmountPerCall,
    MaxCallsTotal,
    ProgramAllowlist,
    SpendCap,
    asset,
    window,
} from "bastion/policies";
import { days, sol } from "bastion/units";
import { beforeAll, describe, expect, it } from "vitest";

import {
    DEVNET_E2E_ENABLED,
    createDevnetContext,
    type DevnetContext,
} from "./env";
import {
    SYSTEM_PROGRAM_ADDRESS,
    sendInstructions,
    signedSystemTransferIx,
    solBalance,
    systemTransferIx,
} from "./tx";

const run = DEVNET_E2E_ENABLED ? describe.sequential : describe.skip;

run("Bastion SOL session devnet e2e", () => {
    let ctx: DevnetContext;
    let handle: SessionHandle;
    let operator: OperatorClient;
    let delegate: Address;

    beforeAll(async () => {
        ctx = await createDevnetContext();
        expect(await solBalance(ctx, ctx.owner.address)).toBeGreaterThan(
            sol(0.08)
        );
    });

    it("opens a session and stores policy accounts on devnet", async () => {
        const opened = await ctx.holder.openSession({
            expiry: { secsFromNow: 3_600 },
            policies: [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                SpendCap({
                    asset: asset.sol(),
                    window: window.fixed(days(1)),
                    max: sol(0.02),
                }),
                AmountPerCall({ asset: asset.sol(), max: sol(0.01) }),
                MaxCallsTotal({ max: 4n }),
            ],
        });
        handle = opened.handle;
        operator = await createOperatorClient(opened.operator);
        [delegate] = await pda.delegate(ctx.owner.address, operator.sessionKey);

        const state = await handle.state();
        expect(state.owner).toBe(ctx.owner.address);
        expect(state.revoked).toBe(false);
        expect(state.policyCount).toBe(4);
        expect(state.nextSeed).toBe(4n);
        expect(await handle.policies()).toHaveLength(4);
    });

    it("executes an allowed delegate transfer and rejects an oversized one", async () => {
        const recipient = await generateKeyPairSigner();
        await sendInstructions(ctx, [
            signedSystemTransferIx(ctx.owner, delegate, sol(0.03)),
        ]);

        const before = await solBalance(ctx, recipient.address);
        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    recipient.address,
                    sol(0.005)
                ),
            },
            { feePayer: ctx.owner }
        );
        const after = await solBalance(ctx, recipient.address);
        expect(after - before).toBe(sol(0.005));
        expect((await operator.state()).actionNonce).toBe(1n);

        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        recipient.address,
                        sol(0.02)
                    ),
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();
    });

    it("sweeps remaining delegate SOL, revokes, and blocks later operator execution", async () => {
        const recipient = await generateKeyPairSigner();

        await handle.revoke();
        expect((await handle.state()).revoked).toBe(true);

        const beforeSweep = await solBalance(ctx, ctx.owner.address);
        await handle.sweep(ctx.owner.address);
        const afterSweep = await solBalance(ctx, ctx.owner.address);
        expect(afterSweep).toBeGreaterThan(beforeSweep);
        expect(await handle.delegateBalance()).toBe(0n);
        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        recipient.address,
                        sol(0.001)
                    ),
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();
    });
});
