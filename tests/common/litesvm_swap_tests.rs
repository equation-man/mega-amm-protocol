//! Testing swaping of tokens.
#![allow(warnings)]
use litesvm::LiteSVM;
use solana_address::Address;
use pinocchio_token;
use pinocchio_token::state::Mint;
use mega_amm_protocol::config::Config;
use solana_sdk::{
    pubkey::Pubkey, account::Account, instruction::{AccountMeta, Instruction},
    sysvar::clock::Clock, signature::{Keypair, Signer}, message::Message,
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

pub fn normal_swap(ctx: &mut AmmTestContext, swap_amount: u64, slippage: u64, target_token: u8) {
    let svm = &mut ctx.svm;

    println!("================ NORMAL SWAP TEST ================");
    println!("Perfoming a normal token swap");

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
    let amount: u64 = swap_amount;
    let min: u64 = slippage;
    let expiration: i64 = 1_800_000_000;
    let is_x: u8 = target_token; // swap X -> Y

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

    // Send transaction to the smart contract
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

/// This swap should not trigger fees or mutate the pool state.
pub fn zero_amount_swap(ctx: &mut AmmTestContext, swap_amount: u64, slippage: u64, target_token: u8) {
    let svm = &mut ctx.svm;

    println!("================ ZERO SWAP TEST ================");
    println!("Attempting swap with ZERO input amount");

    // Advance the slot so setting up TXs (ATA creation/Minting) are unique
    //let clock: Clock = svm.get_sysvar();
    //let current_slot = clock.slot;
    //svm.warp_to_slot(current_slot + 1); // Warps the clock to specified slot

    let user = &ctx.initializer;
    svm.airdrop(&user.pubkey(), 1_000_000_000_000);
    let program_id = ctx.program_id;

    // Create User ATAs for storing x and y tokens
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

    // Mint tokens to user the token to swap, here, x
    mint_tokens(
        svm,
        user,
        &ctx.mint_x,
        &user_x_ata,
        1_000_000,
    );

    // Build Swap Instruction with zero amount
    let amount: u64 = swap_amount; // critical test condition, amount is zero.
    let min: u64 = slippage;
    let expiration: i64 = 1_800_000_000;
    let is_x: u8 = target_token;

    let mut data = Vec::with_capacity(26);
    data.push(3u8); // Swap discriminator
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&min.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());
    data.push(is_x);

    // Vault balances before swap.
    let x_vault_before_swap = get_token_balance(svm, &ctx.vault_x_ata);
    let y_vault_before_swap = get_token_balance(svm, &ctx.vault_y_ata);
    println!("The balance in the vault x before zero swap is {}", x_vault_before_swap);
    println!("The balance in the vault y before zero swap is {}", y_vault_before_swap);
    println!("================================================");
    // token balances before swaps
    let x_before_swap = get_token_balance(svm, &user_x_ata);
    let y_before_swap = get_token_balance(svm, &user_y_ata);

    println!("User ata balance for X before zero swap: {}", x_before_swap);
    println!("User ata balance for Y before zero swap: {}", y_before_swap);
    println!("The amount of token X to be swapped for y: {}", amount);
    println!("The fee charge is {} basis points", ctx.fee);
    println!("================================================");

    // Building the Instruction
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

    // Executing the transaction
    let result = svm.send_transaction(tx);

    println!("Swap result for zero amount: {:#?}", result);

    // Validate balances from the user's wallet
    let x_after_swap = get_token_balance(svm, &user_x_ata);
    let y_after_swap = get_token_balance(svm, &user_y_ata);

    // Token balance from the pool
    let x_vault_after_swap = get_token_balance(svm, &ctx.vault_x_ata);
    let y_vault_after_swap = get_token_balance(svm, &ctx.vault_y_ata);

    println!("User ata balance for X after zero swap: {}", x_after_swap);
    println!("User ata balance for Y after zero swap(minus swap fees): {}", y_after_swap);
    println!("=============================================================");
    println!("The balance in the vault x after zero swap is {}", x_vault_after_swap);
    println!("The balance in the vault y after zero swap is {}", y_vault_after_swap);
    println!("Zero swap correctly rejected");
    println!("=============================================================");

    // Assert failure
    assert!(result.is_err());
}

/// Protecting users from bad pricing and front running
pub fn slippage_protected_swap(ctx: &mut AmmTestContext, swap_amount: u64, slippage: u64, target_token: u8) {
    let svm = &mut ctx.svm;

    println!("================ SLIPPAGE PROTECTION TEST ================");
    println!("Perfoming a slippage protected token swap");

    // Advance the slot so setting up TXs (ATA creation/Minting) are unique
    //let clock: Clock = svm.get_sysvar();
    //let current_slot = clock.slot;
    //svm.warp_to_slot(current_slot + 1); // Warps the clock to specified slot

    let user = &ctx.initializer;
    svm.airdrop(&user.pubkey(), 1_000_000_000_000);
    let program_id = ctx.program_id;

    // Create User ATAs for storing x and y tokens
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

    // Mint tokens to user the token to swap, here, x
    mint_tokens(
        svm,
        user,
        &ctx.mint_x,
        &user_x_ata,
        1_000_000,
    );

    // Build Swap Instruction with zero amount
    let amount: u64 = swap_amount; // critical test condition, amount is zero.
    let min: u64 = slippage;
    let expiration: i64 = 1_800_000_000;
    let is_x: u8 = target_token;

    let mut data = Vec::with_capacity(26);
    data.push(3u8); // Swap discriminator
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&min.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());
    data.push(is_x);

    // Vault balances before swap.
    let x_vault_before_swap = get_token_balance(svm, &ctx.vault_x_ata);
    let y_vault_before_swap = get_token_balance(svm, &ctx.vault_y_ata);
    println!("The balance in the vault x before slippage swap is {}", x_vault_before_swap);
    println!("The balance in the vault y before slippage swap is {}", y_vault_before_swap);
    println!("================================================");
    // token balances before swaps
    let x_before_swap = get_token_balance(svm, &user_x_ata);
    let y_before_swap = get_token_balance(svm, &user_y_ata);

    println!("User ata balance for X before slippage swap: {}", x_before_swap);
    println!("User ata balance for Y before slippage swap: {}", y_before_swap);
    println!("The amount of token X to be slippage swapped for y: {}", amount);
    println!("The fee charge is {} basis points", ctx.fee);
    println!("================================================");

    // Building the Instruction
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

    // Executing the transaction
    let result = svm.send_transaction(tx);

    println!("Slippage test result is: {:#?}", result);
    
    // Validate balances from the user's wallet
    let x_after_swap = get_token_balance(svm, &user_x_ata);
    let y_after_swap = get_token_balance(svm, &user_y_ata);

    // Token balance from the pool
    let x_vault_after_swap = get_token_balance(svm, &ctx.vault_x_ata);
    let y_vault_after_swap = get_token_balance(svm, &ctx.vault_y_ata);

    println!("User ata balance for X after slippage swap: {}", x_after_swap);
    println!("User ata balance for Y after slippage swap(minus swap fees): {}", y_after_swap);
    println!("=============================================================");
    println!("The balance in the vault x after slippage swap is {}", x_vault_after_swap);
    println!("The balance in the vault y after slippage swap is {}", y_vault_after_swap);
    println!("Slippage test correctly failed");
    println!("=============================================================");

    // Assert failure
    assert!(result.is_err());
}
