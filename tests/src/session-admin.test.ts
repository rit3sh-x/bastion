import { generateKeyPairSigner } from "@solana/kit";
import {
    AmountPerCall,
    MaxCallsTotal,
    ProgramAllowlist,
    asset,
} from "bastion/policies";
import { days, sol } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import {
    bootstrap,
    expectProgramRejection,
    run,
    startSession,
    withSession,
} from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion session administration devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.12));
    });

    it("extends the session expiry forward", async () => {
        await withSession(
            ctx,
            [ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] })],
            { expirySecs: 600 },
            async ({ handle }) => {
                const before = (await handle.state()).expiry;
                await handle.extend(before + BigInt(days(1)));
                const after = (await handle.state()).expiry;
                expect(after).toBeGreaterThan(before);
            }
        );
    });

    it("detaches a policy, unblocking a previously capped action", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                MaxCallsTotal({ max: 1n }),
            ],
            { fund: sol(0.02) },
            async ({ handle, operator, delegate }) => {
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
                await expectProgramRejection(send());

                await handle.detach(1n);
                expect(await operator.policies()).toHaveLength(1);

                await send();
            }
        );
    });

    it("updates a policy to raise a per-call cap", async () => {
        await withSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                AmountPerCall({ asset: asset.sol(), max: sol(0.002) }),
            ],
            { fund: sol(0.02) },
            async ({ handle, operator, delegate }) => {
                const recipient = await generateKeyPairSigner();
                const send = () =>
                    operator.execute(
                        {
                            inner: systemTransferIx(
                                delegate,
                                recipient.address,
                                sol(0.005)
                            ),
                        },
                        { feePayer: ctx.owner }
                    );

                await expectProgramRejection(send());

                await handle.update(
                    1n,
                    AmountPerCall({ asset: asset.sol(), max: sol(0.01) })
                );

                await send();
            }
        );
    });

    it("revokes, sweeps, then closes the session and reclaims its rent", async () => {
        const { handle } = await startSession(ctx, [
            ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
        ]);

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
        await handle.close();

        await expect(handle.state()).rejects.toThrow();
    });
});
