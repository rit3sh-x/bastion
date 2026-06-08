import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { AssetArgs, PolicyDataArgs, WindowKindArgs } from "../generated";
import { asset } from "./asset";
import { freshSpendState } from "./state";

export interface PerProgramSpendCapInput {
    /** Program the cap applies to. */
    program: Address;
    /** Window the spent total resets on. Build with `window.fixed/rolling`. */
    window: WindowKindArgs;
    /** Max base units spendable through `program` within the window. */
    max: bigint;
    /** Asset to meter. Defaults to native SOL. Build with `asset.*`. */
    asset?: AssetArgs;
}

/** Cap spend of an asset routed through one program. State starts empty. */
export const PerProgramSpendCap = (
    input: PerProgramSpendCapInput
): PolicyDataArgs =>
    policyData("PerProgramSpendCap", {
        program: input.program,
        asset: input.asset ?? asset.sol(),
        window: input.window,
        max: input.max,
        state: freshSpendState(),
    });
