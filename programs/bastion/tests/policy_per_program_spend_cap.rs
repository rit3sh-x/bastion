mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::system_program;
use anchor_litesvm::{MarkdownBlock, Report};
use bastion::state::policy::{Asset, Policy, PolicyData, WindowKind};
use helpers::*;

#[test]
fn per_program_spend_cap_charges_when_in_scope() {
    let mut md = Report::new(
        "Bastion: a PerProgramSpendCap charges in-scope outflow, then rejects over the cap",
        "A session carries a PerProgramSpendCap scoped to the System program with max 5_000 \
         per fixed window. A first transfer of 3_000 targets the System program and charges \
         within the cap; a second 3_000 would push the window total to 6_000 and is rejected \
         with ProgramSpendCapExceeded.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a PerProgramSpendCap (System program, 5_000 per window)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "PerProgramSpendCap",
        PolicyData::PerProgramSpendCap {
            program: system_program::ID,
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86_400 },
            max: 5_000,
            state: Default::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("A 3_000 transfer targets the System program: in scope, charged within the cap");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(3_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    let pol: Policy = ctx.get_account(&spend_cap).unwrap();
    let spent = match pol.data {
        PolicyData::PerProgramSpendCap { state, .. } => state.spent,
        _ => 0,
    };
    md.check(
        "policy charged the in-scope transfer (window total)",
        3_000u64,
        spent,
    );

    md.step(
        "A second 3_000 pushes the window to 6_000 > 5_000: rejected with ProgramSpendCapExceeded",
    );
    ctx.svm.expire_blockhash();
    let rejection = ctx
        .tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(3_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("ProgramSpendCapExceeded");
    md.block(
        "rejection logs",
        MarkdownBlock::Fenced {
            lang: "console".into(),
            body: rejection.logs_structured_string(),
        },
    );
    ctx.report_execution(&mut md);
}

#[test]
fn per_program_spend_cap_noop_when_out_of_scope() {
    let mut md = Report::new(
        "Bastion: a PerProgramSpendCap scoped to another program no-ops",
        "A session carries a PerProgramSpendCap with max 1, but scoped to a random program \
         (not the System program). A transfer of 50_000 targets the System program, so the \
         policy is out of scope and never charges: the execute succeeds even though the \
         spend dwarfs the cap.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let other_program = Pubkey::new_unique();

    md.step("Open session + attach a PerProgramSpendCap scoped to a different program (max 1)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "PerProgramSpendCap",
        PolicyData::PerProgramSpendCap {
            program: other_program,
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86_400 },
            max: 1,
            state: Default::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("A 50_000 System-program transfer: out of scope for this policy, so it no-ops");
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

    let pol: Policy = ctx.get_account(&spend_cap).unwrap();
    let spent = match pol.data {
        PolicyData::PerProgramSpendCap { state, .. } => state.spent,
        _ => 0,
    };
    md.check(
        "out-of-scope policy never charged despite the 50_000 spend",
        0u64,
        spent,
    );
    ctx.report_execution(&mut md);
}
