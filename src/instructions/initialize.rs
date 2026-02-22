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
        let [
            initializer, vault_x_ata, vault_y_ata,
            mint_x, mint_y,
            mint_lp, config, ata_token_program,
            system_program,
            token_program, _rem_data @ ..
        ] = accounts else {
            return Err(MegaAmmProgramError::InvalidAccountData.into());
        };
        // Checking the accounts. mint and config account check not required 
        // here since they have not been created yet.
        SignerAccount::check(initializer)?;
        //MintInterface::check(mint_lp)?;
        //ProgramAccount::check(config)?;
        
        Ok(Self {
            initializer, vault_x_ata, vault_y_ata,
            mint_x, mint_y, mint_lp, config, ata_token_program,
            system_program, token_program
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
            Seed::from(&seed_binding),
            Seed::from(self.instruction_data.mint_x.as_ref()),
            Seed::from(self.instruction_data.mint_y.as_ref()),
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

// ====================== TESTING INITIALIZATION =======================
//#[cfg(test)]
//mod tests {
//    use super::*;
//    use bincode;
//    use mollusk_svm::{
//        result::Check, Mollusk,
//        program::{
//            keyed_account_for_system_program,
//            create_program_account_loader_v3,
//        },
//    };
//    use solana_system_interface::{ program::ID as SYSTEM_PROGRAM_ID};
//    use mollusk_svm_programs_token;
//    use solana_sdk::{
//        instruction::{Instruction, AccountMeta}, pubkey::Pubkey,
//        signature::{Keypair}, account::Account, signer::Signer,
//    };
//
//    #[test]
//    #[ignore]
//    fn test_initialize_pool() {
//        // 1. Setting up Mollusk VM
//        const program_id: Pubkey = solana_sdk::pubkey!("2qcc8awwigDm897DxTNFwVgKpUpNZ3UbGHxoatBfmgLi");
//        let mut mollusk = Mollusk::default();
//        let (system_program, system_program_account) = keyed_account_for_system_program();
//        let program_account = create_program_account_loader_v3(&program_id);
//        mollusk.add_program(&program_id, "target/deploy/stoic_euclid_liquidity_protocol");
//        mollusk_svm_programs_token::token::add_program(&mut mollusk);
//        mollusk_svm_programs_token::token2022::add_program(&mut mollusk);
//        mollusk_svm_programs_token::associated_token::add_program(&mut mollusk);
//
//        let (token_program, token_program_account) = mollusk_svm_programs_token::token::keyed_account();
//        let (token2022_program, token2022_program_account) = mollusk_svm_programs_token::token2022::keyed_account();
//        let (associated_token_program, associated_token_program_account) = mollusk_svm_programs_token::associated_token::keyed_account();
//
//        // 2. Creating initializer (payer)
//        let initializer = Keypair::new();
//
//        // 3. Deriving Config (pool PDA)
//        let seed: u64 = 42;
//        let mint_x = Pubkey::new_unique();
//        let mint_y = Pubkey::new_unique();
//        //let config_bump: u8 = 255;
//        let (config_pda, config_bump) = Pubkey::find_program_address(
//            &[
//                b"config", &seed.to_le_bytes(), mint_x.as_ref(),
//                mint_y.as_ref(),
//            ],
//            &program_id
//        );
//
//        // 4. LP Mint account(Empty)
//        let (lp_mint_pda, lp_bump) = Pubkey::find_program_address(
//            &[b"lp_mint", config_pda.as_ref()],
//            &program_id
//        );
//
//        // 5. Defining the initial account states.
//        let rent = solana_sdk::sysvar::rent::Rent::default();
//
//
//
//        let accounts = vec![
//            (
//                initializer.pubkey(),
//                Account {
//                    lamports: 10_000_000_000,
//                    data: vec![],
//                    owner: system_program, //SYSTEM_PROGRAM_ID,
//                    executable: false,
//                    rent_epoch: 0,
//                },
//            ),
//            (
//                config_pda, // We are initializing the config pda
//                Account {
//                    lamports: 0,
//                    data: vec![],
//                    owner: system_program, // Config is owned by my program(this program).
//                    executable: false,
//                    rent_epoch: 0,
//                }
//            ),
//            (
//                lp_mint_pda,
//                Account {
//                    lamports: 0,
//                    data: vec![0u8; pinocchio_token::state::Mint::LEN],
//                    owner: pinocchio_token::ID.into(),
//                    executable: false,
//                    rent_epoch: 0,
//                }
//            ),
//            (
//                solana_sdk::sysvar::rent::ID,
//                Account {
//                    lamports: 0,
//                    data: bincode::serialize(&rent).unwrap(),
//                    owner: solana_sdk::sysvar::ID,
//                    executable: false,
//                    rent_epoch: 0,
//                }
//            ),
//            (
//                SYSTEM_PROGRAM_ID,
//                Account {
//                    lamports: 0,
//                    data: vec![],
//                    owner: system_program, //SYSTEM_PROGRAM_ID,
//                    executable: true,
//                    rent_epoch: 0
//                },
//
//            ),
//            (
//                pinocchio_token::ID.into(),
//                Account {
//                    lamports: 0,
//                    data: vec![],
//                    owner: token_program, //pinocchio_token::ID.into(),
//                    executable: true,
//                    rent_epoch: 0
//                },
//            ),
//        ];
//
//        // 6. Build and initialize instruction data.
//        let authority = Pubkey::new_unique();
//        let instruction_data = {
//            let mut data = vec![0u8]; // Discriminator=0
//            data.extend_from_slice(&seed.to_le_bytes());
//            data.extend_from_slice(&10u16.to_le_bytes()); // fee
//            data.extend_from_slice(mint_x.as_ref());
//            data.extend_from_slice(mint_y.as_ref());
//            data.push(config_bump);
//            data.push(9u8); // lp_mint decimals.
//            data.push(lp_bump);
//            data.extend_from_slice(authority.as_ref());
//            data
//        };
//
//        let instruction = Instruction::new_with_bytes(
//            program_id, &instruction_data,
//            vec![
//                AccountMeta::new(initializer.pubkey(), true),
//                AccountMeta::new(lp_mint_pda, false),
//                AccountMeta::new(config_pda, false),
//                AccountMeta::new_readonly(
//                    system_program,
//                    false,
//                ),
//                AccountMeta::new_readonly(
//                   token_program, 
//                    false,
//                ),
//                AccountMeta::new_readonly(
//                    solana_sdk::sysvar::rent::ID,
//                    false,
//                ),
//            ]
//        );
//        let res = mollusk.process_instruction(&instruction, &accounts);
//        println!("{:#?}", res);
//    }
//
//}
