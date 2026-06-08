import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface ProgramBlocklistInput {
    programs: Address[];
}

/** Instructions targeting any of `programs` are rejected. */
export const ProgramBlocklist = (
    input: ProgramBlocklistInput
): PolicyDataArgs =>
    policyData("ProgramBlocklist", { programs: input.programs });
