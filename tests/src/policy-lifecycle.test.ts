import { generateKeyPairSigner } from "@solana/kit";
import {
    createOperatorClient,
    pda,
    type OperatorClient,
    type SessionHandle,
} from "bastion";
import { MaxCallsTotal, ProgramAllowlist } from "bastion/policies";
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

const run = DEVNET_E2E_ENABLED
    ? (name: string, fn: () => void) =>
          describe(name, { concurrent: false }, fn)
    : describe.skip;

run("Bastion live policy lifecycle devnet e2e", () => {
    let ctx: DevnetContext;
    let handle: SessionHandle;
    let operator: OperatorClient;
    let delegate: Awaited<ReturnType<typeof pda.delegate>>[0];

    beforeAll(async () => {
        ctx = await createDevnetContext();
        expect(await solBalance(ctx, ctx.owner.address)).toBeGreaterThan(
            sol(0.08)
        );
    });

    it("operator resolves policies from chain after the credential is issued", async () => {
        const opened = await ctx.holder.openSession({
            expiry: { secsFromNow: 3_600 },
            policies: [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
            ],
        });
        handle = opened.handle;
        operator = await createOperatorClient(opened.operator);
        [delegate] = await pda.delegate(ctx.owner.address, operator.sessionKey);

        await sendInstructions(ctx, [
            signedSystemTransferIx(ctx.owner, delegate, sol(0.02)),
        ]);

        const firstRecipient = await generateKeyPairSigner();
        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    firstRecipient.address,
                    sol(0.001)
                ),
            },
            { feePayer: ctx.owner }
        );

        await handle.attach(MaxCallsTotal({ max: 1n }));
        expect(await operator.policies()).toHaveLength(2);

        const secondRecipient = await generateKeyPairSigner();
        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    secondRecipient.address,
                    sol(0.001)
                ),
            },
            { feePayer: ctx.owner }
        );

        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        secondRecipient.address,
                        sol(0.001)
                    ),
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();
    });

    it("revokes before sweeping because sweep_delegate requires a revoked session", async () => {
        await handle.revoke();
        await handle.sweep(ctx.owner.address);
        expect(await handle.delegateBalance()).toBe(0n);
    });
});
