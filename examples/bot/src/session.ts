import { existsSync, readFileSync, writeFileSync } from "node:fs";

import {
    AccountRole,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    getBase58Encoder,
    type Address,
    type Instruction,
} from "@solana/kit";
import {
    createHolderClient,
    createOperatorClient,
    parseOperatorCredential,
    pda,
    sendTx,
    serializeOperatorCredential,
    sessionKeyFromSecret,
    type BastionHooks,
    type HolderClient,
    type OperatorClient,
    type OperatorCredential,
    type SessionHandle,
} from "bastion";
import {
    associatedTokenAddress,
    buildCreateAtaIdempotentIx,
    buildTokenTransferIx,
} from "bastion/token";
import { sol, tokens } from "bastion/units";

import type { BotConfig } from "./config";
import { buildPolicies, LIMITS } from "./policies";
import { log, toolTrace, warn } from "./ui";

const DELEGATE_SOL = sol(5);
const DELEGATE_TOKENS = 500;
const SESSION_FEE_SOL = sol(1);

const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;

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

async function fundVault(cfg: BotConfig, sessionKey: Address): Promise<void> {
    const rpc = createSolanaRpc(cfg.rpcUrl);
    const rpcSubscriptions = createSolanaRpcSubscriptions(cfg.wsUrl);
    const [delegate] = await pda.delegate(cfg.ownerAddress, sessionKey);

    const ownerAta = await associatedTokenAddress({
        owner: cfg.ownerAddress,
        mint: cfg.mint,
    });
    const delegateAta = await associatedTokenAddress({
        owner: delegate,
        mint: cfg.mint,
    });

    await sendTx({
        rpc,
        rpcSubscriptions,
        feePayer: cfg.owner,
        commitment: "confirmed",
        instructions: [
            systemTransferIx(cfg.ownerAddress, delegate, DELEGATE_SOL),
            systemTransferIx(cfg.ownerAddress, sessionKey, SESSION_FEE_SOL),
            buildCreateAtaIdempotentIx({
                payer: cfg.ownerAddress,
                ata: delegateAta,
                owner: delegate,
                mint: cfg.mint,
            }),
            buildTokenTransferIx({
                source: ownerAta,
                dest: delegateAta,
                authority: cfg.ownerAddress,
                amount: tokens(DELEGATE_TOKENS, cfg.decimals),
            }),
        ],
    });

    log(
        `funded vault → ${delegate}\n  ${Number(DELEGATE_SOL) / 1e9} SOL + ${DELEGATE_TOKENS} ${cfg.symbol}  (session key fees: ${Number(SESSION_FEE_SOL) / 1e9} SOL)\n`
    );
}

export async function openSession(cfg: BotConfig): Promise<AgentContext> {
    log(`owner:   ${cfg.ownerAddress}`);

    const holder = createHolderClient({
        url: cfg.rpcUrl,
        wsUrl: cfg.wsUrl,
        wallet: cfg.owner,
        hooks: hooks(),
        logger: { level: "warn" },
    });

    const reused = await reuseExisting(holder, cfg.credPath);
    if (reused) {
        log(`reusing session ${reused.handle.pubkey}\n`);
        return reused;
    }

    log("opening new session + attaching policies…");
    const { handle, operator: cred } = await holder.openSession({
        expiry: { secsFromNow: LIMITS.sessionDurationSecs },
        policies: buildPolicies(cfg.mint, cfg.decimals),
    });
    writeFileSync(cfg.credPath, serializeOperatorCredential(cred));
    log(`session: ${cred.sessionPda}`);
    log(`operator credential → ${cfg.credPath}`);

    const operator = await createOperatorClient(cred);
    await fundVault(cfg, operator.sessionKey);

    return { handle, operator };
}
