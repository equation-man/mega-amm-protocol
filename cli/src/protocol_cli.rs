//! Commandline interface for calling the protocol's instructions.
use clap::{Args, Parser, Subcommand};
//use crate::protocol_instructions::*;
use solana_sdk::{
    signature::{Keypair, Signer, read_keypair_file},
    pubkey::Pubkey,
};
use shellexpand;
use solana_client::rpc_client::{RpcClient};
use solana_commitment_config::CommitmentConfig;
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

pub fn preparing_trade_artifacts(seed: u64) -> anyhow::Result<()> {
    println!("Preparing the trading environment");
    let program_id = Pubkey::from(megaswap_protocol::ID);
    let trader = load_or_create_wallet("wallets/trader.json")?;
    println!("The trader wallet is  {}", trader.pubkey());
    let mint_x = Keypair::new();
    let mint_y = Keypair::new();
    let (config_pda, _) = Pubkey::find_program_address(
        &[b"config", &seed.to_le_bytes(), mint_x.pubkey().as_ref(), mint_y.pubkey().as_ref()], &program_id,
    );
    let rpc_client = RpcClient::new(
        "https://api.devnet.solana.com".to_string(),
    );
    let account = rpc_client.get_account_with_commitment(
        &config_pda, CommitmentConfig::confirmed(),
    ).map_err(|e| anyhow::anyhow!("{}", e))?;
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
            // Config missing, protocol not yet initialized
            println!("Pool not initialized. Continuing...");
            println!("The initializer is {:?}", initializer);
        }
    }
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
    Ok(())
}

pub fn process_cli() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Exec(args) => {
            match args.init.as_str() {
                "initializer" => {
                    println!("Calling cli initialize command arg is, {}", args.init);
                    let _ = preparing_trade_artifacts(42);
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
