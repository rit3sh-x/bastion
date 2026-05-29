import {
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    type Rpc,
    type RpcSubscriptions,
    type SolanaRpcApi,
    type SolanaRpcSubscriptionsApi,
} from "@solana/kit";
import {
    validateConfig,
    type BastionConfig,
    type BastionHookKey,
    type ResolvedBastionConfig,
} from "./config";
import { BastionSdkError } from "./errors";
import type {
    AfterContext,
    BastionHooks,
    BeforeContext,
    ErrorContext,
} from "./hooks";
import { createLogger, type Logger } from "./logger";
import { createSessionManager, type BastionSessionManager } from "./session";

export interface Bastion {
    readonly session: BastionSessionManager;
    readonly config: Readonly<ResolvedBastionConfig>;
    readonly logger: Logger;
}

export interface CreateBastionConfigFull extends BastionConfig {
    rpcSubscriptions?: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
}

export interface CreateBastionConfigByUrl extends Omit<BastionConfig, "rpc"> {
    url: string;
    wsUrl?: string;
}

export type CreateBastionConfig =
    | CreateBastionConfigFull
    | CreateBastionConfigByUrl;

type AnyHookFn = (
    ctx: BeforeContext | AfterContext | ErrorContext
) => void | Promise<void>;

export function createBastion(config: CreateBastionConfig): Bastion {
    const { rpc, rpcSubscriptions, base } = resolveTransports(config);
    const resolved = validateConfig({ ...base, rpc });
    const logger = createLogger(resolved.logger);

    const pluginHooks: Record<BastionHookKey, AnyHookFn[]> = {
        before: [],
        after: [],
        error: [],
    };
    for (const plugin of resolved.plugins ?? []) {
        plugin.setup?.({
            registerHook(kind, fn) {
                pluginHooks[kind].push(fn as unknown as AnyHookFn);
            },
        });
    }

    const composed: BastionHooks = {};
    if (resolved.hooks?.before || pluginHooks.before.length > 0) {
        composed.before = async (ctx) => {
            if (resolved.hooks?.before) await resolved.hooks.before(ctx);
            for (const fn of pluginHooks.before) await fn(ctx);
        };
    }
    if (resolved.hooks?.after || pluginHooks.after.length > 0) {
        composed.after = async (ctx) => {
            if (resolved.hooks?.after) await resolved.hooks.after(ctx);
            for (const fn of pluginHooks.after) await fn(ctx);
        };
    }
    if (resolved.hooks?.error || pluginHooks.error.length > 0) {
        composed.error = async (ctx) => {
            if (resolved.hooks?.error) await resolved.hooks.error(ctx);
            for (const fn of pluginHooks.error) await fn(ctx);
        };
    }

    const session = createSessionManager(
        { ...resolved, hooks: composed },
        rpcSubscriptions,
        logger
    );
    return { session, config: resolved, logger };
}

function resolveTransports(config: CreateBastionConfig): {
    rpc: Rpc<SolanaRpcApi>;
    rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi> | undefined;
    base: BastionConfig;
} {
    if ("url" in config) {
        const wsUrl = config.wsUrl ?? deriveWsUrl(config.url);
        const { url: _url, wsUrl: _ws, ...rest } = config;
        return {
            rpc: createSolanaRpc(config.url) as unknown as Rpc<SolanaRpcApi>,
            rpcSubscriptions: createSolanaRpcSubscriptions(wsUrl),
            base: rest as BastionConfig,
        };
    }
    return {
        rpc: config.rpc,
        rpcSubscriptions: config.rpcSubscriptions,
        base: config,
    };
}

function deriveWsUrl(httpUrl: string): string {
    if (httpUrl.startsWith("https://")) return "wss://" + httpUrl.slice(8);
    if (httpUrl.startsWith("http://")) return "ws://" + httpUrl.slice(7);
    throw new BastionSdkError({
        code: "InvalidConfig",
        message: `Cannot derive ws URL from "${httpUrl}". Supply config.wsUrl explicitly.`,
    });
}
