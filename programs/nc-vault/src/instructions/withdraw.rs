use crate::error::VaultError;
use crate::instructions::shared::transfer_tokens;
use crate::state::Vault;
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{burn, Burn, Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        has_one = underlying_mint,
        has_one = position_mint,
        has_one = shares_mint,
        seeds = [b"vault", vault.manager.as_ref(), underlying_mint.key().as_ref()],
        bump = vault.bump,
    )]
    pub vault: Box<Account<'info, Vault>>,

    #[account(mint::token_program = token_program)]
    pub underlying_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mint::token_program = token_program)]
    pub position_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, mint::token_program = token_program)]
    pub shares_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = shares_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_shares: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = underlying_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_underlying: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = position_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_position: Box<InterfaceAccount<'info, TokenAccount>>,

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

impl<'info> Withdraw<'info> {
    pub fn withdraw(&mut self, shares: u64) -> Result<()> {
        require!(shares > 0, VaultError::ZeroAmount);
        require!(self.user_shares.amount >= shares, VaultError::NotEnough);

        let supply = self.shares_mint.supply;

        // your cut of each pile, worked out before the burn changes the supply
        let cut = |balance: u64| -> u64 {
            (balance as u128 * shares as u128 / supply as u128) as u64
        };
        let underlying_out = cut(self.vault_underlying.amount);
        let position_out = cut(self.vault_position.amount);

        burn(
            CpiContext::new(
                self.token_program.key(),
                Burn {
                    mint: self.shares_mint.to_account_info(),
                    from: self.user_shares.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            shares,
        )?;

        let manager = self.vault.manager;
        let underlying = self.vault.underlying_mint;
        let seeds = &[
            b"vault".as_ref(),
            manager.as_ref(),
            underlying.as_ref(),
            &[self.vault.bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // paying out both piles is what makes this work with no liquidity.
        // transfer_checked of 0 is legal, so an empty side needs no special case
        transfer_tokens(
            &self.vault_underlying,
            &self.user_underlying,
            underlying_out,
            &self.underlying_mint,
            &self.vault.to_account_info(),
            &self.token_program.to_account_info(),
            Some(signer_seeds),
        )?;

        transfer_tokens(
            &self.vault_position,
            &self.user_position,
            position_out,
            &self.position_mint,
            &self.vault.to_account_info(),
            &self.token_program.to_account_info(),
            Some(signer_seeds),
        )
    }
}
