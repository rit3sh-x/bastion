mod helpers;

use anchor_litesvm::{Lazy, Report};
use bastion::state::policy::{Policy, PolicyData};
use bastion::utils::helpers::BastionSeed;
use helpers::*;

#[test]
fn max_calls_total_allows_up_to_max_then_blocks() {
    let mut md = Report::new(
        "Bastion: a MaxCallsTotal policy allows up to max, then blocks",
        "A session carries a MaxCallsTotal cap of 3. Three executes each charge one use and \
         pass; the fourth pushes used to 4 and is rejected with MaxCallsExceeded.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open a funded session + attach a MaxCallsTotal cap of 3");
    let max_calls = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxCallsTotal",
        PolicyData::MaxCallsTotal { max: 3, used: 0 },
    );
    let tail = transfer_tail(&[max_calls], s.delegate, recipient);

    md.step("Three executes within the budget: each charges one use and passes");
    for i in 0..3 {
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
            .remaining_accounts(&tail)
            .send_ok();
        let pol: Policy = ctx.get_account(&max_calls).unwrap();
        if let PolicyData::MaxCallsTotal { used, .. } = pol.data {
            md.check(&format!("used after call {}", i + 1), (i + 1) as u64, used);
        }
    }

    md.step("A fourth execute pushes used over the cap: rejected with MaxCallsExceeded");
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
        .remaining_accounts(&tail)
        .send_err_named("MaxCallsExceeded");

    md.check(
        "recipient received exactly the three allowed transfers",
        Some(ONE_SOL + 3_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn max_calls_total_rejects_used_nonzero_at_attach() {
    let mut md = Report::new(
        "Bastion: MaxCallsTotal rejects a nonzero `used` at attach",
        "A freshly attached MaxCallsTotal must start with used = 0. Attaching one that claims \
         used = 3 is rejected at validation with InvalidPolicyData.",
    );
    // The attach itself is under test (it must fail), so it stays a manual
    // send_err rather than going through the `attach` helper.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Attach a MaxCallsTotal with used = 3: rejected with InvalidPolicyData");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::MaxCallsTotal { max: 10, used: 3 },
            },
        )
        .send_err_named("InvalidPolicyData");
    ctx.report_execution(&mut md);
}

#[test]
fn max_calls_total_preserves_used_across_update() {
    let mut md = Report::new(
        "Bastion: updating a MaxCallsTotal preserves the accumulated `used`",
        "Attach a cap of 5, spend two calls (used = 2), then update the cap to 7. The new cap \
         takes effect while the accumulated used (2) is preserved across the update.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let mut bundle = s.bundle;

    md.step("Open a funded session + attach a MaxCallsTotal cap of 5");
    let max_calls = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxCallsTotal",
        PolicyData::MaxCallsTotal { max: 5, used: 0 },
    );
    let tail = transfer_tail(&[max_calls], s.delegate, recipient);

    md.step("Spend two calls within the budget (used climbs to 2)");
    for _ in 0..2 {
        ctx.svm.expire_blockhash();
        ctx.tx(&[&session_kp])
            .build(
                bundle,
                bastion::instruction::Execute {
                    wrapped_ixs: vec![transfer_wrapped(1_000)],
                    policy_count: 1,
                    expected_nonce: None,
                    manifest: None,
                },
            )
            .remaining_accounts(&tail)
            .send_ok();
    }
    let mid: Policy = ctx.get_account(&max_calls).unwrap();
    let mid_used = match mid.data {
        PolicyData::MaxCallsTotal { used, .. } => used,
        _ => panic!("expected MaxCallsTotal"),
    };
    md.check("used after two calls", 2u64, mid_used);

    md.step("Update the cap to 7 (PolicyAt seed 0): cap raised, used preserved");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::MaxCallsTotal { max: 7, used: 0 },
            },
        )
        .send_ok();

    let post: Policy = ctx.get_account(&max_calls).unwrap();
    match post.data {
        PolicyData::MaxCallsTotal { max, used } => {
            md.check("cap raised to 7", 7u64, max);
            md.check("used preserved across update", 2u64, used);
        }
        _ => panic!("expected MaxCallsTotal"),
    }
    ctx.report_execution(&mut md);
}
