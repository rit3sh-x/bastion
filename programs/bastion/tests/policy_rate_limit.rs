mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::{AnchorContext, Keypair, Report, TestHelpers};
use bastion::state::counter::CounterState;
use bastion::state::policy::{PolicyData, WindowKind};
use helpers::*;

/// The recipient's starting balance before any transfer: `cast_account` rent-
/// funds it with ONE_SOL.
const RECIPIENT_BASE: u64 = ONE_SOL;

#[test]
fn rate_limit_fixed_allows_up_to_max_then_blocks() {
    let mut md = Report::new(
        "Bastion: a Fixed-window RateLimit allows up to max, blocks, then resets",
        "A session carries a RateLimit of max 3 over a fixed 60s window. Three executes \
         pass; the fourth is rejected with RateLimitExceeded. After the clock advances past \
         the window boundary the counter resets and the next execute passes again.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a RateLimit (max 3 per fixed 60s window)");
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
    let extras = transfer_tail(&[rate_limit], s.delegate, recipient);

    md.step("Three executes within the window: all pass");
    for _ in 0..3 {
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

    md.step("The fourth execute exceeds max: rejected with RateLimitExceeded");
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
        .send_err_named("RateLimitExceeded");

    md.step("Advance past the window boundary (+65s): the counter resets, next execute passes");
    ctx.svm.advance_seconds(65);
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

    md.check(
        "recipient received the 3 in-window + 1 post-reset transfers",
        Some(RECIPIENT_BASE + 4 * 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rate_limit_scope_filter_ignores_out_of_scope_calls() {
    let mut md = Report::new(
        "Bastion: a scoped RateLimit ignores out-of-scope calls",
        "A RateLimit of max 1 is scoped to a program that is never targeted. Five SOL \
         transfers (all out of scope) execute freely: the scope filter means the counter \
         never advances, so none are rejected.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a RateLimit (max 1) scoped to a program never targeted");
    let rate_limit = attach(
        &mut ctx,
        &owner,
        &s,
        "RateLimit",
        PolicyData::RateLimit {
            window: WindowKind::Fixed { secs: 60 },
            max: 1,
            state: CounterState::default(),
            scope: Some(Pubkey::new_unique()),
        },
    );
    let extras = transfer_tail(&[rate_limit], s.delegate, recipient);

    md.step("Five out-of-scope executes: none counted, all pass");
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

    md.check(
        "all five out-of-scope transfers settled",
        Some(RECIPIENT_BASE + 5 * 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rate_limit_rolling_window_slides_across_slots() {
    let mut md = Report::new(
        "Bastion: a Rolling-window RateLimit slides its budget across slots",
        "A RateLimit of max 4 over a rolling 60s window split into 2 slots (30s each). Two \
         calls land in slot 0, two more in slot 1 (filling the window to 4). A fifth call is \
         still inside the 60s window and is rejected. After another 30s the slot-0 calls age \
         out (now 60s old), reopening exactly that budget, and a new call passes.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a Rolling RateLimit (max 4, 60s window / 2 slots of 30s)");
    let rate_limit = attach(
        &mut ctx,
        &owner,
        &s,
        "RateLimit",
        PolicyData::RateLimit {
            window: WindowKind::Rolling { secs: 60, slots: 2 },
            max: 4,
            state: CounterState::default(),
            scope: None,
        },
    );
    let extras = transfer_tail(&[rate_limit], s.delegate, recipient);

    // Each beat sends the same single-transfer Execute; only the expected
    // outcome (ok vs RateLimitExceeded) differs, so a tiny helper builds the ix
    // and the caller picks the terminal assertion.
    fn exec(
        ctx: &mut AnchorContext,
        s: &SessionCast,
        kp: &Keypair,
        extras: &[AccountMeta],
    ) -> anchor_litesvm::TransactionResult {
        ctx.svm.expire_blockhash();
        ctx.tx(&[kp])
            .build(
                s.bundle,
                bastion::instruction::Execute {
                    wrapped_ixs: vec![transfer_wrapped(1_000)],
                    policy_count: 1,
                    expected_nonce: None,
                    manifest: None,
                },
            )
            .remaining_accounts(extras)
            .send_ok()
    }

    md.step("Slot 0: two calls land in the first slot");
    for _ in 0..2 {
        exec(&mut ctx, &s, &session_kp, &extras);
    }

    md.step("Advance into slot 1 (+30s): two more calls fill the window to max (4)");
    ctx.svm.advance_seconds(30);
    for _ in 0..2 {
        exec(&mut ctx, &s, &session_kp, &extras);
    }

    md.step("Fifth call still inside the 60s window: rejected with RateLimitExceeded");
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
        .send_err_named("RateLimitExceeded");

    md.step("Advance a full window past slot 0 (+30s): slot-0 calls age out, budget reopens");
    ctx.svm.advance_seconds(30);
    exec(&mut ctx, &s, &session_kp, &extras);

    md.check(
        "recipient received the 5 accepted transfers",
        Some(RECIPIENT_BASE + 5 * 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
