import type Groq from "groq-sdk";

import { LIMITS, type ActiveCaps } from "./policies";
import type { ToolKit } from "./tools";

type Msg = Groq.Chat.Completions.ChatCompletionMessageParam;

function systemPrompt(caps: ActiveCaps): string {
    const u = caps.unit;
    return [
        "You are an autonomous trading agent operating a Solana wallet that is gated by Bastion — an on-chain policy firewall.",
        `The spend asset is ${u}. In allowance mode the funds stay in the user's own wallet; the agent is granted a capped allowance and the delegate holds no funds.`,
        "Every swap/buy you make is validated on-chain. You CANNOT exceed the limits; if you try, the tool returns ok:false with an errorCode (e.g. AmountPerCallExceeded, SpendCapExceeded, CooldownActive, MaxCallsExceeded). Read it, explain it plainly to the user, and adjust.",
        "",
        "Tools:",
        "- get_portfolio(): current session state, policy count, spend asset, and (allowance mode) the owner's source token account.",
        "- swap(from,to,amount): swap one asset for another.",
        "- buy(token,amount): buy a token.",
        "- revoke(reason): permanent kill switch — confirm with the user first.",
        "",
        `When the user says e.g. "swap 5 ${u} for USDC" or "buy BONK with 2 ${u}", map it to the right tool with the amount they gave (in whole ${u}). If they omit an amount, ask. Call get_portfolio when you need live state. Be concise.`,
        "",
        `Current limits: ${caps.perTrade} ${u} per trade, ${caps.lifetime} ${u} lifetime cap, ${LIMITS.totalCalls} total actions, ${LIMITS.cooldownSecs}s cooldown between calls, session lasts ${LIMITS.sessionDurationSecs / 3600}h.`,
    ].join("\n");
}

export class Agent {
    private readonly messages: Msg[];

    constructor(
        private readonly groq: Groq,
        private readonly model: string,
        private readonly tools: ToolKit,
        private readonly maxSteps: number,
        caps: ActiveCaps
    ) {
        this.messages = [{ role: "system", content: systemPrompt(caps) }];
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
