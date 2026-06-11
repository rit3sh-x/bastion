mod helpers;

use anchor_litesvm::{AccountMeta, Pubkey, Report, TestHelpers};
use bastion::state::policy::PolicyData;
use helpers::*;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// One child policy as the CloseSession dispatch tail expects it: writable (so
/// `close = owner` can drain its rent) and not a signer. Mirrors the program's
/// `AccountMeta::new(policy, false)` per child.
fn child_metas(policies: &[Pubkey]) -> Vec<AccountMeta> {
    policies
        .iter()
        .map(|p| AccountMeta::new(*p, false))
        .collect()
}

/// Open a session for `owner` with a fixed expiry, projecting `bundle`.
fn open_session(
    ctx: &mut anchor_litesvm::AnchorContext,
    owner: &Keypair,
    session_kp: &Keypair,
    bundle: bastion::utils::helpers::BastionBundle,
) {
    ctx.tx(&[owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
}

#[test]
fn close_session_returns_rent_to_owner_when_no_children() {
    let mut md = Report::new(
        "Bastion: closing a childless session returns its rent to the owner",
        "An owner opens a session (paying rent), then closes it with no child policies. \
         The session account is gone and the owner's balance grows by the refunded rent.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    let pre = ctx.svm.get_balance(&owner.pubkey()).unwrap();

    md.step("Open the session (owner pays rent + fee)");
    open_session(&mut ctx, &owner, &session_kp, s.bundle);
    let after_init = ctx.svm.get_balance(&owner.pubkey()).unwrap();
    md.check("init cost the owner rent + fee", true, after_init < pre);

    md.step("Close the session (no children): rent returns to the owner");
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::CloseSession {})
        .send_ok();
    let after_close = ctx.svm.get_balance(&owner.pubkey()).unwrap();

    md.check(
        "close returned rent (owner balance grew)",
        true,
        after_close > after_init,
    );
    md.check(
        "session account closed",
        false,
        ctx.account_exists(&s.session),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn close_session_rejects_when_policy_count_mismatch() {
    let mut md = Report::new(
        "Bastion: close rejects a child count that disagrees with policy_count",
        "A childless session is closed while a single stray account rides in the tail. \
         The session's policy_count is 0, so the one-account tail is a PolicyCountMismatch.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session (no policies attached)");
    open_session(&mut ctx, &owner, &session_kp, s.bundle);

    md.step("Close with one stray child: tail count disagrees with policy_count (0)");
    let fake = Pubkey::new_unique();
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::CloseSession {})
        .remaining_accounts(&child_metas(&[fake]))
        .send_err_named("PolicyCountMismatch");
    ctx.report_execution(&mut md);
}

#[test]
fn close_session_rejects_non_owner() {
    let mut md = Report::new(
        "Bastion: close rejects a non-owner via the session seeds constraint",
        "An attacker signs CloseSession against the real owner's session PDA. The session \
         seeds bind to the signer, so the recomputed PDA no longer matches: ConstraintSeeds.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let attacker = ctx.cast_actor("attacker");
    ctx.svm.airdrop(&attacker.pubkey(), ONE_SOL).unwrap();

    md.step("Open the session (owned by `owner`)");
    open_session(&mut ctx, &owner, &session_kp, s.bundle);

    // Keep the real session PDA, but swap the `owner` account to the attacker.
    // The seeds constraint recomputes [SEED_SESSION, attacker, session_key] and
    // mismatches the stored PDA: ConstraintSeeds (0x7d6).
    md.step("Attacker signs CloseSession against the owner's session: seeds violation");
    let mut bundle = s.bundle;
    bundle.owner = attacker.pubkey();
    ctx.tx(&[&attacker])
        .build(bundle, bastion::instruction::CloseSession {})
        .send_err_named("ConstraintSeeds");

    md.check(
        "session account still present",
        true,
        ctx.account_exists(&s.session),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn close_session_closes_session_and_three_child_policies() {
    let mut md = Report::new(
        "Bastion: close drains the session and all three child policies in one tx",
        "Three policies are attached (ProgramAllowlist, MaxIxSize, Expiry), then CloseSession \
         passes all three in the dispatch tail. Every account is closed and all the rent \
         (session + 3 policies) refunds to the owner.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    open_session(&mut ctx, &owner, &session_kp, s.bundle);

    md.step("Attach three policies (NextPolicy resolves seed 0, 1, 2)");
    // Each attach derives the next policy PDA from the live counter, so capture
    // the resolved address per attach for the dispatch tail and assertions.
    let p_a = ctx.svm.get_pda(
        &[
            bastion::constants::SEED_POLICY,
            s.session.as_ref(),
            &0u64.to_le_bytes(),
        ],
        &bastion::ID,
    );
    ctx.alias(p_a, "Policy[0] ProgramAllowlist");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ProgramAllowlist {
                    programs: vec![anchor_lang::system_program::ID],
                },
            },
        )
        .send_ok();

    ctx.svm.expire_blockhash();
    let p_b = ctx.svm.get_pda(
        &[
            bastion::constants::SEED_POLICY,
            s.session.as_ref(),
            &1u64.to_le_bytes(),
        ],
        &bastion::ID,
    );
    ctx.alias(p_b, "Policy[1] MaxIxSize");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::MaxIxSize {
                    max_accounts: 8,
                    max_data_len: 64,
                },
            },
        )
        .remaining_accounts(&[AccountMeta::new_readonly(p_a, false)])
        .send_ok();

    ctx.svm.expire_blockhash();
    let p_c = ctx.svm.get_pda(
        &[
            bastion::constants::SEED_POLICY,
            s.session.as_ref(),
            &2u64.to_le_bytes(),
        ],
        &bastion::ID,
    );
    ctx.alias(p_c, "Policy[2] Expiry");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 7_200,
                },
            },
        )
        .remaining_accounts(&[
            AccountMeta::new_readonly(p_a, false),
            AccountMeta::new_readonly(p_b, false),
        ])
        .send_ok();

    for p in [p_a, p_b, p_c] {
        md.check("policy attached and present", true, ctx.account_exists(&p));
    }

    let pre_close_balance = ctx.svm.get_balance(&owner.pubkey()).unwrap();

    md.step("Close: session + 3 child policies, all in one tx");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::CloseSession {})
        .remaining_accounts(&child_metas(&[p_a, p_b, p_c]))
        .send_ok();

    md.check(
        "session account closed",
        false,
        ctx.account_exists(&s.session),
    );
    for p in [p_a, p_b, p_c] {
        md.check("child policy closed", false, ctx.account_exists(&p));
    }

    let post_close_balance = ctx.svm.get_balance(&owner.pubkey()).unwrap();
    md.check(
        "rent from session + 3 policies refunded to owner",
        true,
        post_close_balance > pre_close_balance,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn close_session_rejects_foreign_policy_in_children() {
    let mut md = Report::new(
        "Bastion: close rejects a child policy that belongs to another session",
        "Owner attaches a real policy. A second owner opens a separate session and attaches \
         its own policy. Closing the first session with the foreign policy in the tail is \
         rejected (ForeignPolicy): the policy's stored session doesn't match.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session + attach its own real policy");
    open_session(&mut ctx, &owner, &session_kp, s.bundle);
    let real = ctx.svm.get_pda(
        &[
            bastion::constants::SEED_POLICY,
            s.session.as_ref(),
            &0u64.to_le_bytes(),
        ],
        &bastion::ID,
    );
    ctx.alias(real, "Policy (real)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ProgramAllowlist {
                    programs: vec![Pubkey::new_unique()],
                },
            },
        )
        .send_ok();

    md.step("A second owner opens its own session and attaches a foreign policy");
    let other_owner = ctx.cast_actor("other-owner");
    let other_session_kp = ctx.cast_actor("other-session-signer");
    ctx.svm
        .airdrop(&other_owner.pubkey(), ONE_SOL.saturating_mul(2))
        .unwrap();
    let other = cast_session(&mut ctx, &other_owner, &other_session_kp);

    ctx.svm.expire_blockhash();
    open_session(&mut ctx, &other_owner, &other_session_kp, other.bundle);
    let foreign = ctx.svm.get_pda(
        &[
            bastion::constants::SEED_POLICY,
            other.session.as_ref(),
            &0u64.to_le_bytes(),
        ],
        &bastion::ID,
    );
    ctx.alias(foreign, "Foreign policy");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&other_owner])
        .build(
            other.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::ProgramAllowlist {
                    programs: vec![Pubkey::new_unique()],
                },
            },
        )
        .send_ok();

    md.step("Close the first session with the foreign policy in the tail: ForeignPolicy");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::CloseSession {})
        .remaining_accounts(&child_metas(&[foreign]))
        .send_err_named("ForeignPolicy");

    md.check(
        "first session still present",
        true,
        ctx.account_exists(&s.session),
    );
    md.check("real policy still present", true, ctx.account_exists(&real));
    ctx.report_execution(&mut md);
}
