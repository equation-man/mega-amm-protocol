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
}
