import {
    createKeyPairSignerFromBytes,
    generateKeyPairSigner,
    getBase58Encoder,
} from "@solana/kit";
import { fromSecretKey, type SessionSigner } from "bastion";

import { warn } from "./ui";

export async function loadOwnerSigner(secretB58: string | undefined) {
    if (secretB58) {
        const bytes = getBase58Encoder().encode(secretB58);
        if (bytes.length !== 64) {
            warn(
                `OWNER_SECRET decoded to ${bytes.length} bytes; expected 64. Using a throwaway keypair instead.`
            );
            return generateKeyPairSigner();
        }
        return createKeyPairSignerFromBytes(new Uint8Array(bytes));
    }
    warn("OWNER_SECRET unset — generating a throwaway keypair (0 SOL).");
    warn(
        "Live run: solana-keygen new, base58-encode the secret key, set OWNER_SECRET."
    );
    return generateKeyPairSigner();
}

export async function loadSessionSigner(
    secretB58: string | undefined
): Promise<SessionSigner | undefined> {
    if (!secretB58) {
        warn(
            "SESSION_SECRET unset — the SDK will generate an EPHEMERAL session key (held in memory for this run only; its delegate changes per run, so you can't pre-fund it)."
        );
        warn(
            "Set SESSION_SECRET (base58 64-byte) for a stable session + delegate you fund once."
        );
        return undefined;
    }
    const bytes = getBase58Encoder().encode(secretB58);
    if (bytes.length !== 64) {
        warn(
            `SESSION_SECRET decoded to ${bytes.length} bytes; expected 64 — falling back to an SDK-generated ephemeral key.`
        );
        return undefined;
    }
    return fromSecretKey(new Uint8Array(bytes));
}
