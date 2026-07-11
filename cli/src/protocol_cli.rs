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
use std::str::FromStr;
use std::fs;
use toml;
use serde::Deserialize;
use solana_client::rpc_client::{RpcClient};
use solana_commitment_config::CommitmentConfig;
use solana_system_interface::instruction as system_instruction;
use spl_token::instruction::initialize_mint;
use solana_program::program_pack::Pack;
use spl_associated_token_account::{
    get_associated_token_address,
};
use std::path::Path;

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

const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub megaswap_protocol_program_id: String,
    pub rpc_url: String,
}
impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let contents = fs::read_to_string("./cli/Config.toml")?;

        Ok(toml::from_str(&contents)?)
    }
}

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

fn load_or_create_wallet(path: &str) -> anyhow::Result<Keypair> {
    let path_obj = Path::new(path);

    if path_obj.exists() {
        println!("Playground wallet detected loading...");
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
        println!("Playground wallet not found creating...");
        std::fs::write(path, serde_json::to_string(&bytes.to_vec())?)?;
        println!("Your playground wallets created at {}", path);

        Ok(keypair)
    }
}

pub fn ensure_protocol_ready(seed: u64, fee: u16) -> anyhow::Result<()> {
    println!("Ensuring the protocol is ready...");
    let config = Config::load()?;
    let program_id = Pubkey::from_str(&config.megaswap_protocol_program_id)?;
    let (config_pda, config_bump) = Pubkey::find_program_address(
        &[b"config"], &program_id,
    );
    let (mint_lp, lp_bump) = Pubkey::find_program_address(
        &[b"lp_mint", config_pda.as_ref()], &program_id
    );
    let rpc_client = RpcClient::new(config.rpc_url);
    let account = rpc_client.get_account_with_commitment(
        &config_pda, CommitmentConfig::confirmed(),
    ).map_err(|e| anyhow::anyhow!("{}", e))?;
    println!("Solana rpc result is {:?}", &account);

    match account.value {
        Some(account) => {
            // The config account is already initialized.
            println!("Pool already exists.");
            println!("The config account is {:?}", account);
        }
        None => {
            println!("Protocol no yet initialized! Initializing...");
            let initializer = read_keypair_file(
                shellexpand::tilde("~/.config/solana/id.json").to_string()
            ).map_err(|e| anyhow::anyhow!("{}", e))?;
            let recent_blockhash: Hash = rpc_client.get_latest_blockhash()?;

            // Preparing the trade token mints.
            // Creating accounts owned by the token program
            let rent = rpc_client.get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
            let mint_x_keypair = load_or_create_wallet("wallets/mint_x.json")?;
            let create_mint_x = system_instruction::create_account(
                &initializer.pubkey(), &mint_x_keypair.pubkey(), rent,
                spl_token::state::Mint::LEN as u64, &spl_token::id(),
            );
            let mint_y_keypair = load_or_create_wallet("wallets/mint_y.json")?;
            let create_mint_y = system_instruction::create_account(
                &initializer.pubkey(), &mint_y_keypair.pubkey(), rent,
                spl_token::state::Mint::LEN as u64, &spl_token::id()
            );
            // Initializing the token mints.
            let init_mint_x = initialize_mint(
                &spl_token::id(), &mint_x_keypair.pubkey(), &initializer.pubkey(), None, 6,
            )?;
            let init_mint_y = initialize_mint(
                &spl_token::id(), &mint_y_keypair.pubkey(), &initializer.pubkey(), None, 6
            )?;

            // Computing the deterministic ATA. Token vaults held by PDAs
            let vault_x_ata = get_associated_token_address(
                &config_pda, &mint_x_keypair.pubkey()
            );
            let vault_y_ata = get_associated_token_address(
                &config_pda, &mint_y_keypair.pubkey()
            );
            let mut ix_data = vec![0u8];
            ix_data.extend_from_slice(&seed.to_le_bytes());
            ix_data.extend_from_slice(&fee.to_le_bytes());
            ix_data.extend_from_slice(&mint_x_keypair.pubkey().to_bytes());
            ix_data.extend_from_slice(&mint_y_keypair.pubkey().to_bytes());
            ix_data.push(config_bump);
            ix_data.push(6u8);
            ix_data.push(lp_bump);
            ix_data.extend_from_slice(&initializer.pubkey().to_bytes());

            let accounts = vec![
                AccountMeta::new(initializer.pubkey(), true),
                AccountMeta::new(vault_x_ata, false),
                AccountMeta::new(vault_y_ata, false),
                AccountMeta::new_readonly(mint_x_keypair.pubkey(), false),
                AccountMeta::new_readonly(mint_y_keypair.pubkey(), false),
                AccountMeta::new(mint_lp, false),
                AccountMeta::new(config_pda, false),
                AccountMeta::new_readonly(spl_associated_token_account::id(), false),
                AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ];
            let init_ix = Instruction {
                program_id: program_id,
                accounts: accounts,
                data: ix_data
            };
            // Constructing the transaction
            let init_tx = Transaction::new_signed_with_payer(
                &[
                    create_mint_x,
                    init_mint_x,
                    create_mint_y,
                    init_mint_y,
                    init_ix
                ],
                Some(&initializer.pubkey()),
                &[&initializer, &mint_x_keypair, &mint_y_keypair],
                recent_blockhash
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
                    //println!("Calling cli initialize command arg is, {}", args.init);
                    let proto_ready_res = ensure_protocol_ready(42, 200);
                    println!("Protocol readiness {:?}", proto_ready_res);
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
