import { existsSync, readFileSync, writeFileSync } from "node:fs";

import { getBase58Encoder } from "@solana/kit";
import {
    createHolderClient,
    createOperatorClient,
    parseOperatorCredential,
    serializeOperatorCredential,
    sessionKeyFromSecret,
    tokens,
    type BastionHooks,
    type HolderClient,
    type OperatorClient,
    type OperatorCredential,
    type SessionHandle,
} from "bastion";

import type { Env } from "./env";
import { buildPolicies, LIMITS, resolveSpendMode } from "./policies";
import { log, toolTrace, warn } from "./ui";
import { loadOwnerSigner } from "./wallet";

export interface AgentContext {
    handle: SessionHandle;
    operator: OperatorClient;
}

function hooks(): BastionHooks {
    return {
        before(ctx) {
            toolTrace(`bastion:${ctx.op}`, "…");
        },
        error(ctx) {
            warn(`bastion:${ctx.op} → ${ctx.error.code}`);
        },
    };
}

async function reuseExisting(
    holder: HolderClient,
    credPath: string
): Promise<AgentContext | undefined> {
    if (!existsSync(credPath)) return undefined;
    let cred: OperatorCredential;
    try {
        cred = parseOperatorCredential(readFileSync(credPath, "utf8"));
    } catch {
        return undefined;
    }
    const sessionKey = await sessionKeyFromSecret(
        new Uint8Array(getBase58Encoder().encode(cred.sessionSecret))
    );
    const handle = holder.hydrate({ pubkey: cred.sessionPda, sessionKey });
    try {
        const state = await handle.state();
        if (state.revoked) return undefined;
    } catch {
        return undefined;
    }
    log(`rehydrated session ${cred.sessionPda} from ${credPath}`);
    return { handle, operator: await createOperatorClient(cred) };
}

export async function openSession(env: Env): Promise<AgentContext> {
    const wallet = await loadOwnerSigner(env.ownerSecretB58);
    log(`owner:   ${wallet.address}`);

    const holder = createHolderClient({
        url: env.rpcUrl,
        ...(env.wsUrl ? { wsUrl: env.wsUrl } : {}),
        wallet,
        hooks: hooks(),
        logger: { level: "warn" },
    });

    const reused = await reuseExisting(holder, env.credPath);
    if (reused) {
        await reportFunding(reused, env);
        return reused;
    }

    const mode = resolveSpendMode(env);
    const policies = buildPolicies(mode);
    const allowance = mode.mint
        ? {
              mint: mode.mint,
              amount: tokens(env.allowanceTokens, mode.decimals),
          }
        : undefined;

    log("opening new session + attaching policies…");
    const { handle, operator: cred } = await holder.openSession({
        expiry: { secsFromNow: LIMITS.sessionDurationSecs },
        policies,
        ...(allowance ? { allowance } : {}),
    });
    writeFileSync(env.credPath, serializeOperatorCredential(cred));
    log(`${policies.length} policies attached (${mode.symbol} caps)`);
    log(`session: ${cred.sessionPda}`);
    log(`operator credential (ship this) → ${env.credPath}`);

    const ctx: AgentContext = {
        handle,
        operator: await createOperatorClient(cred),
    };
    await reportFunding(ctx, env);
    return ctx;
}

async function reportFunding(ctx: AgentContext, env: Env): Promise<void> {
    const mode = resolveSpendMode(env);
    if (mode.mint) {
        try {
            const source = await ctx.handle.allowanceSource(mode.mint);
            log(
                `allowance mode: agent spends up to ${env.allowanceTokens} ${mode.symbol} from your ATA ${source}\n`
            );
        } catch (e) {
            warn(
                `allowanceSource lookup failed: ${
                    e instanceof Error ? e.message : String(e)
                }`
            );
        }
    } else {
        const bal = Number(await ctx.handle.delegateBalance()) / 1e9;
        log(
            `vault mode: delegate balance ${bal} SOL  (fund the delegate to enable spends)\n`
        );
    }
}
