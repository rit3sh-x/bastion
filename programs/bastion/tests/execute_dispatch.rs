mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_lang::system_program;
use anchor_litesvm::{Report, TestHelpers};
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use bastion::utils::helpers::{BastionBundle, SessionRoot};
use helpers::*;
use solana_signer::Signer;

/// An empty wrapped ix (no inner accounts, no data): the dispatch gate runs but
/// the inner CPI is a no-op. Used by the negative paths that reject *before*
/// dispatch ever fires.
fn empty_wrapped() -> WrappedInstruction {
    WrappedInstruction {
        program_id: system_program::ID,
        accounts: vec![],
        data: vec![],
    }
}

#[test]
fn execute_with_active_expiry_policy_passes() {
    let mut md = Report::new(
        "execute_with_active_expiry_policy_passes: an active Expiry policy lets the transfer through",
        "Open a session, attach an Expiry policy whose window is still open, then execute a \
         wrapped SOL transfer. The policy passes and the delegate's transfer settles.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let recipient = ctx.cast_account("recipient");
    let expiry = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    // `cast_account` rent-funds the recipient with ONE_SOL already, so the
    // post-transfer balance is that starting balance plus the 50_000 transfer.
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();
    ctx.svm.airdrop(&s.delegate, ONE_SOL).unwrap();

    md.step("Owner opens a session and attaches an active Expiry policy");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 60,
                },
            },
        )
        .send_ok();

    md.step("Session executes a wrapped SOL transfer: bundle base + remaining_accounts tail");
    let extras = vec![
        AccountMeta::new_readonly(expiry, false),
        AccountMeta::new(s.delegate, false),
        AccountMeta::new(s.delegate, false),
        AccountMeta::new(recipient, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(50_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 50_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn execute_with_expired_expiry_policy_fails() {
    let mut md = Report::new(
        "execute_with_expired_expiry_policy_fails: an Expiry policy past its window rejects",
        "Attach an Expiry policy with a 30s window, advance the clock past it, then execute. \
         The gate raises ExpiryViolation before dispatch.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let expiry = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();

    md.step("Open session + attach a short-window Expiry policy (30s)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 30,
                },
            },
        )
        .send_ok();

    md.step("Advance the clock 60s past the window, then execute: ExpiryViolation");
    ctx.svm.advance_seconds(60);
    let extras = vec![
        AccountMeta::new_readonly(expiry, false),
        AccountMeta::new_readonly(s.delegate, false),
    ];
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("ExpiryViolation");
    ctx.report_execution(&mut md);
}

#[test]
fn execute_missing_policy_fails_count_mismatch() {
    let mut md = Report::new(
        "execute_missing_policy_fails_count_mismatch: a stated count of 0 with a live policy rejects",
        "A policy is attached, but the execute declares policy_count 0. The gate compares the \
         session's policy_count against the stated count and raises PolicyCountMismatch.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let _expiry = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();

    md.step("Open session + attach an Expiry policy (session.policy_count becomes 1)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 60,
                },
            },
        )
        .send_ok();

    md.step("Execute declares policy_count 0 while the session has 1: PolicyCountMismatch");
    let extras = vec![AccountMeta::new_readonly(s.delegate, false)];
    ctx.svm.expire_blockhash();
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
        .send_err_named("PolicyCountMismatch");
    ctx.report_execution(&mut md);
}

#[test]
fn execute_wrong_policy_set_fails_foreign_policy() {
    let mut md = Report::new(
        "execute_wrong_policy_set_fails_foreign_policy: a sibling session's policy is rejected",
        "Two sessions each carry an Expiry policy. Session A executes but passes session B's \
         policy in the tail. The gate checks PDA derivation against A's session and raises \
         ForeignPolicy.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_a_kp = ctx.cast_actor("session-signer");
    let s_a = cast_session(&mut ctx, &owner, &session_a_kp);

    // A second, sibling session under the same owner. Its bundle is built
    // directly from a SessionRoot so its policy lives in a different PDA family;
    // alias it distinctly so the structured tree keeps the two sessions apart.
    let session_b_kp = ctx.cast_actor("session-signer-b");
    let bundle_b = BastionBundle::from(&SessionRoot {
        owner: owner.pubkey(),
        session_key: session_b_kp.pubkey(),
    });
    ctx.alias(bundle_b.session, "SessionB");
    let policy_b = Pubkey::find_program_address(
        &[
            bastion::constants::SEED_POLICY,
            bundle_b.session.as_ref(),
            &0u64.to_le_bytes(),
        ],
        &bastion::ID,
    )
    .0;
    ctx.alias(policy_b, "ForeignPolicy");
    ctx.svm.airdrop(&session_a_kp.pubkey(), ONE_SOL).unwrap();

    md.step("Open both sessions and attach an Expiry policy to each");
    for (kp, bundle) in [(&session_a_kp, s_a.bundle), (&session_b_kp, bundle_b)] {
        ctx.svm.expire_blockhash();
        ctx.tx(&[&owner])
            .build(
                bundle,
                bastion::instruction::InitSession {
                    args: bastion::InitSessionArgs {
                        session_key: kp.pubkey(),
                        expiry: TEST_CLOCK_TS + 3600,
                    },
                },
            )
            .send_ok();
        ctx.svm.expire_blockhash();
        ctx.tx(&[&owner])
            .build(
                bundle,
                bastion::instruction::AttachPolicy {
                    data: PolicyData::Expiry {
                        not_after: TEST_CLOCK_TS + 60,
                    },
                },
            )
            .send_ok();
    }

    md.step("Session A executes with B's policy in the policy slot: ForeignPolicy");
    let extras = vec![
        AccountMeta::new_readonly(policy_b, false),
        AccountMeta::new_readonly(s_a.delegate, false),
    ];
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_a_kp])
        .build(
            s_a.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("ForeignPolicy");
    ctx.report_execution(&mut md);
}

#[test]
fn execute_foreign_account_in_policy_slot_fails() {
    let mut md = Report::new(
        "execute_foreign_account_in_policy_slot_fails: an arbitrary account in the policy slot is rejected",
        "Attach a real Expiry policy, then execute but place a freshly funded foreign key in \
         the policy slot. The gate's PDA derivation fails and raises ForeignPolicy.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let _expiry = cast_policy(&mut ctx, &s, "ExpiryPolicy");
    let fake = ctx.cast_actor("imposter");
    ctx.alias(fake.pubkey(), "imposter");
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();
    ctx.svm.airdrop(&fake.pubkey(), ONE_SOL).unwrap();

    md.step("Open session + attach an Expiry policy");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 60,
                },
            },
        )
        .send_ok();

    md.step("Execute with a foreign key in the policy slot: ForeignPolicy");
    let extras = vec![
        AccountMeta::new_readonly(fake.pubkey(), false),
        AccountMeta::new_readonly(s.delegate, false),
    ];
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("ForeignPolicy");
    ctx.report_execution(&mut md);
}

#[test]
fn execute_with_no_policies_when_count_zero_passes() {
    let mut md = Report::new(
        "execute_with_no_policies_when_count_zero_passes: zero policies + count 0 dispatches",
        "A session with no policies attached executes a wrapped SOL transfer with policy_count \
         0. The gate finds nothing to enforce and the delegate's transfer settles.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let recipient = ctx.cast_account("recipient");
    // `cast_account` rent-funds the recipient with ONE_SOL already.
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();
    ctx.svm.airdrop(&s.delegate, ONE_SOL).unwrap();

    md.step("Open a session with no policies attached");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Execute a wrapped SOL transfer with policy_count 0");
    let extras = vec![
        AccountMeta::new(s.delegate, false),
        AccountMeta::new(s.delegate, false),
        AccountMeta::new(recipient, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(7_000)],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 7_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn execute_rejects_too_many_policies() {
    let mut md = Report::new(
        "execute_rejects_too_many_policies: a count beyond the cap is rejected up front",
        "Execute declares policy_count 33 (over the per-session cap) with 33 throwaway metas. \
         The gate rejects with PolicyTooMany before touching any account.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();

    md.step("Open a session");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Execute declares policy_count 33 with 33 throwaway metas: PolicyTooMany");
    let metas: Vec<AccountMeta> = (0..33)
        .map(|_| AccountMeta::new_readonly(Pubkey::new_unique(), false))
        .collect();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![empty_wrapped()],
                policy_count: 33,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&metas)
        .send_err_named("PolicyTooMany");
    ctx.report_execution(&mut md);
}

#[test]
fn execute_rejects_foreign_signer_in_wrapped_ix() {
    let mut md = Report::new(
        "execute_rejects_foreign_signer_in_wrapped_ix: a non-delegate signer in the inner ix is rejected",
        "The wrapped ix marks account index 1 (a foreign funded key) as a signer. Only the \
         delegate may sign inner ixs, so the gate raises ForeignSignerNotAllowed.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let recipient = ctx.cast_account("recipient");
    let foreign = ctx.cast_actor("foreign-signer");
    ctx.alias(foreign.pubkey(), "foreign-signer");
    ctx.svm.airdrop(&session_kp.pubkey(), ONE_SOL).unwrap();
    ctx.svm.airdrop(&s.delegate, ONE_SOL).unwrap();
    ctx.svm.airdrop(&foreign.pubkey(), ONE_SOL).unwrap();

    md.step("Open a session");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Execute a wrapped ix whose second account is marked signer (a foreign key)");
    let mut data = vec![0u8; 12];
    data[0..4].copy_from_slice(&2u32.to_le_bytes());
    data[4..12].copy_from_slice(&1_000u64.to_le_bytes());
    let wix = WrappedInstruction {
        program_id: system_program::ID,
        accounts: vec![
            CompactAccountMeta::new(0, true, true),
            CompactAccountMeta::new(1, false, true),
        ],
        data,
    };
    let extras = vec![
        AccountMeta::new(s.delegate, false),
        AccountMeta::new(foreign.pubkey(), false),
        AccountMeta::new(recipient, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![wix],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("ForeignSignerNotAllowed");
    ctx.report_execution(&mut md);
}
