import { generateKeyPairSigner, type Address } from "@solana/kit";
import type { OperatorClient, SessionHandle } from "bastion";
import {
    AmountPerCall,
    MaxCallsTotal,
    ProgramAllowlist,
    SpendCap,
    asset,
    window,
} from "bastion/policies";
import { days, sol } from "bastion/units";
import { beforeAll, expect, it } from "vitest";

import type { DevnetContext } from "./env";
import { bootstrap, run, startSession } from "./harness";
import {
    SYSTEM_PROGRAM_ADDRESS,
    fundDelegate,
    solBalance,
    systemTransferIx,
} from "./tx";

run("Bastion SOL session devnet e2e", () => {
    let ctx: DevnetContext;
    let handle: SessionHandle;
    let operator: OperatorClient;
    let delegate: Address;

    beforeAll(async () => {
        ctx = await bootstrap(sol(0.08));
    });

    it("opens a session and stores policy accounts on devnet", async () => {
        const session = await startSession(ctx, [
            ProgramAllowlist({ programs: [SYSTEM_PROGRAM_ADDRESS] }),
            SpendCap({
                asset: asset.sol(),
                window: window.fixed(days(1)),
                max: sol(0.02),
            }),
            AmountPerCall({ asset: asset.sol(), max: sol(0.01) }),
            MaxCallsTotal({ max: 4n }),
        ]);
        handle = session.handle;
        operator = session.operator;
        delegate = session.delegate;

        const state = await handle.state();
        expect(state.owner).toBe(ctx.owner.address);
        expect(state.revoked).toBe(false);
        expect(state.policyCount).toBe(4);
        expect(state.nextSeed).toBe(4n);
        expect(await handle.policies()).toHaveLength(4);
    });

    it("executes an allowed delegate transfer and rejects an oversized one", async () => {
        const recipient = await generateKeyPairSigner();
        await fundDelegate(ctx, delegate, sol(0.03));

        const before = await solBalance(ctx, recipient.address);
        await operator.execute(
            {
                inner: systemTransferIx(
                    delegate,
                    recipient.address,
                    sol(0.005)
                ),
            },
            { feePayer: ctx.owner }
        );
        const after = await solBalance(ctx, recipient.address);
        expect(after - before).toBe(sol(0.005));
        expect((await operator.state()).actionNonce).toBe(1n);

        await expect(
            operator.execute(
                {
                    inner: systemTransferIx(
                        delegate,
                        recipient.address,
                        sol(0.02)
                    ),
                },
                { feePayer: ctx.owner }
            )
        ).rejects.toThrow();
    });

    it("revokes the session, blocks later operator execution, then sweeps", async () => {
        const recipient = await generateKeyPairSigner();

        await handle.revoke();

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

        await handle.sweep(ctx.owner.address);
    });
});
