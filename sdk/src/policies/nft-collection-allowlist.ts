import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface NftCollectionAllowlistInput {
    collections: Address[];
}

/** Only NFTs from one of `collections` may be touched. */
export const NftCollectionAllowlist = (
    input: NftCollectionAllowlistInput
): PolicyDataArgs =>
    policyData("NftCollectionAllowlist", { collections: input.collections });
