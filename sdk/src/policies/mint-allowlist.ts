import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MintAllowlistInput {
    mints: Address[];
}

/** Only token instructions touching one of `mints` may execute. */
export const MintAllowlist = (input: MintAllowlistInput): PolicyDataArgs =>
    policyData("MintAllowlist", { mints: input.mints });
