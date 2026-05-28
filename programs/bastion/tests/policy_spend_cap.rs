mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::error::BastionError;
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, PolicyData, WindowKind};
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use solana_signer::Signer;

use crate::helpers::*;

fn fund_session_and_delegate(
    svm: &mut litesvm::LiteSVM,
    owner: &solana_keypair::Keypair,
    delegate_lamports: u64,
) -> (Pubkey, solana_keypair::Keypair, Pubkey, Pubkey) {
    let (session_pda, session_kp) = init_session(svm, owner, 86_400).expect("init");
    let session_airdrop = 5_u64.saturating_mul(ONE_SOL);
    airdrop(svm, &session_kp.pubkey(), session_airdrop);
    let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
    airdrop(svm, &delegate, delegate_lamports);
    let dest = Pubkey::new_unique();
    airdrop(svm, &dest, 1);
    (session_pda, session_kp, delegate, dest)
}

#[test]
fn sol_spend_cap_charges_outflow_within_fixed_window() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) =
        fund_session_and_delegate(&mut svm, &owner, 10_u64.saturating_mul(ONE_SOL));

    let data = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 1_000_000,
        state: SpendState::default(),
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
    let extras = extras_sol_transfer_one_policy(&p0, &delegate, &dest);

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(400_000),
        1,
        &extras,
    )
    .expect("first transfer within cap");

    let pol = fetch_policy(&svm, &p0);
    match pol.data {
        PolicyData::SpendCap { state, .. } => assert_eq!(state.spent, 400_000),
        _ => panic!("expected SpendCap"),
    }

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(400_000),
        1,
        &extras,
    )
    .expect("second transfer within cap");

    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(400_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::SpendCapExceeded);
}

#[test]
fn sol_spend_cap_enforces_rent_exempt_floor() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) =
        fund_session_and_delegate(&mut svm, &owner, 1_500_000);

    let spend_cap_max = 100_u64.saturating_mul(ONE_SOL);

    let data = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: spend_cap_max,
        state: SpendState::default(),
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
    let extras = extras_sol_transfer_one_policy(&p0, &delegate, &dest);

    svm.expire_blockhash();
    let err = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000_000),
        1,
        &extras,
    );

    assert!(
        err.is_err(),
        "must fail (rent-exempt floor or system rejection)"
    );
}

#[test]
fn sol_spend_cap_rolling_window_slides_across_slots() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) =
        fund_session_and_delegate(&mut svm, &owner, 10_u64.saturating_mul(ONE_SOL));

    let data = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Rolling { secs: 60, slots: 2 },
        max: 1_000_000,
        state: SpendState::default(),
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
    let extras = extras_sol_transfer_one_policy(&p0, &delegate, &dest);

    for _ in 0..2 {
        svm.expire_blockhash();
        execute(
            &mut svm,
            &session_kp,
            &session_pda,
            transfer_wrapped_ix(400_000),
            1,
            &extras,
        )
        .expect("within rolling cap");
    }

    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(400_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::SpendCapExceeded);

    advance_clock(&mut svm, 35);
    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(400_000),
        1,
        &extras,
    )
    .expect("after slot slides, budget reopens");
}

#[test]
fn spl_token_spend_cap_snapshots_via_token_account_layout() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _session_kp, delegate, _dest) =
        fund_session_and_delegate(&mut svm, &owner, ONE_SOL);

    let mint = Pubkey::new_unique();
    let source_ata = Pubkey::new_unique();
    make_spl_token_account(&mut svm, &source_ata, mint, delegate, 10_000);

    let data = PolicyData::SpendCap {
        asset: Asset::SplToken(mint),
        window: WindowKind::Fixed { secs: 86_400 },
        max: 5_000,
        state: SpendState::default(),
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    let pol = fetch_policy(&svm, &p0);
    match pol.data {
        PolicyData::SpendCap {
            asset: Asset::SplToken(stored_mint),
            max,
            ..
        } => {
            assert_eq!(stored_mint, mint);
            assert_eq!(max, 5_000);
        }
        _ => panic!("expected SpendCap{{SplToken}}"),
    }

    let acct = svm
        .get_account(&source_ata)
        .expect("source ATA account must exist");
    assert_eq!(acct.lamports, 2_039_280, "rent for token account");
    assert!(!acct.data.is_empty(), "token account data present");
}

#[test]
fn sol_spend_cap_noop_when_outflow_is_zero() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) =
        fund_session_and_delegate(&mut svm, &owner, ONE_SOL);

    let data = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 100,
        state: SpendState::default(),
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    let zero_transfer = WrappedInstruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            CompactAccountMeta {
                index: 0,
                flags: 0b11,
            },
            CompactAccountMeta {
                index: 1,
                flags: 0b10,
            },
        ],
        data: {
            let mut d = vec![0u8; 12];
            d[0..4].copy_from_slice(&2u32.to_le_bytes());
            d
        },
    };
    let extras = vec![
        AccountMeta::new(p0, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ];

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        zero_transfer,
        1,
        &extras,
    )
    .expect("zero outflow → no charge");

    let pol = fetch_policy(&svm, &p0);
    match pol.data {
        PolicyData::SpendCap { state, .. } => assert_eq!(state.spent, 0),
        _ => panic!("expected SpendCap"),
    }
}
