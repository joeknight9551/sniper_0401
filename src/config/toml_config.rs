use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ModeConfig {
    pub is_dev_mode: bool,
    pub buy_tx_counter: i32,
}

#[derive(Debug, Deserialize)]
pub struct WalletCredentialConfig {
    pub private_key:String,
}

#[derive(Debug, Deserialize)]
pub struct RelayerConfig {
    pub confirm_service: String,
    pub jito_api_key: String,
    pub nozomi_api_key: String,
    pub zero_slot_key: String,
    pub astralane: String,
}

#[derive(Debug, Deserialize)]
pub struct ConnectionConfig {
    pub rpc_endpoint: String,
    pub grpc_endpoint: String,
    pub grpc_token: String,
}

#[derive(Debug, Deserialize)]
pub struct SellSetting {
    pub take_profit_1: f64,
    pub take_profit_1_sell_percentage: f64,
    pub take_profit_2: f64,
    pub take_profit_2_sell_percentage: f64,
    pub take_profit_3: f64,
    pub take_profit_3_sell_percentage: f64,
    pub take_profit_4: f64,
    pub take_profit_4_sell_percentage: f64,
    pub take_profit_5: f64,
    pub take_profit_5_sell_percentage: f64,
    pub stop_loss: f64,
    pub trailing_1: f64,
    pub trailing_1_stop: f64,
    pub trailing_1_sell_percentage: f64,
    pub trailing_2: f64,
    pub trailing_2_stop: f64,
    pub trailing_2_sell_percentage: f64,
    pub trailing_3: f64,
    pub trailing_3_stop: f64,
    pub trailing_3_sell_percentage: f64,
    pub trailing_4: f64,
    pub trailing_4_stop: f64,
    pub trailing_4_sell_percentage: f64,
    pub trailing_5: f64,
    pub trailing_5_stop: f64,
    pub trailing_5_sell_percentage: f64,
}

#[derive(Debug, Deserialize)]
pub struct MonitorConfig {
    pub stop_no_activity_token_monitoring: bool,
    pub no_activity_time: i64,
}

#[derive(Debug, Deserialize)]
pub struct BuySetting {
    pub buy_amount_sol: f64,
    pub one_time_copy: bool,
}

#[derive(Debug, Deserialize)]
pub struct SlippageConfig {
    pub slippage_percent: u32,
}

#[derive(Debug, Deserialize)]
pub struct FeeConfig {
    pub cu: u64,
    pub priority_fee_micro_lamport: u64, 
    pub third_party_fee: f64,
}

#[derive(Debug, Deserialize)]
pub struct FilterSetting {
    pub volume_filter: bool,
    pub min_volume_limit_sol: i32,
    pub market_cap_filter: bool,
    pub min_market_cap_limit_sol: i32,
}

#[derive(Debug, Deserialize)]
pub struct TargetWallets {
    pub target_wallets: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NonceConfig {
    pub use_nonce: bool,
    pub nonce_account: String,
    pub nonce_authority_key: String,
}

#[derive(Debug, Deserialize)]
pub struct WalletTrackingConfig {
    pub enabled: bool,
    pub tracking_wallet: String,
    pub chain_transfer_min_balance_lamports: u64,
}