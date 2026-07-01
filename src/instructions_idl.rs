#![cfg(feature = "idl")]
/// Nothing in this file should be evaulated during main production builds.
use shank::{ShankAccount};
use borsh::{BorshSerialize, BorshDeserialize};
use solana_program::pubkey::Pubkey;

#[derive(Debug, Clone, ShankInstruction, BorshSerialize, BorshDeserialize)]
#[rustfmt::skip]
pub enum InitializerInstruction {
    /// Initialize the protocol.
    #[account(0, writable, signer, name="initializer", desc="Protocol authority")]
    #[account(1, writable, name="config", desc="Configuration PDA account")]
    #[account(2, writable, name="vault_x_ata", desc="ATA vault for token x")]
    #[account(3, writable, name="vault_y_ata", desc="ATA vault for token y")]
    #[account(4, writable, name="mint_x", desc="Token mint for token x")]
    #[account(5, writable, name="mint_y", desc="Token mint for token y")]
    #[account(6, writable, name="mint_lp", desc="Token mint for the tokens given to LPs to rep pool's liquidity")]
    #[account(7, name="ata_token_program", desc="ATA program used in creating the ata")]
    #[account(8, name="system_program", desc="System program used in creating PDAs")]
    #[account(9, name="token_program", desc="Token program")]
    Initialize {
        seed: u64,
        fee: u16,
        mint_x: [u8; 32],
        mint_y: [u8; 32],
        config_bump: [u8; 1],
        lp_mint_decimals: u8,
        lp_bump: [u8; 1],
        authority: [u8; 32],
    },

    /// Depositing to the protocol
    #[account(0, writable, signer, name="user", desc="User depositing token to provide liquidity")]
    #[account(1, writable, name="vault_x", desc="Token account that holds token x deposited")]
    #[account(2, writable, name="vault_y", desc="Token account that holds token y deposited")]
    #[account(3, writable, name="user_x_ata", desc="User ata for token x")]
    #[account(4, writable, name="user_y_ata", desc="User ata for token y")]
    #[account(5, writable, name="config", desc="Protocol config account")]
    #[account(6, writable, name="mint_lp", desc="Mint account for the pool liquidity tokens")]
    #[account(7, name="user_lp_ata", desc="User ATA for LP tokens")]
    #[account(8, name="token_program", desc="Token program")]
    Deposit {
        amount_x: u64,
        amount_y: u64,
        expiration: i64,
    },

    /// Performing a token swap from the protocol.
    #[account(0, writable, signer, name="user", desc="User who wants to perform the swap")]
    #[account(1, writable, name="vault_x", desc="Holds all token x deposited into the pool")]
    #[account(2, writable, name="vault_y", desc="Holds all token y deposited into the pool")]
    #[account(3, writable, name="user_x_ata", desc="Sends or receives token x")]
    #[account(4, writable, name="user_y_ata", desc="Sends or receives token y")]
    #[account(5, writable, name="config", desc="Protocol configuration account")]
    #[account(6, writable, name="mint_lp", desc="Mint account for the pool liquidity tokens")]
    #[account(7, name="token_program", desc="Token program")]
    Swap {
        amount: u64,
        min_out: u64,
        expiration: i64,
        is_x: u8,
    },

    /// Withdrawing liquidity from the protocol
    #[account(0, writable, signer, name="user", desc="User depositing token to provide liquidity")]
    #[account(1, writable, name="mint_lp", desc="Mint account for the pool liquidity tokens")]
    #[account(2, writable, name="vault_x", desc="Token account that holds token x deposited")]
    #[account(3, writable, name="vault_y", desc="Token account that holds token y deposited")]
    #[account(4, writable, name="user_x_ata", desc="User ata for token x")]
    #[account(5, writable, name="user_y_ata", desc="User ata for token y")]
    #[account(6, writable, name="user_lp_ata", desc="User ata that holds lp tokens")]
    #[account(7, writable, name="config", desc="Protocol config account")]
    #[account(8, name="token_program", desc="Token program")]
    Withdraw {
        lp_to_burn: u64,
        amount_of_x: u64,
        amount_of_y: i64,
        expiration: i64,
        withdraw_mode: u8,
    },
}
