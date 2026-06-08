import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MintBlocklistInput {
    mints: Address[];
}

/** Token instructions touching any of `mints` are rejected. */
export const MintBlocklist = (input: MintBlocklistInput): PolicyDataArgs =>
    policyData("MintBlocklist", { mints: input.mints });
