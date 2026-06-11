mod helpers;

use anchor_litesvm::{AccountMeta, AnchorContext, Pubkey, Report};
use bastion::state::policy::PolicyData;
use bastion::state::session::Session;
use bastion::utils::hash::{compute_policies_hash, EMPTY_POLICIES_HASH};
use helpers::*;

/// Derive a session's policy PDA at an explicit seed. `cast_policy` names slot 0;
/// the cap test walks seeds 0..32, so it derives by hand and reuses the same seeds.
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

/// Read the session account back through the typed deserializer.
fn session_state(ctx: &AnchorContext, session: &Pubkey) -> Session {
    ctx.get_account(session).expect("session account")
}

/// Open the session that every test in this file starts from.
fn init_session(
    ctx: &mut AnchorContext,
    owner: &solana_keypair::Keypair,
    s: &SessionCast,
    expiry: i64,
) {
    ctx.tx(&[owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: s.bundle.session_key,
                    expiry,
                },
            },
        )
        .send_ok();
}

#[test]
fn first_attach_uses_seed_zero_and_updates_hash() {
    let mut md = Report::new(
        "Bastion: the first attach lands at seed 0 and commits the policies-hash",
        "A fresh session carries policy_count 0 and the empty policies-hash sentinel. \
         Attaching an Expiry policy lands it at seed 0, bumps the count to 1, and rewrites \
         the hash to commit over the single new policy PDA.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let expiry_policy = cast_policy(&mut ctx, &s, "ExpiryPolicy");

    md.step("Open the session");
    init_session(&mut ctx, &owner, &s, TEST_CLOCK_TS + 86_400);

    let pre = session_state(&ctx, &s.session);
    md.check("fresh session has no policies", 0u8, pre.policy_count);
    md.check(
        "fresh session carries the empty hash",
        EMPTY_POLICIES_HASH,
        pre.policies_hash,
    );

    md.step("Attach an Expiry policy (Lazy NextPolicy resolves seed 0)");
    let data = PolicyData::Expiry {
        not_after: TEST_CLOCK_TS + 3600,
    };
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy { data: data.clone() },
        )
        .send_ok();

    let post = session_state(&ctx, &s.session);
    md.check("policy_count bumped to 1", 1u8, post.policy_count);
    md.check(
        "hash commits over the single policy",
        compute_policies_hash(&[expiry_policy]),
        post.policies_hash,
    );

    let policy: bastion::state::policy::Policy = ctx.get_account(&expiry_policy).expect("policy");
    md.check(
        "policy points back at the session",
        s.session,
        policy.session,
    );
    md.check("policy landed at seed 0", 0u64, policy.seed);
    md.check("policy enabled", true, policy.enabled);
    md.check("policy carries the attached data", data, policy.data);
    ctx.report_execution(&mut md);
}

#[test]
fn second_attach_uses_seed_one_and_hash_includes_both() {
    let mut md = Report::new(
        "Bastion: a second attach lands at seed 1 and the hash covers both",
        "Attach an Expiry policy (seed 0), then a ForeignSignerNotAllowed policy. The second \
         lands at seed 1, the count reaches 2, and the policies-hash commits over both PDAs.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let p0 = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    let p1 = policy_at(&s.session, 1);
    ctx.alias(p1, "ForeignSignerPolicy");

    md.step("Open the session + attach the first policy (seed 0)");
    init_session(&mut ctx, &owner, &s, TEST_CLOCK_TS + 86_400);
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Attach the second policy (seed 1); the prior policy travels as the readonly tail");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ForeignSignerNotAllowed,
            },
        )
        .remaining_accounts(&[AccountMeta::new_readonly(p0, false)])
        .send_ok();

    let session = session_state(&ctx, &s.session);
    md.check(
        "second policy landed at seed 1",
        1u64,
        session.next_seed - 1,
    );
    md.check("policy_count reached 2", 2u8, session.policy_count);
    md.check(
        "hash commits over both policies",
        compute_policies_hash(&[p0, p1]),
        session.policies_hash,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn attach_rejects_when_existing_count_mismatch() {
    let mut md = Report::new(
        "Bastion: attach rejects when the passed existing-count is wrong",
        "Attach one policy, then try a second attach passing an empty existing-policies tail. \
         The session holds one policy, so remaining_accounts.len() (0) disagrees with \
         policy_count (1) and the program rejects with PolicyCountMismatch.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let _p0 = cast_policy(&mut ctx, &s, "ExpiryPolicy");

    md.step("Open the session + attach the first policy");
    init_session(&mut ctx, &owner, &s, TEST_CLOCK_TS + 86_400);
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Second attach with an empty existing tail: count disagrees, rejected");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ForeignSignerNotAllowed,
            },
        )
        .send_err_named("PolicyCountMismatch");
    md.note("session still holds exactly one policy after the rejected attach");
    md.check(
        "policy_count unchanged at 1",
        1u8,
        session_state(&ctx, &s.session).policy_count,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn attach_allows_up_to_cap_then_rejects() {
    let mut md = Report::new(
        "Bastion: attach fills to the 32-policy cap, then rejects the 33rd",
        "Attach ForeignSignerNotAllowed policies up to MAX_POLICIES_PER_EXECUTE (32), each \
         carrying the full prior set as its readonly tail. The session fills to 32; the next \
         attach is rejected with PolicyTooMany.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    init_session(&mut ctx, &owner, &s, TEST_CLOCK_TS + 86_400);

    md.step("Attach 32 policies, each passing the full prior set as its tail");
    let mut existing: Vec<Pubkey> = Vec::new();
    for i in 0..32u64 {
        ctx.svm.expire_blockhash();
        let metas: Vec<AccountMeta> = existing
            .iter()
            .map(|p| AccountMeta::new_readonly(*p, false))
            .collect();
        ctx.tx(&[&owner])
            .build(
                s.bundle,
                bastion::instruction::AttachPolicy {
                    data: PolicyData::ForeignSignerNotAllowed,
                },
            )
            .remaining_accounts(&metas)
            .send_ok();
        existing.push(policy_at(&s.session, i));
    }

    md.check(
        "session filled to the cap",
        32u8,
        session_state(&ctx, &s.session).policy_count,
    );

    md.step("The 33rd attach is over the cap: rejected with PolicyTooMany");
    ctx.svm.expire_blockhash();
    let metas: Vec<AccountMeta> = existing
        .iter()
        .map(|p| AccountMeta::new_readonly(*p, false))
        .collect();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ForeignSignerNotAllowed,
            },
        )
        .remaining_accounts(&metas)
        .send_err_named("PolicyTooMany");
    ctx.report_execution(&mut md);
}

#[test]
fn close_session_after_attach_closes_child_policy() {
    let mut md = Report::new(
        "Bastion: closing a session closes its child policy too",
        "Attach an Expiry policy, then close the session passing the child policy in the tail. \
         Both the session and the child policy account are closed.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let p0 = cast_policy(&mut ctx, &s, "ExpiryPolicy");

    md.step("Open the session + attach an Expiry policy");
    init_session(&mut ctx, &owner, &s, TEST_CLOCK_TS + 86_400);
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
    md.check(
        "child policy exists before close",
        true,
        ctx.account_exists(&p0),
    );

    md.step("Close the session, passing the child policy as the tail");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::CloseSession {})
        .remaining_accounts(&[AccountMeta::new(p0, false)])
        .send_ok();

    md.check("session closed", false, ctx.account_exists(&s.session));
    md.check("child policy closed", false, ctx.account_exists(&p0));
    ctx.report_execution(&mut md);
}
