mod helpers;

use anchor_litesvm::{Report, TestHelpers};
use bastion::state::policy::PolicyData;
use helpers::*;

/// Mon..Fri set, Sun/Sat clear (bit 0 = Sunday). Matches the original suite.
const WEEKDAY_MASK: u8 = 0x3E;

#[test]
fn time_of_day_allows_in_window_on_weekday() {
    let mut md = Report::new(
        "Bastion: a TimeOfDayWindow policy allows a transfer inside its window on a weekday",
        "The clock is pinned to Monday 10:00. A TimeOfDayWindow of 09:00..17:00 on weekdays \
         is attached; the session executes a wrapped SOL transfer and the policy passes \
         because 10:00 falls inside the window.",
    );
    let now = TEST_CLOCK_TS + 10 * 3600;
    let mut ctx = bastion_ctx();
    ctx.svm.warp_to_timestamp(now);

    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let recipient = ctx.cast_account("recipient");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open a funded session (clock pinned to Mon 10:00)");
    init_session(&mut ctx, &owner, &session_kp, &s, now + DAY);
    ctx.svm.airdrop(&s.delegate, ONE_SOL).unwrap();

    md.step("Attach a TimeOfDayWindow: 09:00..17:00 on weekdays");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "TimeOfDayWindow",
        PolicyData::TimeOfDayWindow {
            start_minute: 9 * 60,
            end_minute: 17 * 60,
            days_mask: WEEKDAY_MASK,
        },
    );

    md.step("Session executes a 1_000 lamport transfer: Mon 10:00 is in-window, allowed");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
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
        "recipient received the in-window transfer",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn time_of_day_blocks_outside_window_same_day() {
    let mut md = Report::new(
        "Bastion: a TimeOfDayWindow policy blocks a transfer outside its window",
        "The clock is pinned to Monday 18:00, one hour past the 09:00..17:00 window. \
         The session executes a wrapped SOL transfer and the policy rejects it with \
         OutsideAllowedTime.",
    );
    let now = TEST_CLOCK_TS + 18 * 3600;
    let mut ctx = bastion_ctx();
    ctx.svm.warp_to_timestamp(now);

    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let recipient = ctx.cast_account("recipient");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open a funded session (clock pinned to Mon 18:00)");
    init_session(&mut ctx, &owner, &session_kp, &s, now + DAY);
    ctx.svm.airdrop(&s.delegate, ONE_SOL).unwrap();

    md.step("Attach a TimeOfDayWindow: 09:00..17:00 on weekdays");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "TimeOfDayWindow",
        PolicyData::TimeOfDayWindow {
            start_minute: 9 * 60,
            end_minute: 17 * 60,
            days_mask: WEEKDAY_MASK,
        },
    );

    md.step("Session executes a transfer at 18:00: past the window, rejected");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
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
        .send_err_named("OutsideAllowedTime");

    md.check(
        "recipient balance unchanged (transfer never settled)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn time_of_day_blocks_disallowed_day() {
    let mut md = Report::new(
        "Bastion: a TimeOfDayWindow policy blocks a transfer on a disallowed day",
        "The clock is pinned to Saturday 12:00 (day 5 past the Monday anchor). The window \
         is all-day (00:00..24:00) but Saturday's bit is clear in the weekday mask, so the \
         execute is rejected with OutsideAllowedTime even at midday.",
    );
    let now = TEST_CLOCK_TS + 5 * DAY + 12 * 3600;
    let mut ctx = bastion_ctx();
    ctx.svm.warp_to_timestamp(now);

    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let recipient = ctx.cast_account("recipient");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open a funded session (clock pinned to Sat 12:00)");
    init_session(&mut ctx, &owner, &session_kp, &s, now + DAY);
    ctx.svm.airdrop(&s.delegate, ONE_SOL).unwrap();

    md.step("Attach an all-day TimeOfDayWindow restricted to weekdays");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "TimeOfDayWindow",
        PolicyData::TimeOfDayWindow {
            start_minute: 0,
            end_minute: 1440,
            days_mask: WEEKDAY_MASK,
        },
    );

    md.step("Session executes on Saturday: day bit is clear, rejected");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
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
        .send_err_named("OutsideAllowedTime");

    md.check(
        "recipient balance unchanged (disallowed day)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn time_of_day_rejects_invalid_params_at_attach() {
    let mut md = Report::new(
        "Bastion: an inverted TimeOfDayWindow is rejected at attach time",
        "A window with start_minute (17:00) after end_minute (09:00) is malformed. The \
         AttachPolicy validates the window and rejects it with InvalidPolicyData before \
         any execute is attempted.",
    );
    // No custom clock here, so `bootstrap` (pinned at TEST_CLOCK_TS) is fine; the
    // attach is under test, so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);
    let policy = cast_policy(&mut ctx, &s, "TimeOfDayWindow");

    md.step("Attach an inverted window (start 17:00 > end 09:00): rejected at validation");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::TimeOfDayWindow {
                    start_minute: 17 * 60,
                    end_minute: 9 * 60,
                    days_mask: WEEKDAY_MASK,
                },
            },
        )
        .send_err_named("InvalidPolicyData");

    md.check(
        "the malformed policy slot was never created",
        true,
        ctx.svm.get_account(&policy).is_none(),
    );
    ctx.report_execution(&mut md);
}
