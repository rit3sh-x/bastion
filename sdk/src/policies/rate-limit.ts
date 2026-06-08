import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs, WindowKindArgs } from "../generated";
import { freshCounterState } from "./state";

export interface RateLimitInput {
    /** Window the call counter resets on. Build with `window.fixed/rolling`. */
    window: WindowKindArgs;
    /** Max calls allowed within the window. */
    max: number;
    /** Optional: meter calls per this counterparty instead of globally. */
    scope?: Address;
}

/** Cap the number of calls within a rolling/fixed window. State starts empty. */
export const RateLimit = (input: RateLimitInput): PolicyDataArgs =>
    policyData("RateLimit", {
        window: input.window,
        max: input.max,
        state: freshCounterState(),
        scope: input.scope ?? null,
    });
