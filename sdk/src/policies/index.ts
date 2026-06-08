// bastion/policies — ergonomic builders for every policy kind.
//
// Each factory takes a single object and returns a `PolicyDataArgs` ready for
// `handle.attach(...)` / `handle.attachMany([...])`. Runtime state (counters,
// spend totals, timestamps) is filled empty by default, so you never pass it.
//
//   import { SpendCap, RateLimit, asset, window } from "bastion/policies";
//   import { sol, days, minutes } from "bastion/units";
//
//   handle.attachMany([
//     SpendCap({ window: window.fixed(days(1)), max: sol(10) }),
//     RateLimit({ window: window.fixed(minutes(1)), max: 5 }),
//   ]);

// Builders for the value types policy inputs take.
export { asset } from "./asset";
export { window } from "./window";

// Empty-state defaults (exposed for advanced/manual construction + tests).
export { freshCounterState, freshSpendState } from "./state";

// Policy factories (one per kind) + their `*Input` interfaces.
export * from "./amount-per-call";
export * from "./cooldown-period";
export * from "./expiry";
export * from "./foreign-signer-not-allowed";
export * from "./ix-discriminator-allowlist";
export * from "./max-calls-total";
export * from "./max-compute-units";
export * from "./max-ix-size";
export * from "./max-priority-fee";
export * from "./min-delegate-balance";
export * from "./mint-allowlist";
export * from "./mint-blocklist";
export * from "./nft-collection-allowlist";
export * from "./nft-collection-blocklist";
export * from "./nft-creator-allowlist";
export * from "./no-account-close";
export * from "./per-counterparty-cap";
export * from "./per-program-spend-cap";
export * from "./program-allowlist";
export * from "./program-blocklist";
export * from "./rate-limit";
export * from "./require-memo";
export * from "./safe-defaults";
export * from "./spend-cap";
export * from "./time-of-day-window";
export * from "./token-authority-guard";

// Type guards for narrowing decoded policy data.
export { isPolicyData, isAsset, isWindowKind } from "../generated";

// Policy-related data types.
export type {
    PolicyData,
    PolicyDataArgs,
    Asset,
    AssetArgs,
    WindowKind,
    WindowKindArgs,
    CounterState,
    CounterStateArgs,
    SpendState,
    SpendStateArgs,
} from "../generated";
