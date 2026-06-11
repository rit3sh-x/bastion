//! Hardening checks: malformed policy data rejected at attach and update, a
//! wrapped compact-meta with reserved bits rejected at decode, and an NFT-only
//! collection policy ignoring an unrelated fungible token account.

mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_litesvm::{Lazy, Report, TokenFabrication, TokenProgram};
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, PolicyData, WindowKind};
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use bastion::utils::helpers::BastionSeed;
use helpers::*;

#[test]
fn attach_rejects_spendcap_nft_count_in_collection() {
    let mut md = Report::new(
        "Bastion: AttachPolicy rejects a SpendCap over NFT-count-in-collection",
        "A SpendCap whose asset is NftCountInCollection is malformed: SpendCap meters a \
         fungible flow, so a collection NFT count is not a valid asset. AttachPolicy must \
         reject it with InvalidPolicyData before the policy account is ever written.",
    );
    // The attach is under test (it must fail), so it stays a manual send_err.
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);
    let spend_cap = cast_policy(&mut ctx, &s, "SpendCap");

    md.step("Attach a SpendCap over NftCountInCollection: malformed, must be rejected");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::SpendCap {
                    asset: Asset::NftCountInCollection(Pubkey::new_unique()),
                    window: WindowKind::Fixed { secs: 60 },
                    max: 3,
                    state: SpendState::default(),
                },
            },
        )
        .send_err_named("InvalidPolicyData");

    md.check(
        "SpendCap policy account was never created",
        false,
        ctx.account_exists(&spend_cap),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn attach_rejects_spendcap_any_nft_count() {
    let mut md = Report::new(
        "Bastion: AttachPolicy rejects a SpendCap over AnyNftCount",
        "AnyNftCount is likewise not a fungible asset a SpendCap can meter; AttachPolicy \
         rejects the malformed policy with InvalidPolicyData.",
    );
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);
    let spend_cap = cast_policy(&mut ctx, &s, "SpendCap");

    md.step("Attach a SpendCap over AnyNftCount: malformed, must be rejected");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::AttachPolicy {
                data: PolicyData::SpendCap {
                    asset: Asset::AnyNftCount,
                    window: WindowKind::Fixed { secs: 60 },
                    max: 3,
                    state: SpendState::default(),
                },
            },
        )
        .send_err_named("InvalidPolicyData");

    md.check(
        "SpendCap policy account was never created",
        false,
        ctx.account_exists(&spend_cap),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn update_policy_rejects_unimplemented_asset_swap() {
    let mut md = Report::new(
        "Bastion: UpdatePolicy rejects an asset swap to an unimplemented asset",
        "A SpendCap is first attached over NativeSol (valid). An UpdatePolicy that swaps the \
         asset to AnyNftCount is the same malformed shape AttachPolicy rejects, so the update \
         must also fail with InvalidPolicyData; the on-chain policy is left untouched.",
    );
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);

    md.step("Open a session + attach a valid SpendCap over NativeSol (seed 0)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 60 },
            max: 1000,
            state: SpendState::default(),
        },
    );

    md.step("Update seed 0 to swap the asset to AnyNftCount: malformed, must be rejected");
    let mut bundle = s.bundle;
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::SpendCap {
                    asset: Asset::AnyNftCount,
                    window: WindowKind::Fixed { secs: 60 },
                    max: 3,
                    state: SpendState::default(),
                },
            },
        )
        .send_err_named("InvalidPolicyData");

    md.check(
        "policy still present after rejected update",
        true,
        ctx.account_exists(&spend_cap),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn execute_rejects_compact_meta_with_reserved_bits() {
    let mut md = Report::new(
        "Bastion: Execute rejects a wrapped ix whose compact meta sets reserved bits",
        "A wrapped System::Transfer is hand-built with a CompactAccountMeta whose flag byte \
         sets a reserved bit (0b1000_0011). Execute decodes the compact metas before \
         dispatch and must reject the malformed meta with InvalidCompactMeta, never reaching \
         the inner transfer.",
    );
    let (mut ctx, _owner, session_kp, s) = bootstrap(ONE_SOL);
    let dest = ctx.cast_account("recipient");

    // A wrapped transfer whose first compact meta sets a reserved flag bit; the
    // malformed flag byte is built by hand (`CompactAccountMeta::new` can't set it).
    let wix = WrappedInstruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            CompactAccountMeta {
                index: 0,
                flags: 0b1000_0011,
            },
            CompactAccountMeta::new(1, false, true),
        ],
        data: transfer_wrapped(1_000).data,
    };
    let extras = transfer_tail(&[], s.delegate, dest);

    md.step("Execute the malformed wrapped ix: must be rejected at decode");
    ctx.tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![wix],
                policy_count: 0,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("InvalidCompactMeta");
    ctx.report_execution(&mut md);
}

#[test]
fn nft_allowlist_ignores_token_accounts_when_mint_not_passed() {
    let mut md = Report::new(
        "Bastion: an NFT-collection allowlist ignores non-NFT token accounts",
        "A session carries an NftCollectionAllowlist. The execute tail includes a fungible \
         SPL token account (USDC) owned by the delegate, but no NFT mint is passed. The \
         policy only constrains NFTs, so the unrelated token account is ignored and the \
         transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let dest = ctx.cast_account("recipient");

    // A fungible USDC token account owned by the delegate, unrelated to any NFT.
    let usdc_mint = Pubkey::new_unique();
    let usdc_acct = Pubkey::new_unique();
    ctx.svm
        .fabricate_token_account(&usdc_acct, TokenProgram::Spl, &usdc_mint, &s.delegate, 100);
    ctx.alias(usdc_acct, "USDC-account");

    md.step("Attach an NftCollectionAllowlist (one allowed collection)");
    let allowlist = attach(
        &mut ctx,
        &owner,
        &s,
        "CollectionAllowlist",
        PolicyData::NftCollectionAllowlist {
            collections: vec![Pubkey::new_from_array([0xCC; 32])],
        },
    );

    md.step("Execute a SOL transfer with a non-NFT token account in the tail: ignored");
    let mut ix_accounts = transfer_ix_accounts(s.delegate, dest);
    ix_accounts.push(AccountMeta::new_readonly(usdc_acct, false));
    let extras = dispatch_tail(&[policy_meta(allowlist, false)], s.delegate, &ix_accounts);
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
        "allowlist policy survived the execute (transfer settled, not rejected)",
        true,
        ctx.account_exists(&allowlist),
    );
    ctx.report_execution(&mut md);
}
