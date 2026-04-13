use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ModeConfig {
    pub is_dev_mode: bool,
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
pub struct MonitorConfig {
    pub stop_no_activity_token_monitoring: bool,
    pub no_activity_time: i64,
}

#[derive(Debug, Deserialize)]
pub struct BuySetting {
    pub buy_amount_sol: f64,
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
pub struct NonceConfig {
    pub use_nonce: bool,
    pub nonce_account: String,
    pub nonce_authority_key: String,
}

#[derive(Debug, Deserialize)]
pub struct SellSetting {
    pub tp1_multiplier: f64,
    pub tp1_sell_pct: f64,
    pub tp2_multiplier: f64,
    pub tp2_sell_pct: f64,
    pub tp3_multiplier: f64,
    pub tp3_sell_pct: f64,
    pub stop_loss_multiplier: f64,
    pub trailing_stop_multiplier: f64,
}

