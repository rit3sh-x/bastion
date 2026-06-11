mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::{Report, TokenFabrication, TokenProgram};
use bastion::state::policy::PolicyData;
use helpers::*;

/// The dispatch tail for a wrapped SOL transfer guarded by one mint-list policy,
/// with a token account appended for the policy to inspect. The policy is
/// read-only (mint-list policies inspect, they don't charge), and the token
/// account rides at the end of the ix-accounts so the policy's scan finds it.
fn token_extras(
    policy: Pubkey,
    delegate: Pubkey,
    dest: Pubkey,
    token_acct: Pubkey,
) -> Vec<AccountMeta> {
    let mut ix_accounts = transfer_ix_accounts(delegate, dest);
    ix_accounts.push(AccountMeta::new_readonly(token_acct, false));
    dispatch_tail(&[policy_meta(policy, false)], delegate, &ix_accounts)
}

#[test]
fn allowlist_passes_when_token_account_mint_matches() {
    let mut md = Report::new(
        "Bastion: a MintAllowlist admits an execute whose token account holds an allowed mint",
        "A session carries a MintAllowlist of one mint. The delegate holds an SPL token \
         account for that exact mint, so an execute presenting it passes the mint check.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    ctx.alias(mint, "mint");
    ctx.svm
        .fabricate_token_account(&token_acct, TokenProgram::Spl, &mint, &s.delegate, 1000);

    md.step("Attach a MintAllowlist with the held mint");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "MintAllowlist",
        PolicyData::MintAllowlist { mints: vec![mint] },
    );

    md.step("Execute presenting the allowlisted token account: mint matches, so it passes");
    ctx.svm.expire_blockhash();
    let extras = token_extras(allowlist, s.delegate, recipient, token_acct);
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
fn allowlist_fails_when_token_account_mint_does_not_match() {
    let mut md = Report::new(
        "Bastion: a MintAllowlist rejects an execute whose token account holds a foreign mint",
        "The allowlist names one mint, but the delegate's token account holds a different \
         (foreign) mint, so the execute is rejected with MintNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let allowed_mint = Pubkey::new_unique();
    let foreign_mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    ctx.alias(allowed_mint, "allowed-mint");
    ctx.alias(foreign_mint, "foreign-mint");
    ctx.svm.fabricate_token_account(
        &token_acct,
        TokenProgram::Spl,
        &foreign_mint,
        &s.delegate,
        1000,
    );

    md.step("Attach a MintAllowlist naming only the allowed mint");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "MintAllowlist",
        PolicyData::MintAllowlist {
            mints: vec![allowed_mint],
        },
    );

    md.step("Execute presenting a foreign-mint token account: rejected with MintNotAllowed");
    ctx.svm.expire_blockhash();
    let extras = token_extras(allowlist, s.delegate, recipient, token_acct);
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
        .send_err_named("MintNotAllowed");

    md.check(
        "transfer was rejected: recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn blocklist_blocks_matching_mint() {
    let mut md = Report::new(
        "Bastion: a MintBlocklist rejects an execute whose token account holds a blocked mint",
        "The blocklist names a mint, and the delegate's token account holds exactly that \
         mint, so the execute is rejected with MintBlocked.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    ctx.alias(mint, "blocked-mint");
    ctx.svm
        .fabricate_token_account(&token_acct, TokenProgram::Spl, &mint, &s.delegate, 1000);

    md.step("Attach a MintBlocklist naming the held mint");
    let blocklist = attach(
        &mut ctx,
        &owner,
        &s,
        "MintBlocklist",
        PolicyData::MintBlocklist { mints: vec![mint] },
    );

    md.step("Execute presenting the blocked-mint token account: rejected with MintBlocked");
    ctx.svm.expire_blockhash();
    let extras = token_extras(blocklist, s.delegate, recipient, token_acct);
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
        .send_err_named("MintBlocked");

    md.check(
        "transfer was rejected: recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn t22_token_account_recognised_via_owner_dispatch() {
    let mut md = Report::new(
        "Bastion: a MintAllowlist recognises a Token-2022 account by its owning program",
        "The held token account is owned by Token-2022 (not classic SPL); the policy still \
         dispatches on the owner, reads the mint, and admits it because the mint is allowlisted.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    ctx.alias(mint, "t22-mint");
    ctx.svm.fabricate_token_account(
        &token_acct,
        TokenProgram::Token2022,
        &mint,
        &s.delegate,
        1000,
    );

    md.step("Attach a MintAllowlist with the T22-held mint");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "MintAllowlist",
        PolicyData::MintAllowlist { mints: vec![mint] },
    );

    md.step("Execute presenting the Token-2022 account: dispatched by owner, mint allowed, passes");
    ctx.svm.expire_blockhash();
    let extras = token_extras(allowlist, s.delegate, recipient, token_acct);
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
fn non_token_accounts_are_skipped() {
    let mut md = Report::new(
        "Bastion: a MintAllowlist is a no-op when the tail carries no token accounts",
        "The allowlist names a mint, but the dispatch tail presents no token account, so \
         there is nothing to inspect; the mint check is a no-op and the execute passes.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Attach a MintAllowlist (its mint is never presented)");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "MintAllowlist",
        PolicyData::MintAllowlist {
            mints: vec![Pubkey::new_unique()],
        },
    );

    md.step("Execute with a tail that carries no token account: mint check is a no-op, passes");
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
