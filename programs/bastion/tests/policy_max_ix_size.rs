mod helpers;

use anchor_litesvm::Report;
use bastion::state::policy::PolicyData;
use helpers::*;

#[test]
fn max_ix_size_passes_under_bounds() {
    let mut md = Report::new(
        "Bastion: MaxIxSize passes a wrapped transfer that fits under (4 accounts, 32 bytes)",
        "A session carries a MaxIxSize policy of max_accounts=4, max_data_len=32. A wrapped \
         SOL transfer uses 2 accounts and 12 bytes of data, comfortably under both caps, so \
         the execute passes and the delegate's transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach MaxIxSize { max_accounts: 4, max_data_len: 32 }");
    let max_ix = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxIxSize",
        PolicyData::MaxIxSize {
            max_accounts: 4,
            max_data_len: 32,
        },
    );

    md.step("Execute a wrapped transfer (2 accounts / 12 bytes): fits under (4, 32)");
    let extras = transfer_tail(&[max_ix], s.delegate, recipient);
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
        "recipient received the transfer",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn max_ix_size_blocks_when_accounts_exceed_cap() {
    let mut md = Report::new(
        "Bastion: MaxIxSize blocks a transfer whose account count exceeds the cap",
        "The policy caps max_accounts=1 (max_data_len=64 is generous). The wrapped transfer \
         carries 2 accounts, over the account cap, so the execute is rejected with IxTooLarge.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach MaxIxSize { max_accounts: 1, max_data_len: 64 }");
    let max_ix = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxIxSize",
        PolicyData::MaxIxSize {
            max_accounts: 1,
            max_data_len: 64,
        },
    );

    md.step("Execute a 2-account transfer: over the account cap, rejected with IxTooLarge");
    let extras = transfer_tail(&[max_ix], s.delegate, recipient);
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
        .send_err_named("IxTooLarge");

    md.check(
        "recipient never received the transfer",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn max_ix_size_blocks_when_data_exceeds_cap() {
    let mut md = Report::new(
        "Bastion: MaxIxSize blocks a transfer whose data length exceeds the cap",
        "The policy caps max_data_len=8 (max_accounts=8 is generous). The wrapped transfer's \
         data is 12 bytes, over the data cap, so the execute is rejected with IxTooLarge.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach MaxIxSize { max_accounts: 8, max_data_len: 8 }");
    let max_ix = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxIxSize",
        PolicyData::MaxIxSize {
            max_accounts: 8,
            max_data_len: 8,
        },
    );

    md.step("Execute a 12-byte transfer: over the data cap, rejected with IxTooLarge");
    let extras = transfer_tail(&[max_ix], s.delegate, recipient);
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
        .send_err_named("IxTooLarge");

    md.check(
        "recipient never received the transfer",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn max_ix_size_rejects_zero_caps_at_attach() {
    let mut md = Report::new(
        "Bastion: MaxIxSize with a zero cap is refused at attach time",
        "A MaxIxSize policy with max_accounts=0 is structurally invalid; the program validates \
         policy data on AttachPolicy and rejects it with InvalidPolicyData before any execute.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step(
        "Attach MaxIxSize { max_accounts: 0, max_data_len: 16 }: rejected with InvalidPolicyData",
    );
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::MaxIxSize {
                    max_accounts: 0,
                    max_data_len: 16,
                },
            },
        )
        .send_err_named("InvalidPolicyData");
    ctx.report_execution(&mut md);
}
