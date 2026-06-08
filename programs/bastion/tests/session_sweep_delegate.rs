mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError::SessionNotRevoked;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn sweep_rejects_when_session_not_revoked() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_key) = init_session(&mut svm, &owner, 3600).expect("init");
    let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_key.pubkey());

    let delegate_funding = 5_u64
        .checked_mul(ONE_SOL)
        .expect("delegate funding overflow");

    airdrop(&mut svm, &delegate_pda, delegate_funding);

    let destination = Pubkey::new_unique();
    airdrop(&mut svm, &destination, ONE_SOL);

    let res = sweep_delegate(&mut svm, &owner, &session_pda, &delegate_pda, &destination);
    assert_svm_anchor_error(res, SessionNotRevoked);
}

#[test]
fn sweep_moves_all_lamports_after_revoke() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_key) = init_session(&mut svm, &owner, 3600).expect("init");
    let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_key.pubkey());

    let funded = 5_u64
        .checked_mul(ONE_SOL)
        .expect("delegate funding overflow");
    airdrop(&mut svm, &delegate_pda, funded);
    assert_eq!(svm.get_balance(&delegate_pda).unwrap(), funded);

    let destination = Pubkey::new_unique();
    airdrop(&mut svm, &destination, ONE_SOL);
    let dest_pre = svm.get_balance(&destination).unwrap();

    revoke_session(&mut svm, &owner, &session_pda).expect("revoke");
    sweep_delegate(&mut svm, &owner, &session_pda, &delegate_pda, &destination).expect("sweep ok");

    let post_delegate = svm.get_balance(&delegate_pda).unwrap_or(0);
    assert_eq!(post_delegate, 0, "delegate fully swept");

    let dest_post = svm.get_balance(&destination).unwrap();
    assert_eq!(
        dest_post,
        dest_pre + funded,
        "destination received exactly the delegate's lamports"
    );
}

#[test]
fn sweep_with_empty_delegate_is_noop() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_key) = init_session(&mut svm, &owner, 3600).expect("init");
    let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_key.pubkey());

    let destination = Pubkey::new_unique();
    airdrop(&mut svm, &destination, ONE_SOL);
    let dest_pre = svm.get_balance(&destination).unwrap();

    revoke_session(&mut svm, &owner, &session_pda).expect("revoke");
    sweep_delegate(&mut svm, &owner, &session_pda, &delegate_pda, &destination)
        .expect("sweep ok even when delegate has nothing");

    assert_eq!(svm.get_balance(&destination).unwrap(), dest_pre);
}
