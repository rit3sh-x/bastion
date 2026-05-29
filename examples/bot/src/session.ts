import {
    createBastion,
    pda,
    tokens,
    type Bastion,
    type BastionHooks,
    type SessionHandle,
} from "bastion";

import type { Env } from "./env";
import { buildPolicies, LIMITS, resolveSpendMode } from "./policies";
import { log, toolTrace, warn } from "./ui";
import { loadOwnerSigner, loadSessionSigner } from "./wallet";

export interface AgentContext {
    bastion: Bastion;
    session: SessionHandle;
}

export async function openSession(env: Env): Promise<AgentContext> {
    const wallet = await loadOwnerSigner(env.ownerSecretB58);
    const sessionKey = await loadSessionSigner(env.sessionSecretB58);
    log(`owner:   ${wallet.address}`);

    const hooks: BastionHooks = {
        before(ctx) {
            toolTrace(`bastion:${ctx.op}`, "…");
        },
        error(ctx) {
            warn(`bastion:${ctx.op} → ${ctx.error.code}`);
        },
    };

    const bastion = createBastion({
        url: env.rpcUrl,
        ...(env.wsUrl ? { wsUrl: env.wsUrl } : {}),
        wallet,
        hooks,
        logger: { level: "warn" },
    });

    const mode = resolveSpendMode(env);
    const openArgs = sessionKey
        ? { expiry: { secsFromNow: LIMITS.sessionDurationSecs }, sessionKey }
        : { expiry: { secsFromNow: LIMITS.sessionDurationSecs } };

    const openAndAttach = async (): Promise<SessionHandle> => {
        const opened = await bastion.session.open(openArgs);
        const policies = buildPolicies(mode);
        await opened.attachMany(policies);
        log(`${policies.length} policies attached (${mode.symbol} caps)`);
        return opened;
    };

    let session: SessionHandle;
    if (sessionKey) {
        const [sessionPda] = await pda.session(
            wallet.address,
            sessionKey.address
        );
        session = bastion.session.hydrate({ pubkey: sessionPda, sessionKey });
        try {
            await session.state();
            log(`rehydrated existing session ${sessionPda}`);
        } catch {
            log("opening new session + attaching policies…");
            session = await openAndAttach();
        }
    } else {
        log("opening new session (SDK-generated key) + attaching policies…");
        session = await openAndAttach();
    }

    log(`session: ${session.pubkey}  (key ${session.sessionKey.address})`);
    const [delegate] = await pda.delegate(
        wallet.address,
        session.sessionKey.address
    );

    if (mode.mint) {
        try {
            const { source, delegate: del } = await session.approveAllowance({
                mint: mode.mint,
                amount: tokens(env.allowanceTokens, mode.decimals),
            });
            log(
                `allowance approved: delegate ${del} may spend up to ${env.allowanceTokens} ${mode.symbol} from your ATA ${source}\n`
            );
        } catch (e) {
            warn(
                `approveAllowance failed — does your ${mode.symbol} ATA exist and hold tokens? ${
                    e instanceof Error ? e.message : String(e)
                }`
            );
        }
    } else {
        const bal = Number(await session.delegateBalance()) / 1e9;
        log(
            `delegate ${delegate} — balance ${bal} SOL  (fund this to enable spends)\n`
        );
    }

    return { bastion, session };
}
