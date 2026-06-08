import { windowKind } from "../generated";
import type { WindowKindArgs } from "../generated";

/**
 * Ergonomic builders for the rolling/fixed window a rate-limit or spend-cap
 * resets on.
 *
 * ```ts
 * RateLimit({ window: window.fixed(minutes(1)), max: 10 })
 * SpendCap({ asset: asset.sol(), window: window.rolling(hours(1), 4), max })
 * ```
 */
export const window = {
    fixed: (secs: number): WindowKindArgs => windowKind("Fixed", { secs }),
    rolling: (secs: number, slots: number): WindowKindArgs =>
        windowKind("Rolling", { secs, slots }),
};
