mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::{
    Creator, MetadataArgs, MetaplexHelpers, Report, TokenFabrication, TokenProgram,
};
use bastion::state::policy::PolicyData;
use helpers::*;

/// The dispatch tail an `Execute` appends for a creator-allowlist policy: the
/// policy meta (read-only), the delegate + the wrapped transfer's accounts, then
/// the NFT mint + its metadata the policy reads.
fn extras_with_nft(
    policy: Pubkey,
    delegate: Pubkey,
    recipient: Pubkey,
    nft_mint: Pubkey,
    metadata: Pubkey,
) -> Vec<AccountMeta> {
    let mut ix_accounts = transfer_ix_accounts(delegate, recipient);
    ix_accounts.push(AccountMeta::new_readonly(nft_mint, false));
    ix_accounts.push(AccountMeta::new_readonly(metadata, false));
    dispatch_tail(&[policy_meta(policy, false)], delegate, &ix_accounts)
}

#[test]
fn nft_creator_allowlist_passes_when_verified_creator_in_list() {
    let mut md = Report::new(
        "Bastion: an NFT-creator-allowlist policy admits a verified creator on the list",
        "A session carries an NftCreatorAllowlist; it presents an NFT whose metadata names a \
         verified creator that is on the list, so an execute passes. The NFT mint and its \
         creator metadata are fabricated by framework helpers.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let allowed_creator = Pubkey::new_from_array([0xAB; 32]);
    let nft_mint = Pubkey::new_unique();
    ctx.svm.fabricate_nft_mint(&nft_mint, TokenProgram::Spl);
    let metadata = ctx.svm.fabricate_metadata(
        &nft_mint,
        &MetadataArgs {
            creators: vec![Creator {
                address: allowed_creator,
                verified: true,
                share: 100,
            }],
            ..Default::default()
        },
    );
    ctx.alias(nft_mint, "NFT");

    md.step("Open session + attach an NftCreatorAllowlist (one allowed creator)");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "CreatorAllowlist",
        PolicyData::NftCreatorAllowlist {
            creators: vec![allowed_creator],
        },
    );

    md.step(
        "Execute presenting the NFT mint + metadata: verified creator is allowed, so it passes",
    );
    let extras = extras_with_nft(allowlist, s.delegate, recipient, nft_mint, metadata);
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
    md.block("account index", ctx.account_index());
}

#[test]
fn nft_creator_allowlist_fails_when_verified_creator_not_in_list() {
    let mut md = Report::new(
        "Bastion: an NFT-creator-allowlist policy rejects a verified creator off the list",
        "A session carries an NftCreatorAllowlist; the NFT's metadata names a *verified* creator \
         that is not on the list, so the execute is rejected with NftCreatorNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let allowed = Pubkey::new_from_array([0xAB; 32]);
    let actual = Pubkey::new_from_array([0xCD; 32]);
    let nft_mint = Pubkey::new_unique();
    ctx.svm.fabricate_nft_mint(&nft_mint, TokenProgram::Spl);
    let metadata = ctx.svm.fabricate_metadata(
        &nft_mint,
        &MetadataArgs {
            creators: vec![Creator {
                address: actual,
                verified: true,
                share: 100,
            }],
            ..Default::default()
        },
    );
    ctx.alias(nft_mint, "NFT");

    md.step("Open session + attach an NftCreatorAllowlist allowing a different creator");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "CreatorAllowlist",
        PolicyData::NftCreatorAllowlist {
            creators: vec![allowed],
        },
    );

    md.step(
        "Execute presenting the NFT: the verified creator is not on the list, so it is rejected",
    );
    let extras = extras_with_nft(allowlist, s.delegate, recipient, nft_mint, metadata);
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
        .send_err_named("NftCreatorNotAllowed");

    md.check(
        "recipient balance unchanged (transfer never settled)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    md.block("account index", ctx.account_index());
    ctx.report_execution(&mut md);
}

#[test]
fn nft_creator_allowlist_fails_when_creator_unverified() {
    let mut md = Report::new(
        "Bastion: an NFT-creator-allowlist policy rejects an unverified creator",
        "A session carries an NftCreatorAllowlist; the NFT's metadata names the listed creator, \
         but it is *unverified*, so the execute is rejected with NftCreatorNotAllowed. Only \
         verified creators count toward the allowlist.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let creator = Pubkey::new_from_array([0xAB; 32]);
    let nft_mint = Pubkey::new_unique();
    ctx.svm.fabricate_nft_mint(&nft_mint, TokenProgram::Spl);
    let metadata = ctx.svm.fabricate_metadata(
        &nft_mint,
        &MetadataArgs {
            creators: vec![Creator {
                address: creator,
                verified: false,
                share: 100,
            }],
            ..Default::default()
        },
    );
    ctx.alias(nft_mint, "NFT");

    md.step("Open session + attach an NftCreatorAllowlist naming the creator");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "CreatorAllowlist",
        PolicyData::NftCreatorAllowlist {
            creators: vec![creator],
        },
    );

    md.step("Execute presenting the NFT: the listed creator is unverified, so it is rejected");
    let extras = extras_with_nft(allowlist, s.delegate, recipient, nft_mint, metadata);
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
        .send_err_named("NftCreatorNotAllowed");

    md.check(
        "recipient balance unchanged (transfer never settled)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    md.block("account index", ctx.account_index());
    ctx.report_execution(&mut md);
}

#[test]
fn nft_creator_allowlist_rejects_empty_creators_at_attach() {
    let mut md = Report::new(
        "Bastion: an NftCreatorAllowlist with no creators is rejected at attach",
        "Attaching an NftCreatorAllowlist whose creator list is empty is malformed: there is no \
         creator that could ever pass, so AttachPolicy fails with InvalidPolicyData before any \
         execute.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Attach an NftCreatorAllowlist with an empty creator list: rejected at attach");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::NftCreatorAllowlist { creators: vec![] },
            },
        )
        .send_err_named("InvalidPolicyData");

    md.block("account index", ctx.account_index());
    ctx.report_execution(&mut md);
}
