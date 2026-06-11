#![allow(dead_code, unused_imports)]

//! The shared test façade. One import — `mod helpers; use helpers::*;` — gives a
//! scenario everything it needs:
//!
//!   * **Plumbing**: `bastion_ctx()` loads the program and pins the clock.
//!   * **Cast**: `cast_session` / `cast_policy` name a session, delegate, and
//!     policy as they derive them, so reports read `SpendCap`, never `policy0`.
//!   * **Scenarios**: `bootstrap` / `open_session` open + fund a session in one
//!     line; `attach` / `attach_all` attach policies (handling the attach-chain
//!     of prior policy accounts) and return their PDAs.
//!   * **Builders**: the wrapped-ix, dispatch-tail, and outer-ix builders are
//!     re-exported from `bastion::utils::helpers` (they encode the program's wire
//!     format, so they live beside the program; see that module).
//!
//! The split is dictated by what compiles where: the builders are pure functions
//! of bastion's layout and need only the crate's regular deps, so they sit in
//! `src`. Anything that drives the SVM — `Keypair` signing, airdrops, the
//! `AnchorContext` — can only use dev-deps, so it lives here.

use anchor_litesvm::{AnchorContext, AnchorLiteSVM, TestHelpers};
use bastion::state::policy::PolicyData;
use bastion::state::session::Session;
use bastion::utils::helpers::{derive_policy, BastionBundle, SessionRoot};
use solana_keypair::Keypair;
use solana_signer::Signer;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};

// Re-export the program-aware builders so a scenario reaches them through this
// one façade (`helpers::transfer_wrapped`, `helpers::transfer_tail`, …) rather
// than a second `use bastion::utils::helpers::...`.
pub use bastion::utils::helpers::{
    derive_delegate, derive_session, dispatch_tail, pin_manifest_ix, policy_meta,
    set_compute_unit_limit_ix, set_compute_unit_price_ix, transfer_ix_accounts, transfer_tail,
    transfer_wrapped,
};

pub const BASTION_SO: &[u8] = include_bytes!("../../../../target/deploy/bastion.so");
pub const ONE_SOL: u64 = 1_000_000_000;
pub const TEST_CLOCK_TS: i64 = 1_704_067_200;
/// One day in seconds — the default session lifetime (`open_session` sets expiry
/// to `TEST_CLOCK_TS + DAY`).
pub const DAY: i64 = 86_400;

// ---------------------------------------------------------------------------
// Plumbing.
// ---------------------------------------------------------------------------

/// Load bastion and pin the clock to a fixed anchor point. Time-advancing tests
/// move from here with `TestHelpers::advance_seconds` / `advance_days`.
pub fn bastion_ctx() -> AnchorContext {
    let mut ctx = AnchorLiteSVM::build_with_program(bastion::ID, "bastion", BASTION_SO);
    ctx.svm.warp_to_timestamp(TEST_CLOCK_TS);
    ctx
}

// ---------------------------------------------------------------------------
// Cast — name the structural accounts (session, delegate, policy) as they derive.
// ---------------------------------------------------------------------------

/// A session and its delegate, derived from `(owner, session_key)` and aliased.
/// Carries the `bundle` every instruction projects its accounts from.
pub struct SessionCast {
    pub bundle: BastionBundle,
    pub session: Pubkey,
    pub delegate: Pubkey,
}

/// Derive + alias the session and delegate PDAs (no on-chain state yet). Use this
/// when a test drives `InitSession` itself; most scenarios want `open_session`.
pub fn cast_session(ctx: &mut AnchorContext, owner: &Keypair, session_kp: &Keypair) -> SessionCast {
    let bundle = BastionBundle::from(&SessionRoot {
        owner: owner.pubkey(),
        session_key: session_kp.pubkey(),
    });
    ctx.alias(bundle.session, "Session");
    ctx.alias(bundle.delegate, "Delegate");
    SessionCast {
        bundle,
        session: bundle.session,
        delegate: bundle.delegate,
    }
}

/// Alias the policy at `seed`, named by its role in this scenario, and return its
/// PDA. Naming it (`"SpendCap"`, `"CollectionAllowlist"`) is what makes a report
/// read by role rather than by slot.
pub fn cast_policy_at(ctx: &mut AnchorContext, s: &SessionCast, role: &str, seed: u64) -> Pubkey {
    let policy = derive_policy(s.session, seed);
    ctx.alias(policy, role);
    policy
}

/// Alias the session's first policy (slot 0) by `role`. Shorthand for
/// `cast_policy_at(.., 0)` — the common single-policy case.
pub fn cast_policy(ctx: &mut AnchorContext, s: &SessionCast, role: &str) -> Pubkey {
    cast_policy_at(ctx, s, role, 0)
}

// ---------------------------------------------------------------------------
// Scenarios — open + fund a session, attach policies. The opening beat every
// test repeats, collapsed to one call.
// ---------------------------------------------------------------------------

/// Send `InitSession` for an already-cast session with an explicit `expiry`. The
/// low-level opener for tests that pin a non-default lifetime (extend / expiry).
pub fn init_session(
    ctx: &mut AnchorContext,
    owner: &Keypair,
    session_kp: &Keypair,
    s: &SessionCast,
    expiry: i64,
) {
    ctx.tx(&[owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry,
                },
            },
        )
        .send_ok();
}

/// Cast, open (expiry `TEST_CLOCK_TS + DAY`), and fund a session: the session
/// signer gets 5 SOL to pay execute fees, the delegate vault gets
/// `delegate_funding`. The canonical session opener — returns the cast.
pub fn open_session(
    ctx: &mut AnchorContext,
    owner: &Keypair,
    session_kp: &Keypair,
    delegate_funding: u64,
) -> SessionCast {
    let s = cast_session(ctx, owner, session_kp);
    init_session(ctx, owner, session_kp, &s, TEST_CLOCK_TS + DAY);
    ctx.svm.airdrop(&session_kp.pubkey(), 5 * ONE_SOL).unwrap();
    ctx.svm.airdrop(&s.delegate, delegate_funding).unwrap();
    s
}

/// The whole opening beat in one line: a funded context, an `owner` and
/// `session-signer` actor, and an open + funded session. Returns
/// `(ctx, owner, session_kp, cast)` so the scenario keeps the keypairs it signs
/// with. Replaces the four-line `bastion_ctx` + two `cast_actor` + `cast_session`
/// preamble.
pub fn bootstrap(delegate_funding: u64) -> (AnchorContext, Keypair, Keypair, SessionCast) {
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = open_session(&mut ctx, &owner, &session_kp, delegate_funding);
    (ctx, owner, session_kp, s)
}

/// Attach `data` as the session's next policy, aliased by `role`, and return its
/// PDA. The attach chain — every already-attached policy riding as a readonly
/// remaining account so the program can re-hash the set — is handled here by
/// reading `Session::next_seed`. Assumes no detach gaps (true for attach-then-
/// execute scenarios); for interleaved detaches drive `AttachPolicy` directly.
pub fn attach(
    ctx: &mut AnchorContext,
    owner: &Keypair,
    s: &SessionCast,
    role: &str,
    data: PolicyData,
) -> Pubkey {
    let session: Session = ctx.get_account(&s.session).expect("session is open");
    let seed = session.next_seed;
    let policy = cast_policy_at(ctx, s, role, seed);
    let prior: Vec<AccountMeta> = (0..seed)
        .map(|i| AccountMeta::new_readonly(derive_policy(s.session, i), false))
        .collect();
    ctx.tx(&[owner])
        .build(s.bundle, bastion::instruction::AttachPolicy { data })
        .remaining_accounts(&prior)
        .send_ok();
    policy
}

/// Attach a batch of `(role, data)` policies in order, each carrying the prior
/// ones as the attach chain. Returns their PDAs in slot order. The multi-policy
/// counterpart to `attach` (e.g. a composition scenario's six policies).
pub fn attach_all(
    ctx: &mut AnchorContext,
    owner: &Keypair,
    s: &SessionCast,
    specs: Vec<(&str, PolicyData)>,
) -> Vec<Pubkey> {
    let mut policies: Vec<Pubkey> = Vec::with_capacity(specs.len());
    for (role, data) in specs {
        let prior: Vec<AccountMeta> = policies
            .iter()
            .map(|p| AccountMeta::new_readonly(*p, false))
            .collect();
        let policy = cast_policy_at(ctx, s, role, policies.len() as u64);
        ctx.tx(&[owner])
            .build(s.bundle, bastion::instruction::AttachPolicy { data })
            .remaining_accounts(&prior)
            .send_ok();
        policies.push(policy);
    }
    policies
}

// ---------------------------------------------------------------------------
// Native ed25519 verify ix — stays test-side because it signs with a `Keypair`
// (a dev-dependency the `src` builders can't reach).
// ---------------------------------------------------------------------------

/// A native ed25519-program verify ix: `signer` signs `message`, laid out in the
/// single-signature wire format the runtime's ed25519 program expects. A
/// manifest-gated execute rides one of these so the program can bind who signed
/// the pinned commitment.
pub fn ed25519_ix(signer: &Keypair, message: &[u8]) -> Instruction {
    let pk = signer.pubkey().to_bytes();
    let sig = signer.sign_message(message);
    let sig_bytes = sig.as_ref();

    let pk_offset: u16 = 16;
    let sig_offset: u16 = 16 + 32;
    let msg_offset: u16 = 16 + 32 + 64;
    let msg_size: u16 = message.len() as u16;
    let any: u16 = u16::MAX;

    let mut data: Vec<u8> = Vec::new();
    data.push(1);
    data.push(0);
    data.extend_from_slice(&sig_offset.to_le_bytes());
    data.extend_from_slice(&any.to_le_bytes());
    data.extend_from_slice(&pk_offset.to_le_bytes());
    data.extend_from_slice(&any.to_le_bytes());
    data.extend_from_slice(&msg_offset.to_le_bytes());
    data.extend_from_slice(&msg_size.to_le_bytes());
    data.extend_from_slice(&any.to_le_bytes());
    data.extend_from_slice(&pk);
    data.extend_from_slice(sig_bytes);
    data.extend_from_slice(message);

    Instruction {
        program_id: bastion::constants::ED25519_PROGRAM_ID,
        accounts: vec![],
        data,
    }
}

// ---------------------------------------------------------------------------
// Outcome assertion.
// ---------------------------------------------------------------------------

/// Whether an execute is expected to land or to be rejected with a named error.
/// Lets a table-driven scenario state its expectation inline.
pub enum Expect<'a> {
    Ok,
    Err(&'a str),
}
