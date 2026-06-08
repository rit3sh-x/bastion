import {
    getArrayEncoder,
    getAddressEncoder,
    type Address,
    type Instruction,
} from "@solana/kit";
import {
    getPolicyDataEncoder,
    type PolicyDataArgs,
    ED25519_PROGRAM_ID,
} from "./generated";

/**
 * The commitment a holder signs and pins: `sha256(borsh(Vec<PolicyData>))`.
 * Matches the on-chain `compute_manifest_hash` (codama emits borsh-compatible
 * bytes; the array encoder uses a u32 length prefix, like borsh).
 */
export async function computeManifestHash(
    manifest: readonly PolicyDataArgs[]
): Promise<Uint8Array> {
    const encoded = getArrayEncoder(getPolicyDataEncoder()).encode([
        ...manifest,
    ]);
    const digest = await crypto.subtle.digest(
        "SHA-256",
        new Uint8Array(encoded)
    );
    return new Uint8Array(digest);
}

/**
 * Assemble an Ed25519 precompile instruction proving `publicKey` signed
 * `message`. Layout mirrors the on-chain parser: a 2-byte header, one 14-byte
 * offsets struct (all data in this same instruction), then pubkey || sig || msg.
 */
export function buildEd25519Instruction(args: {
    publicKey: Uint8Array; // 32
    signature: Uint8Array; // 64
    message: Uint8Array;
}): Instruction {
    const { publicKey, signature, message } = args;
    const pkOffset = 16;
    const sigOffset = 16 + 32;
    const msgOffset = 16 + 32 + 64;
    const ANY = 0xffff;

    const data = new Uint8Array(msgOffset + message.length);
    const view = new DataView(data.buffer);
    data[0] = 1; // num signatures
    data[1] = 0; // padding
    view.setUint16(2, sigOffset, true);
    view.setUint16(4, ANY, true); // sig ix index
    view.setUint16(6, pkOffset, true);
    view.setUint16(8, ANY, true); // pk ix index
    view.setUint16(10, msgOffset, true);
    view.setUint16(12, message.length, true);
    view.setUint16(14, ANY, true); // msg ix index
    data.set(publicKey, pkOffset);
    data.set(signature, sigOffset);
    data.set(message, msgOffset);

    return { programAddress: ED25519_PROGRAM_ID, accounts: [], data };
}

/** 32-byte ed25519 public key bytes for an address. */
export function publicKeyBytes(addr: Address): Uint8Array {
    return new Uint8Array(getAddressEncoder().encode(addr));
}

export interface SignedManifest {
    policies: PolicyDataArgs[];
    manifestHash: Uint8Array;
    signature: Uint8Array;
}

/**
 * Holder-side: compute the manifest hash and ed25519-sign it with the owner's
 * private key. The returned `{ policies, signature }` is what the operator
 * attaches to `execute`; `manifestHash` is what the holder pins via `pinManifest`.
 */
export async function signManifest(
    ownerPrivateKey: CryptoKey,
    manifest: readonly PolicyDataArgs[]
): Promise<SignedManifest> {
    const manifestHash = await computeManifestHash(manifest);
    const sig = await crypto.subtle.sign(
        "Ed25519",
        ownerPrivateKey,
        new Uint8Array(manifestHash)
    );
    return {
        policies: [...manifest],
        manifestHash,
        signature: new Uint8Array(sig),
    };
}
