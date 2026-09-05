pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("Bs4Pdv1uv39jEiePrSfihRW4mV5atCM6qoU2wV4mQfAg");

#[program]
pub mod nc_vault {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.initialize(&ctx.bumps)
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        ctx.accounts.deposit(amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        ctx.accounts.withdraw(shares)
    }

    pub fn invest(ctx: Context<Invest>, amount: u64) -> Result<()> {
        ctx.accounts.invest(amount)
    }

    pub fn divest(ctx: Context<Invest>, amount: u64) -> Result<()> {
        ctx.accounts.divest(amount)
    }
}
