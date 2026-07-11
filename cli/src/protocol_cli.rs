//! Commandline interface for calling the protocol's instructions.
use clap::{Args, Parser, Subcommand};
//use crate::protocol_instructions::*;
use solana_sdk::{
    signature::{Keypair, Signer, read_keypair_file},
    pubkey::Pubkey,
    transaction::Transaction,
    instruction::{AccountMeta, Instruction},
    hash::Hash,
};
use shellexpand;
use spl_token;
use solana_client::rpc_client::{RpcClient};
use solana_commitment_config::CommitmentConfig;
use spl_associated_token_account::{
    instruction::create_associated_token_account,
    get_associated_token_address,
};
use std::path::Path;
use megaswap_protocol::instructions::initialize::{
    InitializeAccounts, InitializeInstructionData,
};

const USAGE: &str = "
MEGASWAP TRADING TERMINAL:
**************************************************
Summary and usage of available commands.
**************************************************
1. Deploying capital.
    megaswap-cli deposit --token <TOKEN TO DEPOSIT> --amount <AMOUNT TO DEPOSIT>
2. Withdrawing funds.
    megaswap-cli withdraw --token <TOKEN TO WITHDRAW> --amount <AMOUNT TO WITHDRAW>
3. Swaping or Trading.
    megaswap-cli swap --deposit --token <TOKEN TO DEPOSIT> --amount <AMOUNT TO SWAP>
";

#[derive(Parser, Debug)]
#[command(name="megaswap-cli")]
#[command(author="Built by The Equation Man")]
#[command(version="1.0")]
#[command(about="MegaSwap Trade Terminal.")]
#[command(after_help=USAGE)]
pub struct Cli {
    // sub commands to execute
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // Argument for initializing the protocol.
    Exec(InitArgs),
    // Deploying capital
    Deposit(DepositArgs),
    // Withdrawing capital
    Withdraw(WithdrawArgs),
    // Swapping or trading the token.
    Swap(SwapArgs),
    // Argument for initializing trades.
    Trade(TradeInitArgs),
}

#[derive(Args, Debug)]
pub struct InitArgs {
    init: String,
}

#[derive(Args, Debug)]
pub struct TradeInitArgs {
    init: String
}

#[derive(Args, Debug)]
pub struct DepositArgs {
    #[arg(short, long)]
    token: String,
    #[arg(short, long)]
    amount: u64,
}

#[derive(Args, Debug)]
pub struct WithdrawArgs {
    #[arg(short, long)]
    token: String,
    #[arg(short, long)]
    amount: u64,
}

#[derive(Args, Debug)]
pub struct SwapArgs {
    #[arg(short, long)]
    deposit: String,
    #[arg(short, long)]
    amount: u64,
}

//#[derive(Debug, Clone, Copy)]
//pub struct TradeEnvSetup {
//    mint_x: Pubkey,
//    mint_y: Pubkey,
//    lp_mint: Pubkey,
//}

fn load_or_create_wallet(path: &str) -> anyhow::Result<Keypair> {
    let path_obj = Path::new(path);

    if path_obj.exists() {
        println!("Wallet detected loading...");
        // Load path if it exists
        Ok(
            read_keypair_file(path).map_err(|e| anyhow::anyhow!("{}", e))?
        )
    } else {
        let keypair = Keypair::new();

        if let Some(parent) = path_obj.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Safe changes to disk.
        let bytes = keypair.to_bytes();
        println!("Wallet not found creating...");
        std::fs::write(path, serde_json::to_string(&bytes.to_vec())?)?;
        println!("Wallet created {}", path);

        Ok(keypair)
    }
}

pub fn ensure_protocol_ready(seed: u64, fee: u16) -> anyhow::Result<()> {
    println!("Ensuring the protocol is ready...");
    let program_id = Pubkey::from(megaswap_protocol::ID);
    let mint_x_keypair = load_or_create_wallet("wallets/mint_x.json")?;
    let mint_y_keypair = load_or_create_wallet("wallets/mint_y.json")?;
    let (config_pda, config_bump) = Pubkey::find_program_address(
        &[b"config", &seed.to_le_bytes(), mint_x_keypair.pubkey().as_ref(), mint_y_keypair.pubkey().as_ref()], &program_id,
    );
    let (mint_lp, lp_bump) = Pubkey::find_program_address(
        &[b"lp_mint", config_pda.as_ref()], &program_id
    );
    let rpc_client = RpcClient::new(
        "https://api.devnet.solana.com".to_string(),
    );
    let account = rpc_client.get_account_with_commitment(
        &config_pda, CommitmentConfig::confirmed(),
    ).map_err(|e| anyhow::anyhow!("{}", e))?;
    let recent_blockhash: Hash = rpc_client.get_latest_blockhash()?;

    match account.value {
        Some(account) => {
            // The config account is already initialized.
            println!("Pool already exists.");
            println!("The config account is {:?}", account);
        }
        None => {
            let initializer = read_keypair_file(
                shellexpand::tilde("~/.config/solana/id.json").to_string()
            ).map_err(|e| anyhow::anyhow!("{}", e))?;

            // Computing the deterministic ATA. Token vaults held by PDAs
            let vault_x_ata = get_associated_token_address(
                &config_pda, &mint_x_keypair.pubkey()
            );
            let vault_y_ata = get_associated_token_address(
                &config_pda, &mint_y_keypair.pubkey()
            );
            let initialize_data = InitializeInstructionData {
                seed, fee, mint_x: mint_x_keypair.pubkey().to_bytes(),
                mint_y: mint_y_keypair.pubkey().to_bytes(),
                config_bump: [config_bump],
                lp_mint_decimals: 6,
                lp_bump: [lp_bump],
                authority: initializer.pubkey().to_bytes()
            };
            let accounts = vec![
                AccountMeta::new(initializer.pubkey(), true),
                AccountMeta::new(config_pda, false),
                AccountMeta::new(vault_x_ata, false),
                AccountMeta::new(vault_y_ata, false),
                AccountMeta::new_readonly(mint_x_keypair.pubkey(), false),
                AccountMeta::new_readonly(mint_y_keypair.pubkey(), false),
                AccountMeta::new(mint_lp, false),
                AccountMeta::new_readonly(spl_associated_token_account::id(), false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ];
            let init_ix = Instruction { program_id, accounts, instruction_data };
            let init_tx = Transaction::new_signed_with_payer(
                &[init_ix], Some(&initializer.pubkey()), &[initializer]], recent_blockhash
            );
            rpc_client.send_and_confirm_transaction(&init_tx)?;
            println!("Protocol initialized successfully...");
        }
    }
    Ok(())
}

pub fn process_cli() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Exec(args) => {
            match args.init.as_str() {
                "initializer" => {
                    println!("Calling cli initialize command arg is, {}", args.init);
                    let _ = ensure_protocol_ready(42, 200);
                    //initialize_proto();
                },
                _ => eprintln!("Error Initializing the protocol")
            }
        },
        Commands::Trade(args) => {
            println!("Initializing trading environment {:?}", args);
        },
        Commands::Deposit(args) => {
            println!("The deposit arg is {:?}", args);
        },
        Commands::Withdraw(args) => {
            println!("Withdrawing liquidity {:?}", args);
        },
        Commands::Swap(args) => {
            println!("Swapping or trading liquidity {:?}", args);
        }
    }
}
