import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MaxPriorityFeeInput {
    /** Max priority fee in micro-lamports per compute unit. */
    maxMicroLamports: bigint;
}

/** Cap the transaction's priority fee. Stateless. */
export const MaxPriorityFee = (input: MaxPriorityFeeInput): PolicyDataArgs =>
    policyData("MaxPriorityFee", { maxMicroLamports: input.maxMicroLamports });
