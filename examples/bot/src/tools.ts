import {
    AccountRole,
    address,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    type Address,
    type Instruction,
} from "@solana/kit";
import {
    BastionSdkError,
    pda,
    sendTx,
    type BastionErrorCode,
    type OperatorClient,
    type SessionHandle,
} from "bastion";
import {
    associatedTokenAddress,
    buildCreateAtaIdempotentIx,
    buildTokenTransferIx,
} from "bastion/token";
import { sol, tokens } from "bastion/units";
import type Groq from "groq-sdk";
import { z } from "zod";

import type { BotConfig } from "./config";
import { LIMITS } from "./policies";
import { toolTrace } from "./ui";

type GroqTool = Groq.Chat.Completions.ChatCompletionTool;

const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;

const json = (v: unknown) => JSON.stringify(v);

function systemTransferIx(
    from: Address,
    to: Address,
    lamports: bigint
): Instruction {
    const data = new Uint8Array(12);
    const view = new DataView(data.buffer);
    view.setUint32(0, 2, true);
    view.setBigUint64(4, lamports, true);
    return {
        programAddress: SYSTEM_PROGRAM_ADDRESS,
        accounts: [
            { address: from, role: AccountRole.WRITABLE_SIGNER },
            { address: to, role: AccountRole.WRITABLE },
        ],
        data,
    };
}

const pubkey = z.string().refine((s) => {
    try {
        address(s);
        return true;
    } catch {
        return false;
    }
}, "must be a base58 Solana pubkey");

const sendSolArgs = z.object({ amount: z.number().positive(), to: pubkey });
const sendSplArgs = z.object({ amount: z.number().positive(), to: pubkey });
const revokeArgs = z.object({ reason: z.string() });

export interface ToolResult {
    content: string;
    revoked: boolean;
}

export interface ToolKit {
    defs: GroqTool[];
    dispatch(name: string, rawArgs: string): Promise<ToolResult>;
}

type GatedOk = { ok: true; amount: number; unit: string; signature: string };
type GatedErr = {
    ok: false;
    amount: number;
    unit: string;
    errorCode?: BastionErrorCode;
    onChainCode?: number | null;
    message?: string;
};
type Gated = GatedOk | GatedErr;

export function buildTools(
    cfg: BotConfig,
    handle: SessionHandle,
    operator: OperatorClient
): ToolKit {
    const rpc = createSolanaRpc(cfg.rpcUrl);
    const rpcSubscriptions = createSolanaRpcSubscriptions(cfg.wsUrl);

    const delegatePromise = pda
        .delegate(handle.owner, operator.sessionKey)
        .then(([d]) => d);

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

    async function sendSol(amount: number, to: Address): Promise<Gated> {
        const delegate = await delegatePromise;
        return runGated(
            systemTransferIx(delegate, to, sol(amount)),
            amount,
            "SOL"
        );
    }

    async function sendSpl(amount: number, to: Address): Promise<Gated> {
        const delegate = await delegatePromise;
        const delegateAta = await associatedTokenAddress({
            owner: delegate,
            mint: cfg.mint,
        });
        const recipientAta = await associatedTokenAddress({
            owner: to,
            mint: cfg.mint,
        });
        await sendTx({
            rpc,
            rpcSubscriptions,
            feePayer: cfg.owner,
            commitment: "confirmed",
            instructions: [
                buildCreateAtaIdempotentIx({
                    payer: cfg.ownerAddress,
                    ata: recipientAta,
                    owner: to,
                    mint: cfg.mint,
                }),
            ],
        });
        return runGated(
            buildTokenTransferIx({
                source: delegateAta,
                dest: recipientAta,
                authority: delegate,
                amount: tokens(amount, cfg.decimals),
            }),
            amount,
            cfg.symbol
        );
    }

    async function tokenVaultBalance(): Promise<number | null> {
        try {
            const delegate = await delegatePromise;
            const delegateAta = await associatedTokenAddress({
                owner: delegate,
                mint: cfg.mint,
            });
            const res = await rpc.getTokenAccountBalance(delegateAta).send();
            return Number(res.value.amount) / 10 ** cfg.decimals;
        } catch {
            return null;
        }
    }

    const defs: GroqTool[] = [
        {
            type: "function",
            function: {
                name: "get_status",
                description:
                    "Read the Bastion-gated wallet's live state: session expiry, revoked flag, attached policy count, and the delegate vault's SOL and token balances. No transaction.",
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
                name: "send_sol",
                description: `Send native SOL from the gated vault to a recipient pubkey. Use for "move 3 SOL to <pubkey>". Routed through Bastion — exceeding the per-call cap (${LIMITS.sol.perTrade} SOL), the daily SpendCap, or the cooldown makes the chain reject, and a typed error is returned.`,
                parameters: {
                    type: "object",
                    properties: {
                        amount: {
                            type: "number",
                            description: "Amount of SOL (whole units).",
                        },
                        to: {
                            type: "string",
                            description: "Recipient wallet pubkey (base58).",
                        },
                    },
                    required: ["amount", "to"],
                    additionalProperties: false,
                },
            },
        },
        {
            type: "function",
            function: {
                name: "send_spl",
                description: `Send ${cfg.symbol} SPL tokens from the gated vault to a recipient pubkey (its associated token account is created if missing). Use for "send 4 spl to <pubkey>". Bastion-gated like send_sol, with the token caps.`,
                parameters: {
                    type: "object",
                    properties: {
                        amount: {
                            type: "number",
                            description: `Amount of ${cfg.symbol} tokens (whole units).`,
                        },
                        to: {
                            type: "string",
                            description: "Recipient wallet pubkey (base58).",
                        },
                    },
                    required: ["amount", "to"],
                    additionalProperties: false,
                },
            },
        },
        {
            type: "function",
            function: {
                name: "revoke",
                description:
                    "Kill switch — permanently revoke the Bastion session. After this no further sends execute. Confirm intent with the user before calling.",
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
            case "get_status": {
                toolTrace("get_status");
                const [state, policies, lamports, tokenBal] = await Promise.all(
                    [
                        handle.state(),
                        handle.policies(),
                        handle.delegateBalance(),
                        tokenVaultBalance(),
                    ]
                );
                return {
                    content: json({
                        sessionPda: handle.pubkey,
                        revoked: state.revoked,
                        expiry: state.expiry.toString(),
                        policyCount: policies.length,
                        vaultSol: Number(lamports) / 1e9,
                        vaultTokens: tokenBal,
                        tokenSymbol: cfg.symbol,
                    }),
                    revoked: false,
                };
            }
            case "send_sol": {
                const a = sendSolArgs.parse(args);
                toolTrace("send_sol", `${a.amount} SOL → ${a.to}`);
                return {
                    content: json(await sendSol(a.amount, address(a.to))),
                    revoked: false,
                };
            }
            case "send_spl": {
                const a = sendSplArgs.parse(args);
                toolTrace("send_spl", `${a.amount} ${cfg.symbol} → ${a.to}`);
                return {
                    content: json(await sendSpl(a.amount, address(a.to))),
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
