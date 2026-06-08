import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface ProgramAllowlistInput {
    programs: Address[];
}

/** Only instructions targeting one of `programs` may execute. */
export const ProgramAllowlist = (
    input: ProgramAllowlistInput
): PolicyDataArgs =>
    policyData("ProgramAllowlist", { programs: input.programs });
