import { generateKeyPairSigner } from "@solana/kit";
import { ProgramAllowlist, ProgramBlocklist } from "bastion/policies";
import { TOKEN_PROGRAM_ADDRESS } from "bastion/token";
import { sol } from "bastion/units";
import { beforeAll, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, expectProgramRejection, run, withSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion program routing devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.1));
    });

    it("ProgramAllowlist rejects a call to a program outside the allowlist", async () => {
        await withSession(
            ctx,
            [ProgramAllowlist({ programs: [TOKEN_PROGRAM_ADDRESS] })],
            { fund: sol(0.02) },
            async ({ operator, delegate }) => {
                const recipient = await generateKeyPairSigner();
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

    it("ProgramAllowlist accepts a call to an allowlisted program", async () => {
        await withSession(
            ctx,
            [ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] })],
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
            }
        );
    });

    it("ProgramBlocklist rejects a call to a blocklisted program", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                ProgramBlocklist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
            ],
            { fund: sol(0.02) },
            async ({ operator, delegate }) => {
                const recipient = await generateKeyPairSigner();
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
});
