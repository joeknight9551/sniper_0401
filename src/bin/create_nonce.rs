/// Utility to create a durable nonce account on Solana mainnet.
///
/// Usage:
///   cargo run --bin create_nonce
///
/// It reads the wallet private key from Config.toml,
/// creates a new nonce account on-chain, and prints the
/// nonce_account and nonce_authority_key values to paste into Config.toml.

use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::LAMPORTS_PER_SOL,
    nonce,
    pubkey::Pubkey,
    signature::Signer,
    signer::keypair::Keypair,
    system_instruction,
    transaction::Transaction,
};
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct Config {
    wallet_config: WalletConfig,
    connection_config: ConnectionConfig,
}

#[derive(Deserialize)]
struct WalletConfig {
    private_key: String,
}

#[derive(Deserialize)]
struct ConnectionConfig {
    rpc_endpoint: String,
}

fn main() {
    println!("=== Solana Durable Nonce Account Creator ===\n");

    // 1. Load Config.toml
    let config_str = fs::read_to_string("Config.toml")
        .expect("Failed to read Config.toml. Make sure it exists in the project root.");
    let config: Config = toml::from_str(&config_str)
        .expect("Failed to parse Config.toml");

    // 2. Create wallet keypair from private key
    let wallet = Keypair::from_base58_string(&config.wallet_config.private_key);
    println!("Wallet public key: {}", wallet.pubkey());

    // 3. Connect to RPC
    let rpc = RpcClient::new_with_commitment(
        config.connection_config.rpc_endpoint.clone(),
        CommitmentConfig::confirmed(),
    );

    // 4. Check wallet balance
    let balance = rpc.get_balance(&wallet.pubkey())
        .expect("Failed to get wallet balance");
    let balance_sol = balance as f64 / LAMPORTS_PER_SOL as f64;
    println!("Wallet balance: {:.6} SOL", balance_sol);

    let rent = rpc
        .get_minimum_balance_for_rent_exemption(nonce::State::size())
        .expect("Failed to get rent exemption");
    let rent_sol = rent as f64 / LAMPORTS_PER_SOL as f64;
    println!("Nonce account rent: {:.6} SOL", rent_sol);

    if balance < rent + 5000 {
        eprintln!(
            "\nERROR: Insufficient balance. Need at least {:.6} SOL (rent + tx fee).",
            rent_sol + 0.000005
        );
        std::process::exit(1);
    }

    // 5. Generate a new keypair for the nonce account
    let nonce_account_keypair = Keypair::new();
    let nonce_pubkey = nonce_account_keypair.pubkey();
    println!("\nNew nonce account public key: {}", nonce_pubkey);

    // 6. Build the transaction to create the nonce account
    //    The wallet is both the funder and the nonce authority.
    let create_nonce_ixs = system_instruction::create_nonce_account(
        &wallet.pubkey(),          // funder
        &nonce_pubkey,             // new nonce account
        &wallet.pubkey(),          // nonce authority (same as wallet)
        rent,                      // lamports for rent exemption
    );

    let recent_blockhash = rpc.get_latest_blockhash()
        .expect("Failed to get recent blockhash");

    let tx = Transaction::new_signed_with_payer(
        &create_nonce_ixs,
        Some(&wallet.pubkey()),
        &[&wallet, &nonce_account_keypair],
        recent_blockhash,
    );

    // 7. Send and confirm the transaction
    println!("\nSending transaction to create nonce account...");
    match rpc.send_and_confirm_transaction(&tx) {
        Ok(sig) => {
            println!("Transaction confirmed! Signature: {}", sig);
        }
        Err(e) => {
            eprintln!("Transaction failed: {:?}", e);
            std::process::exit(1);
        }
    }

    // 8. Print the values to paste into Config.toml
    println!("\n========================================");
    println!("  SUCCESS! Nonce account created.");
    println!("========================================\n");
    println!("Paste these values into your Config.toml under [nonce_config]:\n");
    println!("nonce_account = \"{}\"", nonce_pubkey);
    println!("nonce_authority_key = \"{}\"", config.wallet_config.private_key);
    println!("\n(nonce_authority_key is your wallet private key since");
    println!(" the wallet was set as the nonce authority)\n");

    // 9. Verify the nonce account was created
    println!("Verifying nonce account...");
    match rpc.get_account(&nonce_pubkey) {
        Ok(account) => {
            match bincode::deserialize::<solana_sdk::nonce::state::Versions>(&account.data) {
                Ok(versions) => {
                    let state = versions.state();
                    match state {
                        solana_sdk::nonce::State::Initialized(data) => {
                            println!("Nonce hash: {}", data.blockhash());
                            println!("Authority:  {}", data.authority);
                            println!("\nNonce account is ready to use!");
                        }
                        _ => {
                            eprintln!("Nonce account exists but is not initialized.");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to deserialize nonce account: {:?}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to verify nonce account: {:?}", e);
        }
    }
}
