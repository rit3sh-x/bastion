import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface ExpiryInput {
    /** Unix timestamp (seconds) after which calls are rejected. */
    notAfter: bigint;
}

/** Reject all calls after a deadline. Stateless. */
export const Expiry = (input: ExpiryInput): PolicyDataArgs =>
    policyData("Expiry", { notAfter: input.notAfter });
