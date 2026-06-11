mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::{MarkdownBlock, Report};
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use helpers::*;

/// The guard is a no-op for non-token programs: a plain SOL transfer passes
/// straight through to the delegate's CPI.
#[test]
fn token_authority_guard_passes_for_non_token_ix() {
    let mut md = Report::new(
        "Bastion: a TokenAuthorityGuard is a no-op for non-token instructions",
        "A session carries a TokenAuthorityGuard. A wrapped System::Transfer is not a token \
         instruction, so the guard passes it through and the delegate's transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a TokenAuthorityGuard");
    let guard = attach(
        &mut ctx,
        &owner,
        &s,
        "TokenAuthorityGuard",
        PolicyData::TokenAuthorityGuard,
    );

    md.step("Execute a wrapped SOL transfer: non-token ix, so the guard is a no-op");
    let extras = transfer_tail(&[guard], s.delegate, recipient);
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
        "recipient received the transfer (guard no-op)",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

/// Drive an authority-changing tag against a token program and assert the guard
/// rejects it before any CPI runs. No real token accounts needed: validation
/// precedes `build_cpi_accounts`/`invoke_signed`.
fn assert_guard_blocks_tag(title: &str, intent: &str, tag: u8, token_program: Pubkey) {
    let mut md = Report::new(title, intent);
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach a TokenAuthorityGuard");
    let guard = attach(
        &mut ctx,
        &owner,
        &s,
        "TokenAuthorityGuard",
        PolicyData::TokenAuthorityGuard,
    );
    ctx.alias(token_program, "TokenProgram");

    // The wrapped ix targets a token program with an authority-changing tag.
    // Accounts are positional placeholders (delegate, delegate, recipient); the
    // guard rejects on the tag before any account is touched.
    let wix = WrappedInstruction {
        program_id: token_program,
        accounts: vec![
            CompactAccountMeta::new(0, true, true),
            CompactAccountMeta::new(1, false, true),
            CompactAccountMeta::new(2, false, false),
        ],
        data: vec![tag],
    };
    let extras = dispatch_tail(
        &[policy_meta(guard, true)],
        s.delegate,
        &[
            AccountMeta::new(s.delegate, false),
            AccountMeta::new(recipient, false),
            AccountMeta::new_readonly(token_program, false),
        ],
    );

    md.step(&format!(
        "Execute a token ix (tag {tag}) against the token program: guard rejects pre-CPI"
    ));
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
        .send_err_named("TokenAuthorityChangeNotAllowed");
    md.block(
        "rejection logs",
        MarkdownBlock::Fenced {
            lang: "console".into(),
            body: rejection.logs_structured_string(),
        },
    );

    md.check(
        "recipient untouched (rejected before CPI)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
}

#[test]
fn token_authority_guard_rejects_spl_approve() {
    assert_guard_blocks_tag(
        "Bastion: TokenAuthorityGuard rejects an SPL Token Approve",
        "An SPL Token Approve (tag 4) delegates spend authority; the guard rejects it \
         with TokenAuthorityChangeNotAllowed before any CPI runs.",
        4,
        spl_token_interface::id(),
    );
}

#[test]
fn token_authority_guard_rejects_spl_set_authority() {
    assert_guard_blocks_tag(
        "Bastion: TokenAuthorityGuard rejects an SPL Token SetAuthority",
        "An SPL Token SetAuthority (tag 6) reassigns mint/account authority; the guard \
         rejects it with TokenAuthorityChangeNotAllowed before any CPI runs.",
        6,
        spl_token_interface::id(),
    );
}

#[test]
fn token_authority_guard_rejects_spl_approve_checked() {
    assert_guard_blocks_tag(
        "Bastion: TokenAuthorityGuard rejects an SPL Token ApproveChecked",
        "An SPL Token ApproveChecked (tag 13) delegates spend authority with a decimals \
         check; the guard rejects it with TokenAuthorityChangeNotAllowed before any CPI runs.",
        13,
        spl_token_interface::id(),
    );
}

#[test]
fn token_authority_guard_rejects_t22_set_authority() {
    assert_guard_blocks_tag(
        "Bastion: TokenAuthorityGuard rejects a Token-2022 SetAuthority",
        "A Token-2022 SetAuthority (tag 6) reassigns mint/account authority; the guard \
         rejects it with TokenAuthorityChangeNotAllowed before any CPI runs.",
        6,
        spl_token_2022_interface::id(),
    );
}
