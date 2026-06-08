import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

/** Reject calls whose inner instruction requires a foreign signer. Stateless. */
export const ForeignSignerNotAllowed = (): PolicyDataArgs =>
    policyData("ForeignSignerNotAllowed");
