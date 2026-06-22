import { generateKeyPairSigner } from "@solana/kit";
import {
    MaxComputeUnits,
    MaxPriorityFee,
    ProgramAllowlist,
    RequireMemo,
} from "bastion/policies";
import { sol } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, run, startSession } from "./harness";
import {
    MEMO_PROGRAM_ADDRESS,
    SYSTEM_PROGRAM_ADDRESS,
    systemTransferIx,
} from "./tx";

const MEMO_DATA = new TextEncoder().encode("bastion-e2e");

run("Bastion outer-instruction guards devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.1));
    });

    it("RequireMemo rejects a call with no memo and accepts one carrying a memo", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                RequireMemo({ memoProgram: MEMO_PROGRAM_ADDRESS }),
            ],
            { fund: sol(0.02) }
        );
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
                memo: { program: MEMO_PROGRAM_ADDRESS, data: MEMO_DATA },
            },
            { feePayer: ctx.owner }
        );

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });

    it("MaxComputeUnits accepts a CU limit under the cap and rejects one over it", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MaxComputeUnits({ max: 300_000 }),
            ],
            { fund: sol(0.02) }
        );
        const recipient = await generateKeyPairSigner();

        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    recipient.address,
                    sol(0.001)
                ),
                computeUnitLimit: 250_000,
            },
            { feePayer: ctx.owner }
        );

        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        recipient.address,
                        sol(0.001)
                    ),
                    computeUnitLimit: 400_000,
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });

    it("MaxPriorityFee accepts a price under the cap and rejects one over it", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MaxPriorityFee({ maxMicroLamports: 1_000n }),
            ],
            { fund: sol(0.02) }
        );
        const recipient = await generateKeyPairSigner();

        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    recipient.address,
                    sol(0.001)
                ),
                computeUnitPrice: 500n,
            },
            { feePayer: ctx.owner }
        );

        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        recipient.address,
                        sol(0.001)
                    ),
                    computeUnitPrice: 5_000n,
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });
});
