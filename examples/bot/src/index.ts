import Groq from "groq-sdk";

import { Agent } from "./agent";
import { runRepl } from "./cli";
import { loadConfig } from "./config";
import { LIMITS } from "./policies";
import { openSession } from "./session";
import { buildTools } from "./tools";
import { banner, err, log } from "./ui";

async function main(): Promise<void> {
    const cfg = await loadConfig();
    banner([
        "Bastion-gated agent CLI (Groq)",
        `model: ${cfg.model}    rpc: ${cfg.rpcUrl}`,
        `asset: SOL + ${cfg.symbol} (${cfg.mint})`,
    ]);

    if (!cfg.groqApiKey) {
        err(
            "GROQ_API_KEY is not set — get one at https://console.groq.com/keys and add it to .env."
        );
        process.exit(1);
    }

    const { handle, operator } = await openSession(cfg);
    log(
        `caps: ${LIMITS.sol.perTrade} SOL / ${LIMITS.token.perTrade} ${cfg.symbol} per send, ${LIMITS.sol.lifetime} SOL / ${LIMITS.token.lifetime} ${cfg.symbol} daily\n`
    );

    const groq = new Groq({ apiKey: cfg.groqApiKey });
    const tools = buildTools(cfg, handle, operator);
    const agent = new Agent(groq, cfg.model, tools, cfg.maxSteps, cfg.symbol);

    await runRepl(agent);
}

main().catch((e: unknown) => {
    err(e instanceof Error ? e.message : String(e));
    process.exit(1);
});
