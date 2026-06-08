import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MaxIxSizeInput {
    /** Max account metas the wrapped instruction may carry. */
    maxAccounts: number;
    /** Max instruction data length in bytes. */
    maxDataLen: number;
}

/** Bound the account count and data size of a wrapped instruction. Stateless. */
export const MaxIxSize = (input: MaxIxSizeInput): PolicyDataArgs =>
    policyData("MaxIxSize", {
        maxAccounts: input.maxAccounts,
        maxDataLen: input.maxDataLen,
    });
