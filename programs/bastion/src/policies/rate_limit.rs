use anchor_lang::prelude::*;

use crate::state::counter::CounterState;
use crate::state::policy::WindowKind;

/// pre-CPI mutation hook for `RateLimit`.
///
/// Returns mutated `CounterState`. Caller is responsible for writing it back
/// to the on-chain Policy account.
///
/// If `scope` is set, the policy only applies when `program_id == scope`;
/// otherwise the call is not counted at all.
pub fn charge_rate_limit(
    state: &mut CounterState,
    window: &WindowKind,
    max: u32,
    scope: &Option<Pubkey>,
    program_id: &Pubkey,
    now: i64,
) -> Result<()> {
    if let Some(s) = scope {
        if s != program_id {
            return Ok(());
        }
    }
    match window {
        WindowKind::Fixed { secs } => state.charge_fixed(now, max, *secs),
        WindowKind::Rolling { secs, slots } => state.charge_rolling(now, max, *secs, *slots),
    }
}
