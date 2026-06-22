import { generateKeyPairSigner } from "@solana/kit";
import { ProgramAllowlist, ProgramBlocklist } from "bastion/policies";
import { TOKEN_PROGRAM_ADDRESS } from "bastion/token";
import { sol } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, run, startSession } from "./harness";
import { SYSTEM_PROGRAM_ADDRESS, systemTransferIx } from "./tx";

run("Bastion program routing devnet e2e", () => {
    let ctx: DevnetContext;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.1));
    });

    it("ProgramAllowlist rejects a call to a program outside the allowlist", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [ProgramAllowlist({ programs: [TOKEN_PROGRAM_ADDRESS] })],
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

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });

    it("ProgramAllowlist accepts a call to an allowlisted program", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] })],
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
            },
            { feePayer: ctx.owner }
        );

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });

    it("ProgramBlocklist rejects a call to a blocklisted program", async () => {
        const { handle, operator, delegate } = await startSession(
            ctx,
            [
                ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
                ProgramBlocklist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
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

        await handle.revoke();
        await handle.sweep(ctx.owner.address);
    });
});
