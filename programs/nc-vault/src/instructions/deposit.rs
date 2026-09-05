use crate::error::VaultError;
use crate::instructions::shared::transfer_tokens;
use crate::state::Vault;
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to, Mint, MintTo, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        has_one = underlying_mint,
        has_one = shares_mint,
        seeds = [b"vault", vault.manager.as_ref(), underlying_mint.key().as_ref()],
        bump = vault.bump,
    )]
    pub vault: Box<Account<'info, Vault>>,

    #[account(mint::token_program = token_program)]
    pub underlying_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, mint::token_program = token_program)]
    pub shares_mint: Box<InterfaceAccount<'info, Mint>>,

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
        associated_token::mint = shares_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_shares: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = underlying_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_underlying: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        associated_token::mint = vault.position_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_position: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> Deposit<'info> {
    pub fn deposit(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);

        // these balances are read before the transfer below lands, so this is the pre deposit total
        let total_assets = self.vault_underlying.amount + self.vault_position.amount;
        let supply = self.shares_mint.supply;

        let shares: u64 = if supply == 0 {
            amount // first one in, 1 share = 1 token
        } else {
            u64::try_from(amount as u128 * supply as u128 / total_assets as u128)
                .map_err(|_| VaultError::MathOverflow)?
        };
        // division truncates, so a tiny deposit can round to nothing. fail instead of eating it
        require!(shares > 0, VaultError::ZeroShares);

        transfer_tokens(
            &self.user_underlying,
            &self.vault_underlying,
            amount,
            &self.underlying_mint,
            &self.user.to_account_info(),
            &self.token_program.to_account_info(),
            None,
        )?;

        let manager = self.vault.manager;
        let underlying = self.vault.underlying_mint;
        let seeds = &[
            b"vault".as_ref(),
            manager.as_ref(),
            underlying.as_ref(),
            &[self.vault.bump],
        ];

        mint_to(
            CpiContext::new_with_signer(
                self.token_program.key(),
                MintTo {
                    mint: self.shares_mint.to_account_info(),
                    to: self.user_shares.to_account_info(),
                    authority: self.vault.to_account_info(),
                },
                &[&seeds[..]],
            ),
            shares,
        )
    }
}
