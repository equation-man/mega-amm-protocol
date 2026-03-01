//! Deposit mint_x and mint_y token based on the amount of LP the user
//! wants to mint. We calculate the amount to deposit and check that the amount 
//! isn't greater than min_x and max_y designed by the user. We mint_
//! the right amount of mint_lp in the user ata.
//! All the Associated Token Accounts are initialized outside of our instruction to optimize for
//! performance.
use pinocchio::{
    AccountView, Address, ProgramResult,
    error::ProgramError,
    cpi::{Signer, Seed}, sysvars::{rent::Rent, Sysvar},
};
use pinocchio_token::instructions::Transfer;
use pinocchio::sysvars::clock::Clock;
use crate::helpers::utils::{
    SignerAccount, MintInterface, TokenInterface,
    MintAccount, TokenAccount, ProgramAccount, AssociatedTokenAccount,
};
use crate::helpers::errors::MegaAmmProgramError;
use crate::helpers::math_procs::curve_ops::MegaAmmStableSwapCurve;
use crate::config::{Config, AmmState};
use constant_product_curve::ConstantProduct;
use solana_address;
use pinocchio_log::log;

pub struct DepositAccounts<'info> {
    // User depositing the token into the liquidity of the AMM.(signer)
    pub user: &'info AccountView,
    // Token account that holds all of token X deposited into the pool.(mutable)
    pub vault_x: &'info AccountView,
    // Token account that holds all of token Y deposited into the pool.(mutable)
    pub vault_y: &'info AccountView,
    // User associated token account for token x. Where token X is transferred from into
    // the pool.(mutable)
    pub user_x_ata: &'info AccountView,
    // User associated token account for token y. Where token y is transferred fromt into
    // the pool.(mutable)
    pub user_y_ata: &'info AccountView,
    // The config account for the AMM pool. Stores all the relevant pool parameter and state.
    pub config: &'info AccountView,
    // Mint account that will represent the pool's liquidity.(mutable)
    pub mint_lp: &'info AccountView,
    // User's associated Token account for LP tokens. Destination account where LP tokens will be
    // minted. (Mutable).
    pub user_lp_ata: &'info AccountView,
    // Vault account info or configuration.
    //pub vault: &'info AccountView,
    // SPL Token program account. Required to perform token operations such as minting.(executable)
    pub token_program: &'info AccountView,
}

impl<'info> TryFrom<&'info [AccountView]> for DepositAccounts<'info> {
    type Error = MegaAmmProgramError;
    fn try_from(accounts: &'info [AccountView]) -> Result<Self, Self::Error> {
        let [
            user, config, mint_lp, user_lp_ata,
            vault_x, vault_y, user_x_ata, user_y_ata, 
            token_program, _rem_data @ ..
        ] = accounts else {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        };
        // Checking the accounts.
        SignerAccount::check(user)?;
        MintInterface::check(mint_lp)?;
        TokenInterface::check(vault_x)?;
        TokenInterface::check(vault_y)?;
        AssociatedTokenAccount::check(user_lp_ata, user, mint_lp.address(), token_program)?;
        // Check the config account and load it for mint checks.
        ProgramAccount::check(config)?;
        let conf_state = Config::load(config)?;
        AssociatedTokenAccount::check(user_x_ata, user, conf_state.mint_x(), token_program)?;
        AssociatedTokenAccount::check(user_y_ata, user, conf_state.mint_y(), token_program)?;

        Ok(Self {
            user, mint_lp, vault_x, vault_y, user_x_ata, user_y_ata,
            user_lp_ata, config, token_program
        })
    }
}

pub struct DepositInstructionData {
    // Amount of token x that the user intends to deposit into the pool.
    pub amount_x: u64,
    // Amount of token y that the user intends to deposit into the pool.
    pub amount_y: u64,
    // Expiration of this order, Makes sure that the transaction has to 
    // be done within a certain amount of time.
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData {
    type Error = MegaAmmProgramError;
    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<DepositInstructionData>() {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        let amount_x = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let amount_y = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[16..24].try_into().unwrap());

        if amount_x == 0 || amount_y == 0 {
            return Err(MegaAmmProgramError::InvalidInstructionData.into());
        }

        Ok(Self {
            amount_x, amount_y, expiration
        })
    }
}


pub struct Deposit<'info> {
    pub accounts: DepositAccounts<'info>,
    pub instruction_data: DepositInstructionData,
}

impl<'info> TryFrom<(&'info [u8], &'info [AccountView])> for Deposit<'info> {
    type Error = MegaAmmProgramError;
    fn try_from((data, accounts): (&'info [u8], &'info [AccountView])) -> Result<Self, Self::Error> {
        let accounts = DepositAccounts::try_from(accounts)?;
        let instruction_data = DepositInstructionData::try_from(data)?;
        // Returning the validated struct.
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'info> Deposit<'info> {
    pub const DISCRIMINATOR: &'info u8 = &1;
    // Create vault and submit tokens x and y to the vault.
    pub fn process(&mut self) -> ProgramResult {
        // We first load the config account from self.accounts.config.
        let amm_config = Config::load(self.accounts.config)?;
        if amm_config.state() != AmmState::Initialized.into() {
            return Err(MegaAmmProgramError::Unauthorized.into());
        }

        // Deserializing the token accounts. 
        // Context added to drop all borrows before the accounts are used again
        // for transfer instructions etc.
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

        // Using newton to calculate the amount of LP tokens to be minted.
        // We provide the amounts of token x and y that we want to deposit 
        // in the liquidity pool.
        let balances = [vault_x_amount, vault_y_amount];
        let curve = MegaAmmStableSwapCurve { balances: &balances, fee: 0 };
        log!("About to run newton solver");
        let mint_lp_from_newton = curve.deposit_to_amm(
            100u64, lp_supply, balances.len() as u32, &balances
            ).map_err(|e| { log!("The error is {}", e); ProgramError::Custom(0)})?;
        log!("Lp to mint via newton is: >>>>>> {}", mint_lp_from_newton);

        // Transfer tokens(x & y) from ata to vaults/token accounts of the pool.
        // Amount to transfer is calculated from the lp token to be minted.
        TokenAccount::transfer_spl_tokens(
            self.accounts.user_x_ata,
            self.accounts.vault_x,
            self.accounts.user, // Wallet signer
            self.instruction_data.amount_x, // x tokens amount to transfer
            None, // user signs normally.
        )?;
        TokenAccount::transfer_spl_tokens(
            self.accounts.user_y_ata,
            self.accounts.vault_y,
            self.accounts.user, // Wallet signer
            self.instruction_data.amount_y, // y token amounts to transfer
            None, // user signs normally.
        )?;

        // Getting the lp bump
        let (expected_lp_mint, lp_bump) = Address::find_program_address(
            &[b"lp_mint", self.accounts.config.address().as_ref()],
            &crate::ID.into()
        );
        let mint_config_binding = self.accounts.config.address().to_bytes();
        let lp_mint_bump_binding = [lp_bump];
        let mint_signer_seeds = [
            Seed::from(b"lp_mint"),
            Seed::from(&mint_config_binding),
            Seed::from(&lp_mint_bump_binding),
        ];
        let mint_signer = [Signer::from(&mint_signer_seeds)];

        // Minting the required tokens, for pool share ownership.
        TokenAccount::mint_tokens(
            self.accounts.mint_lp,
            self.accounts.user_lp_ata,
            self.accounts.mint_lp,
            mint_lp_from_newton,
            &mint_signer,
        )?;
        Ok(())
    }
}
