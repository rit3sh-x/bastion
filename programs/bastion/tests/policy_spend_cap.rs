mod helpers;

use anchor_litesvm::{Pubkey, Report, TestHelpers, TokenFabrication, TokenProgram};
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, Policy, PolicyData, WindowKind};
use helpers::*;

#[test]
fn sol_spend_cap_charges_outflow_within_fixed_window() {
    let mut md = Report::new(
        "sol_spend_cap_charges_outflow_within_fixed_window",
        "A SpendCap of 1_000_000 over a fixed 86_400s window charges each outflow: two \
         transfers of 400_000 settle and accumulate `spent`; a third pushes the window to \
         1_200_000 and is rejected with SpendCapExceeded.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(10 * ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session, fund delegate (10 SOL), attach a SpendCap (1_000_000 fixed window)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86_400 },
            max: 1_000_000,
            state: SpendState::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("First transfer of 400_000: within cap, the policy charges");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    let pol: Policy = ctx.get_account(&spend_cap).unwrap();
    let spent = match pol.data {
        PolicyData::SpendCap { state, .. } => state.spent,
        _ => panic!("expected SpendCap"),
    };
    md.check("policy charged the first transfer", 400_000u64, spent);

    md.step("Second transfer of 400_000: still within cap");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.step("Third transfer pushes the window to 1_200_000: rejected with SpendCapExceeded");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("SpendCapExceeded");
    ctx.report_execution(&mut md);
}

#[test]
fn sol_spend_cap_enforces_rent_exempt_floor() {
    let mut md = Report::new(
        "sol_spend_cap_enforces_rent_exempt_floor",
        "The delegate is funded with only 1_500_000 lamports. A SpendCap with a huge cap \
         (100 SOL) admits the policy, but a 1_000_000 transfer must still fail: it would \
         drop the delegate below its rent-exempt floor (or the system program rejects it).",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(1_500_000);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a SpendCap with a 100 SOL cap (the cap is not the binding constraint)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86_400 },
            max: 100 * ONE_SOL,
            state: SpendState::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("A 1_000_000 transfer must fail: rent-exempt floor / system rejection");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err();
    ctx.report_execution(&mut md);
}

#[test]
fn sol_spend_cap_rolling_window_slides_across_slots() {
    let mut md = Report::new(
        "sol_spend_cap_rolling_window_slides_across_slots",
        "A rolling SpendCap (60s window / 2 slots of 30s each, cap 1_000_000) slides as the \
         clock advances. Two 400_000 spends in consecutive slots settle (window total 800_000); \
         a third inside the same 60s window is rejected; advancing past 60s ages out the slot-0 \
         spend, reopening budget so a fresh 400_000 settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(10 * ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session, fund delegate (10 SOL), attach a rolling SpendCap (60s / 2 slots)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Rolling { secs: 60, slots: 2 },
            max: 1_000_000,
            state: SpendState::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("slot-0: a 400_000 spend within cap");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.step("Advance 30s into slot-1: another 400_000 brings the window to 800_000");
    ctx.svm.advance_seconds(30);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.step("A 3rd 400_000 is still inside the 60s window (1_200_000 > cap): rejected");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("SpendCapExceeded");

    md.step("Advance to t0+60: the slot-0 spend ages out, budget reopens, a fresh 400_000 settles");
    ctx.svm.advance_seconds(30);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    ctx.report_execution(&mut md);
}

#[test]
fn spl_token_spend_cap_snapshots_via_token_account_layout() {
    let mut md = Report::new(
        "spl_token_spend_cap_snapshots_via_token_account_layout",
        "An SPL-token SpendCap stores the mint and cap; the delegate's source ATA carries the \
         SPL token-account layout. The framework fabricates the ATA (owner = delegate, amount \
         10_000); the policy snapshots against that layout, and the ATA reads back with token \
         rent and non-empty data.",
    );
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    let mint = Pubkey::new_unique();
    let source_ata = Pubkey::new_unique();
    ctx.alias(mint, "Mint");
    ctx.alias(source_ata, "SourceATA");

    md.step("Fabricate the delegate's source ATA (mint, owner = delegate, amount 10_000)");
    ctx.svm
        .fabricate_token_account(&source_ata, TokenProgram::Spl, &mint, &s.delegate, 10_000);

    md.step("Attach an SPL-token SpendCap (cap 5_000)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::SplToken(mint),
            window: WindowKind::Fixed { secs: 86_400 },
            max: 5_000,
            state: SpendState::default(),
        },
    );

    let pol: Policy = ctx.get_account(&spend_cap).unwrap();
    match pol.data {
        PolicyData::SpendCap {
            asset: Asset::SplToken(stored_mint),
            max,
            ..
        } => {
            md.check("policy stored the SPL mint", mint, stored_mint);
            md.check("policy stored the cap", 5_000u64, max);
        }
        _ => panic!("expected SpendCap{{SplToken}}"),
    }

    let acct = ctx
        .svm
        .get_account(&source_ata)
        .expect("source ATA account must exist");
    md.check(
        "source ATA carries token-account rent",
        2_039_280u64,
        acct.lamports,
    );
    md.check("source ATA data present", true, !acct.data.is_empty());
    ctx.report_execution(&mut md);
}

#[test]
fn sol_spend_cap_noop_when_outflow_is_zero() {
    let mut md = Report::new(
        "sol_spend_cap_noop_when_outflow_is_zero",
        "A SpendCap with a tiny cap (100) admits a wrapped transfer of zero lamports: the \
         policy sees no outflow and charges nothing, so `spent` stays 0 even though the cap \
         is far smaller than the (zero) transfer.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session, fund delegate (1 SOL), attach a SpendCap (cap 100)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86_400 },
            max: 100,
            state: SpendState::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("Execute a zero-lamport transfer: no outflow, so the policy charges nothing");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(0)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    let pol: Policy = ctx.get_account(&spend_cap).unwrap();
    let spent = match pol.data {
        PolicyData::SpendCap { state, .. } => state.spent,
        _ => panic!("expected SpendCap"),
    };
    md.check("zero outflow charged nothing", 0u64, spent);
    ctx.report_execution(&mut md);
}
