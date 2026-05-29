import {
    createKeyPairSignerFromBytes,
    generateKeyPairSigner,
    type Address,
    type KeyPairSigner,
    type TransactionSigner,
} from "@solana/kit";

export type WalletSigner = TransactionSigner<Address>;
export type SessionSigner = KeyPairSigner<Address>;

export async function fromSecretKey(bytes: Uint8Array): Promise<SessionSigner> {
    return createKeyPairSignerFromBytes(bytes);
}

export async function generateSessionKey(): Promise<SessionSigner> {
    return generateKeyPairSigner();
}
