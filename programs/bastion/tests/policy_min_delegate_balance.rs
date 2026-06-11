mod helpers;

use anchor_litesvm::Report;
use bastion::state::policy::PolicyData;
use helpers::*;

#[test]
fn min_delegate_balance_passes_above_floor() {
    let mut md = Report::new(
        "Bastion: a transfer that leaves the delegate above the MinDelegateBalance floor passes",
        "A session carries a MinDelegateBalance floor of ONE_SOL/2. The delegate holds \
         ONE_SOL; a 100_000 transfer leaves it well above the floor, so the post-CPI \
         floor check passes and the transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a MinDelegateBalance floor of ONE_SOL/2");
    let floor_policy = attach(
        &mut ctx,
        &owner,
        &s,
        "MinDelegateBalance",
        PolicyData::MinDelegateBalance { floor: ONE_SOL / 2 },
    );

    md.step("Session transfers 100_000: post-CPI delegate balance stays above the floor");
    let extras = transfer_tail(&[floor_policy], s.delegate, recipient);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(100_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.step("After: the floor passed and the delegate's transfer settled");
    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 100_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn min_delegate_balance_rejects_below_floor() {
    let mut md = Report::new(
        "Bastion: a transfer that would drop the delegate below the floor is rejected",
        "A session carries a MinDelegateBalance floor of ONE_SOL*9/10. The delegate holds \
         ONE_SOL; a transfer of ONE_SOL/5 would leave it below the floor, so the post-CPI \
         floor check rejects with DelegateBalanceTooLow and the whole execute reverts.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a MinDelegateBalance floor of ONE_SOL*9/10");
    let floor_policy = attach(
        &mut ctx,
        &owner,
        &s,
        "MinDelegateBalance",
        PolicyData::MinDelegateBalance {
            floor: ONE_SOL.saturating_mul(9) / 10,
        },
    );

    md.step("Session transfers ONE_SOL/5: post-CPI delegate balance falls below the floor");
    let extras = transfer_tail(&[floor_policy], s.delegate, recipient);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(ONE_SOL / 5)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("DelegateBalanceTooLow");

    md.step("After: the execute reverted; the recipient received nothing");
    md.check(
        "recipient balance unchanged (rent only)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn attach_rejects_zero_floor() {
    let mut md = Report::new(
        "Bastion: attaching a MinDelegateBalance floor of zero is rejected at attach time",
        "A floor of zero is meaningless (every account is above zero), so AttachPolicy \
         validates it away with InvalidPolicyData before the policy is ever stored.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Attach a MinDelegateBalance floor of 0: rejected with InvalidPolicyData");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::MinDelegateBalance { floor: 0 },
            },
        )
        .send_err_named("InvalidPolicyData");
    ctx.report_execution(&mut md);
}
