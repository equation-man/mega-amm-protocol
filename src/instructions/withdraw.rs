//! Withdraw the mint_x and mint_y token based on the amount of lp the user wants to burn.
use pinocchio::{
    AccountView, Address, ProgramResult,
    error::ProgramError,
    cpi::{Signer, Seed},
};
use solana_address;
use crate::helpers::errors::MegaAmmProgramError;
use crate::helpers::utils::{
    SignerAccount, MintInterface, TokenInterface, MintAccount,
    TokenAccount, ProgramAccount, AssociatedTokenAccount,
};
use constant_product_curve::ConstantProduct;
use crate::config::{Config, AmmState};
use pinocchio_log::log;

pub struct WithdrawAccounts<'info> {
    pub user: &'info AccountView,
    pub mint_lp: &'info AccountView,
    pub vault_x: &'info AccountView,
    pub vault_y: &'info AccountView,
    pub user_x_ata: &'info AccountView,
    pub user_y_ata: &'info AccountView,
    pub user_lp_ata: &'info AccountView,
    pub config: &'info AccountView,
    pub token_program: &'info AccountView,
}

impl<'info> TryFrom<&'info [AccountView]> for WithdrawAccounts<'info> {
    type Error = MegaAmmProgramError;
    fn try_from(accounts: &'info [AccountView]) -> Result<Self, Self::Error> {
        let [
            user, mint_lp, vault_x, vault_y, user_x_ata,
            user_y_ata, user_lp_ata, config, token_program,
        ] = accounts else {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        };
        // Checking the accounts.
        SignerAccount::check(user)?;
        MintInterface::check(mint_lp)?;
        TokenInterface::check(vault_x)?;
        TokenInterface::check(vault_y)?;
        AssociatedTokenAccount::check(user_lp_ata, user, mint_lp.address(), token_program)?;
        ProgramAccount::check(config)?;
        let config_state = Config::load(config)?;
        AssociatedTokenAccount::check(user_x_ata, user, config_state.mint_x(), token_program)?;
        AssociatedTokenAccount::check(user_y_ata, user, config_state.mint_y(), token_program)?;

        Ok(Self {
            user, mint_lp, vault_x, vault_y, user_x_ata,
            user_y_ata, user_lp_ata, config, token_program,
        })
    }
}

pub struct WithdrawInstructionData {
    pub amount: u64,
    pub min_x: u64,
    pub min_y: u64,
    pub expiration: i64,
}

impl<'info> TryFrom<&'info [u8]> for WithdrawInstructionData {
    type Error = MegaAmmProgramError;
    fn try_from(data: &'info [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<WithdrawInstructionData>() {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let min_x = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let min_y = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[24..32].try_into().unwrap());

        if amount <= 0 {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        if min_x <=0 || min_y <=0 {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        Ok(Self {
            amount, min_x, min_y, expiration,
        })
    }
}

pub struct Withdraw<'info> {
    pub accounts: WithdrawAccounts<'info>,
    pub instruction_data: WithdrawInstructionData
}

impl<'info> TryFrom<(&'info [u8], &'info [AccountView])> for Withdraw<'info> {
    type Error = MegaAmmProgramError;
    fn try_from((data, accounts): (&'info [u8], &'info [AccountView])) -> Result<Self, Self::Error> {
        let accounts = WithdrawAccounts::try_from(accounts)?;
        let instruction_data = WithdrawInstructionData::try_from(data)?;
        // Validated data.
        Ok(Self {
            accounts, instruction_data,
        })
    }
}

impl<'info> Withdraw<'info> {
    pub const DISCRIMINATOR: &'info u8 = &2;
    pub fn process(&mut self) -> ProgramResult {
        // Loading the config.
        let amm_config = Config::load(self.accounts.config)?;
        if amm_config.state() != AmmState::Initialized.into() {
            return Err(MegaAmmProgramError::Unauthorized.into());
        }

        let (vault_x_amount, vault_y_amount, lp_supply) = {
            let mint_data_ref = self.accounts.mint_lp.try_borrow()?;
            let mint_lp = unsafe {
                pinocchio_token::state::Mint::from_bytes_unchecked(&mint_data_ref)
            };
            let vault_x_data_ref = self.accounts.vault_x.try_borrow()?;
            let vault_x = unsafe {
                pinocchio_token::state::TokenAccount::from_bytes_unchecked(&vault_x_data_ref)
            };
            let vault_y_data_ref = self.accounts.vault_y.try_borrow()?;
            let vault_y = unsafe {
                pinocchio_token::state::TokenAccount::from_bytes_unchecked(&vault_y_data_ref)
            };
            (vault_x.amount(), vault_y.amount(), mint_lp.supply())
        };

        let (x, y) = match lp_supply == self.instruction_data.amount {
            true => (vault_x_amount, vault_y_amount),
            false => {
                let amounts = ConstantProduct::xy_withdraw_amounts_from_l(
                    vault_x_amount,
                    vault_y_amount,
                    lp_supply,
                    self.instruction_data.amount,
                    6,
                ).map_err(|_| ProgramError::InvalidArgument)?;
                (amounts.x, amounts.y)
            }
        };
        // Check for Slippage.
        if !(x >= self.instruction_data.min_x && y >= self.instruction_data.min_y) {
            return Err(ProgramError::InvalidArgument);
        }

        let seed_binding = amm_config.seed().to_le_bytes();
        let conf_bump_binding = amm_config.config_bump();
        let config_signer_seeds = [
            Seed::from(b"config"),
            Seed::from(&seed_binding),
            Seed::from(amm_config.mint_x().as_ref()),
            Seed::from(amm_config.mint_y().as_ref()),
            Seed::from(&conf_bump_binding),
        ];
        let signer_seeds = [Signer::from(&config_signer_seeds)];

        // Transfer tokens x and y from the vault of the pool to the user.
        TokenAccount::transfer_spl_tokens(
            self.accounts.vault_x,
            self.accounts.user_x_ata,
            self.accounts.config,
            x,
            Some(&signer_seeds),
        )?;
        TokenAccount::transfer_spl_tokens(
            self.accounts.vault_y,
            self.accounts.user_y_ata,
            self.accounts.config,
            y,
            Some(&signer_seeds),
        )?;

        // Prevent burning of 0 LP tokens
        if self.instruction_data.amount == 0 {
            return Err(ProgramError::InvalidArgument);
        }

        // Prevent impossible burns.
        if self.instruction_data.amount > lp_supply {
            return Err(ProgramError::InvalidArgument);
        }

        // Burning the required tokens, for pool share ownership.
        TokenAccount::burn_tokens(
            self.accounts.mint_lp,
            self.accounts.user_lp_ata,
            self.accounts.user,
            self.instruction_data.amount,
            None
        )?;

        Ok(())
    }
}
