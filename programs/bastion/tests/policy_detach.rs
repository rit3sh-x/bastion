mod helpers;

use anchor_litesvm::{AccountMeta, Lazy, Pubkey, Report};
use bastion::state::policy::PolicyData;
use bastion::state::session::Session;
use bastion::utils::hash::{compute_policies_hash, EMPTY_POLICIES_HASH};
use bastion::utils::helpers::BastionSeed;
use helpers::*;
use solana_signer::Signer;

/// Derive the policy PDA at a specific seed (the second policy slot, seed 1, in
/// the two-policy scenarios; `cast_policy` already names seed 0).
fn policy_at(session: &Pubkey, seed: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            bastion::constants::SEED_POLICY,
            session.as_ref(),
            &seed.to_le_bytes(),
        ],
        &bastion::ID,
    )
    .0
}

/// Fetch the live `Session` state.
fn session_state(ctx: &anchor_litesvm::AnchorContext, session: &Pubkey) -> Session {
    ctx.get_account(session).expect("session account")
}

#[test]
fn detach_only_policy_returns_to_empty_hash() {
    let mut md = Report::new(
        "Bastion: detaching the only policy returns the session to the empty hash",
        "An owner opens a session and attaches a single Expiry policy, then detaches it. \
         The session's policy_count returns to 0 and its policies_hash returns to the empty \
         sentinel; next_seed does not move (it is monotonic, never decreasing on detach).",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let p0 = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    let mut bundle = s.bundle;

    md.step("Open the session");
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 86_400,
                },
            },
        )
        .send_ok();

    md.step("Attach one Expiry policy (NextPolicy strategy, seed 0)");
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Detach it (PolicyAt seed 0, no other policies in the tail)");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::DetachPolicy { seed: 0 })
        .send_ok();

    let session = session_state(&ctx, &s.session);
    md.check("policy_count back to 0", 0u8, session.policy_count);
    md.check("next_seed unchanged on detach", 1u64, session.next_seed);
    md.check(
        "policies_hash back to empty sentinel",
        EMPTY_POLICIES_HASH,
        session.policies_hash,
    );
    md.check("policy account closed", false, ctx.account_exists(&p0));
    ctx.report_execution(&mut md);
}

#[test]
fn detach_one_of_two_updates_hash_to_remaining() {
    let mut md = Report::new(
        "Bastion: detaching one of two policies updates the hash to the remaining set",
        "Attach two policies (Expiry at seed 0, ForeignSignerNotAllowed at seed 1), then \
         detach the first. policy_count drops to 1, next_seed stays at 2 (monotonic), and \
         policies_hash becomes the hash of the single surviving policy.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let p0 = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    let p1 = policy_at(&s.session, 1);
    ctx.alias(p1, "ForeignSignerPolicy");
    let mut bundle = s.bundle;

    md.step("Open the session");
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 86_400,
                },
            },
        )
        .send_ok();

    md.step("Attach policy 0 (Expiry), no existing policies in the tail");
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Attach policy 1 (ForeignSignerNotAllowed); existing set is [p0]");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ForeignSignerNotAllowed,
            },
        )
        .remaining_accounts(&[AccountMeta::new_readonly(p0, false)])
        .send_ok();

    md.step("Detach policy 0 (PolicyAt seed 0); other set is [p1]");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::DetachPolicy { seed: 0 })
        .remaining_accounts(&[AccountMeta::new_readonly(p1, false)])
        .send_ok();

    let session = session_state(&ctx, &s.session);
    md.check("policy_count drops to 1", 1u8, session.policy_count);
    md.check(
        "next_seed monotonic, never decreases",
        2u64,
        session.next_seed,
    );
    md.check(
        "policies_hash is the surviving policy's hash",
        compute_policies_hash(&[p1]),
        session.policies_hash,
    );
    md.check("p0 closed", false, ctx.account_exists(&p0));
    md.check("p1 still alive", true, ctx.account_exists(&p1));
    ctx.report_execution(&mut md);
}

#[test]
fn detach_with_wrong_other_set_fails_hash_check() {
    let mut md = Report::new(
        "Bastion: detaching with the wrong other-set fails the count/hash check",
        "Attach two policies, then attempt to detach policy 0 while passing an empty other-set \
         (the truthful tail is [p1]). The handler counts the tail against the expected \
         remaining count and rejects with PolicyCountMismatch; p0 stays alive.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let p0 = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    let p1 = policy_at(&s.session, 1);
    ctx.alias(p1, "ForeignSignerPolicy");
    let mut bundle = s.bundle;

    md.step("Open the session");
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 86_400,
                },
            },
        )
        .send_ok();

    md.step("Attach policy 0 (Expiry)");
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Attach policy 1 (ForeignSignerNotAllowed); existing set is [p0]");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ForeignSignerNotAllowed,
            },
        )
        .remaining_accounts(&[AccountMeta::new_readonly(p0, false)])
        .send_ok();

    md.step("Detach policy 0 with an empty other-set: rejected with PolicyCountMismatch");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::DetachPolicy { seed: 0 })
        .send_err_named("PolicyCountMismatch");

    md.check(
        "p0 still present after the rejected detach",
        true,
        ctx.account_exists(&p0),
    );
    let _ = p1;
    ctx.report_execution(&mut md);
}
