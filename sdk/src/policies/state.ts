import type { CounterStateArgs, SpendStateArgs } from "../generated";

/**
 * Fresh, zeroed runtime state for counter-based policies (RateLimit). Returned
 * as a new object each call so no two policies share a mutable `ring` array.
 * Policy factories fill this in by default — callers never pass state.
 */
export const freshCounterState = (): CounterStateArgs => ({
    lastReset: 0n,
    count: 0,
    ring: [0, 0, 0, 0, 0, 0, 0, 0],
});

/**
 * Fresh, zeroed runtime state for spend-based policies (SpendCap,
 * PerProgramSpendCap). New object each call (see {@link freshCounterState}).
 */
export const freshSpendState = (): SpendStateArgs => ({
    lastReset: 0n,
    spent: 0n,
    ring: [0n, 0n, 0n, 0n, 0n, 0n, 0n, 0n],
});
