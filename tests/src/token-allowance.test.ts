import { generateKeyPairSigner } from "@solana/kit";
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
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, expectProgramRejection, run, withSession } from "./harness";
import { createMintWithOwnerAta, createTokenAta, tokenBalance } from "./tx";

const DECIMALS = 6;

run("Bastion SPL allowance devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.08));
    });

    it("mints SPL tokens and spends an approved owner allowance through the operator", async () => {
        const { mint, ownerAta } = await createMintWithOwnerAta(
            ctx,
            DECIMALS,
            tokens(10, DECIMALS)
        );
        const recipient = await generateKeyPairSigner();
        const recipientAta = await createTokenAta(ctx, recipient.address, mint);

        await withSession(
            ctx,
            [
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
            { allowance: { mint, amount: tokens(5, DECIMALS) } },
            async ({ operator, delegate }) => {
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

                await expectProgramRejection(
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
                );
            }
        );
    });
});
