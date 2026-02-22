//! Testing amm depositing of funds.
#![allow(warnings)]
use litesvm::LiteSVM;
use solana_address::Address;
use pinocchio_token;
use pinocchio_token::state::Mint;
use mega_amm_protocol::config::Config;
use solana_sdk::{
    pubkey::Pubkey, account::Account,
    instruction::{AccountMeta, Instruction},
    message::Message, signature::{Keypair, Signer},
    transaction::Transaction,
    sysvar::clock,
};
use spl_associated_token_account::{
    get_associated_token_address,
};
use solana_system_interface::{
    program::ID as SYSTEM_PROGRAM_ID,
};

#[path="./litesvm_setup.rs"]
mod litesvm_setup;
use litesvm_setup::{
    create_ata, build_deposit_ix_data,
    create_legacy_mint, create_pool, create_pda_mint, mint_tokens,
    get_token_balance,
};
use crate::common::context::{DepositTestContext, AmmTestContext};


pub fn deposit_liquidity(ctx: &mut AmmTestContext) -> DepositTestContext {
    //let mut ctx = setup_initialized_amm();

    let user = Keypair::new();
    ctx.svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();

    // Create user ATAs on the client side.
    // Creating the test mints for x and y minting test tokens into them.
    let user_x_ata = create_ata(
        &mut ctx.svm, &user, &ctx.mint_x, &user.pubkey()
    );
    mint_tokens(&mut ctx.svm, &ctx.initializer, &ctx.mint_x, &user_x_ata, 1_000_000_000);
    //println!("The user x ata balance is {}", get_token_balance(&ctx.svm, &user_x_ata));
    let user_y_ata = create_ata(
        &mut ctx.svm, &user, &ctx.mint_y, &user.pubkey()
    );
    mint_tokens(&mut ctx.svm, &ctx.initializer, &ctx.mint_y, &user_y_ata, 1_000_000_000);
    //println!("The user y ata balance is {}", get_token_balance(&ctx.svm, &user_y_ata));

    // Here the mint is already available. Tokens will be minted here, for liquidity providers
    create_pda_mint(&mut ctx.svm, ctx.lp_mint_pda, ctx.lp_mint_pda, 6);
    let user_lp_ata = create_ata(
        &mut ctx.svm, &user, &ctx.lp_mint_pda, &user.pubkey(),
    );

    // Build deposit instruction.
    let deposit_ix_data = build_deposit_ix_data(
        1_000_000_000, 1_000_000_000, 1_000_000_000, i64::MAX,
    );

    let mut instruction_data = vec![1u8];
    //Amount
    instruction_data.extend_from_slice(
        &2_000_000_000_000_000_000u128.to_le_bytes()
    );
    // Max x. Slippage protection for token x.
    instruction_data.extend_from_slice(&1_000_000_000u64.to_le_bytes());
    // Max y. Slippage protection for token y.
    instruction_data.extend_from_slice(&2_000_000_000u64.to_le_bytes());
    // Expiration time.
    instruction_data.extend_from_slice(&1_700_000_000i64.to_le_bytes());

    let accounts = vec![
        AccountMeta::new(user.pubkey(), true),
        AccountMeta::new(ctx.config_pda, false),
        AccountMeta::new(ctx.lp_mint_pda, false),
        AccountMeta::new(user_lp_ata, false),
        AccountMeta::new(ctx.vault_x_ata, false),
        AccountMeta::new(ctx.vault_y_ata, false),
        AccountMeta::new(user_x_ata, false),
        AccountMeta::new(user_y_ata, false),
        AccountMeta::new_readonly(pinocchio_token::ID, false),
    ];

    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &deposit_ix_data,
        accounts,
    );

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );

    let res = ctx.svm.send_transaction(tx);
    //println!("The deposit result is {:#?}", res);
    
    let liquidity_pool = get_token_balance(&ctx.svm, &user_lp_ata);
    println!("The liquidity pool token balance is {}", liquidity_pool);
    println!("=======================================================");

    DepositTestContext {
        user, user_x_ata, user_y_ata, user_lp_ata,
    }
}
