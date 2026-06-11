mod helpers;

use anchor_lang::system_program;
use anchor_litesvm::{Pubkey, Report};
use bastion::state::policy::PolicyData;
use helpers::*;

#[test]
fn allowlist_with_system_program_lets_transfer_through() {
    let mut md = Report::new(
        "Bastion: a ProgramAllowlist naming System lets a wrapped transfer through",
        "A session attaches a ProgramAllowlist containing the System program, then \
         executes a wrapped SOL transfer. The wrapped ix's program_id (System) is in \
         the allowlist, so dispatch is permitted and the delegate's transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a ProgramAllowlist containing the System program");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramAllowlist",
        PolicyData::ProgramAllowlist {
            programs: vec![system_program::ID],
        },
    );

    md.step("Session executes a wrapped SOL transfer: System is allowed, so it passes");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[allowlist], s.delegate, recipient);
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

    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 50_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn allowlist_without_system_program_blocks_transfer() {
    let mut md = Report::new(
        "Bastion: a ProgramAllowlist omitting System blocks a wrapped transfer",
        "A session attaches a ProgramAllowlist containing some unrelated program (not \
         System), then tries a wrapped SOL transfer. System is not in the allowlist, so \
         dispatch is rejected with ProgramNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a ProgramAllowlist that does NOT contain the System program");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramAllowlist",
        PolicyData::ProgramAllowlist {
            programs: vec![Pubkey::new_unique()],
        },
    );

    md.step("Session executes a wrapped SOL transfer: System is not allowed, so it is rejected");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[allowlist], s.delegate, recipient);
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
        .send_err_named("ProgramNotAllowed");

    md.check(
        "recipient unchanged: the transfer never settled",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn blocklist_with_system_program_blocks_transfer() {
    let mut md = Report::new(
        "Bastion: a ProgramBlocklist naming System blocks a wrapped transfer",
        "A session attaches a ProgramBlocklist containing the System program, then tries \
         a wrapped SOL transfer. System is on the blocklist, so dispatch is rejected with \
         ProgramBlocked.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a ProgramBlocklist containing the System program");
    let blocklist = attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramBlocklist",
        PolicyData::ProgramBlocklist {
            programs: vec![system_program::ID],
        },
    );

    md.step("Session executes a wrapped SOL transfer: System is blocked, so it is rejected");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[blocklist], s.delegate, recipient);
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
        .send_err_named("ProgramBlocked");

    md.check(
        "recipient unchanged: the transfer never settled",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn blocklist_without_system_program_allows_transfer() {
    let mut md = Report::new(
        "Bastion: a ProgramBlocklist omitting System lets a wrapped transfer through",
        "A session attaches a ProgramBlocklist containing some unrelated program (not \
         System), then executes a wrapped SOL transfer. System is not on the blocklist, \
         so dispatch is permitted and the delegate's transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a ProgramBlocklist that does NOT contain the System program");
    let blocklist = attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramBlocklist",
        PolicyData::ProgramBlocklist {
            programs: vec![Pubkey::new_unique()],
        },
    );

    md.step("Session executes a wrapped SOL transfer: System is not blocked, so it passes");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[blocklist], s.delegate, recipient);
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

    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 50_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
