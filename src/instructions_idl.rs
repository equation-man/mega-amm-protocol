#![cfg(feature = "idl")]
/// Nothing in this file should be evaulated during main production builds.
use shank::{ShankAccount};
use borsh::{BorshSerialize, BorshDeserialize};
use solana_program::pubkey::Pubkey;

#[derive(Clone, BorshSerialize, BorshDeserialize, ShankAccount)]
pub struct InitializeAccounts {
    pub initializer: Pubkey,
}
