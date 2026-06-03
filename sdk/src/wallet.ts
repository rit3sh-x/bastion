import {
    createKeyPairSignerFromBytes,
    createKeyPairSignerFromPrivateKeyBytes,
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

export interface ExtractableSessionKey {
    signer: SessionSigner;
    secretKey: Uint8Array;
}

export async function generateExtractableSessionKey(): Promise<ExtractableSessionKey> {
    const secretKey = crypto.getRandomValues(new Uint8Array(32));
    const signer = await createKeyPairSignerFromPrivateKeyBytes(secretKey);
    return { signer, secretKey };
}

export async function sessionKeyFromSecret(
    secret: Uint8Array
): Promise<SessionSigner> {
    return createKeyPairSignerFromPrivateKeyBytes(secret);
}
