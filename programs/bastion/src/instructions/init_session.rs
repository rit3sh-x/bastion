use anchor_lang::prelude::*;

use crate::constants::{SEED_DELEGATE, SEED_SESSION};
use crate::error::BastionError;
use crate::state::session::Session;
use crate::utils::hash::EMPTY_POLICIES_HASH;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitSessionArgs {
    pub session_key: Pubkey,
    pub expiry: i64,
}

#[derive(Accounts)]
#[instruction(args: InitSessionArgs)]
pub struct InitSession<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = Session::SPACE,
        seeds = [SEED_SESSION, owner.key().as_ref(), args.session_key.as_ref()],
        bump
    )]
    pub session: Account<'info, Session>,

    pub system_program: Program<'info, System>,
}

impl<'info> InitSession<'info> {
    pub fn init_session_handler(
        &mut self,
        args: InitSessionArgs,
        bumps: &InitSessionBumps,
    ) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;

        require!(args.expiry > now, BastionError::SessionExpired);

        let owner_key = self.owner.key();

        let (_delegate_pda, delegate_bump) = Pubkey::find_program_address(
            &[SEED_DELEGATE, owner_key.as_ref(), args.session_key.as_ref()],
            &crate::ID,
        );

        self.session.set_inner(Session {
            owner: owner_key,
            session_key: args.session_key,
            bump: bumps.session,
            created_at: now,
            expiry: args.expiry,
            revoked: false,
            policy_count: 0,
            next_seed: 0,
            policies_hash: EMPTY_POLICIES_HASH,
            delegate_bump,
        });

        Ok(())
    }
}
