mod helpers;

use anchor_litesvm::{Pubkey, Report};
use bastion::state::policy::{Asset, Policy, PolicyData};
use helpers::*;

#[test]
fn per_counterparty_cap_charges_inflow() {
    let mut md = Report::new(
        "Bastion: a PerCounterpartyCap charges outflow to one receiver, then rejects over the cap",
        "A session carries a PerCounterpartyCap of 5_000 to one recipient. A transfer of \
         3_000 charges within the cap; a second of 2_001 pushes the cumulative total to \
         5_001 and is rejected with CounterpartyCapExceeded.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a PerCounterpartyCap (5_000 to recipient)");
    let cap = attach(
        &mut ctx,
        &owner,
        &s,
        "PerCounterpartyCap",
        PolicyData::PerCounterpartyCap {
            receiver: recipient,
            asset: Asset::NativeSol,
            max: 5_000,
            sent: 0,
        },
    );
    let extras = transfer_tail(&[cap], s.delegate, recipient);

    md.step("Transfer 3_000 to recipient: within the cap, the policy charges it");
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

    md.step("Transfer 2_001 pushes the cumulative total to 5_001: rejected with CounterpartyCapExceeded");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(2_001)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("CounterpartyCapExceeded");

    ctx.report_execution(&mut md);
}

#[test]
fn per_counterparty_cap_accumulates_across_calls() {
    let mut md = Report::new(
        "Bastion: a PerCounterpartyCap accumulates `sent` across separate executes",
        "A session carries a PerCounterpartyCap of 10_000 to one recipient. Three transfers \
         of 3_000 each charge (cumulative 9_000 stays within the cap); the policy's `sent` \
         reads back 9_000. A fourth transfer of 1_001 pushes the total to 10_001 and is \
         rejected with CounterpartyCapExceeded.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a PerCounterpartyCap (10_000 to recipient)");
    let cap = attach(
        &mut ctx,
        &owner,
        &s,
        "PerCounterpartyCap",
        PolicyData::PerCounterpartyCap {
            receiver: recipient,
            asset: Asset::NativeSol,
            max: 10_000,
            sent: 0,
        },
    );
    let extras = transfer_tail(&[cap], s.delegate, recipient);

    md.step("Three transfers of 3_000: cumulative 9_000 stays within the cap");
    for _ in 0..3 {
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
    }

    let pol: Policy = ctx.get_account(&cap).unwrap();
    let sent = match pol.data {
        PolicyData::PerCounterpartyCap { sent, .. } => sent,
        _ => panic!("expected PerCounterpartyCap"),
    };
    md.check("policy accumulated all three transfers", 9_000u64, sent);

    md.step("A fourth transfer of 1_001 pushes the total to 10_001: rejected with CounterpartyCapExceeded");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_001)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("CounterpartyCapExceeded");

    ctx.report_execution(&mut md);
}

#[test]
fn per_counterparty_cap_noop_when_receiver_out_of_scope() {
    let mut md = Report::new(
        "Bastion: a PerCounterpartyCap is a no-op when the transfer's receiver is out of scope",
        "A session carries a PerCounterpartyCap of 1 to an *unrelated* receiver. A transfer \
         of 50_000 to a different recipient is allowed (the cap doesn't apply) and charges \
         nothing: the policy's `sent` stays 0.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let unrelated_receiver = ctx.cast_account("unrelated-receiver");

    md.step("Open session + attach a PerCounterpartyCap (max 1 to an unrelated receiver)");
    let cap = attach(
        &mut ctx,
        &owner,
        &s,
        "PerCounterpartyCap",
        PolicyData::PerCounterpartyCap {
            receiver: unrelated_receiver,
            asset: Asset::NativeSol,
            max: 1,
            sent: 0,
        },
    );

    md.step("Transfer 50_000 to recipient (not the cap's receiver): allowed, no charge");
    let extras = transfer_tail(&[cap], s.delegate, recipient);
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

    let pol: Policy = ctx.get_account(&cap).unwrap();
    let sent = match pol.data {
        PolicyData::PerCounterpartyCap { sent, .. } => sent,
        _ => panic!("expected PerCounterpartyCap"),
    };
    md.check("sent stays zero when receiver was out of scope", 0u64, sent);

    ctx.report_execution(&mut md);
}

#[test]
fn attach_rejects_nft_asset() {
    let mut md = Report::new(
        "Bastion: AttachPolicy rejects an NFT asset on a PerCounterpartyCap",
        "PerCounterpartyCap is a lamport/token cap; attaching one with an NFT asset variant \
         (AnyNftCount) is rejected at attach time.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Attach a PerCounterpartyCap with an NFT asset (AnyNftCount): rejected");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::PerCounterpartyCap {
                    receiver: Pubkey::new_unique(),
                    asset: Asset::AnyNftCount,
                    max: 1,
                    sent: 0,
                },
            },
        )
        .send_err();
    ctx.report_execution(&mut md);
}
