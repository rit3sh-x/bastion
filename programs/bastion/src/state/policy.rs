use anchor_lang::prelude::*;

use crate::state::counter::{CounterState, SpendState};

/// PolicyKind discriminant byte. Stored separately in `Policy.kind` so off-chain
/// clients can filter via `getProgramAccounts` memcmp at a fixed offset without
/// deserialising `PolicyData`.
///
/// MUST stay in lock-step with `PolicyData` variant order.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
pub enum PolicyKind {
    ProgramAllowlist = 0,
    ProgramBlocklist = 1,
    MintAllowlist = 2,
    MintBlocklist = 3,
    NftCollectionAllowlist = 4,
    NftCollectionBlocklist = 5,
    RateLimit = 6,
    SpendCap = 7,
    Expiry = 8,
    ForeignSignerNotAllowed = 9,
    CooldownPeriod = 10,
    AmountPerCall = 11,
    MaxCallsTotal = 12,
    TimeOfDayWindow = 13,
    MaxIxSize = 14,
    NftCreatorAllowlist = 15,
    MinDelegateBalance = 16,
    IxDiscriminatorAllowlist = 17,
    RequireMemo = 18,
    NoAccountClose = 19,
    PerCounterpartyCap = 20,
    PerProgramSpendCap = 21,
    MaxComputeUnits = 22,
    MaxPriorityFee = 23,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Fixed { secs: u32 },
    Rolling { secs: u32, slots: u8 },
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
        discriminators: Vec<[u8; 8]>,
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
}

impl PolicyData {
    pub fn validate<'info>(
        &self,
        clock: &Clock,
        wrapped_ix: &crate::state::wrapped_ix::WrappedInstruction,
        ix_accounts: &[AccountInfo<'info>],
        sysvar_ai: &AccountInfo<'info>,
    ) -> Result<()> {
        use crate::policies::expiry::check_expiry;
        use crate::policies::mint_allowlist::{check_mint_allowlist, check_mint_blocklist};
        use crate::policies::program_allowlist::{
            check_program_allowlist, check_program_blocklist,
        };
        match self {
            PolicyData::Expiry { not_after } => check_expiry(*not_after, clock.unix_timestamp),
            PolicyData::ProgramAllowlist { programs } => {
                check_program_allowlist(programs, &wrapped_ix.program_id)
            }
            PolicyData::ProgramBlocklist { programs } => {
                check_program_blocklist(programs, &wrapped_ix.program_id)
            }
            PolicyData::MintAllowlist { mints } => check_mint_allowlist(mints, ix_accounts),
            PolicyData::MintBlocklist { mints } => check_mint_blocklist(mints, ix_accounts),
            PolicyData::NftCollectionAllowlist { collections } => {
                crate::policies::nft_collection::check_nft_collection_allowlist(
                    collections,
                    ix_accounts,
                )
            }
            PolicyData::NftCollectionBlocklist { collections } => {
                crate::policies::nft_collection::check_nft_collection_blocklist(
                    collections,
                    ix_accounts,
                )
            }
            PolicyData::TimeOfDayWindow {
                start_minute,
                end_minute,
                days_mask,
            } => crate::policies::time_of_day::check_time_of_day(
                clock.unix_timestamp,
                *start_minute,
                *end_minute,
                *days_mask,
            ),
            PolicyData::MaxIxSize {
                max_accounts,
                max_data_len,
            } => crate::policies::max_ix_size::check_max_ix_size(
                wrapped_ix.accounts.len(),
                wrapped_ix.data.len(),
                *max_accounts,
                *max_data_len,
            ),
            PolicyData::NftCreatorAllowlist { creators } => {
                crate::policies::nft_creator_allowlist::check_nft_creator_allowlist(
                    creators,
                    ix_accounts,
                )
            }
            PolicyData::IxDiscriminatorAllowlist {
                program,
                discriminators,
            } => crate::policies::ix_discriminator_allowlist::check_ix_discriminator_allowlist(
                program,
                discriminators,
                &wrapped_ix.program_id,
                &wrapped_ix.data,
            ),
            PolicyData::RequireMemo { memo_program } => {
                crate::policies::require_memo::check_require_memo(memo_program, sysvar_ai)
            }
            PolicyData::NoAccountClose => {
                crate::policies::no_account_close::check_no_account_close(
                    &wrapped_ix.program_id,
                    &wrapped_ix.data,
                )
            }
            PolicyData::MaxComputeUnits { max } => {
                crate::policies::max_compute_units::check_max_compute_units(*max, sysvar_ai)
            }
            PolicyData::MaxPriorityFee { max_micro_lamports } => {
                crate::policies::max_priority_fee::check_max_priority_fee(
                    *max_micro_lamports,
                    sysvar_ai,
                )
            }
            PolicyData::RateLimit { .. }
            | PolicyData::SpendCap { .. }
            | PolicyData::ForeignSignerNotAllowed
            | PolicyData::CooldownPeriod { .. }
            | PolicyData::AmountPerCall { .. }
            | PolicyData::MaxCallsTotal { .. }
            | PolicyData::MinDelegateBalance { .. }
            | PolicyData::PerCounterpartyCap { .. }
            | PolicyData::PerProgramSpendCap { .. } => Ok(()),
        }
    }

    pub fn kind(&self) -> PolicyKind {
        match self {
            PolicyData::ProgramAllowlist { .. } => PolicyKind::ProgramAllowlist,
            PolicyData::ProgramBlocklist { .. } => PolicyKind::ProgramBlocklist,
            PolicyData::MintAllowlist { .. } => PolicyKind::MintAllowlist,
            PolicyData::MintBlocklist { .. } => PolicyKind::MintBlocklist,
            PolicyData::NftCollectionAllowlist { .. } => PolicyKind::NftCollectionAllowlist,
            PolicyData::NftCollectionBlocklist { .. } => PolicyKind::NftCollectionBlocklist,
            PolicyData::RateLimit { .. } => PolicyKind::RateLimit,
            PolicyData::SpendCap { .. } => PolicyKind::SpendCap,
            PolicyData::Expiry { .. } => PolicyKind::Expiry,
            PolicyData::ForeignSignerNotAllowed => PolicyKind::ForeignSignerNotAllowed,
            PolicyData::CooldownPeriod { .. } => PolicyKind::CooldownPeriod,
            PolicyData::AmountPerCall { .. } => PolicyKind::AmountPerCall,
            PolicyData::MaxCallsTotal { .. } => PolicyKind::MaxCallsTotal,
            PolicyData::TimeOfDayWindow { .. } => PolicyKind::TimeOfDayWindow,
            PolicyData::MaxIxSize { .. } => PolicyKind::MaxIxSize,
            PolicyData::NftCreatorAllowlist { .. } => PolicyKind::NftCreatorAllowlist,
            PolicyData::MinDelegateBalance { .. } => PolicyKind::MinDelegateBalance,
            PolicyData::IxDiscriminatorAllowlist { .. } => PolicyKind::IxDiscriminatorAllowlist,
            PolicyData::RequireMemo { .. } => PolicyKind::RequireMemo,
            PolicyData::NoAccountClose => PolicyKind::NoAccountClose,
            PolicyData::PerCounterpartyCap { .. } => PolicyKind::PerCounterpartyCap,
            PolicyData::PerProgramSpendCap { .. } => PolicyKind::PerProgramSpendCap,
            PolicyData::MaxComputeUnits { .. } => PolicyKind::MaxComputeUnits,
            PolicyData::MaxPriorityFee { .. } => PolicyKind::MaxPriorityFee,
        }
    }

    pub fn serialized_len(&self) -> usize {
        borsh::object_length(self).unwrap_or(0)
    }

    pub fn normalize(&mut self) {
        match self {
            PolicyData::ProgramAllowlist { programs }
            | PolicyData::ProgramBlocklist { programs } => programs.sort_unstable(),
            PolicyData::MintAllowlist { mints } | PolicyData::MintBlocklist { mints } => {
                mints.sort_unstable()
            }
            PolicyData::NftCollectionAllowlist { collections }
            | PolicyData::NftCollectionBlocklist { collections } => collections.sort_unstable(),
            PolicyData::NftCreatorAllowlist { creators } => creators.sort_unstable(),
            PolicyData::IxDiscriminatorAllowlist { discriminators, .. } => {
                discriminators.sort_unstable()
            }
            _ => {}
        }
    }

    pub fn validate_attach_params(&self) -> anchor_lang::prelude::Result<()> {
        use crate::constants::MAX_PROGRAMS_PER_LIST;
        use crate::error::BastionError;

        let asset_opt = match self {
            PolicyData::SpendCap { asset, .. }
            | PolicyData::AmountPerCall { asset, .. }
            | PolicyData::PerCounterpartyCap { asset, .. }
            | PolicyData::PerProgramSpendCap { asset, .. } => Some(asset),
            _ => None,
        };
        if let Some(Asset::NftCountInCollection(_) | Asset::AnyNftCount) = asset_opt {
            return Err(error!(BastionError::InvalidPolicyData));
        }

        match self {
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
            PolicyData::MaxIxSize {
                max_accounts,
                max_data_len,
            } => {
                require!(*max_accounts > 0, BastionError::InvalidPolicyData);
                require!(*max_data_len > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::MaxCallsTotal { max, used } => {
                require!(*used == 0, BastionError::InvalidPolicyData);
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::NftCreatorAllowlist { creators } => {
                require!(!creators.is_empty(), BastionError::InvalidPolicyData);
                require!(
                    creators.len() <= MAX_PROGRAMS_PER_LIST,
                    BastionError::ListTooLong
                );
            }
            PolicyData::MinDelegateBalance { floor } => {
                require!(*floor > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::IxDiscriminatorAllowlist { discriminators, .. } => {
                require!(!discriminators.is_empty(), BastionError::InvalidPolicyData);
                require!(
                    discriminators.len() <= MAX_PROGRAMS_PER_LIST,
                    BastionError::ListTooLong
                );
            }
            PolicyData::PerCounterpartyCap { sent, max, .. } => {
                require!(*sent == 0, BastionError::InvalidPolicyData);
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::PerProgramSpendCap { max, .. } => {
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            PolicyData::MaxComputeUnits { max } => {
                require!(*max > 0, BastionError::InvalidPolicyData);
            }
            _ => {}
        }
        Ok(())
    }
}

/// NOTE: NO `#[derive(InitSpace)]` here. `Policy.data: PolicyData` is a
/// variable-size Borsh enum (see `PolicyData::serialized_len`), so a single
/// `INIT_SPACE` constant would be meaningless. Allocation is done via the
/// `Policy::size_for(&data)` function which sums Anchor discriminator +
/// fixed header + actual serialised data length (floored to MIN_DATA_LEN).
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

    pub const MIN_DATA_LEN: usize = 5;

    /// Total bytes (Anchor discriminator + header + serialised data) required to
    /// store a Policy whose `data` field equals the supplied value.
    ///
    /// Uses `Self::DISCRIMINATOR.len()` instead of a literal `8` so we stay in
    /// lock-step with whatever Anchor's discriminator length is (currently 8 bytes;
    /// future-proof). `saturating_add` is fine here because overflow is
    /// mathematically unreachable: 8 (disc) + 51 (HEADER_LEN) + ≤ ~32 KB of
    /// PolicyData payload is always orders of magnitude below `usize::MAX`.
    /// Returning `usize::MAX` on the impossible saturation path would just
    /// make Anchor's `init` fail at allocate-account time anyway.
    pub fn size_for(data: &PolicyData) -> usize {
        let data_len = data.serialized_len().max(Self::MIN_DATA_LEN);
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
    fn size_for_foreign_signer_not_allowed_floored_at_min() {
        let data = PolicyData::ForeignSignerNotAllowed;
        assert_eq!(Policy::size_for(&data), 8 + 51 + Policy::MIN_DATA_LEN);
    }
}
