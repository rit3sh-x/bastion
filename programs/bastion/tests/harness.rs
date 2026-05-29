mod helpers;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn setup_svm_loads_program_and_funds_payer() {
  let (svm, payer) = setup_svm();
  let bal = svm.get_balance(&payer.pubkey()).unwrap();
  assert_eq!(bal, 100 * ONE_SOL);
}

#[test]
fn pda_derivations_are_deterministic() {
  let owner = Pubkey::new_unique();
  let session_key = Pubkey::new_unique();

  let (s1, _) = derive_session_pda(&owner, &session_key);
  let (s2, _) = derive_session_pda(&owner, &session_key);
  assert_eq!(s1, s2, "session PDA must be deterministic");

  let (d1, _) = derive_delegate_pda(&owner, &session_key);
  let (d2, _) = derive_delegate_pda(&owner, &session_key);
  assert_eq!(d1, d2, "delegate PDA must be deterministic");

  let other_key = Pubkey::new_unique();
  let (s3, _) = derive_session_pda(&owner, &other_key);
  assert_ne!(s1, s3);

  let (p0, _) = derive_policy_pda(&s1, 0);
  let (p1, _) = derive_policy_pda(&s1, 1);
  assert_ne!(p0, p1);
}
