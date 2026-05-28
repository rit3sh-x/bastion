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

// TODO: Implement policies