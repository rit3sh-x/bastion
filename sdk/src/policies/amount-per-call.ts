import { policyData } from "../generated";
import type { AssetArgs, PolicyDataArgs } from "../generated";
import { asset } from "./asset";

export interface AmountPerCallInput {
    /** Max base units movable in a single call. */
    max: bigint;
    /** Asset to meter. Defaults to native SOL. Build with `asset.*`. */
    asset?: AssetArgs;
}

/** Cap how much of an asset a single call may move. Stateless. */
export const AmountPerCall = (input: AmountPerCallInput): PolicyDataArgs =>
    policyData("AmountPerCall", {
        asset: input.asset ?? asset.sol(),
        max: input.max,
    });
