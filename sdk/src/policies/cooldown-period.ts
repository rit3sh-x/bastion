import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface CooldownPeriodInput {
    /** Minimum seconds between calls. */
    secs: number;
    /** Optional: enforce the cooldown per this counterparty instead of globally. */
    scope?: Address;
}

/** Enforce a minimum delay between calls. `lastCallTs` starts at 0. */
export const CooldownPeriod = (input: CooldownPeriodInput): PolicyDataArgs =>
    policyData("CooldownPeriod", {
        secs: input.secs,
        lastCallTs: 0n,
        scope: input.scope ?? null,
    });
