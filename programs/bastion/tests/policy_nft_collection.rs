mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::{MetadataArgs, MetaplexHelpers, Report, TokenFabrication, TokenProgram};
use bastion::state::policy::PolicyData;
use helpers::*;

/// Fabricate an NFT mint and its verified-collection metadata, aliased for the
/// authority story. Returns `(nft_mint, metadata)` for the dispatch tail.
fn fabricate_collection_nft(
    ctx: &mut anchor_litesvm::AnchorContext,
    collection: Pubkey,
    collection_alias: &str,
) -> (Pubkey, Pubkey) {
    let nft_mint = Pubkey::new_unique();
    ctx.svm.fabricate_nft_mint(&nft_mint, TokenProgram::Spl);
    let metadata = ctx.svm.fabricate_metadata(
        &nft_mint,
        &MetadataArgs {
            collection: Some((collection, true)),
            ..Default::default()
        },
    );
    ctx.alias(nft_mint, "NFT");
    ctx.alias(collection, collection_alias);
    (nft_mint, metadata)
}

/// The execute dispatch tail for a wrapped SOL transfer guarded by one
/// NFT-collection policy: the policy (read-only, it only inspects), the delegate
/// + the wrapped transfer's accounts, then the NFT mint + metadata the policy
/// reads to resolve the verified collection.
fn nft_collection_extras(
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
fn allowlist_passes_when_nft_belongs_to_allowed_collection() {
    let mut md = Report::new(
        "Bastion: an NftCollectionAllowlist admits an NFT in an allowed collection",
        "A session carries an NftCollectionAllowlist naming one collection; it holds an NFT \
         whose verified collection is that one. An execute presenting the mint + metadata \
         passes and the wrapped transfer pays out.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let collection = Pubkey::new_from_array([0xCC; 32]);
    let (nft_mint, metadata) = fabricate_collection_nft(&mut ctx, collection, "AllowedCollection");

    md.step("Attach an NftCollectionAllowlist naming the one allowed collection");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "CollectionAllowlist",
        PolicyData::NftCollectionAllowlist {
            collections: vec![collection],
        },
    );

    md.step("Execute presenting the NFT mint + metadata: collection is allowed, so it passes");
    let extras = nft_collection_extras(allowlist, s.delegate, recipient, nft_mint, metadata);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(10_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();

    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 10_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn allowlist_fails_when_collection_not_allowed() {
    let mut md = Report::new(
        "Bastion: an NftCollectionAllowlist rejects an NFT outside the list",
        "A session carries an NftCollectionAllowlist naming collection 0xAA; it holds an NFT \
         whose verified collection is 0xBB. An execute presenting the mint + metadata is \
         rejected with NftCollectionNotAllowed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let allowed = Pubkey::new_from_array([0xAA; 32]);
    let actual = Pubkey::new_from_array([0xBB; 32]);
    let (nft_mint, metadata) = fabricate_collection_nft(&mut ctx, actual, "NftCollection");
    ctx.alias(allowed, "AllowedCollection");

    md.step("Attach an NftCollectionAllowlist naming only the allowed (0xAA) collection");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "CollectionAllowlist",
        PolicyData::NftCollectionAllowlist {
            collections: vec![allowed],
        },
    );

    md.step("Execute presenting an NFT from 0xBB: not on the allowlist, so it is rejected");
    let extras = nft_collection_extras(allowlist, s.delegate, recipient, nft_mint, metadata);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(10_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("NftCollectionNotAllowed");

    md.check(
        "recipient received nothing (its rent-funded baseline is unchanged)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn blocklist_blocks_matching_collection() {
    let mut md = Report::new(
        "Bastion: an NftCollectionBlocklist rejects an NFT in a blocked collection",
        "A session carries an NftCollectionBlocklist naming collection 0xDD; it holds an NFT \
         whose verified collection is 0xDD. An execute presenting the mint + metadata is \
         rejected with NftCollectionBlocked.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let blocked = Pubkey::new_from_array([0xDD; 32]);
    let (nft_mint, metadata) = fabricate_collection_nft(&mut ctx, blocked, "BlockedCollection");

    md.step("Attach an NftCollectionBlocklist naming the blocked (0xDD) collection");
    let blocklist = attach(
        &mut ctx,
        &owner,
        &s,
        "CollectionBlocklist",
        PolicyData::NftCollectionBlocklist {
            collections: vec![blocked],
        },
    );

    md.step("Execute presenting an NFT from 0xDD: on the blocklist, so it is rejected");
    let extras = nft_collection_extras(blocklist, s.delegate, recipient, nft_mint, metadata);
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(10_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("NftCollectionBlocked");

    md.check(
        "recipient received nothing (its rent-funded baseline is unchanged)",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
