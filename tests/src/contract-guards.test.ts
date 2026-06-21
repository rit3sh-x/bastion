import { generateKeyPairSigner } from "@solana/kit";
import { createOperatorClient, pda } from "bastion";
import { ProgramAllowlist, TokenAuthorityGuard } from "bastion/policies";
import { TOKEN_PROGRAM_ADDRESS, buildApproveIx } from "bastion/token";
import { sol, tokens } from "bastion/units";
import { beforeAll, describe, expect, it } from "vitest";

import {
    DEVNET_E2E_ENABLED,
    createDevnetContext,
    type DevnetContext,
} from "./env";
import {
    SYSTEM_PROGRAM_ADDRESS,
    createMintWithOwnerAta,
    sendInstructions,
    signedSystemTransferIx,
    solBalance,
    systemTransferIx,
} from "./tx";

const run = DEVNET_E2E_ENABLED ? describe.sequential : describe.skip;
const DECIMALS = 6;

run("Bastion contract guard devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await createDevnetContext();
        expect(await solBalance(ctx, ctx.owner.address)).toBeGreaterThan(
            sol(0.08)
        );
    });

    it("requires the pinned manifest to be supplied on execute", async () => {
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

        await opened.handle.revoke();
        await opened.handle.sweep(ctx.owner.address);
    });

    it("TokenAuthorityGuard rejects approve-style authority changes", async () => {
        const { ownerAta } = await createMintWithOwnerAta(
            ctx,
            DECIMALS,
            tokens(2, DECIMALS)
        );
        const opened = await ctx.holder.openSession({
            expiry: { secsFromNow: 3_600 },
            policies: [
                ProgramAllowlist({ programs: [TOKEN_PROGRAM_ADDRESS] }),
                TokenAuthorityGuard(),
            ],
        });
        const operator = await createOperatorClient(opened.operator);
        const [delegate] = await pda.delegate(
            ctx.owner.address,
            operator.sessionKey
        );
        const attemptedDelegate = await generateKeyPairSigner();

        await sendInstructions(ctx, [
            signedSystemTransferIx(ctx.owner, delegate, sol(0.005)),
        ]);

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

        await opened.handle.revoke();
        await opened.handle.sweep(ctx.owner.address);
    });
});
