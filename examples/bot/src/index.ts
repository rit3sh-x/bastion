import Groq from "groq-sdk";

import { Agent } from "./agent";
import { runRepl } from "./cli";
import { loadEnv, type Env } from "./env";
import {
    activeCaps,
    buildPolicies,
    LIMITS,
    resolveSpendMode,
} from "./policies";
import { openSession } from "./session";
import { buildTools } from "./tools";
import { banner, err, log } from "./ui";

async function main(): Promise<void> {
    const env = loadEnv();
    banner([
        "Bastion-gated trading CLI (Groq)",
        `mode:  ${env.dryRun ? "DRY RUN" : "LIVE"}    rpc: ${env.rpcUrl}`,
        `model: ${env.model}`,
    ]);

    if (env.dryRun) return dryRun(env);

    if (!env.groqApiKey) {
        err(
            "GROQ_API_KEY is not set — get one at https://console.groq.com/keys and add it to .env."
        );
        err(
            "Or run with DRY_RUN=1 to sanity-check policy construction without Groq or a chain."
        );
        process.exit(1);
    }

    const { session } = await openSession(env);
    const groq = new Groq({ apiKey: env.groqApiKey });
    const tools = buildTools(session, env);
    const agent = new Agent(
        groq,
        env.model,
        tools,
        env.maxSteps,
        activeCaps(resolveSpendMode(env))
    );

    await runRepl(agent);
}

function dryRun(env: Env): void {
    const mode = resolveSpendMode(env);
    const caps = activeCaps(mode);
    const policies = buildPolicies(mode);
    log("[dry-run] no Groq call, no chain. Policy envelope:");
    log(
        `  ✓ ${policies.length} codama PolicyDataArgs: ${policies.map((p) => p.__kind).join(", ")}`
    );
    log(
        `  ✓ spend asset ${caps.unit} (${mode.mint ? "allowance mode" : "vault mode"})`
    );
    log(
        `  ✓ per-trade max ${caps.perTrade} ${caps.unit}, lifetime ${caps.lifetime} ${caps.unit}`
    );
    log(
        `  ✓ ${LIMITS.totalCalls} actions, ${LIMITS.cooldownSecs}s cooldown, ${LIMITS.sessionDurationSecs / 3600}h session`
    );
    if (mode.mint) {
        log(
            `  ✓ tier-1 SPL approve ceiling ${env.allowanceTokens} ${caps.unit}`
        );
    }
    log(
        "\nSet GROQ_API_KEY + OWNER_SECRET + RPC_URL (+ MINT for allowance mode) in .env to run live."
    );
}

main().catch((e: unknown) => {
    err(e instanceof Error ? e.message : String(e));
    process.exit(1);
});
