import type { Address, ReadonlyUint8Array } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface IxDiscriminatorAllowlistInput {
    /** Program whose instructions are being filtered. */
    program: Address;
    /** Allowed 8-byte instruction discriminators. */
    discriminators: ReadonlyUint8Array[];
}

/** Restrict a program to a set of instruction discriminators. Stateless. */
export const IxDiscriminatorAllowlist = (
    input: IxDiscriminatorAllowlistInput
): PolicyDataArgs =>
    policyData("IxDiscriminatorAllowlist", {
        program: input.program,
        discriminators: input.discriminators,
    });
