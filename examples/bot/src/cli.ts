import * as readline from "node:readline/promises";
import { stdin, stdout } from "node:process";

import type { Agent } from "./agent";
import { assistant, log, PROMPT, warn } from "./ui";

const HELP = [
    "",
    "Talk to the agent in plain language. Examples:",
    "  swap 0.05 SOL for USDC",
    "  buy BONK with 0.02 SOL",
    "  what's my portfolio?",
    "  stop trading",
    "",
    "Commands:  /help  /exit",
    "",
];

export async function runRepl(agent: Agent): Promise<void> {
    const rl = readline.createInterface({ input: stdin, output: stdout });
    for (const line of HELP) log(line);

    try {
        for (;;) {
            const input = (await rl.question(PROMPT)).trim();
            if (!input) continue;
            if (input === "/exit" || input === "/quit") break;
            if (input === "/help") {
                for (const line of HELP) log(line);
                continue;
            }

            try {
                const { text, revoked } = await agent.chat(input);
                if (text) assistant(text);
                if (revoked) {
                    warn(
                        "session revoked — no further actions can execute. exiting."
                    );
                    break;
                }
            } catch (e) {
                warn(
                    `turn failed: ${e instanceof Error ? e.message : String(e)}`
                );
            }
        }
    } finally {
        rl.close();
    }
    log("bye.");
}
