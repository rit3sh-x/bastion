import {
    AccountRole,
    address,
    type Address,
    type Instruction,
} from "@solana/kit";
import {
    BastionSdkError,
    pda,
    type BastionErrorCode,
    type OperatorClient,
    type SessionHandle,
} from "bastion";
import { associatedTokenAddress, buildTokenTransferIx } from "bastion/token";
import { sol, tokens } from "bastion/units";
import type Groq from "groq-sdk";
import { z } from "zod";

import type { Env } from "./env";
import { resolveSpendMode } from "./policies";
import { toolTrace } from "./ui";

type GroqTool = Groq.Chat.Completions.ChatCompletionTool;

const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111";

export function buildTransferIx(
    from: Address,
    to: Address,
    lamports: bigint
): Instruction {
    const data = new Uint8Array(12);
    const view = new DataView(data.buffer);
    view.setUint32(0, 2, true);
    view.setBigUint64(4, lamports, true);
    return {
        programAddress: address(SYSTEM_PROGRAM_ADDRESS),
        accounts: [
            { address: from, role: AccountRole.WRITABLE_SIGNER },
            { address: to, role: AccountRole.WRITABLE },
        ],
        data,
    };
}

const swapArgs = z.object({
    from: z.string().default("?"),
    to: z.string(),
    amount: z.number().positive(),
});
const buyArgs = z.object({
    token: z.string(),
    amount: z.number().positive(),
});
const revokeArgs = z.object({ reason: z.string() });

export interface ToolResult {
    content: string;
    revoked: boolean;
}

export interface ToolKit {
    defs: GroqTool[];
    dispatch(name: string, rawArgs: string): Promise<ToolResult>;
}

const json = (v: unknown) => JSON.stringify(v);

export function buildTools(
    handle: SessionHandle,
    operator: OperatorClient,
    env: Env
): ToolKit {
    const mode = resolveSpendMode(env);
    const hasDest =
        env.swapDest !== undefined && env.swapDest !== SYSTEM_PROGRAM_ADDRESS;

    type GatedOk = {
        ok: true;
        amount: number;
        unit: string;
        signature: string;
    };
    type GatedErr = {
        ok: false;
        amount: number;
        unit: string;
        errorCode?: BastionErrorCode;
        onChainCode?: number | null;
        error?: string;
        message?: string;
    };
    type Gated = GatedOk | GatedErr;

    async function runGated(
        inner: Instruction,
        amount: number,
        unit: string
    ): Promise<Gated> {
        try {
            const signature = await operator.execute({ inner });
            return { ok: true, amount, unit, signature };
        } catch (e) {
            if (e instanceof BastionSdkError) {
                return {
                    ok: false,
                    amount,
                    unit,
                    errorCode: e.code,
                    onChainCode: e.onChainCode ?? null,
                    message: e.message,
                };
            }
            throw e;
        }
    }

    async function gatedSolTransfer(amount: number): Promise<Gated> {
        const [delegate] = await pda.delegate(
            handle.owner,
            handle.sessionKey.address
        );
        const dest = hasDest ? address(env.swapDest) : handle.owner;
        return runGated(
            buildTransferIx(delegate, dest, sol(amount)),
            amount,
            "SOL"
        );
    }

    async function gatedTokenTransfer(amount: number): Promise<Gated> {
        const mint = mode.mint;
        if (!mint) throw new Error("token transfer without MINT configured");
        if (!hasDest) {
            return {
                ok: false,
                amount,
                unit: mode.symbol,
                error: "SWAP_DEST (recipient wallet) is required in token mode",
            };
        }
        const [delegate] = await pda.delegate(
            handle.owner,
            handle.sessionKey.address
        );
        const source = await handle.allowanceSource(mint);
        const dest = await associatedTokenAddress({
            owner: address(env.swapDest),
            mint,
        });
        const inner = buildTokenTransferIx({
            source,
            dest,
            authority: delegate,
            amount: tokens(amount, mode.decimals),
        });
        return runGated(inner, amount, mode.symbol);
    }

    const gatedSpend = (amount: number): Promise<Gated> =>
        mode.mint ? gatedTokenTransfer(amount) : gatedSolTransfer(amount);

    const assetLabel = mode.symbol;
    const defs: GroqTool[] = [
        {
            type: "function",
            function: {
                name: "get_portfolio",
                description:
                    "Read the Bastion-gated wallet's state: session expiry, revoked flag, attached policy count, the spend asset, and (allowance mode) the owner's source token account. In allowance mode the delegate holds NO funds — it spends from the owner's wallet within the caps.",
                parameters: {
                    type: "object",
                    properties: {},
                    additionalProperties: false,
                },
            },
        },
        {
            type: "function",
            function: {
                name: "swap",
                description: `Swap one asset for another (demo: a stand-in transfer of the spend asset, ${assetLabel}). Routed through Bastion — if it exceeds a spend cap, per-call cap, or cooldown, the chain rejects and a typed error is returned.`,
                parameters: {
                    type: "object",
                    properties: {
                        from: {
                            type: "string",
                            description: "Asset to swap from",
                        },
                        to: {
                            type: "string",
                            description: "Asset to swap to",
                        },
                        amount: {
                            type: "number",
                            description: `Amount in whole units of the spend asset (${assetLabel})`,
                        },
                    },
                    required: ["to", "amount"],
                    additionalProperties: false,
                },
            },
        },
        {
            type: "function",
            function: {
                name: "buy",
                description: `Buy a token (demo: a stand-in transfer of the spend asset, ${assetLabel}). Bastion-gated like swap.`,
                parameters: {
                    type: "object",
                    properties: {
                        token: {
                            type: "string",
                            description: "Token symbol or mint to buy",
                        },
                        amount: {
                            type: "number",
                            description: `${assetLabel} to spend (whole units)`,
                        },
                    },
                    required: ["token", "amount"],
                    additionalProperties: false,
                },
            },
        },
        {
            type: "function",
            function: {
                name: "revoke",
                description:
                    "Kill switch — permanently revoke the Bastion handle. After this no further actions execute. Confirm intent with the user before calling.",
                parameters: {
                    type: "object",
                    properties: {
                        reason: {
                            type: "string",
                            description: "Why you're stopping.",
                        },
                    },
                    required: ["reason"],
                    additionalProperties: false,
                },
            },
        },
    ];

    async function dispatch(
        name: string,
        rawArgs: string
    ): Promise<ToolResult> {
        const args: unknown = rawArgs ? JSON.parse(rawArgs) : {};
        switch (name) {
            case "get_portfolio": {
                toolTrace("get_portfolio");
                const [state, policies, balance] = await Promise.all([
                    handle.state(),
                    handle.policies(),
                    handle.delegateBalance(),
                ]);
                const source = mode.mint
                    ? await handle.allowanceSource(mode.mint)
                    : null;
                return {
                    content: json({
                        sessionPda: handle.pubkey,
                        revoked: state.revoked,
                        expiry: state.expiry.toString(),
                        policyCount: policies.length,
                        spendAsset: mode.symbol,
                        mode: mode.mint ? "allowance" : "vault",
                        allowanceSource: source,
                        delegateBalanceSol: Number(balance) / 1e9,
                    }),
                    revoked: false,
                };
            }
            case "swap": {
                const a = swapArgs.parse(args);
                toolTrace("swap", `${a.amount} ${a.from} → ${a.to}`);
                return {
                    content: json(await gatedSpend(a.amount)),
                    revoked: false,
                };
            }
            case "buy": {
                const a = buyArgs.parse(args);
                toolTrace("buy", `${a.amount} ${mode.symbol} → ${a.token}`);
                return {
                    content: json(await gatedSpend(a.amount)),
                    revoked: false,
                };
            }
            case "revoke": {
                const a = revokeArgs.parse(args);
                toolTrace("revoke", a.reason);
                try {
                    const signature = await handle.revoke();
                    return {
                        content: json({
                            ok: true,
                            signature,
                            reason: a.reason,
                        }),
                        revoked: true,
                    };
                } catch (e) {
                    if (e instanceof BastionSdkError) {
                        return {
                            content: json({
                                ok: false,
                                errorCode: e.code,
                                message: e.message,
                            }),
                            revoked: false,
                        };
                    }
                    throw e;
                }
            }
            default:
                return {
                    content: json({
                        ok: false,
                        error: `unknown tool: ${name}`,
                    }),
                    revoked: false,
                };
        }
    }

    return { defs, dispatch };
}
