import { generateKeyPairSigner } from "@solana/kit";
import { ProgramAllowlist, SpendCap, asset, window } from "bastion/policies";
import { days, sol } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, run, startSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, solBalance, systemTransferIx } from "./tx";

run("Bastion batch execution devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.1));
    });

    it("settles a multi-leg batch atomically and reverts the whole batch past a cap", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                SpendCap({
                    asset: asset.sol(),
                    window: window.fixed(days(1)),
                    max: sol(0.01),
                }),
            ],
            { fund: sol(0.04) }
        );

        const a = await generateKeyPairSigner();
        const b = await generateKeyPairSigner();
        await operator.executeBatch(
            {
                inners: [
                    systemTransferIx(delegate, a.address, sol(0.004)),
                    systemTransferIx(delegate, b.address, sol(0.004)),
                ],
            },
            { feePayer: ctx.owner }
        );
        expect(await solBalance(ctx, a.address)).toBe(sol(0.004));
        expect(await solBalance(ctx, b.address)).toBe(sol(0.004));

        const c = await generateKeyPairSigner();
        const d = await generateKeyPairSigner();
        await expect(
            operator.executeBatch(
                {
                    inners: [
                        systemTransferIx(delegate, c.address, sol(0.004)),
                        systemTransferIx(delegate, d.address, sol(0.004)),
                    ],
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();

        expect(await solBalance(ctx, c.address)).toBe(0n);
        expect(await solBalance(ctx, d.address)).toBe(0n);

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });
});
