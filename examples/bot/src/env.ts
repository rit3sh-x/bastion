export interface Env {
    groqApiKey: string | undefined;
    rpcUrl: string;
    wsUrl: string | undefined;
    ownerSecretB58: string | undefined;
    sessionSecretB58: string | undefined;
    model: string;
    maxSteps: number;
    dryRun: boolean;
    swapDest: string;
    mint: string | undefined;
    mintDecimals: number;
    tokenSymbol: string;
    allowanceTokens: number;
    credPath: string;
}

export function loadEnv(): Env {
    return {
        groqApiKey: process.env.GROQ_API_KEY,
        rpcUrl: process.env.RPC_URL ?? "https://api.devnet.solana.com",
        wsUrl: process.env.WS_URL,
        ownerSecretB58: process.env.OWNER_SECRET,
        sessionSecretB58: process.env.SESSION_SECRET,
        model: process.env.MODEL ?? "llama-3.3-70b-versatile",
        maxSteps: Number(process.env.MAX_STEPS ?? "8"),
        dryRun: process.env.DRY_RUN === "1",
        swapDest: process.env.SWAP_DEST ?? "11111111111111111111111111111111",
        mint: process.env.MINT,
        mintDecimals: Number(process.env.DECIMALS ?? "6"),
        tokenSymbol: process.env.TOKEN_SYMBOL ?? "TOKEN",
        allowanceTokens: Number(process.env.ALLOWANCE ?? "1000"),
        credPath: process.env.OPERATOR_CRED_PATH ?? ".bastion-operator.json",
    };
}
