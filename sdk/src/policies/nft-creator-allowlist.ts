import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface NftCreatorAllowlistInput {
    creators: Address[];
}

/** Only NFTs whose verified creator is in `creators` may be touched. */
export const NftCreatorAllowlist = (
    input: NftCreatorAllowlistInput
): PolicyDataArgs =>
    policyData("NftCreatorAllowlist", { creators: input.creators });
