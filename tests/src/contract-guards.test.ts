import { generateKeyPairSigner } from "@solana/kit";
import { ProgramAllowlist, TokenAuthorityGuard } from "bastion/policies";
import { TOKEN_PROGRAM_ADDRESS, buildApproveIx } from "bastion/token";
import { sol, tokens } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, run, startSession } from "./harness";
import {
    SYSTEM_PROGRAM_ADDRESS,
    createMintWithOwnerAta,
    systemTransferIx,
} from "./tx";

const DECIMALS = 6;

run("Bastion contract guard devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.08));
    });

    it("requires the pinned manifest to be supplied on execute", async () => {
        const { handle, operator, delegate } = await startSession(ctx, [], {
            fund: sol(0.01),
        });
        const signed = await ctx.holder.signManifest([
            ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
        ]);
        await handle.pinManifest(signed.manifestHash);

        const recipient = await generateKeyPairSigner();

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

        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    recipient.address,
                    sol(0.001)
                ),
                manifest: signed,
            },
            { feePayer: ctx.owner }
        );

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });

    it("TokenAuthorityGuard rejects approve-style authority changes", async () => {
        const { ownerAta } = await createMintWithOwnerAta(
            ctx,
            DECIMALS,
            tokens(2, DECIMALS)
        );
        const { handle, operator, delegate } = await startSession(
            ctx,
            [
                ProgramAllowlist({ programs: [TOKEN_PROGRAM_ADDRESS] }),
                TokenAuthorityGuard(),
            ],
            { fund: sol(0.005) }
        );
        const attemptedDelegate = await generateKeyPairSigner();

        await expect(
            operator.execute(
                {
                    inner: buildApproveIx({
                        source: ownerAta,
                        delegate: attemptedDelegate.address,
                        owner: delegate,
                        amount: tokens(1, DECIMALS),
                    }),
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });
});
