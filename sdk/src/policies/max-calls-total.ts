import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MaxCallsTotalInput {
    /** Total calls allowed over the session's lifetime. */
    max: bigint;
}

/** Lifetime cap on total calls. `used` starts at 0. */
export const MaxCallsTotal = (input: MaxCallsTotalInput): PolicyDataArgs =>
    policyData("MaxCallsTotal", { max: input.max, used: 0n });
