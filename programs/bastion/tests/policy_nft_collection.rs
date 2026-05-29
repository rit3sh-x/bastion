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
    let (session_pda, session_kp) = init_session(svm, owner, 86_400).expect("init");
    airdrop(svm, &session_kp.pubkey(), ONE_SOL);
    let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
    airdrop(svm, &delegate, ONE_SOL);
    let dest = Pubkey::new_unique();
    airdrop(svm, &dest, 1);
    (session_pda, session_kp, delegate, dest)
}

#[test]
fn allowlist_passes_when_nft_belongs_to_allowed_collection() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let collection = Pubkey::new_from_array([0xCC; 32]);
    let nft_mint = Pubkey::new_unique();
    make_nft_mint(&mut svm, &nft_mint);
    let metadata = make_verified_collection_metadata(&mut svm, &nft_mint, collection);

    let policy_data = PolicyData::NftCollectionAllowlist {
        collections: vec![collection],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = vec![
        AccountMeta::new_readonly(p0, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
        AccountMeta::new_readonly(nft_mint, false),
        AccountMeta::new_readonly(metadata, false),
    ];
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(10_000),
        1,
        &extras,
    )
    .expect("allowed collection NFT should pass");
}

#[test]
fn allowlist_fails_when_collection_not_allowed() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let allowed = Pubkey::new_from_array([0xAA; 32]);
    let actual = Pubkey::new_from_array([0xBB; 32]);
    let nft_mint = Pubkey::new_unique();
    make_nft_mint(&mut svm, &nft_mint);
    let metadata = make_verified_collection_metadata(&mut svm, &nft_mint, actual);

    let policy_data = PolicyData::NftCollectionAllowlist {
        collections: vec![allowed],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = vec![
        AccountMeta::new_readonly(p0, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
        AccountMeta::new_readonly(nft_mint, false),
        AccountMeta::new_readonly(metadata, false),
    ];
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(10_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::NftCollectionNotAllowed);
}

#[test]
fn blocklist_blocks_matching_collection() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_setup(&mut svm, &owner);

    let blocked = Pubkey::new_from_array([0xDD; 32]);
    let nft_mint = Pubkey::new_unique();
    make_nft_mint(&mut svm, &nft_mint);
    let metadata = make_verified_collection_metadata(&mut svm, &nft_mint, blocked);

    let policy_data = PolicyData::NftCollectionBlocklist {
        collections: vec![blocked],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = vec![
        AccountMeta::new_readonly(p0, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
        AccountMeta::new_readonly(nft_mint, false),
        AccountMeta::new_readonly(metadata, false),
    ];
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(10_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::NftCollectionBlocked);
}
