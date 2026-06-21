import { generateKeyPairSigner } from "@solana/kit";
import { createOperatorClient, pda } from "bastion";
import { ProgramAllowlist } from "bastion/policies";
import { sol } from "bastion/units";
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

run("Bastion manifest and revoke devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await createDevnetContext();
        expect(await solBalance(ctx, ctx.owner.address)).toBeGreaterThan(
            sol(0.05)
        );
    });

    it("executes with a holder-signed manifest, then revoke blocks the operator", async () => {
        const opened = await ctx.holder.openSession({
            expiry: { secsFromNow: 3_600 },
        });
        const signed = await ctx.holder.signManifest([
            ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
        ]);
        await opened.handle.pinManifest(signed.manifestHash);

        const operator = await createOperatorClient(opened.operator);
        const [delegate] = await pda.delegate(
            ctx.owner.address,
            operator.sessionKey
        );
        const recipient = await generateKeyPairSigner();

        await sendInstructions(ctx, [
            signedSystemTransferIx(ctx.owner, delegate, sol(0.01)),
        ]);

        const before = await solBalance(ctx, recipient.address);
        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    recipient.address,
                    sol(0.002)
                ),
                manifest: signed,
            },
            { feePayer: ctx.owner }
        );
        const after = await solBalance(ctx, recipient.address);
        expect(after - before).toBe(sol(0.002));

        await opened.handle.revoke();
        expect((await opened.handle.state()).revoked).toBe(true);

        const beforeSweep = await solBalance(ctx, ctx.owner.address);
        await opened.handle.sweep(ctx.owner.address);
        const afterSweep = await solBalance(ctx, ctx.owner.address);
        expect(afterSweep).toBeGreaterThan(beforeSweep);
        expect(await opened.handle.delegateBalance()).toBe(0n);

        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        recipient.address,
                        sol(0.001)
                    ),
                    manifest: signed,
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();
    });
});
