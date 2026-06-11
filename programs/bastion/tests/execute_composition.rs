mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_litesvm::{AnchorContext, Report};
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use helpers::*;
use solana_keypair::Keypair;
use solana_signer::Signer;

const CU_CAP: u32 = 400_000;
const PRIORITY_CAP: u64 = 50_000;
const DELEGATE_FLOOR: u64 = ONE_SOL / 100;
const HAPPY_LAMPORTS: u64 = 1_000;

/// The discriminator the IxDiscriminatorAllowlist permits: System `Transfer`
/// (tag 2) of 1_000 lamports — the first 8 bytes of the wrapped transfer data.
const HAPPY_DISC: [u8; 8] = [2, 0, 0, 0, 0xE8, 0x03, 0, 0];

/// A composed session: a funded session carrying all six policies, plus a cast
/// recipient. Every scenario starts from `compose()` and then drives one execute.
struct Composed {
    ctx: AnchorContext,
    session_kp: Keypair,
    s: SessionCast,
    policies: Vec<Pubkey>,
    recipient: Pubkey,
}

/// Bootstrap a funded session and attach the six policies in slot order (the
/// shared `attach_all` carries each prior policy as the attach chain so the
/// program can re-hash the set). The recipient is cast — `cast_account` seeds it
/// with 1 SOL, the baseline the happy-path assertion measures against.
fn compose() -> Composed {
    let sys = anchor_lang::system_program::ID;
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let policies = attach_all(
        &mut ctx,
        &owner,
        &s,
        vec![
            (
                "ProgramAllowlist",
                PolicyData::ProgramAllowlist {
                    programs: vec![sys],
                },
            ),
            (
                "MaxPriorityFee",
                PolicyData::MaxPriorityFee {
                    max_micro_lamports: PRIORITY_CAP,
                },
            ),
            (
                "MaxComputeUnits",
                PolicyData::MaxComputeUnits { max: CU_CAP },
            ),
            (
                "MinDelegateBalance",
                PolicyData::MinDelegateBalance {
                    floor: DELEGATE_FLOOR,
                },
            ),
            (
                "IxDiscriminatorAllowlist",
                PolicyData::IxDiscriminatorAllowlist {
                    program: sys,
                    discriminators: vec![HAPPY_DISC.to_vec()],
                },
            ),
            ("RequireMemo", PolicyData::RequireMemo { memo_program: sys }),
        ],
    );
    Composed {
        ctx,
        session_kp,
        s,
        policies,
        recipient,
    }
}

/// The outer ixs the happy path needs: a CU limit under the cap, a priority fee
/// under the cap, and a memo that satisfies RequireMemo (a System self-transfer,
/// which the memo policy — configured with `memo_program = System` — recognizes
/// by program id).
fn happy_outer_ixs(payer: &Pubkey) -> Vec<Instruction> {
    let memo_data = transfer_wrapped(1).data; // a System self-transfer's data
    let memo_ix = Instruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(*payer, false),
        ],
        data: memo_data,
    };
    vec![
        set_compute_unit_limit_ix(200_000),
        set_compute_unit_price_ix(10_000),
        memo_ix,
    ]
}

fn happy_wrapped_ix() -> WrappedInstruction {
    transfer_wrapped(HAPPY_LAMPORTS)
}

/// Assemble the multi-ix execute transaction and send it: the `Execute` projects
/// from the bundle, the six policy metas + delegate/dest tail ride as positional
/// `remaining_accounts`, and `outer` goes ahead of it in the same tx (the
/// policies read those via the instructions sysvar). Routed through the tracked
/// `send_instructions` so pass or fail lands in the report and diagram.
fn send_outer(
    ctx: &mut AnchorContext,
    session_kp: &Keypair,
    s: &SessionCast,
    wrapped: WrappedInstruction,
    extras: &[AccountMeta],
    outer: Vec<Instruction>,
    expect: Expect,
) {
    let mut exec_ix = ctx.program().build_ix(
        s.bundle,
        bastion::instruction::Execute {
            wrapped_ixs: vec![wrapped],
            policy_count: 6,
            expected_nonce: None,
            manifest: None,
        },
    );
    exec_ix.accounts.extend_from_slice(extras);

    let mut ixs = outer;
    ixs.push(exec_ix);

    ctx.svm.expire_blockhash();
    let result = ctx.send_instructions(&ixs, &[session_kp]);
    match expect {
        Expect::Ok => {
            result.assert_success();
        }
        Expect::Err(name) => {
            result.assert_error(name);
        }
    }
}

#[test]
fn happy_path_all_six_policies_satisfied() {
    let mut md = Report::new(
        "Bastion: one execute satisfies all six composed policies at once",
        "A session carries six policies (program allowlist, max priority fee, max compute \
         units, min delegate balance, ix-discriminator allowlist, require-memo). One execute \
         carries the outer compute-budget + memo ixs and a wrapped System transfer that \
         satisfies every policy, so the dispatch lands.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    md.step("Execute: outer CU/price/memo + wrapped transfer, all six satisfied");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        happy_wrapped_ix(),
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
        Expect::Ok,
    );

    md.check(
        "recipient received the wrapped transfer",
        Some(ONE_SOL + HAPPY_LAMPORTS),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rejection_missing_memo() {
    let mut md = Report::new(
        "Bastion: composition rejects when the required memo is absent",
        "All six policies attach, but the execute carries only a CU-limit outer ix (no memo). \
         RequireMemo trips: MissingRequiredMemo.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    md.step("Execute with no memo ix: RequireMemo rejects");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        happy_wrapped_ix(),
        &extras,
        vec![set_compute_unit_limit_ix(200_000)],
        Expect::Err("MissingRequiredMemo"),
    );
    md.check(
        "recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rejection_priority_fee_too_high() {
    let mut md = Report::new(
        "Bastion: composition rejects when the priority fee exceeds the cap",
        "All six policies attach; the execute sets a compute-unit price one over PRIORITY_CAP. \
         MaxPriorityFee trips: PriorityFeeTooHigh.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    md.step("Execute with cu_price = PRIORITY_CAP + 1: MaxPriorityFee rejects");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        happy_wrapped_ix(),
        &extras,
        vec![
            set_compute_unit_limit_ix(200_000),
            set_compute_unit_price_ix(PRIORITY_CAP + 1),
        ],
        Expect::Err("PriorityFeeTooHigh"),
    );
    md.check(
        "recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rejection_compute_units_too_high() {
    let mut md = Report::new(
        "Bastion: composition rejects when the compute-unit limit exceeds the cap",
        "All six policies attach; the execute sets a CU limit one over CU_CAP. \
         MaxComputeUnits trips: ComputeUnitsTooHigh.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    md.step("Execute with cu_limit = CU_CAP + 1: MaxComputeUnits rejects");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        happy_wrapped_ix(),
        &extras,
        vec![
            set_compute_unit_limit_ix(CU_CAP + 1),
            set_compute_unit_price_ix(10_000),
        ],
        Expect::Err("ComputeUnitsTooHigh"),
    );
    md.check(
        "recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rejection_wrong_ix_discriminator() {
    let mut md = Report::new(
        "Bastion: composition rejects a wrapped ix with a disallowed discriminator",
        "All six policies attach; the wrapped ix is a System `Allocate` (tag 9), not the \
         allowlisted `Transfer` (tag 2). IxDiscriminatorAllowlist trips: \
         IxDiscriminatorNotAllowed.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    // A System `Allocate` (tag 9) — same accounts, disallowed discriminator.
    let wix = WrappedInstruction {
        program_id: anchor_lang::system_program::ID,
        accounts: vec![
            CompactAccountMeta::new(0, true, true),
            CompactAccountMeta::new(1, false, true),
        ],
        data: vec![9u8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    };

    md.step("Execute with a non-allowlisted discriminator: IxDiscriminatorAllowlist rejects");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        wix,
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
        Expect::Err("IxDiscriminatorNotAllowed"),
    );
    md.check(
        "recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rejection_min_balance_drained() {
    let mut md = Report::new(
        "Bastion: composition rejects when the delegate drops below the floor",
        "All six policies attach; the delegate is set just below DELEGATE_FLOOR + the transfer \
         amount, so the wrapped transfer would breach the floor. MinDelegateBalance trips: \
         DelegateBalanceTooLow.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    md.step("Drain the delegate to one under the floor-plus-transfer threshold");
    let mut acct = ctx.svm.get_account(&s.delegate).expect("delegate exists");
    acct.lamports = DELEGATE_FLOOR + HAPPY_LAMPORTS - 1;
    ctx.svm
        .set_account(s.delegate, acct)
        .expect("set delegate balance");

    md.step("Execute: MinDelegateBalance rejects");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        happy_wrapped_ix(),
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
        Expect::Err("DelegateBalanceTooLow"),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn rejection_wrong_inner_program() {
    let mut md = Report::new(
        "Bastion: composition rejects a wrapped ix targeting an unallowlisted program",
        "All six policies attach; the wrapped ix targets a random program id, not System. \
         ProgramAllowlist trips: ProgramNotAllowed.",
    );
    let Composed {
        mut ctx,
        session_kp,
        s,
        policies,
        recipient,
    } = compose();
    let extras = transfer_tail(&policies, s.delegate, recipient);

    // A transfer-shaped ix aimed at a program the allowlist never named.
    let wix = WrappedInstruction {
        program_id: Pubkey::new_unique(),
        accounts: vec![CompactAccountMeta::new(0, true, true)],
        data: vec![2u8, 0, 0, 0, 0, 0, 0, 0],
    };

    md.step("Execute targeting a non-allowlisted program: ProgramAllowlist rejects");
    send_outer(
        &mut ctx,
        &session_kp,
        &s,
        wix,
        &extras,
        happy_outer_ixs(&session_kp.pubkey()),
        Expect::Err("ProgramNotAllowed"),
    );
    md.check(
        "recipient untouched",
        Some(ONE_SOL),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
