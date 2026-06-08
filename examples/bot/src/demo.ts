import { address } from "@solana/kit";
import {
    BastionSdkError,
    createBastion,
    pda,
    type SessionHandle,
} from "bastion";
import { associatedTokenAddress, buildTokenTransferIx } from "bastion/token";
import { tokens } from "bastion/units";

import { loadEnv } from "./env";
import { DemoMachine } from "./machine";
import {
    activeCaps,
    buildPolicies,
    LIMITS,
    resolveSpendMode,
} from "./policies";
import { banner, err, log } from "./ui";
import { loadOwnerSigner, loadSessionSigner } from "./wallet";

const fmt = (e: unknown) => (e instanceof Error ? e.message : String(e));
const SYSTEM = "11111111111111111111111111111111";

async function main(): Promise<void> {
    const env = loadEnv();
    const mode = resolveSpendMode(env);
    banner([
        "Bastion allowance demo (no Groq)",
        `rpc:   ${env.rpcUrl}`,
        `asset: ${mode.symbol}${mode.mint ? " (allowance mode)" : ""}`,
    ]);

    if (!mode.mint) {
        err("This demo runs in ALLOWANCE mode — set MINT (and SWAP_DEST).");
        err(
            "See TESTING.md §2 for the spl-token setup (mint + ATAs + balance)."
        );
        process.exit(1);
    }
    if (!env.swapDest || env.swapDest === SYSTEM) {
        err(
            "Set SWAP_DEST to a recipient wallet whose ATA exists (TESTING.md §2)."
        );
        process.exit(1);
    }
    const mint = mode.mint;
    const recipient = address(env.swapDest);

    const wallet = await loadOwnerSigner(env.ownerSecretB58);
    const sessionKey = await loadSessionSigner(env.sessionSecretB58);
    log(`owner:   ${wallet.address}`);

    const bastion = createBastion({
        url: env.rpcUrl,
        ...(env.wsUrl ? { wsUrl: env.wsUrl } : {}),
        wallet,
        logger: { level: "warn" },
    });

    const expiry = { secsFromNow: LIMITS.sessionDurationSecs };
    const openArgs = sessionKey ? { expiry, sessionKey } : { expiry };

    let session: SessionHandle;
    if (sessionKey) {
        const [sessionPda] = await pda.session(
            wallet.address,
            sessionKey.address
        );
        session = bastion.session.hydrate({ pubkey: sessionPda, sessionKey });
        try {
            const st = await session.state();
            if (st.revoked) {
                err(
                    "This session is REVOKED — set a fresh SESSION_SECRET (or unset it) to re-run."
                );
                process.exit(1);
            }
            log(`rehydrated existing session ${sessionPda}`);
        } catch {
            log("opening new session + attaching policies…");
            session = await bastion.session.open(openArgs);
            await session.attachMany(buildPolicies(mode));
        }
    } else {
        log("opening new session (SDK-generated key) + attaching policies…");
        session = await bastion.session.open(openArgs);
        await session.attachMany(buildPolicies(mode));
    }

    const sessionPda = session.pubkey;
    const [delegate] = await pda.delegate(
        wallet.address,
        session.sessionKey.address
    );
    log(`session: ${sessionPda}  (key ${session.sessionKey.address})`);

    const machine = new DemoMachine();
    machine.transition("SESSION_READY", {
        session: sessionPda,
        policies: (await session.policies()).length,
        asset: mode.symbol,
    });

    const cap = env.allowanceTokens;
    const { source } = await session.approveAllowance({
        mint,
        amount: tokens(cap, mode.decimals),
    });
    const dest = await associatedTokenAddress({ owner: recipient, mint });
    machine.transition("ALLOWANCE_GRANTED", {
        delegate,
        capTokens: cap,
        source,
        dest,
    });
    log(
        `approved: delegate ${delegate} may spend ≤ ${cap} ${mode.symbol} from YOUR ATA`
    );
    log(`source (owner ATA):     ${source}`);
    log(`dest   (recipient ATA): ${dest}\n`);

    const transfer = (amount: number) =>
        buildTokenTransferIx({
            source,
            dest,
            authority: delegate,
            amount: tokens(amount, mode.decimals),
        });

    const caps = activeCaps(mode);
    const perCall = caps.perTrade;

    const failAmt = perCall * 5;
    log(
        `[1] over-cap trade ${failAmt} ${mode.symbol} (per-call cap ${perCall}, within the ${cap} SPL allowance) — expect REJECT…`
    );
    try {
        await session.execute({ inner: transfer(failAmt) });
        err("    ✗ expected a rejection but it SETTLED");
    } catch (e) {
        if (e instanceof BastionSdkError) {
            machine.transition("GUARD_ENFORCED", {
                attemptedTokens: failAmt,
                blockedBy: e.message,
                code: e.onChainCode ?? e.code,
            });
            log(
                `    ✓ REJECTED — ${e.message}${e.onChainCode ? ` (#${e.onChainCode})` : ""}`
            );
        } else {
            err(`    ✗ rejected with a non-typed error: ${fmt(e)}`);
        }
    }

    const passAmt = perCall / 2;
    log(
        `\n[2] in-policy trade ${passAmt} ${mode.symbol} — funds leave YOUR ATA…`
    );
    try {
        const sig = await session.execute({ inner: transfer(passAmt) });
        machine.transition("TRADE_SETTLED", {
            amountTokens: passAmt,
            signature: sig,
        });
        log(`    ✓ SETTLED — ${sig}`);
    } catch (e) {
        err(`    ✗ unexpected failure: ${fmt(e)}`);
    }

    log(`\n[3] revoke allowance + session…`);
    await session.revokeAllowance({ mint });
    await session.revoke();
    machine.transition("REVOKED", { allowanceCleared: true });
    machine.transition("DONE", { transitions: machine.events.length });
    log(`    ✓ allowance cleared + session revoked`);
    log(`\ndone.`);
}

main().catch((e: unknown) => {
    err(fmt(e));
    process.exit(1);
});
