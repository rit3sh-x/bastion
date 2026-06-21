import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { config as dotenv } from "dotenv";

import {
    createKeyPairSignerFromBytes,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    getBase58Encoder,
    type KeyPairSigner,
    type Rpc,
    type RpcSubscriptions,
    type SolanaRpcApi,
    type SolanaRpcSubscriptionsApi,
} from "@solana/kit";
import { createHolderClient, type HolderClient } from "bastion";

const here = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(here, "..");

dotenv({
    path: resolve(packageDir, ".env"),
    override: false,
    quiet: true,
});

export const DEVNET_E2E_ENABLED = process.env.BASTION_DEVNET_E2E === "1";
export const RPC_URL = process.env.RPC_URL ?? "https://api.devnet.solana.com";
export const WS_URL = RPC_URL.replace(/^https:/, "wss:").replace(
    /^http:/,
    "ws:"
);

export interface DevnetContext {
    rpc: Rpc<SolanaRpcApi>;
    rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
    owner: KeyPairSigner;
    holder: HolderClient;
}

export async function createDevnetContext(): Promise<DevnetContext> {
    const owner = await loadOwner();
    const rpc = createSolanaRpc(RPC_URL) as Rpc<SolanaRpcApi>;
    const rpcSubscriptions = createSolanaRpcSubscriptions(
        WS_URL
    ) as RpcSubscriptions<SolanaRpcSubscriptionsApi>;
    return {
        rpc,
        rpcSubscriptions,
        owner,
        holder: createHolderClient({
            url: RPC_URL,
            wsUrl: WS_URL,
            wallet: owner,
            logger: { level: "warn" },
        }),
    };
}

async function loadOwner(): Promise<KeyPairSigner> {
    if (process.env.OWNER_SECRET) {
        const bytes = new Uint8Array(
            getBase58Encoder().encode(process.env.OWNER_SECRET)
        );
        if (bytes.length !== 64) {
            throw new Error(
                `OWNER_SECRET decoded to ${bytes.length} bytes; expected 64`
            );
        }
        return createKeyPairSignerFromBytes(bytes);
    }

    const keypairPath = expandHome(
        process.env.OWNER_KEYPAIR ?? "~/.config/solana/id.json"
    );
    if (!existsSync(keypairPath)) {
        throw new Error(
            "Set OWNER_SECRET or OWNER_KEYPAIR to run devnet e2e tests"
        );
    }
    const parsed = JSON.parse(readFileSync(keypairPath, "utf8")) as unknown;
    if (!Array.isArray(parsed) || parsed.length !== 64) {
        throw new Error(`${keypairPath} must contain a 64-byte keypair array`);
    }
    return createKeyPairSignerFromBytes(new Uint8Array(parsed as number[]));
}

function expandHome(path: string): string {
    return path.startsWith("~/") ? resolve(homedir(), path.slice(2)) : path;
}
