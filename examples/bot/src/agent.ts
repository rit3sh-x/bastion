import type Groq from "groq-sdk";

import { LIMITS } from "./policies";
import type { ToolKit } from "./tools";

type Msg = Groq.Chat.Completions.ChatCompletionMessageParam;

function systemPrompt(symbol: string): string {
    return [
        "You are an autonomous agent operating a Solana wallet that is gated by Bastion — an on-chain policy firewall.",
        `You move two assets out of a delegate vault: native SOL and an SPL token (${symbol}). The vault is pre-funded; you spend from it within hard on-chain caps.`,
        "Every send is validated on-chain. You CANNOT exceed the limits; if you try, the tool returns ok:false with an errorCode (e.g. AmountPerCallExceeded, SpendCapExceeded, CooldownActive, MaxCallsExceeded). Read it, explain it plainly to the user, and adjust.",
        "",
        "Tools:",
        "- get_status(): session state, policy count, and the vault's SOL + token balances.",
        "- send_sol(amount, to): send `amount` SOL to the `to` pubkey.",
        `- send_spl(amount, to): send \`amount\` ${symbol} tokens to the \`to\` pubkey.`,
        "- revoke(reason): permanent kill switch — confirm with the user first.",
        "",
        `Map plain requests to a tool: "move 3 sol to <pubkey>" → send_sol(3, "<pubkey>"); "send 4 ${symbol} to <pubkey>" → send_spl(4, "<pubkey>"). Pass the recipient pubkey through verbatim. If the amount, asset, or recipient is missing, ask. Call get_status when you need live balances. Be concise.`,
        "",
        `Current caps — SOL: ${LIMITS.sol.perTrade}/send, ${LIMITS.sol.lifetime} daily. ${symbol}: ${LIMITS.token.perTrade}/send, ${LIMITS.token.lifetime} daily. ${LIMITS.totalCalls} total actions, ${LIMITS.cooldownSecs}s cooldown between sends, session lasts ${LIMITS.sessionDurationSecs / 3600}h.`,
    ].join("\n");
}

export class Agent {
    private readonly messages: Msg[];

    constructor(
        private readonly groq: Groq,
        private readonly model: string,
        private readonly tools: ToolKit,
        private readonly maxSteps: number,
        symbol: string
    ) {
        this.messages = [{ role: "system", content: systemPrompt(symbol) }];
    }

    async chat(userInput: string): Promise<{ text: string; revoked: boolean }> {
        this.messages.push({ role: "user", content: userInput });
        let revoked = false;

        for (let step = 0; step < this.maxSteps; step++) {
            const res = await this.groq.chat.completions.create({
                model: this.model,
                messages: this.messages,
                tools: this.tools.defs,
                tool_choice: "auto",
                temperature: 0.3,
            });

            const choice = res.choices[0]?.message;
            if (!choice) break;
            this.messages.push(choice);

            const calls = choice.tool_calls ?? [];
            if (calls.length === 0) {
                return { text: choice.content ?? "", revoked };
            }

            for (const call of calls) {
                if (call.type !== "function") continue;
                const out = await this.tools.dispatch(
                    call.function.name,
                    call.function.arguments
                );
                if (out.revoked) revoked = true;
                this.messages.push({
                    role: "tool",
                    tool_call_id: call.id,
                    content: out.content,
                });
            }
        }

        return {
            text: "(stopped: reached the max tool steps for this turn — ask me to continue)",
            revoked,
        };
    }
}
