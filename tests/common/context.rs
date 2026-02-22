//! This is a shared context across tests.
#![allow(warnings)]
use litesvm::LiteSVM;
use solana_sdk::{
    account::Account, pubkey::Pubkey, signature::Keypair,
};
use solana_system_interface::program::ID as SYSTEM_PROGRAM_ID;

pub struct AmmTestContext {
    pub svm: LiteSVM,
    pub program_id: Pubkey,

    pub initializer: Keypair,

    pub seed: u64,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,

    pub vault_x_ata: Pubkey,
    pub vault_y_ata: Pubkey,
    pub config_pda: Pubkey,
    pub lp_mint_pda: Pubkey,
}

pub struct DepositTestContext {
    pub user: Keypair,
    pub user_x_ata: Pubkey,
    pub user_y_ata: Pubkey, 
    pub user_lp_ata: Pubkey,
}
