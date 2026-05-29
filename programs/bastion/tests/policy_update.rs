mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, PolicyData, WindowKind};

use crate::helpers::*;

#[test]
fn update_policy_replaces_data_in_place() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _session_kp) = init_session(&mut svm, &owner, 86_400).expect("init");

  let initial = PolicyData::ProgramAllowlist {
    programs: vec![Pubkey::new_unique()],
  };
  let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

  let new_progs = vec![
    Pubkey::new_unique(),
    Pubkey::new_unique(),
    Pubkey::new_unique(),
  ];
  let new_data = PolicyData::ProgramAllowlist {
    programs: new_progs.clone(),
  };

  svm.expire_blockhash();
  update_policy(&mut svm, &owner, &session_pda, &p0, seed, new_data).expect("update ok");

  let policy = fetch_policy(&svm, &p0);
  match policy.data {
    PolicyData::ProgramAllowlist { programs } => assert_eq!(programs, new_progs),
    _ => panic!("kind changed"),
  }
}

#[test]
fn update_policy_rejects_kind_change() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let initial = PolicyData::ProgramAllowlist {
    programs: vec![Pubkey::new_unique()],
  };
  let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

  let wrong_kind = PolicyData::Expiry {
    not_after: now(&svm).checked_add(1000).expect("not_after overflow"),
  };
  svm.expire_blockhash();
  let res = update_policy(&mut svm, &owner, &session_pda, &p0, seed, wrong_kind);
  assert_svm_anchor_error(res, BastionError::PolicyKindMismatch);
}

#[test]
fn update_policy_swaps_window_kind_within_same_policy_kind() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let initial = PolicyData::SpendCap {
    asset: Asset::NativeSol,
    window: WindowKind::Fixed { secs: 3_600 },
    max: 1_000_000,
    state: SpendState::default(),
  };
  let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

  let swapped = PolicyData::SpendCap {
    asset: Asset::NativeSol,
    window: WindowKind::Rolling {
      secs: 3_600,
      slots: 4,
    },
    max: 2_000_000,
    state: SpendState::default(),
  };
  svm.expire_blockhash();
  update_policy(&mut svm, &owner, &session_pda, &p0, seed, swapped).expect("window swap ok");

  let pol = fetch_policy(&svm, &p0);
  match pol.data {
    PolicyData::SpendCap { window, max, .. } => {
      assert_eq!(max, 2_000_000, "max raised");
      assert!(matches!(
        window,
        WindowKind::Rolling {
          secs: 3_600,
          slots: 4
        }
      ));
    }
    _ => panic!("expected SpendCap after update"),
  }
}
