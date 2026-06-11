//! Off-chain test scaffolding: PDA derivations, the `anchor-litesvm` account
//! bundle, wrapped-instruction builders, and outer instruction constructors.
//!
//! Lives in `src/` (not `tests/`) so layout changes update one place. Uses only
//! regular crate deps; `Keypair`/`LiteSVM` helpers live in `tests/helpers/mod.rs`.

use crate::constants::{COMPUTE_BUDGET_ID, SEED_DELEGATE, SEED_POLICY, SEED_SESSION};
use crate::state::session::Session;
use crate::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_litesvm::{AnchorContext, Bundle, BundleFrom, Lazy, Resolve};

/// Session PDA for `(owner, session_key)`.
pub fn derive_session(owner: Pubkey, session_key: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[SEED_SESSION, owner.as_ref(), session_key.as_ref()],
        &crate::ID,
    )
    .0
}

/// Delegate (vault) PDA for `(owner, session_key)`.
pub fn derive_delegate(owner: Pubkey, session_key: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[SEED_DELEGATE, owner.as_ref(), session_key.as_ref()],
        &crate::ID,
    )
    .0
}

/// Policy PDA for slot `seed` of `session`. Slots are 0-indexed; `Session::next_seed` is the next free slot.
pub fn derive_policy(session: Pubkey, seed: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[SEED_POLICY, session.as_ref(), &seed.to_le_bytes()],
        &crate::ID,
    )
    .0
}

/// Deferred policy-PDA resolver. `NextPolicy` reads the live session; `PolicyAt` pins a slot.
#[derive(Copy, Clone, Debug)]
pub enum BastionSeed {
    NextPolicy(Pubkey),
    PolicyAt(Pubkey, u64),
}

impl Resolve for BastionSeed {
    fn resolve(self, ctx: &AnchorContext) -> Option<Pubkey> {
        match self {
            BastionSeed::NextPolicy(session) => {
                let s: Session = ctx.get_account(&session).ok()?;
                Some(derive_policy(session, s.next_seed))
            }
            BastionSeed::PolicyAt(session, seed) => Some(derive_policy(session, seed)),
        }
    }
}

/// Fixture root: `(owner, session_key)`. All bundle fields derive from these.
#[derive(Copy, Clone, Debug)]
pub struct SessionRoot {
    pub owner: Pubkey,
    pub session_key: Pubkey,
}

/// Shared account set for every bastion instruction.
#[derive(Copy, Clone, Debug, Bundle, BundleFrom)]
#[from_fixtures(r: SessionRoot)]
pub struct BastionBundle {
    pub owner: Pubkey,
    pub session_key: Pubkey,
    #[from(derive_session(r.owner, r.session_key))]
    pub session: Pubkey,
    #[from(Lazy::Deferred(BastionSeed::NextPolicy(derive_session(r.owner, r.session_key))))]
    pub policy: Lazy<BastionSeed>,
    #[from(derive_delegate(r.owner, r.session_key))]
    pub delegate: Pubkey,
    #[from(Pubkey::new_from_array([0u8; 32]))]
    pub destination: Pubkey,
    #[from(Pubkey::from_str_const("Sysvar1nstructions1111111111111111111111111"))]
    pub instructions_sysvar: Pubkey,
}

/// `System::Transfer` of `lamports`: account 0 (signer+writable) → account 1
/// (writable). Built by appending the tag then the amount — no indexing — so the
/// `indexing_slicing` lint stays satisfied without an error path.
pub fn transfer_wrapped(lamports: u64) -> WrappedInstruction {
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend_from_slice(&lamports.to_le_bytes());
    WrappedInstruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            CompactAccountMeta::new(0, true, true),
            CompactAccountMeta::new(1, false, true),
        ],
        data,
    }
}

/// Non-signer policy meta; `writable` for stateful policies (SpendCap, RateLimit, …).
pub fn policy_meta(policy: Pubkey, writable: bool) -> AccountMeta {
    if writable {
        AccountMeta::new(policy, false)
    } else {
        AccountMeta::new_readonly(policy, false)
    }
}

/// Ix-accounts for a System transfer: `[source (w), dest (w), System (r)]`.
pub fn transfer_ix_accounts(source: Pubkey, dest: Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(source, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ]
}

/// Builds `[policies.., delegate, ix_accounts..]`. Seeded from `policies` (so no
/// empty-`Vec` arithmetic for the capacity) then extended — `arithmetic_side_effects`-clean.
pub fn dispatch_tail(
    policies: &[AccountMeta],
    delegate: Pubkey,
    ix_accounts: &[AccountMeta],
) -> Vec<AccountMeta> {
    let mut tail = policies.to_vec();
    tail.push(AccountMeta::new(delegate, false));
    tail.extend_from_slice(ix_accounts);
    tail
}

/// Dispatch tail for a wrapped transfer; all policies marked writable.
pub fn transfer_tail(policies: &[Pubkey], delegate: Pubkey, dest: Pubkey) -> Vec<AccountMeta> {
    let metas: Vec<AccountMeta> = policies.iter().map(|p| policy_meta(*p, true)).collect();
    dispatch_tail(&metas, delegate, &transfer_ix_accounts(delegate, dest))
}

/// `ComputeBudget::SetComputeUnitLimit` (tag 2).
pub fn set_compute_unit_limit_ix(limit: u32) -> Instruction {
    let mut data = vec![2u8];
    data.extend_from_slice(&limit.to_le_bytes());
    Instruction {
        program_id: COMPUTE_BUDGET_ID,
        accounts: vec![],
        data,
    }
}

/// `ComputeBudget::SetComputeUnitPrice` (tag 3), in µ-lamports.
pub fn set_compute_unit_price_ix(price: u64) -> Instruction {
    let mut data = vec![3u8];
    data.extend_from_slice(&price.to_le_bytes());
    Instruction {
        program_id: COMPUTE_BUDGET_ID,
        accounts: vec![],
        data,
    }
}

/// `PinManifest` ix; pass an all-zero hash to un-pin.
pub fn pin_manifest_ix(owner: Pubkey, session: Pubkey, manifest_hash: [u8; 32]) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: crate::accounts::PinManifest { owner, session }.to_account_metas(None),
        data: crate::instruction::PinManifest { manifest_hash }.data(),
    }
}
