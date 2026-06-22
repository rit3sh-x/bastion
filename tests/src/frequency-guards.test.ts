import { generateKeyPairSigner } from "@solana/kit";
import {
    CooldownPeriod,
    ProgramAllowlist,
    RateLimit,
    window,
} from "bastion/policies";
import { days, sol } from "bastion/units";
import { beforeAll, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, expectProgramRejection, run, withSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion frequency guards devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.08));
    });

    it("CooldownPeriod rejects a second call inside the cooldown window", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                CooldownPeriod({ secs: 45 }),
            ],
            { fund: sol(0.02) },
            async ({ operator, delegate }) => {
                const recipient = await generateKeyPairSigner();
                await operator.execute(
                    {
                        inner: systemTransferIx(
                            delegate,
                            recipient.address,
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
                                recipient.address,
                                sol(0.001)
                            ),
                        },
                        { feePayer: ctx.owner }
                    )
                );
            }
        );
    });

    it("RateLimit allows up to max calls per window, then rejects the next", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                RateLimit({ window: window.fixed(days(1)), max: 2 }),
            ],
            { fund: sol(0.02) },
            async ({ operator, delegate }) => {
                const recipient = await generateKeyPairSigner();
                const send = () =>
                    operator.execute(
                        {
                            inner: systemTransferIx(
                                delegate,
                                recipient.address,
                                sol(0.001)
                            ),
                        },
                        { feePayer: ctx.owner }
                    );

                await send();
                await send();
                await expectProgramRejection(send());
            }
        );
    });
});
