use std::fs;
use once_cell::sync::Lazy;
use serde::Deserialize;

pub mod bot_config;
pub mod toml_config;

pub use bot_config::*;
pub use toml_config::*;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub mode: ModeConfig,
    pub wallet_config: WalletCredentialConfig,
    pub relayer_config: RelayerConfig,
    pub connection_config: ConnectionConfig,
    pub sell_setting: SellSetting,
    pub monitor_setting: MonitorConfig,
    pub buy_setting: BuySetting,
    pub slippage_config: SlippageConfig,
    pub fee_config: FeeConfig,
    pub filter_setting: FilterSetting,
    pub target_wallets: TargetWallets,
    pub nonce_config: NonceConfig,
}


pub static CONFIG: Lazy<Config> = Lazy::new(||{
    let content = fs::read_to_string("Config.toml").expect("Failed to read config.toml file");
    toml::from_str(&content).expect("Failed to parse config file.")
});