//! Swapping the tokens.
use pinocchio::{
    AccountView, Address, error::ProgramError, ProgramResult,
    cpi::{Signer, Seed}
};
use crate::helpers::utils::{
    SignerAccount, MintInterface, TokenInterface,
    MintAccount, TokenAccount, ProgramAccount, AssociatedTokenAccount,
};
use crate::helpers::errors::MegaAmmProgramError;
use pinocchio::sysvars::clock::Clock;
use crate::config::{Config, AmmState};
use constant_product_curve::{ConstantProduct, LiquidityPair};
use solana_address;
use pinocchio_log::log;

pub struct SwapAccounts<'info> {
    pub user: &'info AccountView,
    // Holds all token x deposited into the pool.
    pub vault_x: &'info AccountView,
    // Holds all token y deposited into the pool.
    pub vault_y: &'info AccountView,
    // Sends or receives token x to or from the pool.
    pub user_x_ata: &'info AccountView,
    // Sends or receives token x to or from the pool.
    pub user_y_ata: &'info AccountView,
    // Configuration account for the AMM pool.
    pub config: &'info AccountView,
    pub mint_lp: &'info AccountView,
    // SPL token program account.
    pub token_program: &'info AccountView,
}

impl<'info> TryFrom<&'info [AccountView]> for SwapAccounts<'info> {
    type Error = MegaAmmProgramError;
    fn try_from(accounts: &'info [AccountView]) -> Result<Self, Self::Error> {
        let [
            user,  vault_x, vault_y, user_x_ata, user_y_ata,
            config, mint_lp, token_program, _rem_data @ ..
        ] = accounts else {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        };
        // Checking the accounts.
        SignerAccount::check(user)?;
        MintInterface::check(mint_lp)?;
        TokenInterface::check(vault_x)?;
        TokenInterface::check(vault_y)?;
        ProgramAccount::check(config)?;
        let config_state = Config::load(config)?;
        AssociatedTokenAccount::check(user_x_ata, user, config_state.mint_x(), token_program)?;
        AssociatedTokenAccount::check(user_y_ata, user, config_state.mint_y(), token_program)?;

        Ok(Self {
            user, vault_x, vault_y, user_x_ata, user_y_ata,
            config, mint_lp, token_program,
        })
    }
}

pub struct SwapInstructionData {
    pub amount: u64,
    pub min: u64,
    pub expiration: i64,
    pub is_x: u8, // Swap being performed from token X to Y, bool value (1 or 0)
}

impl<'info> TryFrom<&'info [u8]> for SwapInstructionData {
    type Error = MegaAmmProgramError;
    fn try_from(data: &'info [u8]) -> Result<Self, Self::Error> {
        if data.len() != (8*3+1) {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let min = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[16..24].try_into().unwrap());
        let is_x = data[24];

        if amount <= 0 || min <= 0 {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        Ok(Self {
            amount, min, expiration, is_x,
        })
    }
}

pub struct Swap<'info> {
    pub accounts: SwapAccounts<'info>,
    pub instruction_data: SwapInstructionData,
}

impl<'info> TryFrom<(&'info [u8], &'info [AccountView])> for Swap<'info> {
    type Error = MegaAmmProgramError;
    fn try_from((data, accounts): (&'info [u8], &'info [AccountView])) -> Result<Self, Self::Error> {
        let accounts = SwapAccounts::try_from(accounts)?;
        let instruction_data = SwapInstructionData::try_from(data)?;

        Ok(Self {
            accounts, instruction_data
        })
    }
}

impl<'info> Swap<'info> {
    pub const DISCRIMINATOR: &'info u8 = &3;
    pub fn process(&mut self) -> ProgramResult {
        let amm_config = Config::load(self.accounts.config)?;
        if amm_config.state() != AmmState::Initialized.into() {
            return Err(MegaAmmProgramError::Unauthorized.into());
        }

        // Deserializing token accounts.
        let (vault_x, vault_y, lp_supply) = {
            let mint_data_ref = self.accounts.mint_lp.try_borrow()?;
            let mint_lp = unsafe {
                pinocchio_token::state::Mint::from_bytes_unchecked(&mint_data_ref)
            };

            let v_x_ref = self.accounts.vault_x.try_borrow()?;
            let v_x = unsafe { pinocchio_token::state::TokenAccount::from_bytes_unchecked(&v_x_ref) };

            let v_y_ref = self.accounts.vault_y.try_borrow()?;
            let v_y = unsafe { pinocchio_token::state::TokenAccount::from_bytes_unchecked(&v_y_ref) };
            (v_x.amount(), v_y.amount(), mint_lp.supply())
        };

        // Swap calculations
        let mut curve = ConstantProduct::init(
            vault_x,
            vault_y,
            lp_supply, // fake lp supply since lp supply is not necessary here.
            amm_config.fee(),
            None
        ).map_err(|_| MegaAmmProgramError::InvalidInstructionData)?;

        let p = match self.instruction_data.is_x == 1 {
            true => LiquidityPair::X,
            false => LiquidityPair::Y,
        };
        let swap_result = curve.swap(
            p, self.instruction_data.amount, self.instruction_data.min
        ).map_err(|_| MegaAmmProgramError::InvalidInstructionData)?;
        // Checking for correct values
        if swap_result.deposit == 0 || swap_result.withdraw == 0 {
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

        if self.instruction_data.is_x == 1 {
            // Swapping x for y, X from user to the pool.
            TokenAccount::transfer_spl_tokens(
                self.accounts.user_x_ata,
                self.accounts.vault_x,
                self.accounts.user,
                swap_result.deposit,
                None,
            )?;
            // Transfer token from pool to user for x.
            TokenAccount::transfer_spl_tokens(
                self.accounts.vault_y,
                self.accounts.user_y_ata,
                self.accounts.config,
                swap_result.withdraw,
                Some(&signer_seeds),
            )?;
        } else {
            // Swapping y for x, Y from user to the pool.
            TokenAccount::transfer_spl_tokens(
                self.accounts.user_y_ata,
                self.accounts.vault_y,
                self.accounts.user,
                swap_result.deposit,
                None,
            )?;
            // Transfer token from pool to user for y.
            TokenAccount::transfer_spl_tokens(
                self.accounts.vault_x,
                self.accounts.user_x_ata,
                self.accounts.config,
                swap_result.withdraw,
                Some(&signer_seeds),
            )?;
        }
        Ok(())
    }
}
