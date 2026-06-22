import { generateKeyPairSigner } from "@solana/kit";
import { ProgramAllowlist } from "bastion/policies";
import { sol } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, run, startSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, solBalance, systemTransferIx } from "./tx";

run("Bastion manifest and revoke devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.05));
    });

    it("executes with a holder-signed manifest, then revoke blocks the operator", async () => {
        const { handle, operator, delegate } = await startSession(ctx, [], {
            fund: sol(0.01),
        });
        const signed = await ctx.holder.signManifest([
            ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
        ]);
        await handle.pinManifest(signed.manifestHash);

        const recipient = await generateKeyPairSigner();
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

        await handle.revoke();

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

        await handle.sweep(ctx.owner.address);
    });
});
