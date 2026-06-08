import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface NftCollectionBlocklistInput {
    collections: Address[];
}

/** NFTs from any of `collections` are rejected. */
export const NftCollectionBlocklist = (
    input: NftCollectionBlocklistInput
): PolicyDataArgs =>
    policyData("NftCollectionBlocklist", { collections: input.collections });
