import { address, type Address } from "@solana/kit";
import {
    AmountPerCall,
    asset,
    CooldownPeriod,
    MaxCallsTotal,
    MaxComputeUnits,
    MaxPriorityFee,
    SpendCap,
    window,
    type PolicyDataArgs,
} from "bastion/policies";
import { days, hours, microLamports, sol, tokens } from "bastion/units";

import type { Env } from "./env";

export const LIMITS = {
    sol: { perTrade: 0.1, lifetime: 1 },
    token: { perTrade: 10, lifetime: 100 },
    totalCalls: 10n,
    sessionDurationSecs: hours(1),
    cooldownSecs: 5,
    cuLimit: 400_000,
    priorityFeeCap: microLamports(50_000),
} as const;

export interface SpendMode {
    mint?: Address;
    decimals: number;
    symbol: string;
}

export function resolveSpendMode(env: Env): SpendMode {
    if (env.mint) {
        return {
            mint: address(env.mint),
            decimals: env.mintDecimals,
            symbol: env.tokenSymbol,
        };
    }
    return { decimals: 9, symbol: "SOL" };
}

export interface ActiveCaps {
    perTrade: number;
    lifetime: number;
    unit: string;
}

export function activeCaps(mode: SpendMode): ActiveCaps {
    const caps = mode.mint ? LIMITS.token : LIMITS.sol;
    return {
        perTrade: caps.perTrade,
        lifetime: caps.lifetime,
        unit: mode.symbol,
    };
}

export function buildPolicies(mode: SpendMode): PolicyDataArgs[] {
    const spendAsset = mode.mint ? asset.splToken(mode.mint) : asset.sol();
    const toBase = (whole: number): bigint =>
        mode.mint ? tokens(whole, mode.decimals) : sol(whole);
    const caps = mode.mint ? LIMITS.token : LIMITS.sol;

    return [
        SpendCap({
            asset: spendAsset,
            window: window.fixed(days(1)),
            max: toBase(caps.lifetime),
        }),
        AmountPerCall({ asset: spendAsset, max: toBase(caps.perTrade) }),
        MaxCallsTotal({ max: LIMITS.totalCalls }),
        CooldownPeriod({ secs: LIMITS.cooldownSecs }),
        MaxPriorityFee({ maxMicroLamports: LIMITS.priorityFeeCap }),
        MaxComputeUnits({ max: LIMITS.cuLimit }),
    ];
}
