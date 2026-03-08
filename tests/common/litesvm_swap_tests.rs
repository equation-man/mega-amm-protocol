//! Testing swaping of tokens.
#![allow(warnings)]
use litesvm::LiteSVM;
use solana_address::Address;
use pinocchio_token;
use pinocchio_token::state::Mint;
use mega_amm_protocol::config::Config;
use solana_sdk::{
    pubkey::Pubkey, account::Account, instruction::{AccountMeta, Instruction},
    sysvar::clock, signature::{Keypair, Signer}, message::Message,
    transaction::Transaction,
};
use solana_system_interface::{
    program::ID as SYSTEM_PROGRAM_ID,
};
use crate::common::context::{DepositTestContext, AmmTestContext};
use crate::common::litesvm_setup::{
    create_ata, create_pda_mint, mint_tokens, get_token_balance
};

use spl_token::ID as TOKEN_PROGRAM_ID;

pub fn swap_tokens(ctx: &mut AmmTestContext) {
    let svm = &mut ctx.svm;
    let user = &ctx.initializer; // initializer is acting as user
    svm.airdrop(&user.pubkey(), 1_000_000_000_000);
    let program_id = ctx.program_id;

    // Create User ATAs
    let user_x_ata = create_ata(
        svm,
        &user,
        &ctx.mint_x,
        &user.pubkey(),
    );

    let user_y_ata = create_ata(
        svm,
        &user,
        &ctx.mint_y,
        &user.pubkey(),
    );

    // Mint tokens to user so they can swap
    mint_tokens(
        svm,
        user,
        &ctx.mint_x,
        &user_x_ata,
        1_000_000,
    );

    // Build Swap Instruction Data
    let amount: u64 = 10_000;
    let min: u64 = 1;
    let expiration: i64 = 1_800_000_000;
    let is_x: u8 = 1; // swap X -> Y

    let mut data = Vec::with_capacity(26);
    data.push(3u8); // Swap discriminator (must match on-chain)
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&min.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());
    data.push(is_x);

    // Vault balances before swap.
    let x_vault_before_swap = get_token_balance(svm, &ctx.vault_x_ata);
    let y_vault_before_swap = get_token_balance(svm, &ctx.vault_y_ata);
    println!("The balance in the vault x before swap is {}", x_vault_before_swap);
    println!("The balance in the vault y before swap is {}", y_vault_before_swap);
    println!("================================================");
    // token balances before swaps
    let x_before_swap = get_token_balance(svm, &user_x_ata);
    let y_before_swap = get_token_balance(svm, &user_y_ata);

    println!("User ata balance for X before swap: {}", x_before_swap);
    println!("User ata balance for Y before swap: {}", y_before_swap);
    println!("The amount of token X to be swapped for y: {}", amount);
    println!("The fee charge is {} basis points", ctx.fee);
    println!("================================================");

    // Build Instruction
    let accounts = vec![
        AccountMeta::new(user.pubkey(), true),

        AccountMeta::new(ctx.vault_x_ata, false),
        AccountMeta::new(ctx.vault_y_ata, false),

        AccountMeta::new(user_x_ata, false),
        AccountMeta::new(user_y_ata, false),

        AccountMeta::new(ctx.config_pda, false),
        AccountMeta::new(ctx.lp_mint_pda, false),

        AccountMeta::new_readonly(pinocchio_token::ID, false),
    ];

    let instruction = Instruction::new_with_bytes(
        program_id,
        &data,
        accounts,
    );

    let tx = Transaction::new(
        &[&user],
        Message::new(&[instruction], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    //println!("Swap result: {:#?}", result);

    // Validate balances from the user's wallet
    let x_after_swap = get_token_balance(svm, &user_x_ata);
    let y_after_swap = get_token_balance(svm, &user_y_ata);

    // Token balance from the pool
    let x_vault_after_swap = get_token_balance(svm, &ctx.vault_x_ata);
    let y_vault_after_swap = get_token_balance(svm, &ctx.vault_y_ata);


    println!("User ata balance for X after swap: {}", x_after_swap);
    println!("User ata balance for Y after swap(minus swap fees): {}", y_after_swap);
    println!("=============================================================");
    println!("The balance in the vault x after swap is {}", x_vault_after_swap);
    println!("The balance in the vault y after swap is {}", y_vault_after_swap);
    println!("=============================================================");

    // Return some amount of the other token that should be greater than 0.
    // Here we are swapping x, so y should be greater than 0.
    // Or we should have more y in the wallet than before and less y in the pool than before
    // We should have less x in the wallet than before and more x in the pool than before
    // We should have less x in the wallet than before and more x in the pool than before
    //assert!(x_after < 100_000);
    //assert!(y_after > 0);
}

