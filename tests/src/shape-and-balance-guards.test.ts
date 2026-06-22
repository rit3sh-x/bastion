import { generateKeyPairSigner } from "@solana/kit";
import {
    MaxIxSize,
    MinDelegateBalance,
    ProgramAllowlist,
} from "bastion/policies";
import { sol } from "bastion/units";
import { beforeAll, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, expectProgramRejection, run, withSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion shape and balance guards devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.1));
    });

    it("MaxIxSize rejects an instruction carrying too many accounts", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MaxIxSize({ maxAccounts: 1, maxDataLen: 64 }),
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

    it("MaxIxSize accepts an instruction within the account and data bounds", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MaxIxSize({ maxAccounts: 4, maxDataLen: 64 }),
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
            }
        );
    });

    it("MinDelegateBalance rejects a transfer that breaches the floor", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MinDelegateBalance({ floor: sol(1) }),
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

    it("MinDelegateBalance accepts a transfer that keeps the delegate above the floor", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MinDelegateBalance({ floor: sol(0.001) }),
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
            }
        );
    });
});
