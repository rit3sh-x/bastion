use anchor_lang::prelude::*;

use crate::{
    constants::{MAX_DISCRIMINATOR_LEN, MAX_PROGRAMS_PER_LIST},
    error::BastionError,
    policies::{
        expiry::check_expiry,
        ix_discriminator_allowlist::check_ix_discriminator_allowlist,
        max_compute_units::check_max_compute_units,
        max_ix_size::check_max_ix_size,
        max_priority_fee::check_max_priority_fee,
        mint_allowlist::{check_mint_allowlist, check_mint_blocklist},
        nft_collection::{check_nft_collection_allowlist, check_nft_collection_blocklist},
        nft_creator_allowlist::check_nft_creator_allowlist,
        no_account_close::check_no_account_close,
        program_allowlist::{check_program_allowlist, check_program_blocklist},
        require_memo::check_require_memo,
        time_of_day::check_time_of_day,
        token_authority_guard::check_token_authority_guard,
    },
    state::counter::{CounterState, SpendState},
    state::wrapped_ix::WrappedInstruction,
};

/// PolicyKind discriminant byte. Stored separately in `Policy.kind` so off-chain
/// clients can filter via `getProgramAccounts` memcmp at a fixed offset without
/// deserialising `PolicyData`.
///
/// MUST stay in lock-step with `PolicyData` variant order.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
pub enum PolicyKind {
    /// Sentinel for a freshly-created, not-yet-written policy account. MUST stay
    /// at discriminant 0, in lock-step with `PolicyData::Uninitialized`.
    Uninitialized = 0,
    ProgramAllowlist = 1,
    ProgramBlocklist = 2,
    MintAllowlist = 3,
    MintBlocklist = 4,
    NftCollectionAllowlist = 5,
    NftCollectionBlocklist = 6,
    RateLimit = 7,
    SpendCap = 8,
    Expiry = 9,
    ForeignSignerNotAllowed = 10,
    CooldownPeriod = 11,
    AmountPerCall = 12,
    MaxCallsTotal = 13,
    TimeOfDayWindow = 14,
    MaxIxSize = 15,
    NftCreatorAllowlist = 16,
    MinDelegateBalance = 17,
    IxDiscriminatorAllowlist = 18,
    RequireMemo = 19,
    NoAccountClose = 20,
    PerCounterpartyCap = 21,
    PerProgramSpendCap = 22,
    MaxComputeUnits = 23,
    MaxPriorityFee = 24,
    TokenAuthorityGuard = 25,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Fixed { secs: u32 },
    Rolling { secs: u32, slots: u8 },
}

/// A counter/spend window is well-formed only with a non-zero duration (and, for
/// Rolling, a slot count in `1..=MAX_RING_SLOTS`). A zero-second window expires
/// on every call, so the charge logic resets each time and the cap enforces
/// nothing — reject it at attach/update.
fn window_valid(w: &WindowKind) -> bool {
    match w {
        WindowKind::Fixed { secs } => *secs >= 1,
        WindowKind::Rolling { secs, slots } => {
            *secs >= 1 && *slots >= 1 && usize::from(*slots) <= crate::constants::MAX_RING_SLOTS
        }
    }
}

/// On an `update_policy` window reshape, re-express the carried counter so the
/// new enforcer reads the consumed budget: Fixed keeps it in `count`,
/// Rolling in `sum(ring)`. Same-shape edits are a no-op (raw carry is correct).
fn rewindow_counter(
    s: &mut crate::state::counter::CounterState,
    old: &WindowKind,
    new: &WindowKind,
) {
    match (old, new) {
        (WindowKind::Fixed { .. }, WindowKind::Rolling { slots, .. }) => {
            s.seed_ring_from_count(*slots)
        }
        (WindowKind::Rolling { .. }, WindowKind::Fixed { .. }) => s.collapse_ring_to_count(),
        _ => {}
    }
}

/// `rewindow_counter` for the SpendCap/PerProgramSpendCap `SpendState`.
fn rewindow_spend(s: &mut crate::state::counter::SpendState, old: &WindowKind, new: &WindowKind) {
    match (old, new) {
        (WindowKind::Fixed { .. }, WindowKind::Rolling { slots, .. }) => {
            s.seed_ring_from_spent(*slots)
        }
        (WindowKind::Rolling { .. }, WindowKind::Fixed { .. }) => s.collapse_ring_to_spent(),
        _ => {}
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum Asset {
    NativeSol,
    SplToken(Pubkey),
    Token2022(Pubkey),
    NftCountInCollection(Pubkey),
    AnyNftCount,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum PolicyData {
    Uninitialized,
    ProgramAllowlist {
        programs: Vec<Pubkey>,
    },
    ProgramBlocklist {
        programs: Vec<Pubkey>,
    },
    MintAllowlist {
        mints: Vec<Pubkey>,
    },
    MintBlocklist {
        mints: Vec<Pubkey>,
    },
    NftCollectionAllowlist {
        collections: Vec<Pubkey>,
    },
    NftCollectionBlocklist {
        collections: Vec<Pubkey>,
    },
    RateLimit {
        window: WindowKind,
        max: u32,
        state: CounterState,
        scope: Option<Pubkey>,
    },
    SpendCap {
        asset: Asset,
        window: WindowKind,
        max: u64,
        state: SpendState,
    },
    Expiry {
        not_after: i64,
    },
    ForeignSignerNotAllowed,
    CooldownPeriod {
        secs: u32,
        last_call_ts: i64,
        scope: Option<Pubkey>,
    },
    AmountPerCall {
        asset: Asset,
        max: u64,
    },
    MaxCallsTotal {
        max: u64,
        used: u64,
    },
    TimeOfDayWindow {
        start_minute: u16,
        end_minute: u16,
        days_mask: u8,
    },
    MaxIxSize {
        max_accounts: u8,
        max_data_len: u16,
    },
    NftCreatorAllowlist {
        creators: Vec<Pubkey>,
    },
    MinDelegateBalance {
        floor: u64,
    },
    IxDiscriminatorAllowlist {
        program: Pubkey,
        discriminators: Vec<Vec<u8>>,
    },
    RequireMemo {
        memo_program: Pubkey,
    },
    NoAccountClose,
    PerCounterpartyCap {
        receiver: Pubkey,
        asset: Asset,
        max: u64,
        sent: u64,
    },
    PerProgramSpendCap {
        program: Pubkey,
        asset: Asset,
        window: WindowKind,
        max: u64,
        state: SpendState,
    },
    MaxComputeUnits {
        max: u32,
    },
    MaxPriorityFee {
        max_micro_lamports: u64,
    },
    TokenAuthorityGuard,
}

impl PolicyData {
    pub fn validate<'info>(
        &self,
        clock: &Clock,
        wrapped_ix: &WrappedInstruction,
        ix_accounts: &[AccountInfo<'info>],
        sysvar_ai: &AccountInfo<'info>,
    ) -> Result<()> {
        match self {
            PolicyData::Expiry { not_after } => check_expiry(*not_after, clock.unix_timestamp),
            PolicyData::IxDiscriminatorAllowlist {
                program,
                discriminators,
            } => check_ix_discriminator_allowlist(
                program,
                discriminators,
                &wrapped_ix.program_id,
                &wrapped_ix.data,
            ),
            PolicyData::MaxComputeUnits { max } => check_max_compute_units(*max, sysvar_ai),
            PolicyData::MaxIxSize {
                max_accounts,
                max_data_len,
            } => check_max_ix_size(
                wrapped_ix.accounts.len(),
                wrapped_ix.data.len(),
                *max_accounts,
                *max_data_len,
            ),
            PolicyData::MaxPriorityFee { max_micro_lamports } => {
                check_max_priority_fee(*max_micro_lamports, sysvar_ai)
            }
            PolicyData::MintAllowlist { mints } => check_mint_allowlist(mints, ix_accounts),
            PolicyData::MintBlocklist { mints } => check_mint_blocklist(mints, ix_accounts),
            PolicyData::NftCollectionAllowlist { collections } => {
                check_nft_collection_allowlist(collections, ix_accounts)
            }
            PolicyData::NftCollectionBlocklist { collections } => {
                check_nft_collection_blocklist(collections, ix_accounts)
            }
            PolicyData::NftCreatorAllowlist { creators } => {
                check_nft_creator_allowlist(creators, ix_accounts)
            }
            PolicyData::NoAccountClose => {
                check_no_account_close(&wrapped_ix.program_id, &wrapped_ix.data)
            }
            PolicyData::ProgramAllowlist { programs } => {
                check_program_allowlist(programs, &wrapped_ix.program_id)
            }
            PolicyData::ProgramBlocklist { programs } => {
                check_program_blocklist(programs, &wrapped_ix.program_id)
            }
            PolicyData::RequireMemo { memo_program } => check_require_memo(memo_program, sysvar_ai),
            PolicyData::TimeOfDayWindow {
                start_minute,
                end_minute,
                days_mask,
            } => check_time_of_day(clock.unix_timestamp, *start_minute, *end_minute, *days_mask),
            PolicyData::TokenAuthorityGuard => {
                check_token_authority_guard(&wrapped_ix.program_id, &wrapped_ix.data)
            }
            PolicyData::Uninitialized => Err(error!(BastionError::InvalidPolicyData)),
            PolicyData::AmountPerCall { .. }
            | PolicyData::CooldownPeriod { .. }
            | PolicyData::ForeignSignerNotAllowed
            | PolicyData::MaxCallsTotal { .. }
            | PolicyData::MinDelegateBalance { .. }
            | PolicyData::PerCounterpartyCap { .. }
            | PolicyData::PerProgramSpendCap { .. }
            | PolicyData::RateLimit { .. }
            | PolicyData::SpendCap { .. } => Ok(()),
        }
    }

    /// True for `validate()`-only policies — no persisted state, no pre/post
    /// balance delta. Only these may live in a signed manifest: the
    /// stateful kinds need on-chain accounts to track counters/spend.
    pub fn is_stateless(&self) -> bool {
        matches!(
            self,
            PolicyData::Expiry { .. }
                | PolicyData::ForeignSignerNotAllowed
                | PolicyData::IxDiscriminatorAllowlist { .. }
                | PolicyData::MaxComputeUnits { .. }
                | PolicyData::MaxIxSize { .. }
                | PolicyData::MaxPriorityFee { .. }
                | PolicyData::MintAllowlist { .. }
                | PolicyData::MintBlocklist { .. }
                | PolicyData::NftCollectionAllowlist { .. }
                | PolicyData::NftCollectionBlocklist { .. }
                | PolicyData::NftCreatorAllowlist { .. }
                | PolicyData::NoAccountClose
                | PolicyData::ProgramAllowlist { .. }
                | PolicyData::ProgramBlocklist { .. }
                | PolicyData::RequireMemo { .. }
                | PolicyData::TimeOfDayWindow { .. }
                | PolicyData::TokenAuthorityGuard
        )
    }

    pub fn kind(&self) -> PolicyKind {
        match self {
            PolicyData::Uninitialized => PolicyKind::Uninitialized,
            PolicyData::AmountPerCall { .. } => PolicyKind::AmountPerCall,
            PolicyData::CooldownPeriod { .. } => PolicyKind::CooldownPeriod,
            PolicyData::Expiry { .. } => PolicyKind::Expiry,
            PolicyData::ForeignSignerNotAllowed => PolicyKind::ForeignSignerNotAllowed,
            PolicyData::IxDiscriminatorAllowlist { .. } => PolicyKind::IxDiscriminatorAllowlist,
            PolicyData::MaxCallsTotal { .. } => PolicyKind::MaxCallsTotal,
            PolicyData::MaxComputeUnits { .. } => PolicyKind::MaxComputeUnits,
            PolicyData::MaxIxSize { .. } => PolicyKind::MaxIxSize,
            PolicyData::MaxPriorityFee { .. } => PolicyKind::MaxPriorityFee,
            PolicyData::MinDelegateBalance { .. } => PolicyKind::MinDelegateBalance,
            PolicyData::MintAllowlist { .. } => PolicyKind::MintAllowlist,
            PolicyData::MintBlocklist { .. } => PolicyKind::MintBlocklist,
            PolicyData::NftCollectionAllowlist { .. } => PolicyKind::NftCollectionAllowlist,
            PolicyData::NftCollectionBlocklist { .. } => PolicyKind::NftCollectionBlocklist,
            PolicyData::NftCreatorAllowlist { .. } => PolicyKind::NftCreatorAllowlist,
            PolicyData::NoAccountClose => PolicyKind::NoAccountClose,
            PolicyData::PerCounterpartyCap { .. } => PolicyKind::PerCounterpartyCap,
            PolicyData::PerProgramSpendCap { .. } => PolicyKind::PerProgramSpendCap,
            PolicyData::ProgramAllowlist { .. } => PolicyKind::ProgramAllowlist,
            PolicyData::ProgramBlocklist { .. } => PolicyKind::ProgramBlocklist,
            PolicyData::RateLimit { .. } => PolicyKind::RateLimit,
            PolicyData::RequireMemo { .. } => PolicyKind::RequireMemo,
            PolicyData::SpendCap { .. } => PolicyKind::SpendCap,
            PolicyData::TimeOfDayWindow { .. } => PolicyKind::TimeOfDayWindow,
            PolicyData::TokenAuthorityGuard => PolicyKind::TokenAuthorityGuard,
        }
    }

    pub fn serialized_len(&self) -> usize {
        borsh::object_length(self).unwrap_or(0)
    }

    pub fn normalize(&mut self) {
        match self {
            PolicyData::IxDiscriminatorAllowlist { discriminators, .. } => {
                discriminators.sort_unstable()
            }
            PolicyData::MintAllowlist { mints } | PolicyData::MintBlocklist { mints } => {
                mints.sort_unstable()
            }
            PolicyData::NftCollectionAllowlist { collections }
            | PolicyData::NftCollectionBlocklist { collections } => collections.sort_unstable(),
            PolicyData::NftCreatorAllowlist { creators } => creators.sort_unstable(),
            PolicyData::ProgramAllowlist { programs }
            | PolicyData::ProgramBlocklist { programs } => programs.sort_unstable(),
            _ => {}
        }
    }

    pub fn validate_attach_params(&self) -> anchor_lang::prelude::Result<()> {
        require!(
            !matches!(self, PolicyData::Uninitialized),
            BastionError::InvalidPolicyData
        );

        let asset_opt = match self {
            PolicyData::AmountPerCall { asset, .. }
            | PolicyData::PerCounterpartyCap { asset, .. }
            | PolicyData::PerProgramSpendCap { asset, .. }
            | PolicyData::SpendCap { asset, .. } => Some(asset),
            _ => None,
        };
        if let Some(Asset::AnyNftCount | Asset::NftCountInCollection(_)) = asset_opt {
            return Err(error!(BastionError::InvalidPolicyData));
        }

        match self {
            PolicyData::IxDiscriminatorAllowlist { discriminators, .. } => {
                require!(!discriminators.is_empty(), BastionError::InvalidPolicyData);
                require!(
                    discriminators.len() <= MAX_PROGRAMS_PER_LIST,
                    BastionError::ListTooLong
                );
                // Each entry is a 1..=MAX_DISCRIMINATOR_LEN byte leading prefix of
                // the inner ix data (the program's tag, optionally + leading arg
                // bytes to pin values). A zero-length entry would prefix-match
                // every instruction (allow-all bypass); over-long entries bloat
                // the account. Reject both.
                for d in discriminators {
                    require!(
                        (1..=MAX_DISCRIMINATOR_LEN).contains(&d.len()),
                        BastionError::InvalidPolicyData
                    );
                }
            }
            PolicyData::MaxCallsTotal { max, used } => {
                require!(*used == 0, BastionError::InvalidPolicyData);
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::MaxComputeUnits { max } => {
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::MaxIxSize {
                max_accounts,
                max_data_len,
            } => {
                require!(*max_accounts > 0, BastionError::InvalidPolicyData);
                require!(*max_data_len > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::MinDelegateBalance { floor } => {
                require!(*floor > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::NftCreatorAllowlist { creators } => {
                require!(!creators.is_empty(), BastionError::InvalidPolicyData);
                require!(
                    creators.len() <= MAX_PROGRAMS_PER_LIST,
                    BastionError::ListTooLong
                );
            }
            PolicyData::PerCounterpartyCap { sent, max, .. } => {
                require!(*sent == 0, BastionError::InvalidPolicyData);
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::PerProgramSpendCap { max, window, .. } => {
                require!(*max > 0, BastionError::InvalidPolicyData);
                require!(window_valid(window), BastionError::InvalidWindow);
            }
            PolicyData::RateLimit { window, .. } => {
                require!(window_valid(window), BastionError::InvalidWindow);
            }
            PolicyData::SpendCap { window, .. } => {
                require!(window_valid(window), BastionError::InvalidWindow);
            }
            PolicyData::CooldownPeriod { secs, .. } => {
                require!(*secs >= 1, BastionError::InvalidWindow);
            }
            PolicyData::TimeOfDayWindow {
                start_minute,
                end_minute,
                days_mask,
            } => {
                require!(*start_minute < *end_minute, BastionError::InvalidPolicyData);
                require!(*end_minute <= 1440, BastionError::InvalidPolicyData);
                require!(*days_mask != 0, BastionError::InvalidPolicyData);
                require!((*days_mask & 0x80) == 0, BastionError::InvalidPolicyData);
            }
            _ => {}
        }
        Ok(())
    }

    /// On `update_policy`, resume the existing policy's accumulated runtime state
    /// into the replacement config, so a config edit (e.g. raising `max`) never
    /// wipes it. Only the runtime counters/timestamps are carried; everything
    /// else comes from the new value. Caller guarantees `self.kind() ==
    /// old.kind()`; non-stateful kinds are a no-op.
    pub fn carry_state_from(&mut self, old: &PolicyData) {
        match (self, old) {
            (
                PolicyData::RateLimit {
                    state,
                    window: new_window,
                    ..
                },
                PolicyData::RateLimit {
                    state: old_state,
                    window: old_window,
                    ..
                },
            ) => {
                *state = *old_state;
                rewindow_counter(state, old_window, new_window);
            }
            (
                PolicyData::SpendCap {
                    state,
                    window: new_window,
                    ..
                },
                PolicyData::SpendCap {
                    state: old_state,
                    window: old_window,
                    ..
                },
            ) => {
                *state = *old_state;
                rewindow_spend(state, old_window, new_window);
            }
            (
                PolicyData::PerProgramSpendCap {
                    state,
                    window: new_window,
                    ..
                },
                PolicyData::PerProgramSpendCap {
                    state: old_state,
                    window: old_window,
                    ..
                },
            ) => {
                *state = *old_state;
                rewindow_spend(state, old_window, new_window);
            }
            (
                PolicyData::CooldownPeriod { last_call_ts, .. },
                PolicyData::CooldownPeriod {
                    last_call_ts: old_ts,
                    ..
                },
            ) => *last_call_ts = *old_ts,
            (
                PolicyData::MaxCallsTotal { used, .. },
                PolicyData::MaxCallsTotal { used: old_used, .. },
            ) => *used = *old_used,
            (
                PolicyData::PerCounterpartyCap { sent, .. },
                PolicyData::PerCounterpartyCap { sent: old_sent, .. },
            ) => *sent = *old_sent,
            _ => {}
        }
    }
}

/// NOTE: NO `#[derive(InitSpace)]` here. `Policy.data: PolicyData` is a
/// variable-size Borsh enum (see `PolicyData::serialized_len`), so a single
/// `INIT_SPACE` constant would be meaningless. Allocation is done via the
/// `Policy::size_for(&data)` function which sums Anchor discriminator +
/// fixed header + actual serialised data length.
#[account]
pub struct Policy {
    pub session: Pubkey,
    pub seed: u64,
    pub bump: u8,
    pub kind: u8,
    pub enabled: bool,
    pub created_at: i64,
    pub data: PolicyData,
}

impl Policy {
    pub const HEADER_LEN: usize = 32usize // session: Pubkey
        .checked_add(8).expect("Policy::HEADER_LEN overflow")  // seed: u64
        .checked_add(1).expect("Policy::HEADER_LEN overflow")  // bump: u8
        .checked_add(1).expect("Policy::HEADER_LEN overflow")  // kind: u8
        .checked_add(1).expect("Policy::HEADER_LEN overflow")  // enabled: bool
        .checked_add(8).expect("Policy::HEADER_LEN overflow"); // created_at: i64

    /// Total bytes (Anchor discriminator + header + serialised data) required to
    /// store a Policy whose `data` field equals the supplied value.
    ///
    /// No artificial floor is needed: Anchor's init-time decode of a zero account
    /// only ever reads PolicyData tag 0 (`Uninitialized`, a zero-payload unit
    /// variant = 1 byte), which fits in any account. The `.max(1)` only guards the
    /// unreachable `serialized_len() == 0` path from `object_length`'s error case.
    ///
    /// Uses `Self::DISCRIMINATOR.len()` instead of a literal `8` to stay in
    /// lock-step with Anchor's discriminator length. `saturating_add` is fine:
    /// 8 (disc) + 51 (HEADER_LEN) + ≤ ~32 KB of payload is far below `usize::MAX`.
    pub fn size_for(data: &PolicyData) -> usize {
        let data_len = data.serialized_len().max(1);
        Self::DISCRIMINATOR
            .len()
            .saturating_add(Self::HEADER_LEN)
            .saturating_add(data_len)
    }

    pub fn new(session: Pubkey, seed: u64, bump: u8, created_at: i64, data: PolicyData) -> Self {
        let kind = data.kind() as u8;
        Self {
            session,
            seed,
            bump,
            kind,
            enabled: true,
            created_at,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    fn assert_roundtrip(d: &PolicyData) {
        let bytes = borsh::to_vec(d).unwrap();
        let decoded = PolicyData::try_from_slice(&bytes).unwrap();
        assert_eq!(d, &decoded);
        assert_eq!(d.serialized_len(), bytes.len());
    }

    #[test]
    fn roundtrip_program_allowlist() {
        assert_roundtrip(&PolicyData::ProgramAllowlist {
            programs: vec![pk(1), pk(2), pk(3)],
        });
    }

    #[test]
    fn roundtrip_program_blocklist() {
        assert_roundtrip(&PolicyData::ProgramBlocklist {
            programs: vec![pk(1)],
        });
    }

    #[test]
    fn roundtrip_mint_allowlist() {
        assert_roundtrip(&PolicyData::MintAllowlist {
            mints: vec![pk(9), pk(10)],
        });
    }

    #[test]
    fn roundtrip_mint_blocklist() {
        assert_roundtrip(&PolicyData::MintBlocklist { mints: vec![pk(9)] });
    }

    #[test]
    fn roundtrip_nft_collection_allowlist() {
        assert_roundtrip(&PolicyData::NftCollectionAllowlist {
            collections: vec![pk(7), pk(8)],
        });
    }

    #[test]
    fn roundtrip_nft_collection_blocklist() {
        assert_roundtrip(&PolicyData::NftCollectionBlocklist {
            collections: vec![pk(7)],
        });
    }

    #[test]
    fn roundtrip_rate_limit_fixed_no_scope() {
        assert_roundtrip(&PolicyData::RateLimit {
            window: WindowKind::Fixed { secs: 60 },
            max: 3,
            state: CounterState::default(),
            scope: None,
        });
    }

    #[test]
    fn roundtrip_rate_limit_rolling_with_scope() {
        assert_roundtrip(&PolicyData::RateLimit {
            window: WindowKind::Rolling { secs: 60, slots: 6 },
            max: 10,
            state: CounterState::default(),
            scope: Some(pk(42)),
        });
    }

    #[test]
    fn roundtrip_spend_cap_native_sol() {
        assert_roundtrip(&PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86400 },
            max: 1_000_000_000,
            state: SpendState::default(),
        });
    }

    #[test]
    fn roundtrip_spend_cap_spl_token() {
        assert_roundtrip(&PolicyData::SpendCap {
            asset: Asset::SplToken(pk(99)),
            window: WindowKind::Fixed { secs: 86400 },
            max: 100_000_000,
            state: SpendState::default(),
        });
    }

    #[test]
    fn roundtrip_spend_cap_token_2022() {
        assert_roundtrip(&PolicyData::SpendCap {
            asset: Asset::Token2022(pk(99)),
            window: WindowKind::Rolling {
                secs: 3600,
                slots: 4,
            },
            max: 50_000,
            state: SpendState::default(),
        });
    }

    #[test]
    fn roundtrip_spend_cap_nft_count_in_collection() {
        assert_roundtrip(&PolicyData::SpendCap {
            asset: Asset::NftCountInCollection(pk(123)),
            window: WindowKind::Fixed { secs: 86400 },
            max: 3,
            state: SpendState::default(),
        });
    }

    #[test]
    fn roundtrip_spend_cap_any_nft_count() {
        assert_roundtrip(&PolicyData::SpendCap {
            asset: Asset::AnyNftCount,
            window: WindowKind::Fixed { secs: 86400 },
            max: 5,
            state: SpendState::default(),
        });
    }

    #[test]
    fn roundtrip_expiry() {
        assert_roundtrip(&PolicyData::Expiry {
            not_after: 1_900_000_000,
        });
    }

    #[test]
    fn roundtrip_foreign_signer_not_allowed() {
        assert_roundtrip(&PolicyData::ForeignSignerNotAllowed);
    }

    #[test]
    fn kind_matches_data_variant() {
        let cases: Vec<(PolicyData, PolicyKind)> = vec![
            (
                PolicyData::ProgramAllowlist { programs: vec![] },
                PolicyKind::ProgramAllowlist,
            ),
            (
                PolicyData::ProgramBlocklist { programs: vec![] },
                PolicyKind::ProgramBlocklist,
            ),
            (
                PolicyData::MintAllowlist { mints: vec![] },
                PolicyKind::MintAllowlist,
            ),
            (
                PolicyData::MintBlocklist { mints: vec![] },
                PolicyKind::MintBlocklist,
            ),
            (
                PolicyData::NftCollectionAllowlist {
                    collections: vec![],
                },
                PolicyKind::NftCollectionAllowlist,
            ),
            (
                PolicyData::NftCollectionBlocklist {
                    collections: vec![],
                },
                PolicyKind::NftCollectionBlocklist,
            ),
            (PolicyData::Expiry { not_after: 0 }, PolicyKind::Expiry),
            (
                PolicyData::ForeignSignerNotAllowed,
                PolicyKind::ForeignSignerNotAllowed,
            ),
        ];
        for (data, expected) in cases {
            assert_eq!(data.kind(), expected);
            let bytes = borsh::to_vec(&data).unwrap();
            assert_eq!(bytes[0], expected as u8);
        }
    }

    #[test]
    fn policy_new_sets_kind_byte() {
        let data = PolicyData::Expiry { not_after: 42 };
        let p = Policy::new(pk(1), 0, 255, 1_700_000_000, data);
        assert_eq!(p.kind, PolicyKind::Expiry as u8);
        assert!(p.enabled);
    }

    #[test]
    fn size_for_expiry_matches_manual_calc() {
        let data = PolicyData::Expiry { not_after: 0 };
        assert_eq!(Policy::HEADER_LEN, 51);
        assert_eq!(Policy::size_for(&data), 8 + 51 + 1 + 8);
    }

    #[test]
    fn size_for_program_allowlist_grows_with_list() {
        let small = PolicyData::ProgramAllowlist { programs: vec![] };
        let bigger = PolicyData::ProgramAllowlist {
            programs: vec![pk(0); 3],
        };
        assert_eq!(Policy::size_for(&small), 8 + 51 + 1 + 4);
        assert_eq!(Policy::size_for(&bigger), 8 + 51 + 1 + 4 + 3 * 32);
    }

    #[test]
    fn size_for_foreign_signer_not_allowed() {
        let data = PolicyData::ForeignSignerNotAllowed;
        assert_eq!(Policy::size_for(&data), 8 + 51 + 1);
    }

    #[test]
    fn uninitialized_sentinel_rejected_on_attach() {
        assert!(PolicyData::Uninitialized.validate_attach_params().is_err());
    }

    #[test]
    fn roundtrip_ix_discriminator_allowlist_variable_len() {
        assert_roundtrip(&PolicyData::IxDiscriminatorAllowlist {
            program: pk(5),
            discriminators: vec![vec![3], vec![2, 0, 0, 0], vec![9; 8]],
        });
    }

    #[test]
    fn ix_discriminator_attach_accepts_mixed_lengths() {
        let data = PolicyData::IxDiscriminatorAllowlist {
            program: pk(5),
            discriminators: vec![vec![3], vec![2, 0, 0, 0], vec![9; 8], vec![1; 12]],
        };
        assert!(data.validate_attach_params().is_ok());
    }

    #[test]
    fn ix_discriminator_attach_rejects_empty_entry() {
        let data = PolicyData::IxDiscriminatorAllowlist {
            program: pk(5),
            discriminators: vec![vec![]],
        };
        assert!(data.validate_attach_params().is_err());
    }

    #[test]
    fn ix_discriminator_attach_rejects_oversize_entry() {
        let data = PolicyData::IxDiscriminatorAllowlist {
            program: pk(5),
            discriminators: vec![vec![0u8; 33]],
        };
        assert!(data.validate_attach_params().is_err());
    }

    #[test]
    fn zeroed_account_decodes_as_uninitialized() {
        let smallest = PolicyData::ForeignSignerNotAllowed;
        let size = Policy::size_for(&smallest);
        let zeros = vec![0u8; size];
        let mut slice: &[u8] = &zeros;
        let p = <Policy as anchor_lang::AccountDeserialize>::try_deserialize_unchecked(&mut slice)
            .expect("zero account must decode (Anchor init invariant)");
        assert!(matches!(p.data, PolicyData::Uninitialized));
    }

    fn rate_limit_w(window: WindowKind, state: CounterState) -> PolicyData {
        PolicyData::RateLimit {
            window,
            max: 3,
            state,
            scope: None,
        }
    }
    fn spend_cap_w(window: WindowKind, state: SpendState) -> PolicyData {
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window,
            max: 1_000_000,
            state,
        }
    }

    #[test]
    fn attach_rejects_zero_second_fixed_window() {
        assert!(
            rate_limit_w(WindowKind::Fixed { secs: 0 }, CounterState::default())
                .validate_attach_params()
                .is_err()
        );
        assert!(
            spend_cap_w(WindowKind::Fixed { secs: 0 }, SpendState::default())
                .validate_attach_params()
                .is_err()
        );
        assert!(PolicyData::CooldownPeriod {
            secs: 0,
            last_call_ts: 0,
            scope: None,
        }
        .validate_attach_params()
        .is_err());
    }

    #[test]
    fn attach_rejects_zero_slot_or_zero_sec_rolling_window() {
        assert!(rate_limit_w(
            WindowKind::Rolling { secs: 60, slots: 0 },
            CounterState::default()
        )
        .validate_attach_params()
        .is_err());
        assert!(rate_limit_w(
            WindowKind::Rolling { secs: 0, slots: 6 },
            CounterState::default()
        )
        .validate_attach_params()
        .is_err());
        assert!(rate_limit_w(
            WindowKind::Rolling {
                secs: 60,
                slots: 200
            },
            CounterState::default()
        )
        .validate_attach_params()
        .is_err());
    }

    #[test]
    fn attach_accepts_well_formed_windows() {
        assert!(
            rate_limit_w(WindowKind::Fixed { secs: 60 }, CounterState::default())
                .validate_attach_params()
                .is_ok()
        );
        assert!(rate_limit_w(
            WindowKind::Rolling { secs: 60, slots: 6 },
            CounterState::default()
        )
        .validate_attach_params()
        .is_ok());
        assert!(
            spend_cap_w(WindowKind::Fixed { secs: 86_400 }, SpendState::default())
                .validate_attach_params()
                .is_ok()
        );
    }

    #[test]
    fn reshape_fixed_to_rolling_keeps_exhausted_rate_limit_exhausted() {
        // Old policy: Fixed window, exhausted at max 3 (count = 3, ring empty).
        let mut old_state = CounterState::default();
        old_state.charge_fixed(1_000, 3, 60).unwrap();
        old_state.charge_fixed(1_001, 3, 60).unwrap();
        old_state.charge_fixed(1_002, 3, 60).unwrap();
        assert_eq!(old_state.count, 3);
        assert_eq!(old_state.ring.iter().sum::<u32>(), 0);

        let old = rate_limit_w(WindowKind::Fixed { secs: 60 }, old_state);
        let mut new = rate_limit_w(
            WindowKind::Rolling { secs: 60, slots: 6 },
            CounterState::default(),
        );
        new.carry_state_from(&old);

        // Carried state must present the consumed budget in the rolling
        // representation: sum(ring) == 3, so the next rolling charge trips the cap.
        if let PolicyData::RateLimit { state, .. } = new {
            assert_eq!(state.ring.iter().sum::<u32>(), 3);
        } else {
            panic!("expected RateLimit");
        }
    }

    #[test]
    fn reshape_fixed_to_rolling_keeps_spend_cap_consumed() {
        let mut old_state = SpendState::default();
        old_state
            .charge_fixed(1_000, 900_000, 1_000_000, 60)
            .unwrap();
        let old = spend_cap_w(WindowKind::Fixed { secs: 60 }, old_state);
        let mut new = spend_cap_w(
            WindowKind::Rolling { secs: 60, slots: 6 },
            SpendState::default(),
        );
        new.carry_state_from(&old);
        if let PolicyData::SpendCap { state, .. } = new {
            assert_eq!(state.ring.iter().sum::<u64>(), 900_000);
        } else {
            panic!("expected SpendCap");
        }
    }

    #[test]
    fn reshape_same_kind_carries_state_verbatim() {
        let mut old_state = CounterState::default();
        old_state.charge_fixed(1_000, 5, 60).unwrap();
        old_state.charge_fixed(1_001, 5, 60).unwrap();
        let old = rate_limit_w(WindowKind::Fixed { secs: 60 }, old_state);
        let mut new = rate_limit_w(WindowKind::Fixed { secs: 120 }, CounterState::default());
        new.carry_state_from(&old);
        if let PolicyData::RateLimit { state, .. } = new {
            assert_eq!(state.count, 2); // same-kind edit resumes count verbatim
        } else {
            panic!("expected RateLimit");
        }
    }
}
