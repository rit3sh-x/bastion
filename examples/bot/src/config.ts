import { readFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { address, type Address } from "@solana/kit";
import { fromSecretKey, type SessionSigner } from "bastion";

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const TEMP_DIR = join(PACKAGE_ROOT, "temp");

export interface BotConfig {
    rpcUrl: string;
    wsUrl: string;
    mint: Address;
    decimals: number;
    symbol: string;
    owner: SessionSigner;
    ownerAddress: Address;
    credPath: string;
    groqApiKey: string | undefined;
    model: string;
    maxSteps: number;
}

interface TempConfig {
    rpcUrl: string;
    wsUrl: string;
    mint: string;
    decimals: number;
    symbol: string;
    ownerKeypair: string;
}

const SETUP_HINT =
    "Run the provisioner first:  pnpm -F @examples/bot provision";

function readTempConfig(): TempConfig {
    let raw: string;
    try {
        raw = readFileSync(join(TEMP_DIR, "config.json"), "utf8");
    } catch {
        throw new Error(`temp/config.json not found. ${SETUP_HINT}`);
    }
    return JSON.parse(raw) as TempConfig;
}

async function loadKeypair(path: string): Promise<SessionSigner> {
    let bytes: number[];
    try {
        bytes = JSON.parse(readFileSync(path, "utf8")) as number[];
    } catch {
        throw new Error(`keypair not found at ${path}. ${SETUP_HINT}`);
    }
    return fromSecretKey(Uint8Array.from(bytes));
}

export async function loadConfig(): Promise<BotConfig> {
    const cfg = readTempConfig();
    const owner = await loadKeypair(join(TEMP_DIR, basename(cfg.ownerKeypair)));
    return {
        rpcUrl: cfg.rpcUrl,
        wsUrl: cfg.wsUrl,
        mint: address(cfg.mint),
        decimals: cfg.decimals,
        symbol: cfg.symbol,
        owner,
        ownerAddress: owner.address,
        credPath: join(TEMP_DIR, "operator.json"),
        groqApiKey: process.env.GROQ_API_KEY,
        model: process.env.MODEL ?? "llama-3.3-70b-versatile",
        maxSteps: Number(process.env.MAX_STEPS ?? "8"),
    };
}
