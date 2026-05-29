mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::state::counter::{CounterState, SpendState};
use bastion::state::policy::{Asset, PolicyData, WindowKind};
use bastion::BastionError;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn demo_full_scenario_replay() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 86_400).expect("init");

  let session_airdrop = 10_u64
    .checked_mul(ONE_SOL)
    .expect("session airdrop overflow");

  airdrop(&mut svm, &session_kp.pubkey(), session_airdrop);

  let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());

  let delegate_airdrop = 20_u64
    .checked_mul(ONE_SOL)
    .expect("delegate airdrop overflow");

  airdrop(&mut svm, &delegate, delegate_airdrop);

  let dest = Pubkey::new_unique();
  airdrop(&mut svm, &dest, 1);

  let p1_data = PolicyData::ProgramAllowlist {
    programs: vec![anchor_lang::system_program::ID],
  };
  let (p1, _) = attach_policy(&mut svm, &owner, &session_pda, p1_data, &[]).expect("attach 1");

  svm.expire_blockhash();
  let p2_data = PolicyData::RateLimit {
    window: WindowKind::Fixed { secs: 60 },
    max: 3,
    state: CounterState::default(),
    scope: None,
  };
  let (p2, _) = attach_policy(&mut svm, &owner, &session_pda, p2_data, &[p1]).expect("attach 2");

  svm.expire_blockhash();
  let p3_data = PolicyData::SpendCap {
    asset: Asset::NativeSol,
    window: WindowKind::Fixed { secs: 60 },
    max: 500_000,
    state: SpendState::default(),
  };
  let (p3, _) =
    attach_policy(&mut svm, &owner, &session_pda, p3_data, &[p1, p2]).expect("attach 3");

  let mk_extras = |policies: Vec<Pubkey>| -> Vec<AccountMeta> {
    let mut v: Vec<AccountMeta> = policies
      .into_iter()
      .map(|p| AccountMeta::new(p, false))
      .collect();
    v.push(AccountMeta::new(delegate, false));
    v.push(AccountMeta::new(delegate, false));
    v.push(AccountMeta::new(dest, false));
    v.push(AccountMeta::new_readonly(
      anchor_lang::system_program::ID,
      false,
    ));
    v
  };

  for i in 0..3 {
    svm.expire_blockhash();
    execute(
      &mut svm,
      &session_kp,
      &session_pda,
      transfer_wrapped_ix(100_000),
      3,
      &mk_extras(vec![p1, p2, p3]),
    )
    .unwrap_or_else(|e| panic!("transfer {} should pass: {:?}", i, e.err));
  }

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(100_000),
    3,
    &mk_extras(vec![p1, p2, p3]),
  );

  assert_svm_anchor_error(res, BastionError::RateLimitExceeded);

  advance_clock(&mut svm, 65);
  svm.expire_blockhash();

  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(600_000),
    3,
    &mk_extras(vec![p1, p2, p3]),
  );

  assert_svm_anchor_error(res, BastionError::SpendCapExceeded);

  svm.expire_blockhash();
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(50_000),
    3,
    &mk_extras(vec![p1, p2, p3]),
  )
  .expect("transfer within reset cap should pass");
}
