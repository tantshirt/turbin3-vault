use crate::state::Vault;
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    // has to be declared before shares_mint so mint::decimals can point at it
    #[account(mint::token_program = token_program)]
    pub underlying_mint: Box<InterfaceAccount<'info, Mint>>,

    // stands in for an LP token. 1 of these = 1 underlying that's been put to work
    #[account(mint::token_program = token_program)]
    pub position_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = manager,
        seeds = [b"vault", manager.key().as_ref(), underlying_mint.key().as_ref()],
        bump,
        space = 8 + Vault::INIT_SPACE,
    )]
    pub vault: Box<Account<'info, Vault>>,

    // the vault PDA is the mint authority, so nobody can print shares out of band
    #[account(
        init,
        payer = manager,
        seeds = [b"shares", vault.key().as_ref()],
        bump,
        mint::decimals = underlying_mint.decimals,
        mint::authority = vault,
        mint::token_program = token_program,
    )]
    pub shares_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = manager,
        associated_token::mint = underlying_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_underlying: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = manager,
        associated_token::mint = position_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_position: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn initialize(&mut self, bumps: &InitializeBumps) -> Result<()> {
        self.vault.set_inner(Vault {
            manager: self.manager.key(),
            underlying_mint: self.underlying_mint.key(),
            position_mint: self.position_mint.key(),
            shares_mint: self.shares_mint.key(),
            bump: bumps.vault,
        });
        Ok(())
    }
}
