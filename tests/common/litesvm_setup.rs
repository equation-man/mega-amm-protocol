//! Setting up accounts. Every test will call this.
//! The amm initialization test functions are here.
#![allow(warnings)]
use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
    sysvar::rent::Rent,
};
use pinocchio_token::instructions::InitializeMint2;
use solana_system_interface::program::ID as SYSTEM_PROGRAM_ID;
use solana_system_interface::instruction as system_instruction;
use solana_system_interface::instruction::create_account;
use spl_associated_token_account::ID as ATA_PROGRAM_ID;
use solana_program::program_pack::Pack; //Trait to enable Mint::LEN
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};
use spl_token::instruction as token_ix;
use spl_token::state::Mint;
use spl_token::state::Account as TokenAccount;
use spl_token::ID as TOKEN_PROGRAM_ID;
use spl_token::solana_program::program_option::COption;

use crate::common::context::{AmmTestContext};

pub fn setup_initialized_amm() -> AmmTestContext {
    let program_id = solana_sdk::pubkey!("2qcc8awwigDm897DxTNFwVgKpUpNZ3UbGHxoatBfmgLi");
    let bytes = include_bytes!("../../target/deploy/mega_amm_protocol.so");

    let rent = solana_sdk::sysvar::rent::Rent::default();
    let initializer = Keypair::new();
    let mut svm = LiteSVM::new();

    svm.add_program(program_id, bytes);
    svm.airdrop(&initializer.pubkey(), 10_000_000_000).unwrap();

    // ---- PDAs ----
    let seed: u64 = 42;
    let mint_x = create_test_mint(&mut svm, &initializer, &initializer.pubkey(), 6); 
    let mint_y = create_test_mint(&mut svm, &initializer, &initializer.pubkey(), 6); 

    let (config_pda, config_bump) = Pubkey::find_program_address(
        &[b"config", &seed.to_le_bytes(), mint_x.as_ref(), mint_y.as_ref()],
        &program_id,
    );
    let vault_x_ata = get_associated_token_address(&config_pda, &mint_x);
    let vault_y_ata = get_associated_token_address(&config_pda, &mint_y);

    let (lp_mint_pda, lp_bump) = Pubkey::find_program_address(
        &[b"lp_mint", config_pda.as_ref()],
        &program_id,
    );

    let authority = Pubkey::new_unique();
    let mut instruction_data = vec![0u8];
    // Seed
    instruction_data.extend_from_slice(&seed.to_le_bytes());
    // Fee
    instruction_data.extend_from_slice(&10u16.to_le_bytes());
    // mintx_x
    instruction_data.extend_from_slice(mint_x.as_ref());
    // mint_y
    instruction_data.extend_from_slice(mint_y.as_ref());
    // config_bump
    instruction_data.push(config_bump);
    // lp_mint_decimals
    instruction_data.push(9u8);
    // lp bump
    instruction_data.push(lp_bump);
    // AMM admin authority
    instruction_data.extend_from_slice(authority.as_ref());

    let accounts = vec![
        AccountMeta::new(initializer.pubkey(), true),

        AccountMeta::new(vault_x_ata, false),
        AccountMeta::new(vault_y_ata, false),

        AccountMeta::new(mint_x, false),
        AccountMeta::new(mint_y, false),

        AccountMeta::new(lp_mint_pda, false),
        AccountMeta::new(config_pda, false),

        AccountMeta::new(ATA_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(pinocchio_token::ID, false),
        AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false),
    ];

    let instruction = Instruction::new_with_bytes(
        program_id,
        &instruction_data,
        accounts,
    );

    let tx = Transaction::new(
        &[&initializer],
        Message::new(&[instruction], Some(&initializer.pubkey())),
        svm.latest_blockhash(),
    );

    let tx_init = svm.send_transaction(tx);
    //println!("The amm initialization is {:#?}", tx_init);
    println!("The amm fee at initialization is {}", 10);

    AmmTestContext {
        svm,
        program_id,
        initializer,
        seed,
        mint_x,
        mint_y,
        vault_x_ata,
        vault_y_ata,
        config_pda,
        lp_mint_pda,
    }
}

pub fn create_ata(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);

    let ix = create_associated_token_account(
        &payer.pubkey(),
        owner,
        mint,
        &spl_token::ID.into(),
    );

    let tx = Transaction::new(
        &[payer],
        Message::new(&[ix], Some(&payer.pubkey())),
        svm.latest_blockhash(),
    );

    let creating_ata = svm.send_transaction(tx);
    ata
}

pub fn build_deposit_ix_data(
    amount_x: u64, amount_y: u64, expiration: i64,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(1+8*3+8);

    data.push(1u8); // deposit instruction discriminator. Must match the onchain one.
    data.extend_from_slice(&amount_x.to_le_bytes());
    data.extend_from_slice(&amount_y.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());

    data
}

pub fn create_legacy_mint(
    svm: &mut LiteSVM, payer: &Keypair, mint: Pubkey, decimals: u8
) {
    let mint_len = Mint::LEN;

    svm.set_account(
        mint,
        Account {
            lamports: svm.minimum_balance_for_rent_exemption(mint_len),
            data: vec![0u8; mint_len],
            owner: pinocchio_token::ID.into(),
            executable: false,
            rent_epoch: 0,
        }
    );

    let ix = spl_token::instruction::initialize_mint2(
        &spl_token::ID, &mint, &payer.pubkey(), None, decimals,
    ).unwrap();

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );

    svm.send_transaction(tx).unwrap();
}

pub fn create_pool(svm: &mut LiteSVM, vault_x: &Pubkey, vault_y: &Pubkey) {
    let rent_exempt = 1_000_000; // Approximate lamport for rent exempt.
    for vault in &[vault_x, vault_y] {
        svm.set_account(
            **vault,
            Account {
                lamports: rent_exempt,
                data: vec![0u8; spl_token::state::Account::LEN],
                owner: spl_token::ID,
                executable: false,
                rent_epoch: 0,
            }
        );
    }
}


pub fn create_test_mint(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint_authority: &Pubkey,
    decimals: u8,
) -> Pubkey {
    let mint = Keypair::new();

    let rent = solana_sdk::sysvar::rent::Rent::default();
    let mint_space = Mint::LEN;
    let lamports = rent.minimum_balance(mint_space);

    // Create mint account
    let create_account_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        lamports,
        mint_space as u64,
        &spl_token::ID,
    );

    // Initialize mint
    let init_mint_ix = token_ix::initialize_mint(
        &spl_token::ID,
        &mint.pubkey(),
        mint_authority,
        None,
        decimals,
    ).unwrap();

    let tx = Transaction::new_signed_with_payer(
        &[create_account_ix, init_mint_ix],
        Some(&payer.pubkey()),
        &[payer, &mint],
        svm.latest_blockhash(),
    );

    svm.send_transaction(tx).unwrap();

    mint.pubkey()
}

pub fn create_pda_mint(
    svm: &mut LiteSVM,
    mint_pubkey: Pubkey,
    mint_authority: Pubkey,
    decimals: u8
) {
    let rent = Rent::default();
    let lamports = rent.minimum_balance(Mint::LEN);
    let mint_state = Mint {
        mint_authority: COption::Some(mint_authority),
        supply: 0,
        decimals,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let mut data = vec![0u8; Mint::LEN];
    Mint::pack(mint_state, &mut data).unwrap();

    svm.set_account(
        mint_pubkey,
        Account {
            lamports, data, owner: TOKEN_PROGRAM_ID,
            executable: false, rent_epoch: 0,
        },
    );
}

pub fn mint_tokens(
    svm: &mut LiteSVM, authority: &Keypair, mint: &Pubkey,
    destination_ata: &Pubkey, amount: u64,
) {
    let ix = token_ix::mint_to(
        &spl_token::ID, mint, destination_ata,
        &authority.pubkey(), &[], amount,
    ).unwrap();
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&authority.pubkey()),
        &[authority], // Both sign if different
        svm.latest_blockhash(),
    );

    svm.send_transaction(tx).unwrap();
}

pub fn get_token_balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let acc = svm.get_account(token_account).unwrap();
    let token_acc = TokenAccount::unpack(&acc.data).unwrap();
    token_acc.amount
}
