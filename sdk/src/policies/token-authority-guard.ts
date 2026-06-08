import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

/** Block SPL/Token-2022 `Approve`/`ApproveChecked`/`SetAuthority`. Stateless. */
export const TokenAuthorityGuard = (): PolicyDataArgs =>
    policyData("TokenAuthorityGuard");
