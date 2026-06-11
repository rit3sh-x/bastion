mod helpers;

use anchor_lang::system_program;
use anchor_litesvm::{Report, TestHelpers};
use bastion::state::counter::{CounterState, SpendState};
use bastion::state::policy::{Asset, PolicyData, WindowKind};
use helpers::*;

#[test]
fn demo_full_scenario_replay() {
    let mut md = Report::new(
        "Bastion: the full policy stack replayed end to end",
        "One session carries three stacked policies: ProgramAllowlist, RateLimit (max 3 per \
         60s), and SpendCap (500_000 per 60s). Three 100_000 transfers pass; the fourth trips \
         RateLimitExceeded. After the window resets, a 600_000 transfer trips SpendCapExceeded, \
         and a final 50_000 transfer passes within the reset cap.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(20 * ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach the stacked policies: ProgramAllowlist, then RateLimit, then SpendCap");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramAllowlist",
        PolicyData::ProgramAllowlist {
            programs: vec![system_program::ID],
        },
    );
    let rate_limit = attach(
        &mut ctx,
        &owner,
        &s,
        "RateLimit",
        PolicyData::RateLimit {
            window: WindowKind::Fixed { secs: 60 },
            max: 3,
            state: CounterState::default(),
            scope: None,
        },
    );
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 60 },
            max: 500_000,
            state: SpendState::default(),
        },
    );

    let extras = transfer_tail(&[allowlist, rate_limit, spend_cap], s.delegate, recipient);

    md.step("Three transfers of 100_000: all within rate limit (max 3) and cap");
    for _ in 0..3 {
        ctx.svm.expire_blockhash();
        ctx.tx(&[&session_kp])
            .build(
                s.bundle,
                bastion::instruction::Execute {
                    wrapped_ixs: vec![transfer_wrapped(100_000)],
                    policy_count: 3,
                    expected_nonce: None,
                    manifest: None,
                },
            )
            .remaining_accounts(&extras)
            .send_ok();
    }

    md.step("Fourth transfer in the same window: rejected with RateLimitExceeded");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(100_000)],
                policy_count: 3,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("RateLimitExceeded");

    md.step("Advance the clock 65s: the rate-limit and spend windows reset");
    ctx.svm.advance_seconds(65);

    md.step("A 600_000 transfer in the fresh window: over the 500_000 cap, SpendCapExceeded");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(600_000)],
                policy_count: 3,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("SpendCapExceeded");

    md.step("A 50_000 transfer within the reset cap: passes");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(50_000)],
                policy_count: 3,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.check(
        "recipient received the 3 allowed transfers plus the final 50_000",
        Some(ONE_SOL + 3 * 100_000 + 50_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
