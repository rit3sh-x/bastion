import { generateKeyPairSigner } from "@solana/kit";
import { createOperatorClient, pda } from "bastion";
import {
    AmountPerCall,
    MintAllowlist,
    ProgramAllowlist,
    SpendCap,
    asset,
    window,
} from "bastion/policies";
import { TOKEN_PROGRAM_ADDRESS, buildTokenTransferIx } from "bastion/token";
import { days, sol, tokens } from "bastion/units";
import { beforeAll, describe, expect, it } from "vitest";

import {
    DEVNET_E2E_ENABLED,
    createDevnetContext,
    type DevnetContext,
} from "./env";
import {
    createMintWithOwnerAta,
    createTokenAta,
    solBalance,
    tokenBalance,
} from "./tx";

const run = DEVNET_E2E_ENABLED ? describe.sequential : describe.skip;
const DECIMALS = 6;

run("Bastion SPL allowance devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await createDevnetContext();
        expect(await solBalance(ctx, ctx.owner.address)).toBeGreaterThan(
            sol(0.08)
        );
    });

    it("mints SPL tokens and spends an approved owner allowance through the operator", async () => {
        const { mint, ownerAta } = await createMintWithOwnerAta(
            ctx,
            DECIMALS,
            tokens(10, DECIMALS)
        );
        const recipient = await generateKeyPairSigner();
        const recipientAta = await createTokenAta(ctx, recipient.address, mint);

        const opened = await ctx.holder.openSession({
            expiry: { secsFromNow: 3_600 },
            policies: [
                ProgramAllowlist({ programs: [TOKEN_PROGRAM_ADDRESS] }),
                MintAllowlist({ mints: [mint] }),
                SpendCap({
                    asset: asset.splToken(mint),
                    window: window.fixed(days(1)),
                    max: tokens(5, DECIMALS),
                }),
                AmountPerCall({
                    asset: asset.splToken(mint),
                    max: tokens(2, DECIMALS),
                }),
            ],
            allowance: {
                mint,
                amount: tokens(5, DECIMALS),
            },
        });
        const operator = await createOperatorClient(opened.operator);
        const [delegate] = await pda.delegate(
            ctx.owner.address,
            operator.sessionKey
        );

        const before = await tokenBalance(ctx, recipientAta);
        await operator.execute(
            {
                inner: buildTokenTransferIx({
                    source: ownerAta,
                    dest: recipientAta,
                    authority: delegate,
                    amount: tokens(1, DECIMALS),
                }),
            },
            { feePayer: ctx.owner }
        );
        const after = await tokenBalance(ctx, recipientAta);
        expect(after - before).toBe(tokens(1, DECIMALS));

        await expect(
            operator.execute(
                {
                    inner: buildTokenTransferIx({
                        source: ownerAta,
                        dest: recipientAta,
                        authority: delegate,
                        amount: tokens(3, DECIMALS),
                    }),
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();

        await opened.handle.revoke();
    });
});
