use crate::error::VaultError;
use crate::state::VaultState;
use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    // the seeds are the ownership check: this PDA only derives from the signer's key,
    // so nobody can pass someone else's vault_state here
    #[account(
        seeds = [b"state", user.key().as_ref()],
        bump = vault_state.state_bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.key().as_ref()],
        bump = vault_state.vault_bump,
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Withdraw<'info> {
    pub fn withdraw(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);

        // a system account can't drop below rent exempt or the transfer just dies
        let min = Rent::get()?.minimum_balance(0);
        require!(
            self.vault.lamports().saturating_sub(amount) >= min,
            VaultError::InsufficientFunds
        );

        let state_key = self.vault_state.key();
        let seeds = &[
            b"vault".as_ref(),
            state_key.as_ref(),
            &[self.vault_state.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: self.vault.to_account_info(),
            to: self.user.to_account_info(),
        };
        transfer(
            CpiContext::new_with_signer(System::id(), cpi_accounts, signer_seeds),
            amount,
        )
    }
}
