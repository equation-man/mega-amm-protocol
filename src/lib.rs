//! This is the program's entrypoint.
#![allow(warnings)]
#![cfg_attr(not(feature = "std"), no_std)]
//#![no_std] 

use pinocchio::{
    Address, AccountView, entrypoint,
    ProgramResult, error::ProgramError
};
use pinocchio_pubkey::declare_id;
use pinocchio_log::log;

pub mod helpers;
pub mod instructions;
pub mod config;
use helpers::*;
use instructions::{
    initialize::Initialize,
    deposit::Deposit,
    withdraw::Withdraw,
    swap::Swap,
};
use config::*;

declare_id!("2qcc8awwigDm897DxTNFwVgKpUpNZ3UbGHxoatBfmgLi");

entrypoint!(process_instructions);

fn process_instructions(
    _program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8]
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((Initialize::DISCRIMINATOR, data)) => Initialize::try_from((data, accounts))?.process(),
        Some((Deposit::DISCRIMINATOR, data)) => Deposit::try_from((data, accounts))?.process(),
        Some((Withdraw::DISCRIMINATOR, data)) => Withdraw::try_from((data, accounts))?.process(),
        Some((Swap::DISCRIMINATOR, data)) => Swap::try_from((data, accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
