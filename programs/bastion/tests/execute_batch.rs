//! Batch (multi-leg) Execute semantics: per-leg charging, atomic revert when a
//! leg trips a policy, the empty-batch guard, per-batch nonce + chain-hash
//! advance, and the optional expected-nonce assertion.

mod helpers;

use anchor_litesvm::{AnchorContext, Pubkey, Report};
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, Policy, PolicyData, WindowKind};
use bastion::state::session::Session;
use helpers::*;

fn spend_cap(max: u64) -> PolicyData {
    PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max,
        state: SpendState::default(),
    }
}

fn dest_lamports(ctx: &AnchorContext, dest: &Pubkey) -> u64 {
    ctx.svm.get_balance(dest).unwrap_or(0)
}

#[test]
fn batch_applies_all_legs_and_charges_per_leg() {
    let mut md = Report::new(
        "Bastion: a 2-leg batch applies every leg and charges the SpendCap per leg",
        "A session carries a SpendCap of 1_000_000. One Execute carries two legs \
         (100_000 + 200_000); both transfers settle, the policy accumulates 300_000 \
         across the legs, and the session's nonce advances exactly once for the batch.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open the session and attach a SpendCap (1_000_000 per fixed window)");
    let policy = attach(&mut ctx, &owner, &s, "SpendCap", spend_cap(1_000_000));
    let extras = transfer_tail(&[policy], s.delegate, recipient);
    let before = dest_lamports(&ctx, &recipient);

    md.step("One Execute carries two legs (100_000 + 200_000), both within the cap");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(100_000), transfer_wrapped(200_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.check(
        "both legs transferred (recipient delta)",
        300_000u64,
        dest_lamports(&ctx, &recipient) - before,
    );

    let pol: Policy = ctx.get_account(&policy).unwrap();
    let spent = match pol.data {
        PolicyData::SpendCap { state, .. } => state.spent,
        _ => panic!("expected SpendCap"),
    };
    md.check("spend accumulated across legs", 300_000u64, spent);

    let session: Session = ctx.get_account(&s.session).unwrap();
    md.check(
        "one nonce increment for the whole batch",
        1u64,
        session.action_nonce,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn batch_reverts_atomically_when_a_leg_exceeds_cap() {
    let mut md = Report::new(
        "Bastion: a batch reverts atomically when one leg pushes the SpendCap over",
        "A session carries a SpendCap of 150_000. One Execute carries two legs of \
         100_000; the second leg pushes the window total to 200_000 and the whole \
         batch reverts with SpendCapExceeded: no transfer settles, no spend persists, \
         no nonce advances.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open the session and attach a SpendCap (150_000 per fixed window)");
    let policy = attach(&mut ctx, &owner, &s, "SpendCap", spend_cap(150_000));
    let extras = transfer_tail(&[policy], s.delegate, recipient);
    let before = dest_lamports(&ctx, &recipient);

    md.step("A 2-leg batch (100_000 + 100_000) exceeds the cap on leg 2: rejected");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(100_000), transfer_wrapped(100_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("SpendCapExceeded");

    md.check(
        "leg-1 transfer rolled back (atomic)",
        before,
        dest_lamports(&ctx, &recipient),
    );

    let pol: Policy = ctx.get_account(&policy).unwrap();
    let spent = match pol.data {
        PolicyData::SpendCap { state, .. } => state.spent,
        _ => panic!("expected SpendCap"),
    };
    md.check("no spend persisted", 0u64, spent);

    let session: Session = ctx.get_account(&s.session).unwrap();
    md.check(
        "nonce not incremented on failed batch",
        0u64,
        session.action_nonce,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn empty_batch_rejected() {
    let mut md = Report::new(
        "Bastion: an empty batch is rejected",
        "An Execute carrying zero legs is rejected with EmptyBatch before any \
         dispatch runs.",
    );
    let (mut ctx, _owner, session_kp, s) = bootstrap(ONE_SOL);

    md.step("Execute with no legs: rejected with EmptyBatch");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .send_err_named("EmptyBatch");
    ctx.report_execution(&mut md);
}

#[test]
fn cooldown_same_scope_batch_rejects() {
    let mut md = Report::new(
        "Bastion: a CooldownPeriod policy rejects a batch in the same scope",
        "A session carries a CooldownPeriod of 60s. A 2-leg batch in the same scope \
         trips the cooldown on leg 2 and the whole batch reverts with CooldownActive.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open the session and attach a CooldownPeriod (60s)");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "Cooldown",
        PolicyData::CooldownPeriod {
            secs: 60,
            last_call_ts: 0,
            scope: None,
        },
    );
    let extras = transfer_tail(&[policy], s.delegate, recipient);

    md.step("A 2-leg batch in the same scope trips the cooldown on leg 2: rejected");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000), transfer_wrapped(1_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("CooldownActive");
    ctx.report_execution(&mut md);
}

#[test]
fn chain_hash_advances_per_execute() {
    let mut md = Report::new(
        "Bastion: the session's chain hash advances per Execute",
        "Each Execute folds the action into the session's chain hash. From genesis \
         (all-zero), the first Execute advances it off zero, and the second advances \
         it again.",
    );
    let (mut ctx, _owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let extras = transfer_tail(&[], s.delegate, recipient);

    let genesis: Session = ctx.get_account(&s.session).unwrap();
    md.check(
        "chain hash starts at genesis",
        [0u8; 32],
        genesis.chain_hash,
    );

    md.step("Execute 1: chain advances off genesis");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    let h1 = ctx.get_account::<Session>(&s.session).unwrap().chain_hash;
    md.check("chain advanced off genesis", true, h1 != [0u8; 32]);

    md.step("Execute 2: chain advances again");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    let h2 = ctx.get_account::<Session>(&s.session).unwrap().chain_hash;
    md.check("chain advanced again", true, h2 != h1);
    ctx.report_execution(&mut md);
}

#[test]
fn nonce_increments_per_execute() {
    let mut md = Report::new(
        "Bastion: the session's action nonce increments per Execute",
        "Each Execute commits one nonce increment. From 0, the first Execute lands at \
         1, the second at 2.",
    );
    let (mut ctx, _owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let extras = transfer_tail(&[], s.delegate, recipient);

    md.check(
        "nonce starts at 0",
        0u64,
        ctx.get_account::<Session>(&s.session).unwrap().action_nonce,
    );

    md.step("Execute 1: nonce -> 1");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    md.check(
        "nonce after execute 1",
        1u64,
        ctx.get_account::<Session>(&s.session).unwrap().action_nonce,
    );

    md.step("Execute 2: nonce -> 2");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    md.check(
        "nonce after execute 2",
        2u64,
        ctx.get_account::<Session>(&s.session).unwrap().action_nonce,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn nonce_assertion_enforced() {
    let mut md = Report::new(
        "Bastion: an Execute can assert the expected nonce",
        "An Execute may pin its expected nonce. Asserting 5 while the session is at 0 \
         is rejected with NonceMismatch; asserting the matching 0 passes and advances \
         the nonce to 1.",
    );
    let (mut ctx, _owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let extras = transfer_tail(&[], s.delegate, recipient);

    md.step("Assert nonce 5 while the session is at 0: rejected with NonceMismatch");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 0,
                expected_nonce: Some(5),
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("NonceMismatch");

    md.step("Assert the matching nonce 0: passes and advances to 1");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 0,
                expected_nonce: Some(0),
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    md.check(
        "matching nonce passed and advanced",
        1u64,
        ctx.get_account::<Session>(&s.session).unwrap().action_nonce,
    );
    ctx.report_execution(&mut md);
}
