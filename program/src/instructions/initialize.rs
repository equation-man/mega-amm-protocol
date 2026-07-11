//! Initializes the Config account and store all the infomation needed
//! for the amm to operate correctly. Also creates the mint_lp 
//! Mint account ans assigns the mint_authority to the config account.
use core::mem::MaybeUninit;
use solana_address;
use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Signer, Seed}, sysvars::{rent::Rent, Sysvar},
};
use crate::helpers::errors::MegaAmmProgramError;
use crate::helpers::utils::{
    SignerAccount, MintInterface, TokenInterface,
    ProgramAccount, MintAccount, TokenAccount,
    AssociatedTokenAccount,
};
use crate::config::Config;
use pinocchio_log::log;
use pinocchio_associated_token_account;
use pinocchio_associated_token_account::{
    instructions::{Create, CreateIdempotent},
};

pub struct InitializeAccounts<'a> {
    // Creator of the config account, 
    // signer and mutable to pay for initializations(config & mint_lp).
    // does not have to be authority over it.
    pub initializer: &'a AccountView,
    // This is the configuration account being initialized. must be mutable.
    pub config: &'a AccountView,
    // Will be vault ata to be created for token x
    pub vault_x_ata: &'a AccountView,
    // Will be vault ata to be created for token y
    pub vault_y_ata: &'a AccountView,
    // Mint account for pool tokens
    pub mint_x: &'a AccountView,
    pub mint_y: &'a AccountView,
    // Mint account that will represent the pool's liquidity.
    // mint_authority should be set to the config account, and must be mutable.
    pub mint_lp: &'a AccountView,
    pub ata_token_program: &'a AccountView,
    pub system_program: &'a AccountView,
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for InitializeAccounts<'a> {
    type Error = MegaAmmProgramError;
    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        // Follows the order they are from the client
        let [
            initializer, 
            vault_x_ata, vault_y_ata,
            mint_x, mint_y,
            mint_lp, config, ata_token_program,
            system_program,
            token_program,
        ] = accounts else {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        };
        // Checking the accounts. mint and config account check not required 
        // here since they have not been created yet.
        SignerAccount::check(initializer)?;
        //MintInterface::check(mint_lp)?;
        //ProgramAccount::check(config)?;
        
        Ok(Self {
            initializer: initializer,
            vault_x_ata: vault_x_ata, vault_y_ata: vault_y_ata,
            mint_x: mint_x, mint_y: mint_y, mint_lp: mint_lp,
            config: config, ata_token_program: ata_token_program,
            system_program: system_program, token_program: token_program
        })
    }
}

#[repr(C, packed)]
pub struct InitializeInstructionData {
    pub seed: u64, // Random number used for PDA seed derivation, for unique pool instances.
    pub fee: u16, // Swap fee, expressed in basis points(1 basis point = 0.01%).
    pub mint_x: [u8; 32], // SPL token mint address for token X in the pool.
    pub mint_y: [u8; 32], // SPL token mint address for token Y in the pool
    pub config_bump: [u8; 1], // Bump seed for deriving the config.
    pub lp_mint_decimals: u8, // Mint decimals.
    pub lp_bump: [u8; 1], // Bump seed used for deriving the lp_mint account PDA. Must be a u8.
    pub authority: [u8; 32], // Public key with admin auth over the AMM. Immutable pool if absent.
}

impl<'a> TryFrom<&'a [u8]> for InitializeInstructionData {
    type Error = MegaAmmProgramError;
    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        const INITIALIZE_DATA_LEN_WITH_AUTHORITY: usize = size_of::<InitializeInstructionData>();
        const INITIALIZE_DATA_LEN: usize = INITIALIZE_DATA_LEN_WITH_AUTHORITY - size_of::<[u8; 32]>();
        match data.len() {
            INITIALIZE_DATA_LEN_WITH_AUTHORITY => {
                Ok(unsafe { (data.as_ptr() as *const Self).read_unaligned() })
            }
            INITIALIZE_DATA_LEN => {
                // Authority is not present. We need to build buffer and add it at the end before
                // transmitting buffer to the struct.
                let mut raw: MaybeUninit<[u8; INITIALIZE_DATA_LEN_WITH_AUTHORITY]> = MaybeUninit::uninit();
                let raw_ptr = raw.as_mut_ptr() as *mut u8;
                unsafe {
                    // Copy the provided data.
                    core::ptr::copy_nonoverlapping(data.as_ptr(), raw_ptr, INITIALIZE_DATA_LEN);
                    // Add the authority to the end of the buffer.
                    core::ptr::write_bytes(raw_ptr.add(INITIALIZE_DATA_LEN), 0, 32);
                    // Now transmute to the struct.
                    Ok((raw.as_ptr() as *const Self).read_unaligned())
                }
            }
            _ => Err(MegaAmmProgramError::InvalidAccountData.into()),
        }
    }
}

pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub instruction_data: InitializeInstructionData,
}
impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for Initialize<'a> {
    type Error = MegaAmmProgramError;
    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = InitializeAccounts::try_from(accounts)?;
        let instruction_data = InitializeInstructionData::try_from(data)?;
        Ok(Self {accounts, instruction_data})
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;
    pub fn process(&mut self) -> ProgramResult {
        // Initialize Config account and store all the config information.
        // Create the mint_lp Mint account and assign the mint_authority to the Config account.
        // Creating the config (pool).
        let seed_binding = self.instruction_data.seed.to_le_bytes();
        let conf_bump_binding = self.instruction_data.config_bump;
        let config_signer_seeds = [
            Seed::from(b"config"),
            Seed::from(&conf_bump_binding),
        ];
        let signer_seeds = [Signer::from(&config_signer_seeds)];
        ProgramAccount::init::<Config>(
            self.accounts.initializer,
            self.accounts.config,
            &signer_seeds,
            Config::LEN
        )?;

        // Creating mint_lp. We will pass address of the config(pool) above.
        let (expected_lp_mint, lp_bump) = Address::find_program_address(
            &[b"lp_mint", self.accounts.config.address().as_ref()],
            &crate::ID.into()
        );

        // Initializing the mint and config accounts.
        let config = Config::load_mut(self.accounts.config)?;
        config.set_inner(
            self.instruction_data.seed,
            self.accounts.config.address().to_bytes(),
            self.instruction_data.mint_x,
            self.instruction_data.mint_y,
            self.instruction_data.fee,
            self.instruction_data.config_bump,
        );

        // Creating ata for the pool vaults.
        AssociatedTokenAccount::init(
            &self.accounts.vault_x_ata,
            &self.accounts.mint_x,
            &self.accounts.initializer,
            &self.accounts.config,
            &self.accounts.system_program,
            &self.accounts.token_program,
            &self.accounts.ata_token_program,
        )?;
        AssociatedTokenAccount::init(
            &self.accounts.vault_y_ata,
            &self.accounts.mint_y,
            &self.accounts.initializer,
            &self.accounts.config,
            &self.accounts.system_program,
            &self.accounts.token_program,
            &self.accounts.ata_token_program,
        )?;

        let mint_config_binding = self.accounts.config.address().to_bytes();
        let lp_mint_bump_binding = [lp_bump];
        let mint_signer_seeds = [
            Seed::from(b"lp_mint"),
            Seed::from(&mint_config_binding),
            Seed::from(&lp_mint_bump_binding),
        ];
        let mint_signer = [Signer::from(&mint_signer_seeds)];
        // Confirm if the supplied mint address is similar to the derived(expected_mint_lp)
        // address check before Mint init.
        MintAccount::init(
            self.accounts.mint_lp,
            self.accounts.initializer,
            self.instruction_data.lp_mint_decimals,
            self.accounts.config.address(),
            &mint_signer,
            None, // LP tokens should not be freezable.
        )?;
        Ok(())
    }
}
