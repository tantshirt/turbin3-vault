use crate::error::VaultError;
use crate::instructions::shared::transfer_tokens;
use crate::state::Vault;
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Invest<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        has_one = manager,
        has_one = underlying_mint,
        has_one = position_mint,
        seeds = [b"vault", manager.key().as_ref(), underlying_mint.key().as_ref()],
        bump = vault.bump,
    )]
    pub vault: Box<Account<'info, Vault>>,

    #[account(mint::token_program = token_program)]
    pub underlying_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mint::token_program = token_program)]
    pub position_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = underlying_mint,
        associated_token::authority = manager,
        associated_token::token_program = token_program,
    )]
    pub manager_underlying: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = position_mint,
        associated_token::authority = manager,
        associated_token::token_program = token_program,
    )]
    pub manager_position: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = underlying_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_underlying: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = position_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_position: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> Invest<'info> {
    // one amount for both legs, so the swap is always 1:1 and the vault's total never moves.
    // there's no way to call this that drains value out of the vault
    pub fn invest(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);
        require!(self.vault_underlying.amount >= amount, VaultError::NotEnough);

        // cash out of the vault, signed by the vault PDA
        self.vault_pays(&self.vault_underlying, &self.manager_underlying, &self.underlying_mint, amount)?;
        // position tokens back in, signed by the manager
        self.manager_pays(&self.manager_position, &self.vault_position, &self.position_mint, amount)
    }

    pub fn divest(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);
        require!(self.vault_position.amount >= amount, VaultError::NotEnough);

        self.vault_pays(&self.vault_position, &self.manager_position, &self.position_mint, amount)?;
        self.manager_pays(&self.manager_underlying, &self.vault_underlying, &self.underlying_mint, amount)
    }

    fn vault_pays(
        &self,
        from: &InterfaceAccount<'info, TokenAccount>,
        to: &InterfaceAccount<'info, TokenAccount>,
        mint: &InterfaceAccount<'info, Mint>,
        amount: u64,
    ) -> Result<()> {
        let manager = self.vault.manager;
        let underlying = self.vault.underlying_mint;
        let seeds = &[
            b"vault".as_ref(),
            manager.as_ref(),
            underlying.as_ref(),
            &[self.vault.bump],
        ];

        transfer_tokens(
            from,
            to,
            amount,
            mint,
            &self.vault.to_account_info(),
            &self.token_program.to_account_info(),
            Some(&[&seeds[..]]),
        )
    }

    fn manager_pays(
        &self,
        from: &InterfaceAccount<'info, TokenAccount>,
        to: &InterfaceAccount<'info, TokenAccount>,
        mint: &InterfaceAccount<'info, Mint>,
        amount: u64,
    ) -> Result<()> {
        transfer_tokens(
            from,
            to,
            amount,
            mint,
            &self.manager.to_account_info(),
            &self.token_program.to_account_info(),
            None,
        )
    }
}
