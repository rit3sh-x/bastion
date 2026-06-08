import type { PolicyDataArgs } from "../generated";
import { NoAccountClose } from "./no-account-close";
import { TokenAuthorityGuard } from "./token-authority-guard";

/**
 * Deny-by-default authority lockdown bundle.
 *
 * Spend caps measure a token account's `amount` before and after a CPI. Three
 * SPL / Token-2022 instructions move *authority* without moving `amount`, so a
 * cap never fires and the funds leave on a later, un-gated transaction:
 *
 * - `Approve` (4) / `ApproveChecked` (13) — grant a new SPL delegate.
 * - `SetAuthority` (6) — hand over owner / close authority.
 * - `CloseAccount` (9) — drain rent + force-close.
 *
 * This bundle blocks all four:
 * - `TokenAuthorityGuard` → Approve, ApproveChecked, SetAuthority.
 * - `NoAccountClose` → CloseAccount.
 *
 * Both are stateless, so the bundle is valid inside a holder-signed manifest.
 * Attach with `handle.attachMany(safeDefaultPolicies())`, or pass it to
 * `openSession({ policies: safeDefaultPolicies() })`. Recommended whenever the
 * operator (session) key is shipped to a semi-trusted party. A fresh array is
 * returned each call so callers may safely spread or mutate it.
 */
export function safeDefaultPolicies(): PolicyDataArgs[] {
    return [TokenAuthorityGuard(), NoAccountClose()];
}
