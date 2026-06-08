import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface MaxComputeUnitsInput {
    /** Max compute-unit limit the transaction may request. */
    max: number;
}

/** Cap the transaction's requested compute-unit limit. Stateless. */
export const MaxComputeUnits = (input: MaxComputeUnitsInput): PolicyDataArgs =>
    policyData("MaxComputeUnits", { max: input.max });
