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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BastionError;
    use crate::state::counter::CounterState;
    use crate::state::policy::WindowKind;
    use crate::utils::general::assert_anchor_error;

    #[test]
    fn no_scope_fixed_charges() {
        let mut state = CounterState::default();

        charge_rate_limit(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            3,
            &None,
            &Pubkey::new_unique(),
            1_000,
        )
        .unwrap();

        assert_eq!(state.count, 1);
    }

    #[test]
    fn matching_scope_charges() {
        let program = Pubkey::new_unique();
        let mut state = CounterState::default();

        charge_rate_limit(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            3,
            &Some(program),
            &program,
            1_000,
        )
        .unwrap();

        assert_eq!(state.count, 1);
    }

    #[test]
    fn non_matching_scope_is_noop() {
        let scope = Pubkey::new_unique();
        let program = Pubkey::new_unique();

        let mut state = CounterState::default();

        charge_rate_limit(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            3,
            &Some(scope),
            &program,
            1_000,
        )
        .unwrap();

        assert_eq!(state.count, 0);
        assert_eq!(state.last_reset, 0);
    }

    #[test]
    fn fixed_window_propagates_rate_limit_error() {
        let mut state = CounterState::default();

        charge_rate_limit(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            1,
            &None,
            &Pubkey::new_unique(),
            1_000,
        )
        .unwrap();

        let res = charge_rate_limit(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            1,
            &None,
            &Pubkey::new_unique(),
            1_001,
        );

        assert_anchor_error(res, BastionError::RateLimitExceeded);
    }

    #[test]
    fn rolling_window_charges() {
        let mut state = CounterState::default();

        charge_rate_limit(
            &mut state,
            &WindowKind::Rolling { secs: 60, slots: 6 },
            5,
            &None,
            &Pubkey::new_unique(),
            1_000,
        )
        .unwrap();

        assert_eq!(state.count, 1);
    }

    #[test]
    fn rolling_window_propagates_rate_limit_error() {
        let mut state = CounterState::default();

        charge_rate_limit(
            &mut state,
            &WindowKind::Rolling { secs: 60, slots: 6 },
            1,
            &None,
            &Pubkey::new_unique(),
            1_000,
        )
        .unwrap();

        let res = charge_rate_limit(
            &mut state,
            &WindowKind::Rolling { secs: 60, slots: 6 },
            1,
            &None,
            &Pubkey::new_unique(),
            1_001,
        );

        assert_anchor_error(res, BastionError::RateLimitExceeded);
    }
}
