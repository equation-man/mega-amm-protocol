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
use crate::helpers::math_procs::curve_ops::MegaAmmStableSwapCurve;
use crate::helpers::math_procs::numerical_ops::get_d;
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

#[repr(C)]
pub struct WithdrawInstructionData {
    pub lp_to_burn: u64,
    pub amount_of_x: u64,
    pub amount_of_y: u64,
    pub expiration: i64,
    pub withdraw_mode: u8,
}

impl<'info> TryFrom<&'info [u8]> for WithdrawInstructionData {
    type Error = MegaAmmProgramError;
    fn try_from(data: &'info [u8]) -> Result<Self, Self::Error> {
        if data.len() != (8+8+8+8+1) {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        let lp_to_burn = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let amount_of_x = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let amount_of_y = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[24..32].try_into().unwrap());
        let withdraw_mode = data[32];

        if withdraw_mode != 0 && withdraw_mode != 1 {
            log!("Withdraw mode error");
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        if lp_to_burn < 0 {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        if amount_of_x < 0 || amount_of_y < 0 {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        Ok(Self {
            withdraw_mode, lp_to_burn, amount_of_x, amount_of_y, expiration
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

        log!("Vault amounts before withdrawal x {} y {}", vault_x_amount, vault_y_amount);
        // Used for pda signing during withdrawal.
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

        log!("Enter the newton solvers");
        let balances = [vault_x_amount, vault_y_amount];
        let mut curve = MegaAmmStableSwapCurve { balances: &balances, fee: 0};
        // Transfer token amounts returned, list of amounts of tokens to move.
        log!("Getting the withdraw amounts");
        if self.instruction_data.withdraw_mode == 0 {
            log!("Balanced withdrawal");
            // Balanced withdrawal. Specifying the lps to burn comes from the frontend.
            // This is a slice, arranged as it was supplied to the curve
            // i.e for above, x is at the 1st index then y, these are amounts to send
            let new_balances = curve.withdraw_from_amm(self.instruction_data.lp_to_burn, lp_supply, 0, None, None, None)
                .map_err(|_| ProgramError::Custom(2))?;
            log!("Balanced withdrawal");

            // Transfer tokens x from the pool to the user.
            TokenAccount::transfer_spl_tokens(
                self.accounts.vault_x,
                self.accounts.user_x_ata,
                self.accounts.config,
                new_balances[0],
                Some(&signer_seeds),
            )?;
            // Transfer token y from the pool to the user.
            TokenAccount::transfer_spl_tokens(
                self.accounts.vault_y,
                self.accounts.user_y_ata,
                self.accounts.config,
                new_balances[0],
                Some(&signer_seeds),
            )?;

            // Burning the required tokens, for pool share ownership.
            TokenAccount::burn_tokens(
                self.accounts.mint_lp,
                self.accounts.user_lp_ata,
                self.accounts.user,
                self.instruction_data.lp_to_burn,
                None
            )?;
            Ok(())
        } else {
            // Calculating d_current and the lps to burn.
            log!("Getting the d current");
            let d_current = get_d(100, curve.balances, 2).map_err(|_| ProgramError::Custom(0))?;
            let d_new = get_d(100, &[self.instruction_data.amount_of_x, self.instruction_data.amount_of_y], 2).map_err(|_| ProgramError::Custom(1))?;
            let spread = d_current.checked_sub(d_new).ok_or(ProgramError::Custom(2))?;
            let lp_to_burn = lp_supply.checked_mul(spread).ok_or(ProgramError::Custom(3))?.checked_div(d_new).ok_or(ProgramError::Custom(4))?;
            // Specifying lps to burn is calculated by the smart contract.
            let new_balances = curve.withdraw_from_amm(lp_to_burn, lp_supply, 1, Some(d_current), Some(100), Some(amm_config.fee().into()))
                .map_err(|_| ProgramError::Custom(2))?;
            // arranged with the one to be sent to the user being the first.
            // The one not to send is 0 and comes second.
            // First token on the list is token x and second is token y
            // The token not to be withdrawn will remain unchanged from the frontend
            // compare amount_of_x with, vault_x or vault_y
            log!("The values of x are {}, vault is {}", self.instruction_data.amount_of_x, vault_x_amount);
            log!("The values of y are {}, vault is {}", self.instruction_data.amount_of_y, vault_y_amount);
            Ok(())
        }
    }
}
