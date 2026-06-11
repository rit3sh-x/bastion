mod helpers;

use anchor_litesvm::{Report, TestHelpers};
use bastion::state::policy::{Asset, PolicyData};
use helpers::*;

#[test]
fn amount_per_call_blocks_over_limit_single_transfer() {
    let mut md = Report::new(
        "Bastion: AmountPerCall caps each transfer independently (stateless)",
        "A session carries an AmountPerCall of 100_000. Two under-limit transfers \
         (50_000 then 80_000) both pass: the check is per-call, so it does not \
         accumulate. A single 200_000 transfer exceeds the per-call ceiling and is \
         rejected with AmountPerCallExceeded.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach an AmountPerCall (NativeSol, max 100_000)");
    let amount_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "AmountPerCall",
        PolicyData::AmountPerCall {
            asset: Asset::NativeSol,
            max: 100_000,
        },
    );
    let extras = transfer_tail(&[amount_cap], s.delegate, recipient);

    md.step("A 50_000 transfer: under the per-call limit, passes");
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

    md.step("An 80_000 transfer: still under the limit, passes (stateless per-call)");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(80_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.check(
        "recipient received both under-limit transfers (baseline + 50_000 + 80_000)",
        Some(ONE_SOL + 50_000 + 80_000),
        ctx.svm.get_balance(&recipient),
    );

    md.step("A 200_000 transfer: over the per-call ceiling, rejected");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(200_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("AmountPerCallExceeded");

    md.check(
        "recipient balance unchanged after the rejected over-limit transfer",
        Some(ONE_SOL + 50_000 + 80_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn amount_per_call_rejects_unimplemented_nft_count_at_attach() {
    let mut md = Report::new(
        "Bastion: AmountPerCall over AnyNftCount is rejected at attach",
        "AmountPerCall is only implemented for NativeSol (and token assets). An \
         AnyNftCount variant has no meaningful per-call NFT semantics, so the \
         program rejects it at AttachPolicy with InvalidPolicyData rather than \
         silently accepting a no-op policy.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Attach AmountPerCall over AnyNftCount: rejected with InvalidPolicyData");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::AmountPerCall {
                    asset: Asset::AnyNftCount,
                    max: 1,
                },
            },
        )
        .send_err_named("InvalidPolicyData");
    ctx.report_execution(&mut md);
}

#[test]
fn cooldown_and_amount_per_call_compose() {
    let mut md = Report::new(
        "Bastion: a Cooldown and an AmountPerCall compose over one execute",
        "Two policies guard the same session: a 30s Cooldown (slot 0) and an \
         AmountPerCall of 50_000 (slot 1). The first 40_000 call passes both. A \
         second immediate 40_000 call trips the Cooldown (CooldownActive) even \
         though it is under the amount limit. After advancing past the cooldown, a \
         60_000 call clears the cooldown but trips the amount limit \
         (AmountPerCallExceeded). Both policies see every call.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a 30s Cooldown (slot 0) then an AmountPerCall of 50_000 (slot 1)");
    let policies = attach_all(
        &mut ctx,
        &owner,
        &s,
        vec![
            (
                "Cooldown",
                PolicyData::CooldownPeriod {
                    secs: 30,
                    last_call_ts: 0,
                    scope: None,
                },
            ),
            (
                "AmountPerCall",
                PolicyData::AmountPerCall {
                    asset: Asset::NativeSol,
                    max: 50_000,
                },
            ),
        ],
    );
    // Both policies in the dispatch tail (writable: cooldown stamps its
    // last_call_ts), then the transfer's positional accounts.
    let extras = transfer_tail(&policies, s.delegate, recipient);

    md.step("Call 1 (40_000): within both the cooldown window and the amount cap");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(40_000)],
                policy_count: 2,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.step("Call 2 (40_000) immediately: under the amount cap but inside the cooldown");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(40_000)],
                policy_count: 2,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("CooldownActive");

    md.step("Advance 35s past the cooldown, then a 60_000 call: cooldown clears, amount trips");
    ctx.svm.advance_seconds(35);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(60_000)],
                policy_count: 2,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("AmountPerCallExceeded");

    md.check(
        "recipient received only the one allowed call (baseline + 40_000)",
        Some(ONE_SOL + 40_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
