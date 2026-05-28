#![allow(clippy::diverging_sub_expression)]

use anchor_lang::prelude::*;

pub mod constants;
pub mod error;
pub mod instructions;
pub mod policies;
pub mod state;
pub mod utils;

pub use constants::*;
pub use error::*;
pub use instructions::*;
pub use state::*;
pub use utils::*;

declare_id!("GkCMDTvNwvAusUk5u28mXQ8c8A4zs1y4hbbEcVZciSm1");

#[program]
pub mod bastion {
    use super::*;

    pub fn init_session(ctx: Context<InitSession>, args: InitSessionArgs) -> Result<()> {
        ctx.accounts.init_session_handler(args, &ctx.bumps)
    }

    pub fn revoke_session(ctx: Context<RevokeSession>) -> Result<()> {
        ctx.accounts.revoke_session_handler()
    }

    pub fn extend_session(ctx: Context<ExtendSession>, args: ExtendSessionArgs) -> Result<()> {
        ctx.accounts.extend_session_handler(args)
    }

    pub fn close_session<'info>(ctx: Context<'info, CloseSession<'info>>) -> Result<()> {
        ctx.accounts.close_session_handler(ctx.remaining_accounts)
    }

    pub fn sweep_delegate<'info>(ctx: Context<'info, SweepDelegate<'info>>) -> Result<()> {
        ctx.accounts.sweep_delegate_handler(&ctx.bumps)
    }

    pub fn attach_policy<'info>(
        ctx: Context<'info, AttachPolicy<'info>>,
        data: PolicyData,
    ) -> Result<()> {
        ctx.accounts
            .attach_policy_handler(ctx.remaining_accounts, data, &ctx.bumps)
    }

    pub fn detach_policy<'info>(ctx: Context<'info, DetachPolicy<'info>>, seed: u64) -> Result<()> {
        ctx.accounts
            .detach_policy_handler(seed, ctx.remaining_accounts)
    }

    pub fn execute<'info>(
        ctx: Context<'info, Execute<'info>>,
        wrapped_ix: WrappedInstruction,
        policy_count: u8,
    ) -> Result<()> {
        ctx.accounts
            .execute_handler(ctx.remaining_accounts, wrapped_ix, policy_count)
    }

    pub fn update_policy(
        ctx: Context<UpdatePolicy>,
        seed: u64,
        new_data: PolicyData,
    ) -> Result<()> {
        ctx.accounts.update_policy_handler(seed, new_data)
    }
}
