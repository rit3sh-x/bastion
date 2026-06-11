mod helpers;

use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::Report;
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use helpers::*;

/// The classic SPL Token program id (hard-wired, since the policy keys off it).
const SPL_TOKEN_ID: anchor_lang::prelude::Pubkey = anchor_lang::prelude::Pubkey::new_from_array([
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93, 0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91, 0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
]);

#[test]
fn no_account_close_passes_for_non_token_ix() {
    let mut md = Report::new(
        "Bastion: NoAccountClose lets a non-token ix through",
        "A session carries a NoAccountClose policy. A wrapped SOL transfer (a non-token \
         ix) is not a CloseAccount, so the policy treats it as a no-op and the transfer \
         settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Owner opens a session and attaches a NoAccountClose policy");
    let no_close = attach(
        &mut ctx,
        &owner,
        &s,
        "NoAccountClose",
        PolicyData::NoAccountClose,
    );

    md.step("Session executes a wrapped SOL transfer: not a CloseAccount, so it passes");
    let extras = transfer_tail(&[no_close], s.delegate, recipient);
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

    md.step("After: the non-token ix was a no-op for the policy; the transfer settled");
    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn no_account_close_rejects_spl_close_account() {
    let mut md = Report::new(
        "Bastion: NoAccountClose rejects an SPL CloseAccount",
        "A session carries a NoAccountClose policy. A wrapped SPL CloseAccount (token \
         program, instruction tag 9) is exactly what the policy guards against, so the \
         execute is rejected with AccountCloseNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Owner opens a session and attaches a NoAccountClose policy");
    let no_close = attach(
        &mut ctx,
        &owner,
        &s,
        "NoAccountClose",
        PolicyData::NoAccountClose,
    );

    // An SPL CloseAccount (tag 9): account 0 (closed) writable+signer-ish,
    // account 1 (dest) writable, account 2 (authority) readonly.
    let wix = WrappedInstruction {
        program_id: SPL_TOKEN_ID,
        accounts: vec![
            CompactAccountMeta::new(0, true, true),
            CompactAccountMeta::new(1, false, true),
            CompactAccountMeta::new(2, false, false),
        ],
        data: vec![9u8],
    };
    // The CloseAccount's ix-accounts: [closed (delegate), dest (recipient),
    // SPL Token program], appended after the policy + delegate AccountInfo.
    let extras = dispatch_tail(
        &[policy_meta(no_close, true)],
        s.delegate,
        &[
            AccountMeta::new(s.delegate, false),
            AccountMeta::new(recipient, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
        ],
    );

    md.step("Session attempts to execute the wrapped CloseAccount: the policy rejects it");
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
        .send_err_named("AccountCloseNotAllowed");

    md.check(
        "execute was rejected (no CloseAccount escaped the policy)",
        true,
        rejection
            .logs_structured_string()
            .contains("AccountCloseNotAllowed"),
    );
    ctx.report_execution(&mut md);
}
