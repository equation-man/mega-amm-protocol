//! Testing amm program initialization.
//! Testing withdrawal logic.
#![allow(warnings)]
use litesvm::LiteSVM;
use solana_address::Address;
use solana_message::Message;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;
use pinocchio_token;
use pinocchio_token::state::Mint;
use mega_amm_protocol::config::Config;
use solana_sdk::{
    pubkey::Pubkey, account::Account,
    instruction::{AccountMeta, Instruction},
};
use solana_system_interface::{
    program::ID as SYSTEM_PROGRAM_ID,
};

use crate::common::context::{DepositTestContext, AmmTestContext};
use crate::common::litesvm_deposit_tests::deposit_liquidity;
#[path="./litesvm_setup.rs"]
mod litesvm_setup;
use litesvm_setup::{get_token_balance};


pub fn withdraw_liquidity(ctx: &mut AmmTestContext, deposit: &DepositTestContext) {

    let mut withdraw_ix_data = vec![2u8];
    withdraw_ix_data.extend_from_slice(&5_00u64.to_le_bytes()); // amount u64
    withdraw_ix_data.extend_from_slice(&1_00u64.to_le_bytes()); // min_x u64
    withdraw_ix_data.extend_from_slice(&1_00u64.to_le_bytes()); // min_y u64
    withdraw_ix_data.extend_from_slice(&1_700_000_000i64.to_le_bytes()); // expiration.

    let withdraw_accounts = vec![
        AccountMeta::new(deposit.user.pubkey(), true),
        AccountMeta::new(ctx.lp_mint_pda, false),
        AccountMeta::new(ctx.vault_x_ata, false),
        AccountMeta::new(ctx.vault_y_ata, false),
        AccountMeta::new(deposit.user_x_ata, false),
        AccountMeta::new(deposit.user_y_ata, false),
        AccountMeta::new(deposit.user_lp_ata, false),
        AccountMeta::new(ctx.config_pda, false),
        AccountMeta::new_readonly(pinocchio_token::ID, false),
    ];

    let withdraw_ix = Instruction::new_with_bytes(
        ctx.program_id,
        &withdraw_ix_data,
        withdraw_accounts,
    );

    let tx = Transaction::new(
        &[&deposit.user],
        Message::new(&[withdraw_ix], Some(&deposit.user.pubkey())),
        ctx.svm.latest_blockhash(),
    );

    let withdraw_res = ctx.svm.send_transaction(tx);
    //println!("The withdraw res is {:#?}", withdraw_res);
    println!("The amount withdrawn is: {}", 500);
    let vault_x_balance = get_token_balance(&ctx.svm, &ctx.vault_x_ata);
    let vault_y_balance = get_token_balance(&ctx.svm, &ctx.vault_y_ata);
    println!("The vault x balance after pool withdrawal: {}", vault_x_balance);
    println!("The vault y balance after pool withdrawal: {}", vault_y_balance);
}
