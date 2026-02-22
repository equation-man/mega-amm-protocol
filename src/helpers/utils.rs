//! Accounts creation and verification utilities.
//! Supports both token-2022 and legacy token program.
use solana_address;
use pinocchio::{
    AccountView, Address, error::ProgramError, ProgramResult,
    cpi::{Signer, Seed}, sysvars::{rent::Rent, Sysvar}
};
use pinocchio_token_2022::ID as TOKEN_2022_PROGRAM_ID;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::{
    InitializeMint2, InitializeAccount3, MintTo, Transfer,
    Burn,
};
use pinocchio_associated_token_account::{
    instructions::{Create, CreateIdempotent},
};
use pinocchio_log::log;

use crate::helpers::MegaAmmProgramError;
use crate::config::Config;

const TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET: usize = 165;
pub const TOKEN_2022_MINT_DISCRIMINATOR: u8 = 0x01;
pub const TOKEN_2022_TOKEN_ACCOUNT_DISCRIMINATOR: u8 = 0x02;
// Signer accounts checks.
pub struct SignerAccount;
impl SignerAccount {
    /// Confirm if account is signer or not.
    pub fn check(account: &AccountView) -> Result<(), MegaAmmProgramError> {
        if !account.is_signer() {
            return Err(MegaAmmProgramError::InvalidSignature.into());
        }
        Ok(())
    }
}

// Performing checks with interfaces to support both the legacy token programs
// and token-2022 standards.
pub struct MintInterface;
impl MintInterface {
    pub fn check(account: &AccountView) -> Result<(), MegaAmmProgramError> {
        if !account.owned_by(&TOKEN_2022_PROGRAM_ID) {
            if !account.owned_by(&pinocchio_token::ID) {
                return Err(MegaAmmProgramError::InvalidOwner.into());
            } else {
                if account.data_len().ne(&pinocchio_token::state::Mint::LEN) {
                    return Err(MegaAmmProgramError::InvalidAccountData.into());
                }
            }
        } else {
            let data = account.try_borrow()?;
            if data.len().ne(&pinocchio_token::state::Mint::LEN) {
                if data.len().le(&TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET) {
                    return Err(MegaAmmProgramError::InvalidAccountData.into());
                }
                if data[TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET].ne(&TOKEN_2022_MINT_DISCRIMINATOR) {
                    return Err(MegaAmmProgramError::InvalidAccountData.into());
                }
            }
        }
        Ok(())
    }
}

pub struct TokenInterface;
impl TokenInterface {
    pub fn check(account: &AccountView) -> Result<(), MegaAmmProgramError> {
        if !account.owned_by(&TOKEN_2022_PROGRAM_ID) {
            if !account.owned_by(&pinocchio_token::ID) {
                return Err(MegaAmmProgramError::InvalidOwner.into());
            } else {
                if account.data_len().ne(&pinocchio_token::state::TokenAccount::LEN) {
                    return Err(MegaAmmProgramError::InvalidAccountData.into());
                }
            }
        } else {
            let data = account.try_borrow()?;
            if data.len().ne(&pinocchio_token::state::TokenAccount::LEN) {
                if data.len().le(&TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET) {
                    return Err(MegaAmmProgramError::InvalidAccountData.into());
                }
                if data[TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET]
                    .ne(&TOKEN_2022_TOKEN_ACCOUNT_DISCRIMINATOR) {
                        return Err(MegaAmmProgramError::InvalidAccountData.into());
                }
            }
        }
        Ok(())
    }
}

// TOKEN ACCOUNTS OPERATIONS. NOT DOING CHECKS. ITS HANDLED BY THE INTERFACE(TokenInterface and
// MintInterface)
pub struct MintAccount;
impl MintAccount {
    pub fn init(
        account: &AccountView, payer: &AccountView,
        decimals: u8, mint_authority: &Address,
        mint_signer: &[Signer],
        freeze_authority: Option<&Address>
    ) -> ProgramResult {
        // Get required lamports for rent.
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(pinocchio_token::state::Mint::LEN);
        // Funding the account with required lamports.
        // We ran invoke_signed since the mint is controlled by PDA
        CreateAccount {
            from: payer, to: account, lamports, space: pinocchio_token::state::Mint::LEN as u64,
            owner: &pinocchio_token::ID,
        }.invoke_signed(&mint_signer)?;
        InitializeMint2 {
            mint: account,
            decimals,
            mint_authority,
            freeze_authority,
        }.invoke()
    }

    pub fn init_if_needed(
        account: &AccountView,
        payer: &AccountView,
        decimals: u8,
        mint_authority: &Address,
        mint_signer: &[Signer],
        freeze_authority: Option<&Address>
    ) -> ProgramResult {
        match MintInterface::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Ok(Self::init(account, payer, decimals, mint_authority, mint_signer, freeze_authority)?),
        }
    }
}

pub struct TokenAccount;
impl TokenAccount {
    pub fn init(
        account: &AccountView,
        mint: &AccountView,
        payer: &AccountView,
        owner: &Address,
    ) -> ProgramResult {
        // Getting the required lamports.
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(pinocchio_token::state::TokenAccount::LEN);
        // Funding the account with the required lamports.
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: pinocchio_token::state::TokenAccount::LEN as u64,
            owner: &pinocchio_token::ID,
        }.invoke()?;
        InitializeAccount3 {
            account, mint, owner
        }.invoke()
    }

    pub fn init_if_needed(
        account: &AccountView, mint: &AccountView,
        payer: &AccountView, owner: &Address
    ) -> ProgramResult {
        match TokenInterface::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Ok(Self::init(account, mint, payer, owner)?),
        }
    }

    pub fn mint_tokens(
        mint: &AccountView,
        account: &AccountView,
        authority: &AccountView,
        amount: u64,
        mint_signer: &[Signer],
    ) -> ProgramResult {
        MintTo {
            mint,
            account,
            mint_authority: authority,
            amount
        }.invoke_signed(&mint_signer)
    }

    pub fn burn_tokens(
        mint: &AccountView,
        from: &AccountView,
        authority: &AccountView,
        amount: u64,
        signer_seeds: Option<&[Signer]>,
    ) -> ProgramResult {
        let burn_ix = Burn {
            mint,
            account: from,
            authority,
            amount,
        };

        match signer_seeds {
            Some(seeds) => burn_ix.invoke_signed(&seeds),
            None => burn_ix.invoke(),
        }
    }

    pub fn transfer_spl_tokens(
        from: &AccountView, to: &AccountView,
        authority: &AccountView, amount: u64,
        signer_seeds: Option<&[Signer]>,
    ) -> ProgramResult {
        match signer_seeds {
            Some(seeds) => {
                Transfer {
                    from, to, authority, amount,
                }.invoke_signed(seeds)
            },
            None => {
                Transfer {
                    from, to, authority, amount
                }.invoke()
            }
        }
    }
}

pub struct AssociatedTokenAccount;
impl AssociatedTokenAccount {
    pub fn check(
        account: &AccountView, 
        authority: &AccountView,
        mint: &Address,
        token_program: &AccountView
    ) -> Result<(), MegaAmmProgramError> {
        // Validate token account structure and owner
        TokenInterface::check(account)?;
        // Validating the PDA address.
        if Address::find_program_address(&[authority.address().as_ref(), token_program.address().as_ref(), mint.as_ref()],
            &pinocchio_associated_token_account::ID).0.ne(account.address()) {
            return Err(MegaAmmProgramError::InvalidAddress.into());
        }
        Ok(())
    }

    pub fn init(
            account: &AccountView,
            mint: &AccountView,
            payer: &AccountView,
            owner: &AccountView,
            system_program: &AccountView,
            token_program: &AccountView,
            ata_program: &AccountView,
        ) -> ProgramResult {

        CreateIdempotent {
            funding_account: payer,
            account,
            wallet: owner,
            mint,
            system_program,
            token_program,
        }.invoke()
    }

    pub fn init_if_needed(
        account: &AccountView,
        mint: &AccountView,
        payer: &AccountView,
        owner: &AccountView,
        system_program: &AccountView,
        token_program: &AccountView,
        ata_program: &AccountView,
    ) -> Result<(), MegaAmmProgramError> {
        match Self::check(account, payer, mint.address(), token_program) {
            Ok(_) => Ok(()),
            Err(_) => Ok(Self::init(account, mint, payer, owner, system_program, token_program, ata_program)?),
        }
    }
}

pub struct ProgramAccount;
impl ProgramAccount {
    pub const LEN: usize = size_of::<Self>();
    pub fn check(account: &AccountView) -> Result<(), MegaAmmProgramError> {
        // Check if this account is owned by this program
        if !account.owned_by(&Address::new_from_array(crate::ID)) {
            return Err(MegaAmmProgramError::InvalidOwner.into());
        }
        if account.data_len().ne(&Config::LEN) {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        }
        Ok(())
    }

    pub fn init<'a, T: Sized>(
        payer: &AccountView, account: &AccountView,
        signer: &[Signer], space: usize
    ) -> ProgramResult {
        // Get required lamports for rent.
        let rent = Rent::get()?;
        let lamports = rent.try_minimum_balance(space)?;
        // Create signer with seeds slice.
        // Create the account.
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: space as u64,
            owner: &Address::new_from_array(crate::ID),
        }.invoke_signed(&signer)?;
        Ok(())
    }

    pub fn close(account: &AccountView, destination: &AccountView) -> ProgramResult {
        {
            let mut data = account.try_borrow_mut()?;
            data[0] = 0xff;
        }
        destination.set_lamports(account.lamports());
        account.resize(1)?;
        account.close()
    }
}
