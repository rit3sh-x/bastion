mod helpers;

use anchor_litesvm::{AnchorContext, Pubkey, Report, TestHelpers};
use bastion::state::session::Session;
use helpers::*;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// Open a session at `TEST_CLOCK_TS + expiry_offset` and hand back its initial
/// expiry. Every test starts here, so the offset is the only knob.
fn open_session(
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

fn fetch_session(ctx: &AnchorContext, session: &Pubkey) -> Session {
    ctx.get_account::<Session>(session).expect("deser Session")
}

#[test]
fn extend_session_advances_expiry() {
    let mut md = Report::new(
        "Bastion: extend advances a live session's expiry",
        "An owner opens a session, then extends it to a later expiry. The new \
         value sticks; every other field of the session is untouched.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session (expiry = clock + 3600)");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    let pre = fetch_session(&ctx, &s.session);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    md.step("Extend to a later expiry (+7200)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry },
            },
        )
        .send_ok();

    let post = fetch_session(&ctx, &s.session);
    md.check("expiry advanced to new value", new_expiry, post.expiry);
    md.check("owner unchanged", pre.owner, post.owner);
    md.check("session_key unchanged", pre.session_key, post.session_key);
    md.check("created_at unchanged", pre.created_at, post.created_at);
    md.check("revoked unchanged", pre.revoked, post.revoked);
    md.check(
        "policy_count unchanged",
        pre.policy_count,
        post.policy_count,
    );
    md.check(
        "policies_hash unchanged",
        pre.policies_hash,
        post.policies_hash,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn extend_session_rejects_revoked() {
    let mut md = Report::new(
        "Bastion: extend rejects a revoked session",
        "A revoked session is dead; extend must refuse it with SessionRevoked.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    md.step("Revoke it");
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::RevokeSession {})
        .send_ok();
    ctx.svm.expire_blockhash();

    let pre = fetch_session(&ctx, &s.session);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    md.step("Extend a revoked session: SessionRevoked");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry },
            },
        )
        .send_err_named("SessionRevoked");
    ctx.report_execution(&mut md);
}

#[test]
fn extend_session_rejects_already_expired() {
    let mut md = Report::new(
        "Bastion: extend rejects an already-expired session",
        "Once the clock passes a session's expiry, extend can't resurrect it; it \
         fails with SessionExpired.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session (expiry = clock + 3600)");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    md.step("Advance the clock past expiry (+3601)");
    ctx.svm.advance_seconds(3601);
    ctx.svm.expire_blockhash();

    let pre = fetch_session(&ctx, &s.session);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    md.step("Extend an expired session: SessionExpired");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry },
            },
        )
        .send_err_named("SessionExpired");
    ctx.report_execution(&mut md);
}

#[test]
fn extend_session_rejects_non_monotonic() {
    let mut md = Report::new(
        "Bastion: extend must move expiry forward",
        "Shrinking the expiry is rejected (NewExpiryNotGreater) and leaves the \
         stored expiry unchanged.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    let pre = fetch_session(&ctx, &s.session);
    let shrunk = pre.expiry.checked_sub(60).expect("no underflow");

    md.step("Extend to an earlier expiry (-60): NewExpiryNotGreater");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry: shrunk },
            },
        )
        .send_err_named("NewExpiryNotGreater");

    let post = fetch_session(&ctx, &s.session);
    md.check("expiry unchanged on rejection", pre.expiry, post.expiry);
    ctx.report_execution(&mut md);
}

#[test]
fn extend_session_rejects_equal_expiry() {
    let mut md = Report::new(
        "Bastion: extend is strictly greater, equal is rejected",
        "Extending to the exact same expiry is not forward progress; the guard is \
         strict-greater, so it fails with NewExpiryNotGreater.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    let pre = fetch_session(&ctx, &s.session);

    md.step("Extend to the same expiry: NewExpiryNotGreater");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs {
                    new_expiry: pre.expiry,
                },
            },
        )
        .send_err_named("NewExpiryNotGreater");
    ctx.report_execution(&mut md);
}

#[test]
fn extend_session_rejects_non_owner() {
    let mut md = Report::new(
        "Bastion: only the owner can extend",
        "An attacker signs an extend against the owner's session. The seeds are \
         derived from the signer's key, so the PDA no longer matches and the \
         constraint fails. The stored expiry is untouched.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let attacker = ctx.cast_actor("attacker");

    md.step("Open the session (real owner)");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    let pre = fetch_session(&ctx, &s.session);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    // Same session account, but the attacker in the owner slot: the seeds
    // constraint re-derives the PDA from the attacker's key, so it mismatches.
    let mut bundle = s.bundle;
    bundle.owner = attacker.pubkey();
    bundle.session = s.session;

    md.step("Attacker signs extend against the owner's session: seeds violation");
    ctx.tx(&[&attacker])
        .build(
            bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry },
            },
        )
        .send_err();

    let post = fetch_session(&ctx, &s.session);
    md.check("expiry unchanged on rejection", pre.expiry, post.expiry);
    ctx.report_execution(&mut md);
}

#[test]
fn extend_session_can_be_chained() {
    let mut md = Report::new(
        "Bastion: extends chain",
        "Two extends in a row each move expiry forward; the final value is the \
         last one written.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    open_session(&mut ctx, &owner, &session_kp, &s, TEST_CLOCK_TS + 3600);

    let pre = fetch_session(&ctx, &s.session);
    let e1 = pre.expiry.checked_add(3600).expect("no overflow");

    md.step("First extend (+3600)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry: e1 },
            },
        )
        .send_ok();

    ctx.svm.expire_blockhash();
    let e2 = e1.checked_add(3600).expect("no overflow");

    md.step("Second extend (+3600 again)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::ExtendSession {
                args: bastion::ExtendSessionArgs { new_expiry: e2 },
            },
        )
        .send_ok();

    let post = fetch_session(&ctx, &s.session);
    md.check("expiry is the last value written", e2, post.expiry);
    ctx.report_execution(&mut md);
}
