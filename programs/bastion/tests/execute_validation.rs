//! Session-state guard rails on `Execute`: a policy-free happy path, plus the
//! revoked / expired / wrong-signer rejections that fire before any dispatch.

mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_lang::system_program;
use anchor_litesvm::{Report, TestHelpers};
use bastion::state::wrapped_ix::WrappedInstruction;
use helpers::*;
use solana_signer::Signer;

/// An empty wrapped ix (no inner accounts, no data): the guard-rail tests never
/// reach dispatch, so the wrapped payload's body doesn't matter, only that the
/// session check fires first.
fn empty_wrapped() -> WrappedInstruction {
    WrappedInstruction {
        program_id: system_program::ID,
        accounts: vec![],
        data: vec![],
    }
}

/// The minimal dispatch tail for the reject paths: just the delegate. The session
/// guard fails before dispatch consults it, so a single meta is enough to satisfy
/// account decoding.
fn delegate_only_extras(delegate: &Pubkey) -> Vec<AccountMeta> {
    vec![AccountMeta::new_readonly(*delegate, false)]
}

#[test]
fn execute_succeeds_on_active_session_with_no_policies_and_real_cpi() {
    let mut md = Report::new(
        "Bastion: Execute runs a real SOL transfer on an active, policy-free session",
        "An owner opens an active session and funds its delegate. The session signer \
         executes a wrapped System::Transfer of 100_000 lamports to a recipient. With no \
         policies (policy_count = 0), the only gate is the session's active state; the \
         delegate PDA signs the inner transfer.",
    );
    let (mut ctx, _owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let pre_dest = ctx.svm.get_balance(&recipient).unwrap();

    md.step("Session executes a wrapped System::Transfer (100_000) via the delegate");
    let extras = transfer_tail(&[], s.delegate, recipient);
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(100_000)],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    let post_dest = ctx.svm.get_balance(&recipient).unwrap();
    md.check(
        "recipient received the transfer",
        pre_dest + 100_000,
        post_dest,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn execute_rejects_revoked_session() {
    let mut md = Report::new(
        "Bastion: Execute rejects a revoked session",
        "An owner opens a session, then revokes it. A subsequent Execute on that session \
         fails with SessionRevoked before any dispatch.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);

    md.step("Revoke the session");
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::RevokeSession {})
        .send_ok();

    md.step("Execute on the revoked session: rejected with SessionRevoked");
    let extras = delegate_only_extras(&s.delegate);
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("SessionRevoked");
    md.check("session rejected as revoked", true, true);
    ctx.report_execution(&mut md);
}

#[test]
fn execute_rejects_expired_session() {
    let mut md = Report::new(
        "Bastion: Execute rejects an expired session",
        "An owner opens a session with a 60s expiry, then the clock advances 120s. A \
         subsequent Execute fails with SessionExpired before any dispatch.",
    );
    // Custom (short) expiry, so this opens the session by hand rather than via
    // `bootstrap` (which pins a one-day expiry).
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open a short-lived session (60s expiry)");
    init_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 60);
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();

    md.step("Advance the clock past expiry (+120s)");
    ctx.svm.advance_seconds(120);

    md.step("Execute on the expired session: rejected with SessionExpired");
    let extras = delegate_only_extras(&s.delegate);
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("SessionExpired");
    md.check("session rejected as expired", true, true);
    ctx.report_execution(&mut md);
}

#[test]
fn execute_rejects_wrong_signer() {
    let mut md = Report::new(
        "Bastion: Execute rejects a wrong signer (ConstraintSeeds)",
        "An owner opens a session. An attacker (a different keypair) tries to drive Execute \
         against that session PDA. The session is derived from the signer's key, so the \
         attacker's key fails the seed constraint (ConstraintSeeds / 0x7d6).",
    );
    let (mut ctx, _owner, _session_kp, s) = bootstrap(ONE_SOL);
    let attacker = ctx.cast_actor("attacker");

    md.step("Attacker drives Execute against the session PDA: fails ConstraintSeeds");
    // The attacker signs, so the Execute base must project the session_key from
    // the attacker; the session PDA stays the owner's, which is exactly the seed
    // mismatch the program rejects. Build the base by hand off the attacker key.
    let mut bundle = s.bundle;
    bundle.session_key = attacker.pubkey();
    let res = ctx
        .tx(&[&attacker])
        .build(
            bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .send_err();
    let logs = res.logs().join("\n");
    assert!(
        logs.contains("ConstraintSeeds") || logs.contains("0x7d6"),
        "expected ConstraintSeeds; got:\n{}",
        logs
    );
    md.check("wrong signer rejected by seed constraint", true, true);
    ctx.report_execution(&mut md);
}
