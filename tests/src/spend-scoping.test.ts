import { generateKeyPairSigner } from "@solana/kit";
import {
    PerCounterpartyCap,
    PerProgramSpendCap,
    ProgramAllowlist,
    window,
} from "bastion/policies";
import { days, sol } from "bastion/units";
import { beforeAll, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, expectProgramRejection, run, withSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion spend scoping devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.1));
    });

    it("PerCounterpartyCap limits lifetime spend to one receiver but spares others", async () => {
        const capped = await generateKeyPairSigner();
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                PerCounterpartyCap({
                    receiver: capped.address,
                    max: sol(0.01),
                }),
            ],
            { fund: sol(0.03) },
            async ({ operator, delegate }) => {
                await operator.execute(
                    {
                        inner: systemTransferIx(
                            delegate,
                            capped.address,
                            sol(0.006)
                        ),
                    },
                    { feePayer: ctx.owner }
                );

                const other = await generateKeyPairSigner();
                await operator.execute(
                    {
                        inner: systemTransferIx(
                            delegate,
                            other.address,
                            sol(0.006)
                        ),
                    },
                    { feePayer: ctx.owner }
                );

                await expectProgramRejection(
                    operator.execute(
                        {
                            inner: systemTransferIx(
                                delegate,
                                capped.address,
                                sol(0.006)
                            ),
                        },
                        { feePayer: ctx.owner }
                    )
                );
            }
        );
    });

    it("PerProgramSpendCap limits spend routed through a program within the window", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                PerProgramSpendCap({
                    program: SYSTEM_PROGRAM_ADDRESS,
                    window: window.fixed(days(1)),
                    max: sol(0.01),
                }),
            ],
            { fund: sol(0.03) },
            async ({ operator, delegate }) => {
                const first = await generateKeyPairSigner();
                await operator.execute(
                    {
                        inner: systemTransferIx(
                            delegate,
                            first.address,
                            sol(0.006)
                        ),
                    },
                    { feePayer: ctx.owner }
                );

                const second = await generateKeyPairSigner();
                await expectProgramRejection(
                    operator.execute(
                        {
                            inner: systemTransferIx(
                                delegate,
                                second.address,
                                sol(0.006)
                            ),
                        },
                        { feePayer: ctx.owner }
                    )
                );
            }
        );
    });
});
