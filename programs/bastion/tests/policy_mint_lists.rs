mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;
use bastion::BastionError;

fn build_setup(
    svm: &mut litesvm::LiteSVM,
    owner: &solana_keypair::Keypair,
) -> (Pubkey, solana_keypair::Keypair, Pubkey, Pubkey) {
    let (session_pda, session_kp) = init_session(svm, owner, 3600).expect("init");
    airdrop(svm, &session_kp.pubkey(), ONE_SOL);
    let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
    airdrop(svm, &delegate_pda, ONE_SOL);
    let dest = Pubkey::new_unique();
    airdrop(svm, &dest, 1);
    (session_pda, session_kp, delegate_pda, dest)
}

fn extras_with_token(
    policy: &Pubkey,
    delegate: &Pubkey,
    dest: &Pubkey,
    token_acct: &Pubkey,
) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(*policy, false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
        AccountMeta::new_readonly(*token_acct, false),
    ]
}

#[test]
fn allowlist_passes_when_token_account_mint_matches() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    make_spl_token_account(&mut svm, &token_acct, mint, delegate, 1000);

    let data = PolicyData::MintAllowlist { mints: vec![mint] };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = extras_with_token(&p0, &delegate, &dest, &token_acct);
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    )
    .expect("allowlist match ok");
}

#[test]
fn allowlist_fails_when_token_account_mint_does_not_match() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let allowed_mint = Pubkey::new_unique();
    let foreign_mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    make_spl_token_account(&mut svm, &token_acct, foreign_mint, delegate, 1000);

    let data = PolicyData::MintAllowlist {
        mints: vec![allowed_mint],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = extras_with_token(&p0, &delegate, &dest, &token_acct);
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::MintNotAllowed);
}

#[test]
fn blocklist_blocks_matching_mint() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();
    make_spl_token_account(&mut svm, &token_acct, mint, delegate, 1000);

    let data = PolicyData::MintBlocklist { mints: vec![mint] };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = extras_with_token(&p0, &delegate, &dest, &token_acct);
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::MintBlocked);
}

#[test]
fn t22_token_account_recognised_via_owner_dispatch() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let mint = Pubkey::new_unique();
    let token_acct = Pubkey::new_unique();

    make_t22_token_account(&mut svm, &token_acct, mint, delegate, 1000);

    let data = PolicyData::MintAllowlist { mints: vec![mint] };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = extras_with_token(&p0, &delegate, &dest, &token_acct);
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    )
    .expect("T22 mint allowlisted ok");
}

#[test]
fn non_token_accounts_are_skipped() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let data = PolicyData::MintAllowlist {
        mints: vec![Pubkey::new_unique()],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = vec![
        AccountMeta::new_readonly(p0, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ];
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    )
    .expect("no token accounts → mint check is a no-op");
}
