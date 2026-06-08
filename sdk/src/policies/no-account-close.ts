import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

/** Block SPL/Token-2022 `CloseAccount`. Stateless. */
export const NoAccountClose = (): PolicyDataArgs =>
    policyData("NoAccountClose");
