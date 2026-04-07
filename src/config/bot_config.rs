use crate::*;
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use reqwest::Client;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    commitment_config::CommitmentLevel,
    pubkey::Pubkey,
    signer::{Signer, keypair::Keypair},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

pub static WALLET_PUB_KEY: Lazy<Pubkey> = Lazy::new(|| {
    let wallet: Keypair = Keypair::from_base58_string(&CONFIG.wallet_config.private_key);
    wallet.pubkey()
});

pub static WALLET_KEYPAIR: Lazy<Keypair> = Lazy::new(|| {
    let wallet: Keypair = Keypair::from_base58_string(&CONFIG.wallet_config.private_key);
    wallet
});

pub static TARGET_WALLETS: Lazy<Vec<String>> =
    Lazy::new(|| CONFIG.target_wallets.target_wallets.clone());

pub static CONFIRM_SERVICE: Lazy<String> =
    Lazy::new(|| CONFIG.relayer_config.confirm_service.clone());

pub static JITO_API_KEY: Lazy<String> = Lazy::new(|| CONFIG.relayer_config.jito_api_key.clone());

pub static NOZOMI_API_KEY: Lazy<String> =
    Lazy::new(|| CONFIG.relayer_config.nozomi_api_key.clone());

pub static ZERO_SLOT_API_KEY: Lazy<String> =
    Lazy::new(|| CONFIG.relayer_config.zero_slot_key.clone());

pub static ASTRALANE_ENDPOINT: Lazy<String> =
    Lazy::new(|| CONFIG.relayer_config.astralane.clone());

pub static RPC_ENDPOINT: Lazy<String> = Lazy::new(|| CONFIG.connection_config.rpc_endpoint.clone());
pub static RPC_CLINET: Lazy<Arc<RpcClient>> = Lazy::new(|| {
    Arc::new(RpcClient::new_with_commitment(
        CONFIG.connection_config.rpc_endpoint.clone(),
        CommitmentConfig {
            commitment: CommitmentLevel::Processed,
        },
    ))
});

lazy_static! {
    pub static ref AUTO_TURN_OFF: AtomicBool = AtomicBool::new(false);
    /// Global lock: true = currently holding a token, skip new buys
    pub static ref IS_HOLDING_POSITION: AtomicBool = AtomicBool::new(false);
    /// Track consecutive losses. When this reaches 2, skip the next buy.
    pub static ref CONSECUTIVE_LOSSES: AtomicU32 = AtomicU32::new(0);
    /// When true, skip the next matching token, then reset.
    pub static ref SKIP_NEXT_BUY: AtomicBool = AtomicBool::new(false);
    /// Set to true when wallet tracking confirms the distribution pattern.
    pub static ref WALLET_TRACKING_CONFIRMED: AtomicBool = AtomicBool::new(false);
}

pub static RPC_ENDPOINTL: Lazy<String> =
    Lazy::new(|| CONFIG.connection_config.rpc_endpoint.clone());
pub static RPC_CLIENT: Lazy<Arc<RpcClient>> = Lazy::new(|| {
    Arc::new(RpcClient::new_with_commitment(
        CONFIG.connection_config.rpc_endpoint.clone(),
        CommitmentConfig {
            commitment: CommitmentLevel::Processed,
        },
    ))
});

pub static HTTP_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| Arc::new(Client::new()));

pub static GRPC_ENDPOINT: Lazy<String> =
    Lazy::new(|| CONFIG.connection_config.grpc_endpoint.clone());
pub static GRPC_TOKEN: Lazy<String> = Lazy::new(|| CONFIG.connection_config.grpc_token.clone());

pub static STOP_NO_ACTIVITY_TOKEN_MONITORING: Lazy<bool> =
    Lazy::new(|| CONFIG.monitor_setting.stop_no_activity_token_monitoring);

pub static NO_ACTIVITY_TIME: Lazy<i64> = Lazy::new(|| CONFIG.monitor_setting.no_activity_time);

pub static SLIPPAGE: Lazy<f64> =
    Lazy::new(|| 1.0 + CONFIG.slippage_config.slippage_percent as f64 / 100.0);

pub static VOLUME_FILTER: Lazy<bool> = Lazy::new(|| CONFIG.filter_setting.volume_filter);
pub static MIN_VOLUME_LIMIT_SOL: Lazy<i32> =
    Lazy::new(|| CONFIG.filter_setting.min_volume_limit_sol);

pub static MARKET_CAP_FILTER: Lazy<bool> = Lazy::new(|| CONFIG.filter_setting.market_cap_filter);
pub static MIN_MARKET_CAP_LIMIT_SOL: Lazy<i32> =
    Lazy::new(|| CONFIG.filter_setting.min_market_cap_limit_sol);

pub async fn show_bot_settings() {
    println!("Initializing bot.");
    println!("Loding bot settings...");
    println!("RPC Endpoint: {}", *RPC_ENDPOINT);
    println!("gRPC Endpoint: {}", *GRPC_ENDPOINT);
    println!("Confirm Service: {}", *CONFIRM_SERVICE);
    println!("Checking bot status...");
    println!("Public Key: {:?}", *WALLET_PUB_KEY);
    if *USE_NONCE {
        println!("Nonce Mode: ENABLED");
        println!("Nonce Account: {:?}", *NONCE_PUBKEY);
    } else {
        println!("Nonce Mode: DISABLED (using recent blockhash)");
    }
    println!("Bot started!");
}

pub static DEV_MODE: Lazy<bool> = Lazy::new(|| CONFIG.mode.is_dev_mode);

pub static BUY_AMOUNT_SOL: Lazy<f64> = Lazy::new(|| CONFIG.buy_setting.buy_amount_sol);

pub static BUY_TX_COUNTER: Lazy<AtomicI32> =
    Lazy::new(|| AtomicI32::new(CONFIG.mode.buy_tx_counter));

pub static BUY_COUNTER: Lazy<AtomicI32> = Lazy::new(|| AtomicI32::new(1));

pub fn get_buy_tx_remain_counter() -> i32 {
    BUY_TX_COUNTER.load(Ordering::SeqCst)
}

pub fn get_buy_counter() -> i32 {
    BUY_COUNTER.load(Ordering::SeqCst)
}

pub fn decrease_buy_counter() {
    BUY_COUNTER.fetch_sub(1, Ordering::SeqCst);
}

pub fn increase_buy_counter() {
    BUY_COUNTER.fetch_add(1, Ordering::SeqCst);
}

pub fn decrease_buy_tx_remain_counter() {
    BUY_TX_COUNTER.fetch_sub(1, Ordering::SeqCst);
}

pub static PRIORITY_FEE: Lazy<(u64, u64, f64)> = Lazy::new(|| {
    let cu: u64 = CONFIG.fee_config.cu;
    let priority_fee_micro_lamport = CONFIG.fee_config.priority_fee_micro_lamport;
    let third_party_fee = CONFIG.fee_config.third_party_fee;

    (cu, priority_fee_micro_lamport, third_party_fee)
});

pub static USE_NONCE: Lazy<bool> = Lazy::new(|| {
    CONFIG.nonce_config.use_nonce
        && !CONFIG.nonce_config.nonce_account.is_empty()
        && !CONFIG.nonce_config.nonce_authority_key.is_empty()
});

pub static NONCE_PUBKEY: Lazy<Option<Pubkey>> = Lazy::new(|| {
    if *USE_NONCE {
        Some(
            CONFIG
                .nonce_config
                .nonce_account
                .parse::<Pubkey>()
                .expect("Invalid nonce_account pubkey in Config.toml"),
        )
    } else {
        None
    }
});

pub static NONCE_AUTHORITY: Lazy<Option<Keypair>> = Lazy::new(|| {
    if *USE_NONCE {
        Some(Keypair::from_base58_string(
            &CONFIG.nonce_config.nonce_authority_key,
        ))
    } else {
        None
    }
});
