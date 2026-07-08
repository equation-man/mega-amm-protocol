//! Commandline interface for calling the protocol's instructions.
use crate::protocol_instructions::*;

pub fn preparing_artifacts() {
    println!("Preparing wallets and test mints");
    // Create initialize
    // Create traders
    // create token mints
    // Fund traders
    // Generate Seed
    // Derive Config PDA
    // Derive LP Mint PDA
    // Derive Vault X ATA
    // Derive Vault Y ATA
    // Return all the addresses to be used in initialize instruction
}

pub fn process(action: &str, target: &str, amount: &str) {
    match action {
        "initialize" => {
            println!("Calling cli initialize command");
            preparing_artifacts();
            initialize_proto();
        },
        "deposit" => {
            println!("Deploying capital");
            println!("Target token: {}", target);
            println!("Amount: {}", amount);
        },
        "swap" => {
            println!("Swapping tokens");
            println!("Target token: {}", target);
            println!("Amount: {}", amount);
        },
        "withdraw" => {
            println!("Withdrawing capital");
            println!("Target token: {}", target);
            println!("Amount: {}", amount);
        },
        _ => eprintln!("Action unknown"),
    }
}
