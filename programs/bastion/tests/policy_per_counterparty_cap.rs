mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::policy::{Asset, PolicyData};
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn per_counterparty_cap_charges_inflow() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::PerCounterpartyCap {
        receiver: dest,
        asset: Asset::NativeSol,
        max: 5_000,
        sent: 0,
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(3_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    )
    .expect("under cap");
    svm.expire_blockhash();

    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(2_001),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::CounterpartyCapExceeded);
}

#[test]
fn per_counterparty_cap_accumulates_across_calls() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::PerCounterpartyCap {
        receiver: dest,
        asset: Asset::NativeSol,
        max: 10_000,
        sent: 0,
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    for amt in [3_000_u64, 3_000, 3_000] {
        svm.expire_blockhash();
        execute(
            &mut svm,
            &session_kp,
            &session_pda,
            transfer_wrapped_ix(amt),
            1,
            &extras_sol_transfer_one_policy(&p, &delegate, &dest),
        )
        .expect("under cap (cumulative 9_000 ≤ 10_000)");
    }

    let pol = fetch_policy(&svm, &p);
    match pol.data {
        PolicyData::PerCounterpartyCap { sent, .. } => assert_eq!(sent, 9_000),
        _ => panic!("expected PerCounterpartyCap"),
    }

    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_001),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::CounterpartyCapExceeded);
}

#[test]
fn per_counterparty_cap_noop_when_receiver_out_of_scope() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let unrelated_receiver = Pubkey::new_unique();

    let data = PolicyData::PerCounterpartyCap {
        receiver: unrelated_receiver,
        asset: Asset::NativeSol,
        max: 1,
        sent: 0,
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    )
    .expect("out-of-scope receiver → no charge, no error");

    let pol = fetch_policy(&svm, &p);
    match pol.data {
        PolicyData::PerCounterpartyCap { sent, .. } => {
            assert_eq!(
                sent, 0,
                "sent must remain zero when receiver was out of scope"
            );
        }
        _ => panic!("expected PerCounterpartyCap"),
    }
}

#[test]
fn attach_rejects_nft_asset() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");
    let session = fetch_session(&svm, &session_pda);
    let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
    let ix = attach_policy_ix(
        &owner.pubkey(),
        &session_pda,
        &policy_pda,
        PolicyData::PerCounterpartyCap {
            receiver: Pubkey::new_unique(),
            asset: Asset::AnyNftCount,
            max: 1,
            sent: 0,
        },
        &[],
    );
    send_ix(&mut svm, ix, &[&owner]).expect_err("NFT asset variants rejected");
}
