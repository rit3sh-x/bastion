mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use litesvm::types::FailedTransactionMetadata;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::helpers::*;

const CU_CAP: u32 = 400_000;
const PRIORITY_CAP: u64 = 50_000;
const DELEGATE_FLOOR: u64 = ONE_SOL / 100;

const HAPPY_LAMPORTS: u64 = 1_000;

const HAPPY_DISC: [u8; 8] = [2, 0, 0, 0, 0xE8, 0x03, 0, 0];

fn build_six_policy_session(
    svm: &mut LiteSVM,
    owner: &Keypair,
) -> (Pubkey, Keypair, Pubkey, Pubkey, [Pubkey; 6]) {
    let (session_pda, session_kp) = init_session(svm, owner, 86_400).expect("init");
    airdrop(svm, &session_kp.pubkey(), ONE_SOL);
    let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
    airdrop(svm, &delegate, ONE_SOL);
    let dest = Pubkey::new_unique();
    airdrop(svm, &dest, 1);

    let allowed_disc = HAPPY_DISC;

    let p0 = attach_policy(
        svm,
        owner,
        &session_pda,
        PolicyData::ProgramAllowlist {
            programs: vec![anchor_lang::system_program::ID],
        },
        &[],
    )
    .expect("p0")
    .0;
    let p1 = attach_policy(
        svm,
        owner,
        &session_pda,
        PolicyData::MaxPriorityFee {
            max_micro_lamports: PRIORITY_CAP,
        },
        &[p0],
    )
    .expect("p1")
    .0;
    let p2 = attach_policy(
        svm,
        owner,
        &session_pda,
        PolicyData::MaxComputeUnits { max: CU_CAP },
        &[p0, p1],
    )
    .expect("p2")
    .0;
    let p3 = attach_policy(
        svm,
        owner,
        &session_pda,
        PolicyData::MinDelegateBalance {
            floor: DELEGATE_FLOOR,
        },
        &[p0, p1, p2],
    )
    .expect("p3")
    .0;
    let p4 = attach_policy(
        svm,
        owner,
        &session_pda,
        PolicyData::IxDiscriminatorAllowlist {
            program: anchor_lang::system_program::ID,
            discriminators: vec![allowed_disc.to_vec()],
        },
        &[p0, p1, p2, p3],
    )
    .expect("p4")
    .0;

    let p5 = attach_policy(
        svm,
        owner,
        &session_pda,
        PolicyData::RequireMemo {
            memo_program: anchor_lang::system_program::ID,
        },
        &[p0, p1, p2, p3, p4],
    )
    .expect("p5")
    .0;

    (
        session_pda,
        session_kp,
        delegate,
        dest,
        [p0, p1, p2, p3, p4, p5],
    )
}

fn extras_six(policies: &[Pubkey; 6], delegate: &Pubkey, dest: &Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(policies[0], false),
        AccountMeta::new(policies[1], false),
        AccountMeta::new(policies[2], false),
        AccountMeta::new(policies[3], false),
        AccountMeta::new(policies[4], false),
        AccountMeta::new(policies[5], false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ]
}

fn send_outer(
    svm: &mut LiteSVM,
    session_kp: &Keypair,
    session_pda: &Pubkey,
    wrapped: WrappedInstruction,
    extras: &[AccountMeta],
    extra_outer: Vec<Instruction>,
) -> std::result::Result<(), FailedTransactionMetadata> {
    execute_with_outer_ixs(
        svm,
        session_kp,
        session_pda,
        wrapped,
        6,
        extras,
        extra_outer,
    )
}

fn happy_outer_ixs(payer: &Pubkey) -> Vec<Instruction> {
    let memo_data = {
        let mut d = vec![0u8; 12];
        d[0..4].copy_from_slice(&2u32.to_le_bytes());
        d[4..12].copy_from_slice(&1u64.to_le_bytes());
        d
    };
    let memo_ix = Instruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(*payer, false),
        ],
        data: memo_data,
    };
    vec![set_cu_limit_ix(200_000), set_cu_price_ix(10_000), memo_ix]
}

fn happy_wrapped_ix() -> WrappedInstruction {
    transfer_wrapped_ix(HAPPY_LAMPORTS)
}

#[test]
fn happy_path_all_six_policies_satisfied() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);
    send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        happy_wrapped_ix(),
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
    )
    .expect("all six policies satisfied → tx should land");
}

#[test]
fn rejection_missing_memo() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);

    let outer = vec![set_cu_limit_ix(200_000)];
    let res = send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        happy_wrapped_ix(),
        &extras,
        outer,
    );

    assert_svm_anchor_error(res, BastionError::MissingRequiredMemo);
}

#[test]
fn rejection_priority_fee_too_high() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);
    let outer = vec![set_cu_limit_ix(200_000), set_cu_price_ix(PRIORITY_CAP + 1)];
    let res = send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        happy_wrapped_ix(),
        &extras,
        outer,
    );
    assert_svm_anchor_error(res, BastionError::PriorityFeeTooHigh);
}

#[test]
fn rejection_compute_units_too_high() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);
    let outer = vec![set_cu_limit_ix(CU_CAP + 1), set_cu_price_ix(10_000)];
    let res = send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        happy_wrapped_ix(),
        &extras,
        outer,
    );
    assert_svm_anchor_error(res, BastionError::ComputeUnitsTooHigh);
}

#[test]
fn rejection_wrong_ix_discriminator() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);

    let wix = WrappedInstruction {
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
        data: vec![9u8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    };
    let res = send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        wix,
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
    );
    assert_svm_anchor_error(res, BastionError::IxDiscriminatorNotAllowed);
}

#[test]
fn rejection_min_balance_drained() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);

    let mut acct = svm.get_account(&delegate).expect("delegate exists");

    acct.lamports = DELEGATE_FLOOR + HAPPY_LAMPORTS - 1;
    svm.set_account(delegate, acct)
        .expect("set delegate balance");

    let res = send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        happy_wrapped_ix(),
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
    );
    assert_svm_anchor_error(res, BastionError::DelegateBalanceTooLow);
}

#[test]
fn rejection_wrong_inner_program() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest, policies) =
        build_six_policy_session(&mut svm, &owner);
    let extras = extras_six(&policies, &delegate, &dest);

    let wix = WrappedInstruction {
        program_id: Pubkey::new_unique(),
        accounts: vec![CompactAccountMeta {
            index: 0,
            flags: 0b11,
        }],
        data: vec![2u8, 0, 0, 0, 0, 0, 0, 0],
    };
    let res = send_outer(
        &mut svm,
        &session_kp,
        &session_pda,
        wix,
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
    );
    assert_svm_anchor_error(res, BastionError::ProgramNotAllowed);
}
