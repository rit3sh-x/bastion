use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;
use solana_instructions_sysvar::ID as INSTRUCTIONS_SYSVAR_ID;

use crate::constants::{MAX_POLICIES_PER_EXECUTE, SEED_DELEGATE, SEED_SESSION};
use crate::error::BastionError;
use crate::policies::amount_per_call::check_amount_per_call;
use crate::policies::min_delegate_balance::check_min_balance;
use crate::policies::per_counterparty_cap::{
    charge_counterparty_cap, snapshot_for_asset_at_receiver,
};
use crate::policies::per_program_spend_cap::charge_per_program_spend_cap;
use crate::policies::spend_cap::{charge_spend_cap, snapshot_for_asset, SpendCapCharge};
use crate::state::policy::{Asset, Policy, PolicyData};
use crate::state::session::Session;
use crate::state::wrapped_ix::WrappedInstruction;
use crate::utils::hash::compute_policies_hash;

#[derive(Accounts)]
pub struct Execute<'info> {
    pub session_key: Signer<'info>,

    #[account(
        seeds = [SEED_SESSION, session.owner.as_ref(), session_key.key().as_ref()],
        bump = session.bump,
    )]
    pub session: Account<'info, Session>,

    /// CHECK:
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,
}

struct ExecutionContext<'info> {
    pub policy_infos: Vec<&'info AccountInfo<'info>>,
    pub policies: Vec<Policy>,
    pub ix_accounts: &'info [AccountInfo<'info>],
    pub delegate_ai: &'info AccountInfo<'info>,
    pub expected_delegate: Pubkey,
}

struct PolicySnapshot {
    idx: usize,
    pre: u64,
    asset: Asset,
    action: PostAction,
}

enum PostAction {
    SpendCap,
    AmountPerCall,
    PerCounterpartyCap { receiver: Pubkey },
    PerProgramSpendCap,
}

impl<'info> Execute<'info> {
    pub fn execute_handler(
        &mut self,
        remaining_accounts: &'info [AccountInfo<'info>],
        wrapped_ix: WrappedInstruction,
        policy_count: u8,
    ) -> Result<()> {
        let clock = self.validate_session()?;

        let mut exec = self.collect_execution_context(remaining_accounts, policy_count)?;

        self.validate_policies(&clock, &wrapped_ix, &exec)?;

        self.pre_cpi_state_updates(&clock, &wrapped_ix, &mut exec)?;

        let snapshots = self.pre_cpi_snapshots(&wrapped_ix, &exec)?;

        let (metas, infos) = self.build_cpi_accounts(&wrapped_ix, &exec)?;

        self.invoke_wrapped_ix(&wrapped_ix, metas, infos)?;

        self.post_cpi_enforcement(&clock, &mut exec, &snapshots)?;

        Ok(())
    }

    fn validate_session(&self) -> Result<Clock> {
        let session = &self.session;

        require!(!session.revoked, BastionError::SessionRevoked);

        let clock = Clock::get()?;

        require!(
            clock.unix_timestamp <= session.expiry,
            BastionError::SessionExpired
        );

        Ok(clock)
    }

    fn collect_execution_context(
        &self,
        remaining_accounts: &'info [AccountInfo<'info>],
        policy_count: u8,
    ) -> Result<ExecutionContext<'info>> {
        let session = &self.session;

        let n = policy_count as usize;

        require!(n <= MAX_POLICIES_PER_EXECUTE, BastionError::PolicyTooMany);

        require!(
            remaining_accounts.len() > n,
            BastionError::InvalidCompactMeta
        );

        let policy_infos_slice = remaining_accounts
            .get(..n)
            .ok_or(BastionError::InvalidPolicyData)?;

        let mut policy_infos = Vec::with_capacity(n);
        let mut policies = Vec::with_capacity(n);
        let mut keys = Vec::with_capacity(n);

        for ai in policy_infos_slice {
            require!(ai.owner == &crate::ID, BastionError::ForeignPolicy);

            let raw = ai.try_borrow_data()?;

            let p: Policy = AccountDeserialize::try_deserialize(&mut &raw[..])
                .map_err(|_| error!(BastionError::ForeignPolicy))?;

            require_keys_eq!(p.session, session.key(), BastionError::ForeignPolicy);

            require!(p.enabled, BastionError::PolicyDisabled);

            keys.push(ai.key());
            policies.push(p);
            policy_infos.push(ai);
        }

        require!(
            n == usize::from(session.policy_count),
            BastionError::PolicyCountMismatch
        );

        require!(
            compute_policies_hash(&keys) == session.policies_hash,
            BastionError::PolicyHashMismatch
        );

        let delegate_ai = remaining_accounts
            .get(n)
            .ok_or(BastionError::InvalidPolicyData)?;

        let expected_delegate = Pubkey::create_program_address(
            &[
                SEED_DELEGATE,
                session.owner.as_ref(),
                session.session_key.as_ref(),
                &[session.delegate_bump],
            ],
            &crate::ID,
        )
        .map_err(|_| error!(BastionError::InvalidPda))?;

        require_keys_eq!(
            *delegate_ai.key,
            expected_delegate,
            BastionError::InvalidPda
        );

        let start = n.checked_add(1).ok_or(BastionError::NumericalOverflow)?;

        let ix_accounts = remaining_accounts
            .get(start..)
            .ok_or(BastionError::InvalidPolicyData)?;

        Ok(ExecutionContext {
            policy_infos,
            policies,
            ix_accounts,
            delegate_ai,
            expected_delegate,
        })
    }

    fn validate_policies(
        &self,
        clock: &Clock,
        wrapped_ix: &WrappedInstruction,
        exec: &ExecutionContext<'info>,
    ) -> Result<()> {
        let sysvar_ai = self.instructions_sysvar.to_account_info();

        for p in &exec.policies {
            p.data
                .validate(clock, wrapped_ix, exec.ix_accounts, &sysvar_ai)?;
        }

        Ok(())
    }

    fn pre_cpi_state_updates(
        &self,
        clock: &Clock,
        wrapped_ix: &WrappedInstruction,
        exec: &mut ExecutionContext<'info>,
    ) -> Result<()> {
        for (i, p) in exec.policies.iter_mut().enumerate() {
            let mut dirty = false;

            match &mut p.data {
                PolicyData::RateLimit {
                    window,
                    max,
                    state,
                    scope,
                } => {
                    crate::policies::rate_limit::charge_rate_limit(
                        state,
                        window,
                        *max,
                        scope,
                        &wrapped_ix.program_id,
                        clock.unix_timestamp,
                    )?;

                    dirty = true;
                }

                PolicyData::CooldownPeriod {
                    secs,
                    last_call_ts,
                    scope,
                } => {
                    crate::policies::cooldown::charge_cooldown(
                        last_call_ts,
                        *secs,
                        scope,
                        &wrapped_ix.program_id,
                        clock.unix_timestamp,
                    )?;

                    dirty = true;
                }

                PolicyData::MaxCallsTotal { max, used } => {
                    crate::policies::max_calls_total::charge_lifetime(used, *max)?;

                    dirty = true;
                }

                _ => {}
            }

            if dirty {
                let policy_info = exec
                    .policy_infos
                    .get(i)
                    .ok_or(BastionError::InvalidPolicyData)?;

                self.write_policy_back(policy_info, p)?;
            }
        }

        Ok(())
    }

    /// snapshot pre-CPI balances for every spend-related policy that's in scope
    /// for this wrapped ix. The returned vec is consumed by `post_cpi_enforcement`
    /// after `invoke_signed` returns to compute the actual delta.
    fn pre_cpi_snapshots(
        &self,
        wrapped_ix: &WrappedInstruction,
        exec: &ExecutionContext<'info>,
    ) -> Result<Vec<PolicySnapshot>> {
        let mut snaps = Vec::new();

        for (i, p) in exec.policies.iter().enumerate() {
            match &p.data {
                PolicyData::SpendCap { asset, .. } => {
                    let pre = snapshot_for_asset(asset, exec.ix_accounts, exec.delegate_ai)?;
                    snaps.push(PolicySnapshot {
                        idx: i,
                        pre,
                        asset: asset.clone(),
                        action: PostAction::SpendCap,
                    });
                }

                PolicyData::AmountPerCall { asset, .. } => {
                    let pre = snapshot_for_asset(asset, exec.ix_accounts, exec.delegate_ai)?;
                    snaps.push(PolicySnapshot {
                        idx: i,
                        pre,
                        asset: asset.clone(),
                        action: PostAction::AmountPerCall,
                    });
                }

                PolicyData::PerCounterpartyCap {
                    receiver, asset, ..
                } => {
                    let pre = snapshot_for_asset_at_receiver(asset, exec.ix_accounts, receiver)?;
                    snaps.push(PolicySnapshot {
                        idx: i,
                        pre,
                        asset: asset.clone(),
                        action: PostAction::PerCounterpartyCap {
                            receiver: *receiver,
                        },
                    });
                }

                PolicyData::PerProgramSpendCap { program, asset, .. } => {
                    // Scope filter: out-of-scope txs are a complete no-op
                    // (no snapshot, no charge, no state mutation).
                    if program != &wrapped_ix.program_id {
                        continue;
                    }
                    let pre = snapshot_for_asset(asset, exec.ix_accounts, exec.delegate_ai)?;
                    snaps.push(PolicySnapshot {
                        idx: i,
                        pre,
                        asset: asset.clone(),
                        action: PostAction::PerProgramSpendCap,
                    });
                }

                _ => {}
            }
        }

        Ok(snaps)
    }

    fn build_cpi_accounts(
        &self,
        wrapped_ix: &WrappedInstruction,
        exec: &ExecutionContext<'info>,
    ) -> Result<(Vec<AccountMeta>, Vec<AccountInfo<'info>>)> {
        let mut metas = Vec::with_capacity(wrapped_ix.accounts.len());

        let mut infos = Vec::with_capacity(wrapped_ix.accounts.len());

        for m in &wrapped_ix.accounts {
            require!(m.flags_well_formed(), BastionError::InvalidCompactMeta);

            let idx = m.index as usize;

            require!(
                idx < exec.ix_accounts.len(),
                BastionError::InvalidCompactMeta
            );

            let acct = exec
                .ix_accounts
                .get(idx)
                .ok_or(BastionError::InvalidPolicyData)?;

            if m.is_signer() {
                require_keys_eq!(
                    *acct.key,
                    exec.expected_delegate,
                    BastionError::ForeignSignerNotAllowed
                );
            }

            metas.push(AccountMeta {
                pubkey: *acct.key,
                is_signer: m.is_signer(),
                is_writable: m.is_writable(),
            });

            infos.push(acct.clone());
        }

        Ok((metas, infos))
    }

    fn invoke_wrapped_ix(
        &self,
        wrapped_ix: &WrappedInstruction,
        metas: Vec<AccountMeta>,
        infos: Vec<AccountInfo<'info>>,
    ) -> Result<()> {
        let ix = Instruction {
            program_id: wrapped_ix.program_id,
            accounts: metas,
            data: wrapped_ix.data.clone(),
        };

        let delegate_seeds: &[&[u8]] = &[
            SEED_DELEGATE,
            self.session.owner.as_ref(),
            self.session.session_key.as_ref(),
            &[self.session.delegate_bump],
        ];

        invoke_signed(&ix, &infos, &[delegate_seeds])?;

        Ok(())
    }

    /// Post-CPI enforcement: for each pre-snapshot, recompute the post-CPI
    /// balance and dispatch to the matching policy charge/check. Then run any
    /// stateless post-CPI floors (MinDelegateBalance).
    fn post_cpi_enforcement(
        &self,
        clock: &Clock,
        exec: &mut ExecutionContext<'info>,
        snapshots: &[PolicySnapshot],
    ) -> Result<()> {
        for snap in snapshots {
            let post = match &snap.action {
                PostAction::PerCounterpartyCap { receiver } => {
                    snapshot_for_asset_at_receiver(&snap.asset, exec.ix_accounts, receiver)?
                }
                _ => snapshot_for_asset(&snap.asset, exec.ix_accounts, exec.delegate_ai)?,
            };

            let policy_info = exec
                .policy_infos
                .get(snap.idx)
                .copied()
                .ok_or(BastionError::InvalidPolicyData)?;

            let p = exec
                .policies
                .get_mut(snap.idx)
                .ok_or(BastionError::InvalidPolicyData)?;

            let mut dirty = false;

            match (&mut p.data, &snap.action) {
                (
                    PolicyData::SpendCap {
                        state,
                        window,
                        max,
                        asset,
                    },
                    PostAction::SpendCap,
                ) => {
                    // Clone asset to a local so charge_spend_cap can hold &Asset
                    // independent of the &mut state borrow on the same enum.
                    let asset_local = asset.clone();
                    charge_spend_cap(SpendCapCharge {
                        state,
                        window,
                        max: *max,
                        pre: snap.pre,
                        post,
                        asset: &asset_local,
                        delegate: exec.delegate_ai,
                        now: clock.unix_timestamp,
                    })?;
                    dirty = true;
                }

                (PolicyData::AmountPerCall { max, .. }, PostAction::AmountPerCall) => {
                    check_amount_per_call(*max, snap.pre, post)?;
                }

                (
                    PolicyData::PerCounterpartyCap { sent, max, .. },
                    PostAction::PerCounterpartyCap { .. },
                ) => {
                    charge_counterparty_cap(sent, *max, snap.pre, post)?;
                    dirty = true;
                }

                (
                    PolicyData::PerProgramSpendCap {
                        state, window, max, ..
                    },
                    PostAction::PerProgramSpendCap,
                ) => {
                    charge_per_program_spend_cap(
                        state,
                        window,
                        *max,
                        snap.pre,
                        post,
                        clock.unix_timestamp,
                    )?;
                    dirty = true;
                }

                _ => {}
            }

            if dirty {
                self.write_policy_back(policy_info, p)?;
            }
        }

        // Stateless post-CPI floor: MinDelegateBalance. Independent of any
        // snapshot — just compares current `delegate.lamports()` against floor.
        for p in &exec.policies {
            if let PolicyData::MinDelegateBalance { floor } = &p.data {
                check_min_balance(exec.delegate_ai, *floor)?;
            }
        }

        Ok(())
    }

    fn write_policy_back(&self, ai: &AccountInfo<'info>, policy: &Policy) -> Result<()> {
        let mut raw = ai.try_borrow_mut_data()?;

        let mut writer: &mut [u8] = &mut raw;

        AccountSerialize::try_serialize(policy, &mut writer)?;

        Ok(())
    }
}
