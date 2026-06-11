mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_litesvm::{Report, TestHelpers};
use bastion::state::policy::PolicyData;
use helpers::*;

#[test]
fn cooldown_blocks_second_call_within_window() {
    let mut md = Report::new(
        "Bastion: a CooldownPeriod policy gates repeated executes by a 60s window",
        "A session carries a CooldownPeriod of 60s. The first execute seeds the timer; a \
         second execute 30s later (inside the window) is rejected with CooldownActive; a \
         third execute after the full 65s have elapsed is allowed again.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a CooldownPeriod (60s window, unscoped)");
    let cooldown = attach(
        &mut ctx,
        &owner,
        &s,
        "CooldownPeriod",
        PolicyData::CooldownPeriod {
            secs: 60,
            last_call_ts: 0,
            scope: None,
        },
    );
    let extras = transfer_tail(&[cooldown], s.delegate, recipient);

    md.step("First execute: seeds the cooldown timer");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.step("Second execute 30s later (inside the window): rejected with CooldownActive");
    ctx.svm.advance_seconds(30);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("CooldownActive");

    md.step("Third execute after 65s total (window elapsed): allowed again");
    ctx.svm.advance_seconds(35);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
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
fn cooldown_scope_filter_ignores_out_of_scope() {
    let mut md = Report::new(
        "Bastion: a scoped CooldownPeriod never arms for out-of-scope calls",
        "A CooldownPeriod scoped to a target the executes never touch never arms: five \
         back-to-back executes all pass, because the scope filter excludes them from the \
         timer.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a CooldownPeriod scoped to an unrelated target");
    let cooldown = attach(
        &mut ctx,
        &owner,
        &s,
        "CooldownPeriod",
        PolicyData::CooldownPeriod {
            secs: 60,
            last_call_ts: 0,
            scope: Some(Pubkey::new_unique()),
        },
    );
    let extras = transfer_tail(&[cooldown], s.delegate, recipient);

    md.step("Five back-to-back executes: all pass (out of scope, cooldown never arms)");
    for _ in 0..5 {
        ctx.svm.expire_blockhash();
        ctx.tx(&[&session_kp])
            .build(
                s.bundle,
                bastion::instruction::Execute {
                    wrapped_ixs: vec![transfer_wrapped(1_000)],
                    policy_count: 1,
                    expected_nonce: None,
                    manifest: None,
                },
            )
            .remaining_accounts(&extras)
            .send_ok();
    }

    ctx.report_execution(&mut md);
}
