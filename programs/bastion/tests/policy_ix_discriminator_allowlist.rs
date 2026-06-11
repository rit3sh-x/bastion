//! IxDiscriminatorAllowlist: an allowlist scoped to a program permits only
//! wrapped instructions whose leading bytes match one of its (variable-length)
//! discriminators. Scope is the first gate; matching is a prefix compare, so a
//! 4-byte System tag is enough. Empty lists are rejected at attach.

mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_litesvm::Report;
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use helpers::*;

/// A wrapped System ix whose data is an arbitrary 8-byte discriminator prefix
/// followed by `lamports`. Mirrors `transfer_wrapped`'s account shape but lets
/// the test choose the leading bytes the allowlist inspects.
fn make_wrapped_with_disc_prefix(prefix: [u8; 8], lamports: u64) -> WrappedInstruction {
    let mut data = prefix.to_vec();
    data.extend_from_slice(&lamports.to_le_bytes());
    WrappedInstruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            CompactAccountMeta::new(0, true, true),
            CompactAccountMeta::new(1, false, true),
        ],
        data,
    }
}

#[test]
fn ix_disc_allowlist_passes_for_allowed_discriminator() {
    let mut md = Report::new(
        "Bastion: an IxDiscriminatorAllowlist scoped to another program is a no-op",
        "The allowlist targets a random program, so the wrapped System transfer is \
         out of scope: the policy never inspects its discriminator and the execute \
         settles. Scope is the first gate, before any discriminator comparison.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let allowed_disc = [9u8; 8];

    md.step("Attach an allowlist scoped to a random (out-of-scope) program");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "IxDiscriminatorAllowlist",
        PolicyData::IxDiscriminatorAllowlist {
            program: Pubkey::new_unique(),
            discriminators: vec![allowed_disc.to_vec()],
        },
    );

    md.step("Execute a wrapped System transfer: out of scope, so the policy is a no-op");
    let extras = transfer_tail(&[allowlist], s.delegate, recipient);
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
        "recipient received the transfer (policy no-op)",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn ix_disc_allowlist_rejects_non_matching_disc_in_scope() {
    let mut md = Report::new(
        "Bastion: an in-scope wrapped ix with a non-allowed discriminator is rejected",
        "The allowlist targets the System program and permits only discriminator \
         [9; 8]. A wrapped System ix whose leading bytes are [2,0,...] is in scope but \
         not on the list, so it is rejected with IxDiscriminatorNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let allowed_disc = [9u8; 8];

    md.step("Attach an allowlist scoped to System, permitting only discriminator [9; 8]");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "IxDiscriminatorAllowlist",
        PolicyData::IxDiscriminatorAllowlist {
            program: anchor_lang::system_program::ID,
            discriminators: vec![allowed_disc.to_vec()],
        },
    );

    md.step("Execute an in-scope ix with discriminator [2,0,...]: not on the list");
    let wix = make_wrapped_with_disc_prefix([2u8, 0, 0, 0, 0, 0, 0, 0], 1_000);
    let extras = transfer_tail(&[allowlist], s.delegate, recipient);
    ctx.svm.expire_blockhash();
    let rejection = ctx
        .tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![wix],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("IxDiscriminatorNotAllowed");

    md.check(
        "recipient unchanged (transfer never settled)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    md.block(
        "rejection logs",
        anchor_litesvm::MarkdownBlock::Fenced {
            lang: "console".into(),
            body: rejection.logs_structured_string(),
        },
    );
    ctx.report_execution(&mut md);
}

#[test]
fn attach_rejects_empty_disc_list() {
    let mut md = Report::new(
        "Bastion: attaching an IxDiscriminatorAllowlist with an empty list is rejected",
        "An allowlist with no discriminators would permit nothing while claiming to \
         gate a program; the program rejects it at attach time with InvalidPolicyData, \
         before any execute can reference it.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Attach an allowlist with an empty discriminator list: rejected");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::IxDiscriminatorAllowlist {
                    program: anchor_lang::system_program::ID,
                    discriminators: vec![],
                },
            },
        )
        .send_err_named("InvalidPolicyData");

    md.check(
        "session has no policy attached",
        0u8,
        ctx.get_account::<bastion::state::session::Session>(&s.session)
            .unwrap()
            .policy_count,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn ix_disc_allowlist_passes_for_4byte_system_tag() {
    let mut md = Report::new(
        "Bastion: an IxDiscriminatorAllowlist matches a 4-byte System instruction tag",
        "Discriminators are variable-length: an allowlist scoped to System permitting only \
         the 4-byte tag [2,0,0,0] (System::Transfer) admits a wrapped transfer whose leading \
         bytes are exactly that tag. The match is on the prefix, not a fixed 8-byte width.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach an allowlist scoped to System, permitting only the 4-byte tag [2,0,0,0]");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "IxDiscriminatorAllowlist",
        PolicyData::IxDiscriminatorAllowlist {
            program: anchor_lang::system_program::ID,
            discriminators: vec![vec![2, 0, 0, 0]],
        },
    );

    md.step("Execute a wrapped System transfer (tag [2,0,0,0]): on the list, so it settles");
    let extras = transfer_tail(&[allowlist], s.delegate, recipient);
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
        "recipient received the transfer (4-byte tag allowed)",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn ix_disc_allowlist_rejects_wrong_4byte_tag() {
    let mut md = Report::new(
        "Bastion: an IxDiscriminatorAllowlist rejects a non-matching 4-byte tag",
        "The allowlist scoped to System permits only the 4-byte tag [7,0,0,0]. A wrapped \
         System transfer leads with [2,0,0,0], in scope but not on the list, so it is \
         rejected with IxDiscriminatorNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach an allowlist scoped to System, permitting only the 4-byte tag [7,0,0,0]");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "IxDiscriminatorAllowlist",
        PolicyData::IxDiscriminatorAllowlist {
            program: anchor_lang::system_program::ID,
            discriminators: vec![vec![7, 0, 0, 0]],
        },
    );

    md.step("Execute a wrapped System transfer (tag [2,0,0,0]): not on the list, rejected");
    let extras = transfer_tail(&[allowlist], s.delegate, recipient);
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
        .send_err_named("IxDiscriminatorNotAllowed");

    md.check(
        "recipient unchanged (transfer never settled)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
