import { generateKeyPairSigner } from "@solana/kit";
import { MaxCallsTotal, ProgramAllowlist } from "bastion/policies";
import { sol } from "bastion/units";
import { beforeAll, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, expectProgramRejection, run, withSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion live policy lifecycle devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.08));
    });

    it("enforces a policy attached mid-session, then revokes and sweeps", async () => {
        await withSession(
            ctx,
            [ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] })],
            { fund: sol(0.02) },
            async ({ handle, operator, delegate }) => {
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

                await expectProgramRejection(
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
                );
            }
        );
    });
});
