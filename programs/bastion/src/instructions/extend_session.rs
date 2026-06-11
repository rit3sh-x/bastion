use anchor_lang::prelude::*;

use crate::constants::SEED_SESSION;
use crate::error::BastionError;
use crate::state::session::Session;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExtendSessionArgs {
    pub new_expiry: i64,
}

#[cfg_attr(
    not(target_os = "solana"),
    derive(anchor_litesvm::BundledPubkeys),
    bundled_with(crate::utils::helpers::BastionBundle)
)]
#[derive(Accounts)]
pub struct ExtendSession<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [
            SEED_SESSION,
            owner.key().as_ref(),
            session.session_key.as_ref()
        ],
        bump = session.bump,
        has_one = owner,
    )]
    pub session: Account<'info, Session>,
}

impl<'info> ExtendSession<'info> {
    pub fn extend_session_handler(&mut self, args: ExtendSessionArgs) -> Result<()> {
        self.validate_extend(&args)?;

        self.session.expiry = args.new_expiry;

        Ok(())
    }

    fn validate_extend(&self, args: &ExtendSessionArgs) -> Result<()> {
        require!(!self.session.revoked, BastionError::SessionRevoked);

        let now = Clock::get()?.unix_timestamp;

        require!(now <= self.session.expiry, BastionError::SessionExpired);

        require!(
            args.new_expiry > self.session.expiry,
            BastionError::NewExpiryNotGreater
        );

        Ok(())
    }
}
