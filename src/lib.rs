//! This is the program's entrypoint.
#![allow(warnings)]
#![cfg_attr(not(feature = "std"), no_std)]

use pinocchio::{
    Address, AccountView,
    ProgramResult, error::ProgramError
};
use pinocchio_pubkey::declare_id;
use pinocchio_log::log;

pub mod helpers;
pub mod instructions;
pub mod config;
#[cfg(feature = "idl" )]
pub mod instructions_idl;

use helpers::*;
use instructions::{
    initialize::Initialize,
    deposit::Deposit,
    withdraw::Withdraw,
    swap::Swap,
};
use config::*;

declare_id!("BHXSSPSY1DqDbLGUhf33bTRd8jxhNvGatqGNR14Huxwc");

#[cfg(all(not(feature = "std"), not(feature = "no-entrypoint")))]
mod entrypoint {
    use pinocchio::{default_allocator, nostd_panic_handler, program_entrypoint};

    // Minimum overhead global allocator
    default_allocator!();

    // Zero overhead aborting panic handler for saving CUs
    nostd_panic_handler!();

    // Register the custom raw SVM entrypoint
    program_entrypoint!(super::process_instructions);
}

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
