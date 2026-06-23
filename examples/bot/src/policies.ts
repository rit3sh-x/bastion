import { type Address } from "@solana/kit";
import {
    AmountPerCall,
    asset,
    CooldownPeriod,
    MaxCallsTotal,
    MaxComputeUnits,
    MaxPriorityFee,
    ProgramAllowlist,
    SpendCap,
    window,
    type PolicyDataArgs,
} from "bastion/policies";
import { days, hours, microLamports, sol, tokens } from "bastion/units";
import { TOKEN_PROGRAM_ADDRESS } from "bastion/token";

const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;

export const LIMITS = {
    sol: { perTrade: 0.1, lifetime: 1 },
    token: { perTrade: 10, lifetime: 100 },
    totalCalls: 20n,
    sessionDurationSecs: hours(1),
    cooldownSecs: 5,
    cuLimit: 400_000,
    priorityFeeCap: microLamports(50_000),
} as const;

export function buildPolicies(
    mint: Address,
    decimals: number
): PolicyDataArgs[] {
    return [
        ProgramAllowlist({
            programs: [SYSTEM_PROGRAM_ADDRESS, TOKEN_PROGRAM_ADDRESS],
        }),
        SpendCap({
            asset: asset.sol(),
            window: window.fixed(days(1)),
            max: sol(LIMITS.sol.lifetime),
        }),
        AmountPerCall({ asset: asset.sol(), max: sol(LIMITS.sol.perTrade) }),
        SpendCap({
            asset: asset.splToken(mint),
            window: window.fixed(days(1)),
            max: tokens(LIMITS.token.lifetime, decimals),
        }),
        AmountPerCall({
            asset: asset.splToken(mint),
            max: tokens(LIMITS.token.perTrade, decimals),
        }),
        MaxCallsTotal({ max: LIMITS.totalCalls }),
        CooldownPeriod({ secs: LIMITS.cooldownSecs }),
        MaxPriorityFee({ maxMicroLamports: LIMITS.priorityFeeCap }),
        MaxComputeUnits({ max: LIMITS.cuLimit }),
    ];
}
