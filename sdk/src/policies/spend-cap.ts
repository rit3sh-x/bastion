import { policyData } from "../generated";
import type { AssetArgs, PolicyDataArgs, WindowKindArgs } from "../generated";
import { asset } from "./asset";
import { freshSpendState } from "./state";

export interface SpendCapInput {
    /** Window the spent total resets on. Build with `window.fixed/rolling`. */
    window: WindowKindArgs;
    /** Max base units spendable within the window. */
    max: bigint;
    /** Asset to meter. Defaults to native SOL. Build with `asset.*`. */
    asset?: AssetArgs;
}

/** Cap total spend of an asset within a window. State starts empty. */
export const SpendCap = (input: SpendCapInput): PolicyDataArgs =>
    policyData("SpendCap", {
        asset: input.asset ?? asset.sol(),
        window: input.window,
        max: input.max,
        state: freshSpendState(),
    });
