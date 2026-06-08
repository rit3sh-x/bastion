import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface RequireMemoInput {
    /** Address of the memo program that must be present in the transaction. */
    memoProgram: Address;
}

/** Require a memo instruction accompany the call. Stateless. */
export const RequireMemo = (input: RequireMemoInput): PolicyDataArgs =>
    policyData("RequireMemo", { memoProgram: input.memoProgram });
