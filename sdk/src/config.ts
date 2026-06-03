import type { Address, Commitment, Rpc, SolanaRpcApi } from "@solana/kit";
import { BASTION_PROGRAM_ADDRESS } from "@bastion/generated";
import { BastionSdkError } from "./errors";
import type { BastionHooks } from "./hooks";
import type { LoggerConfig } from "./logger";
import type { WalletSigner } from "./wallet";

export interface BastionConfig {
    rpc: Rpc<SolanaRpcApi>;
    wallet: WalletSigner;
    programId?: Address;
    commitment?: Commitment;
    hooks?: BastionHooks;
    logger?: LoggerConfig;
    advanced?: AdvancedConfig;
    plugins?: readonly BastionPlugin[];
}

export interface AdvancedConfig {
    readCommitment?: Commitment;
    policyScanLimit?: number;
}

export interface BastionPlugin {
    readonly name: string;
    setup?(ctx: PluginContext): void;
}

export type BastionHookKey = keyof BastionHooks;

export interface PluginContext {
    registerHook<K extends BastionHookKey>(
        kind: K,
        fn: NonNullable<BastionHooks[K]>
    ): void;
}

export interface ResolvedBastionConfig extends BastionConfig {
    programId: Address;
    commitment: Commitment;
    advanced: Required<AdvancedConfig>;
}

export function validateConfig(config: BastionConfig): ResolvedBastionConfig {
    if (!config.rpc) {
        throw new BastionSdkError({
            code: "InvalidConfig",
            message: "BastionConfig.rpc is required",
        });
    }
    if (!config.wallet?.address) {
        throw new BastionSdkError({
            code: "InvalidConfig",
            message: "BastionConfig.wallet (TransactionSigner) is required",
        });
    }
    const commitment = config.commitment ?? "confirmed";
    return {
        ...config,
        programId: config.programId ?? BASTION_PROGRAM_ADDRESS,
        commitment,
        advanced: {
            readCommitment: config.advanced?.readCommitment ?? commitment,
            policyScanLimit: config.advanced?.policyScanLimit ?? 1000,
        },
    };
}
