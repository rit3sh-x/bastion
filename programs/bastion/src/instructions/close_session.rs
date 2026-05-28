use anchor_lang::prelude::*;

use crate::constants::SEED_SESSION;
use crate::error::BastionError;
use crate::state::policy::Policy;
use crate::state::session::Session;
use crate::utils::hash::compute_policies_hash;

#[derive(Accounts)]
pub struct CloseSession<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [SEED_SESSION, owner.key().as_ref(), session.session_key.as_ref()],
        bump = session.bump,
        has_one = owner,
        close = owner,
    )]
    pub session: Account<'info, Session>,
}

impl<'info> CloseSession<'info> {
    pub fn close_session_handler(
        &mut self,
        remaining_accounts: &[AccountInfo<'info>],
    ) -> Result<()> {
        let n = usize::from(self.session.policy_count);
        require!(
            remaining_accounts.len() == n,
            BastionError::PolicyCountMismatch
        );

        let session_key = self.session.key();
        let stored_hash = self.session.policies_hash;

        let mut keys: Vec<Pubkey> = Vec::with_capacity(n);
        for policy_ai in remaining_accounts {
            require!(policy_ai.owner == &crate::ID, BastionError::ForeignPolicy);
            let data = policy_ai.try_borrow_data()?;
            let policy: Policy = AccountDeserialize::try_deserialize(&mut &data[..])
                .map_err(|_| error!(BastionError::ForeignPolicy))?;
            require_keys_eq!(policy.session, session_key, BastionError::ForeignPolicy);
            keys.push(policy_ai.key());
        }

        let computed = compute_policies_hash(&keys);
        require!(computed == stored_hash, BastionError::PolicyHashMismatch);

        let owner_ai = self.owner.to_account_info();
        for policy_ai in remaining_accounts {
            Self::close_account_to(policy_ai, &owner_ai)?;
        }

        Ok(())
    }

    fn close_account_to(target: &AccountInfo, destination: &AccountInfo) -> Result<()> {
        let lamports = target.lamports();

        **destination.try_borrow_mut_lamports()? = destination
            .lamports()
            .checked_add(lamports)
            .ok_or(BastionError::NumericalOverflow)?;

        **target.try_borrow_mut_lamports()? = target
            .lamports()
            .checked_sub(lamports)
            .ok_or(BastionError::NumericalOverflow)?;

        target.assign(&system_program::ID);

        target.try_borrow_mut_data()?.fill(0);

        Ok(())
    }
}
