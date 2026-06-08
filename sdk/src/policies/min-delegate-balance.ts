import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MinDelegateBalanceInput {
    /** Minimum lamports the delegate must retain after the call. */
    floor: bigint;
}

/** Require the delegate keep at least `floor` lamports. Stateless. */
export const MinDelegateBalance = (
    input: MinDelegateBalanceInput
): PolicyDataArgs => policyData("MinDelegateBalance", { floor: input.floor });
