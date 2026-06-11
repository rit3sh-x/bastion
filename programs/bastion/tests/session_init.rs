mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_litesvm::Report;
use bastion::state::session::Session;
use bastion::utils::helpers::{BastionBundle, SessionRoot};
use helpers::*;
use solana_signer::Signer;

/// The InitSession args for an `expiry` absolute timestamp.
fn init_args(session_key: Pubkey, expiry: i64) -> bastion::instruction::InitSession {
    bastion::instruction::InitSession {
        args: bastion::InitSessionArgs {
            session_key,
            expiry,
        },
    }
}

#[test]
fn init_session_creates_account_with_correct_fields() {
    let mut md = Report::new(
        "Bastion: InitSession writes the expected Session layout",
        "An owner opens a session one hour out. The created PDA is owned by the bastion \
         program, and every Session field reflects the args + the pinned clock.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    let t_now = TEST_CLOCK_TS;
    let expiry = t_now + 3600;
    let expected_bump = Pubkey::find_program_address(
        &[
            bastion::constants::SEED_SESSION,
            owner.pubkey().as_ref(),
            session_kp.pubkey().as_ref(),
        ],
        &bastion::ID,
    )
    .1;

    md.step("Open the session (owner pays rent, expiry one hour out)");
    ctx.tx(&[&owner])
        .build(s.bundle, init_args(session_kp.pubkey(), expiry))
        .send_ok();

    md.step("Read back the Session account and check every field");
    let acct = ctx
        .svm
        .get_account(&s.session)
        .expect("session account exists");
    // `require` is the hard variant: it records the row *and* panics on mismatch,
    // preserving the original `assert_eq!` strength (a wrong field is a failure).
    md.require(
        "session account owned by bastion program",
        bastion::ID,
        acct.owner,
    );

    let session: Session = ctx.load(&s.session);
    md.require("owner", owner.pubkey(), session.owner);
    md.require("session_key", session_kp.pubkey(), session.session_key);
    md.require("bump", expected_bump, session.bump);
    md.require("expiry", expiry, session.expiry);
    md.require("not revoked", false, session.revoked);
    md.require("policy_count", 0u8, session.policy_count);
    md.require("policies_hash zeroed", [0u8; 32], session.policies_hash);
    md.require("created_at == Clock", t_now, session.created_at);
    ctx.report_execution(&mut md);
}

#[test]
fn init_session_rejects_duplicate_pda() {
    let mut md = Report::new(
        "Bastion: InitSession rejects a duplicate PDA",
        "The session PDA derives from (owner, session_key). Initializing the same pair twice \
         must fail: the account is already in use.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    let expiry = TEST_CLOCK_TS + 3600;

    md.step("First InitSession succeeds");
    ctx.tx(&[&owner])
        .build(s.bundle, init_args(session_kp.pubkey(), expiry))
        .send_ok();

    md.step("Second InitSession on the same PDA fails (already in use)");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(s.bundle, init_args(session_kp.pubkey(), expiry))
        .send_err();

    md.check(
        "session still present after rejected re-init",
        true,
        ctx.account_exists(&s.session),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn init_session_rejects_session_key_equal_owner() {
    // V3: operator (session_key) must differ from holder (owner).
    let mut md = Report::new(
        "Bastion: InitSession rejects session_key == owner",
        "The operator (session_key) must differ from the holder (owner). Passing the owner's \
         pubkey as the session key is rejected with SessionKeyIsOwner.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    // session_key collides with the owner: build the bundle off that root directly.
    let bundle = BastionBundle::from(&SessionRoot {
        owner: owner.pubkey(),
        session_key: owner.pubkey(),
    });
    ctx.alias(bundle.session, "Session");
    ctx.alias(bundle.delegate, "Delegate");

    let expiry = TEST_CLOCK_TS + 3600;

    md.step("InitSession with session_key == owner is rejected");
    ctx.tx(&[&owner])
        .build(bundle, init_args(owner.pubkey(), expiry))
        .send_err_named("SessionKeyIsOwner");

    md.check(
        "no session account created",
        false,
        ctx.account_exists(&bundle.session),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn init_session_rejects_past_expiry() {
    let mut md = Report::new(
        "Bastion: InitSession rejects a past expiry",
        "A session must expire in the future. An expiry before the current clock is rejected \
         with SessionExpired.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    let past_expiry = TEST_CLOCK_TS - 100;

    md.step("InitSession with an expiry in the past is rejected");
    ctx.tx(&[&owner])
        .build(s.bundle, init_args(session_kp.pubkey(), past_expiry))
        .send_err_named("SessionExpired");

    md.check(
        "no session account created",
        false,
        ctx.account_exists(&s.session),
    );
    ctx.report_execution(&mut md);
}
