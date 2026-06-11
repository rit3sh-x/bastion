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

#[cfg_attr(
    not(target_os = "solana"),
    derive(anchor_litesvm::BundledPubkeys),
    bundled_with(crate::utils::helpers::BastionBundle)
)]
#[derive(Accounts)]
pub struct Execute<'info> {
    pub session_key: Signer<'info>,

    #[account(
        mut,
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

/// One deduplicated balance snapshot, shared by every policy that measures the
/// same (asset, scope). computed once pre-CPI and once post-CPI
/// no matter how many policies reference it.
struct SnapSlot {
    asset: Asset,
    scope: SnapScope,
    pre: u64,
}

/// Which accounts a snapshot sums. `SelfControlled` = delegate vault + owner
/// allowance source (`[delegate, owner]`); `Receiver` = a single counterparty.
#[derive(Clone, PartialEq, Eq)]
enum SnapScope {
    SelfControlled,
    Receiver(Pubkey),
}

/// A spend policy's post-CPI charge, bound by `slot_idx` to the snapshot whose
/// pre/post delta drives it. Many charges may share one slot.
struct Charge {
    policy_idx: usize,
    slot_idx: usize,
    action: PostAction,
}

enum PostAction {
    SpendCap,
    AmountPerCall,
    PerCounterpartyCap,
    PerProgramSpendCap,
}

impl<'info> Execute<'info> {
    pub fn execute_handler(
        &mut self,
        remaining_accounts: &'info [AccountInfo<'info>],
        wrapped_ixs: Vec<WrappedInstruction>,
        policy_count: u8,
        expected_nonce: Option<u64>,
        manifest: Option<Vec<PolicyData>>,
    ) -> Result<()> {
        let clock = self.validate_session()?;

        // a batch must carry at least one leg.
        require!(!wrapped_ixs.is_empty(), BastionError::EmptyBatch);

        // optional ordering assertion for multi-tx sequences. `None` keeps
        // the simple single-call path assertion-free; `Some(n)` rejects stale or
        // out-of-order submissions (checked against the pre-increment nonce).
        if let Some(n) = expected_nonce {
            require!(n == self.session.action_nonce, BastionError::NonceMismatch);
        }

        // Policies + delegate + ix-account pool are collected once and shared by
        // every leg (each leg's CompactAccountMeta indexes the same pool).
        let mut exec = self.collect_execution_context(remaining_accounts, policy_count)?;

        // A pinned manifest is BINDING: once the owner commits a manifest hash,
        // the holder cannot opt out of those policies by simply omitting the
        // argument. Require it whenever one is pinned. (The reverse — supplying
        // a manifest with none pinned — is rejected inside verify_manifest as
        // ManifestNotPinned.)
        if self.session.manifest_hash != [0u8; 32] {
            require!(manifest.is_some(), BastionError::ManifestRequired);
        }

        // A holder-signed stateless manifest extends the policy set
        // off-chain. Verify once: pinned hash + ed25519 binding (owner over the
        // commitment) + every entry stateless.
        let sysvar_ai = self.instructions_sysvar.to_account_info();
        if let Some(m) = &manifest {
            crate::utils::manifest::verify_manifest(
                m,
                &self.session.manifest_hash,
                &self.session.owner,
                &sysvar_ai,
            )?;
        }

        // Each leg runs the full validate→charge→snapshot→CPI→enforce
        // pipeline. Every leg executes inside this single instruction, so any leg
        // failing reverts the whole batch — atomic, no partial intra-tx commit.
        // Frequency/spend state mutates cumulatively (charged per leg = N
        // back-to-back executes at the same timestamp).
        for wrapped_ix in &wrapped_ixs {
            // Defense-in-depth: the wrapped CPI must never re-invoke Bastion itself
            // (its privileged ix require owner/session_key signers the delegate
            // lacks, but reject explicitly so no future ix is reachable via self-CPI).
            require_keys_neq!(
                wrapped_ix.program_id,
                crate::ID,
                BastionError::SelfCpiNotAllowed
            );

            self.validate_policies(&clock, wrapped_ix, &exec, &sysvar_ai)?;

            // Manifest (stateless) policies validate against each leg too.
            if let Some(m) = &manifest {
                for p in m {
                    p.validate(&clock, wrapped_ix, exec.ix_accounts, &sysvar_ai)?;
                }
            }

            self.pre_cpi_state_updates(&clock, wrapped_ix, &mut exec)?;

            let (slots, charges) = self.pre_cpi_snapshots(wrapped_ix, &exec)?;

            let (metas, infos) = self.build_cpi_accounts(wrapped_ix, &exec)?;

            self.invoke_wrapped_ix(wrapped_ix, metas, infos)?;

            self.post_cpi_enforcement(&clock, &mut exec, &slots, &charges)?;
        }

        // One monotonic increment per execute (per tx, not per leg).
        self.session.action_nonce = self
            .session
            .action_nonce
            .checked_add(1)
            .ok_or(BastionError::NumericalOverflow)?;

        // Extend the tamper-evident audit chain over this batch. Commit to
        // each leg's program_id + data (the meaningful payload; accounts are
        // indices into the shared pool).
        let mut batch_bytes: Vec<u8> = Vec::new();
        for w in &wrapped_ixs {
            batch_bytes.extend_from_slice(w.program_id.as_ref());
            batch_bytes.extend_from_slice(&w.data);
        }
        self.session.chain_hash = crate::utils::hash::compute_chain_hash(
            &self.session.chain_hash,
            &batch_bytes,
            self.session.action_nonce,
        );

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
        sysvar_ai: &AccountInfo<'info>,
    ) -> Result<()> {
        for p in &exec.policies {
            p.data
                .validate(clock, wrapped_ix, exec.ix_accounts, sysvar_ai)?;
        }

        Ok(())
    }

    /// Compute the balance a `(scope, asset)` snapshot measures. Used identically
    /// pre- and post-CPI so the delta is apples-to-apples.
    fn snap(
        &self,
        scope: &SnapScope,
        asset: &Asset,
        exec: &ExecutionContext<'info>,
    ) -> Result<u64> {
        match scope {
            SnapScope::SelfControlled => snapshot_for_asset(
                asset,
                exec.ix_accounts,
                exec.delegate_ai,
                &self.session.owner,
            ),
            SnapScope::Receiver(r) => snapshot_for_asset_at_receiver(asset, exec.ix_accounts, r),
        }
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
    ) -> Result<(Vec<SnapSlot>, Vec<Charge>)> {
        let mut slots: Vec<SnapSlot> = Vec::with_capacity(exec.policies.len());
        let mut charges: Vec<Charge> = Vec::with_capacity(exec.policies.len());

        for (i, p) in exec.policies.iter().enumerate() {
            let (asset, scope, action) = match &p.data {
                PolicyData::SpendCap { asset, .. } => {
                    (asset, SnapScope::SelfControlled, PostAction::SpendCap)
                }
                PolicyData::AmountPerCall { asset, .. } => {
                    (asset, SnapScope::SelfControlled, PostAction::AmountPerCall)
                }
                PolicyData::PerCounterpartyCap {
                    receiver, asset, ..
                } => (
                    asset,
                    SnapScope::Receiver(*receiver),
                    PostAction::PerCounterpartyCap,
                ),
                PolicyData::PerProgramSpendCap { program, asset, .. } => {
                    // Scope filter: out-of-scope txs are a complete no-op
                    // (no snapshot, no charge, no state mutation).
                    if program != &wrapped_ix.program_id {
                        continue;
                    }
                    (
                        asset,
                        SnapScope::SelfControlled,
                        PostAction::PerProgramSpendCap,
                    )
                }
                _ => continue,
            };

            // Dedup: K policies on the same (asset, scope) share ONE
            // snapshot — scan + decode the accounts once, not once per policy.
            let slot_idx = match slots
                .iter()
                .position(|s| s.scope == scope && &s.asset == asset)
            {
                Some(idx) => idx,
                None => {
                    let pre = self.snap(&scope, asset, exec)?;
                    slots.push(SnapSlot {
                        asset: asset.clone(),
                        scope: scope.clone(),
                        pre,
                    });
                    slots.len().saturating_sub(1)
                }
            };

            charges.push(Charge {
                policy_idx: i,
                slot_idx,
                action,
            });
        }

        Ok((slots, charges))
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
        slots: &[SnapSlot],
        charges: &[Charge],
    ) -> Result<()> {
        // Recompute each distinct snapshot's post-CPI balance ONCE, then
        // fan the pre/post pair out to every charge that shares the slot.
        // Full re-scan per slot — accounts may have been initialized mid-CPI.
        let mut post_vals: Vec<u64> = Vec::with_capacity(slots.len());
        for s in slots {
            post_vals.push(self.snap(&s.scope, &s.asset, exec)?);
        }

        for charge in charges {
            let slot = slots
                .get(charge.slot_idx)
                .ok_or(BastionError::InvalidPolicyData)?;
            let pre = slot.pre;
            let post = *post_vals
                .get(charge.slot_idx)
                .ok_or(BastionError::InvalidPolicyData)?;

            let policy_info = exec
                .policy_infos
                .get(charge.policy_idx)
                .copied()
                .ok_or(BastionError::InvalidPolicyData)?;

            let p = exec
                .policies
                .get_mut(charge.policy_idx)
                .ok_or(BastionError::InvalidPolicyData)?;

            let mut dirty = false;

            match (&mut p.data, &charge.action) {
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
                        pre,
                        post,
                        asset: &asset_local,
                        delegate: exec.delegate_ai,
                        now: clock.unix_timestamp,
                    })?;
                    dirty = true;
                }

                (PolicyData::AmountPerCall { max, .. }, PostAction::AmountPerCall) => {
                    check_amount_per_call(*max, pre, post)?;
                }

                (
                    PolicyData::PerCounterpartyCap { sent, max, .. },
                    PostAction::PerCounterpartyCap,
                ) => {
                    charge_counterparty_cap(sent, *max, pre, post)?;
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
                        pre,
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
