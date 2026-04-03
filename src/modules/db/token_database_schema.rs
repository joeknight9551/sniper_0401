use colored::Colorize;
use solana_sdk::pubkey::Pubkey;

use crate::*;
use crate::{MintEvent, MintInstructionAccounts};

#[derive(Clone, Debug)]
pub struct TokenDatabaseSchema {
    pub token_mint: Pubkey,
    pub cashback_enabled: bool,
    pub token_creator: Pubkey,
    pub token_total_supply: u64,
    pub token_price: f64,
    pub token_is_purchased: bool,
    pub token_balance: u64,
    pub token_buying_point_price: f64,
    pub token_marketcap: f64,
    pub token_volume: Option<f64>,
    pub pump_fun_swap_accounts: PumpFunSwapAccounts,
    pub last_event: LastEvent,
    pub token_sell_status: TokenSellStatus,
    pub token_mint_timestamp: i64,
    pub token_buy_now: bool,
}

impl TokenDatabaseSchema {
    pub async fn new_from_mint(
        mint_event: MintEvent,
        mint_instruction_accounts: MintInstructionAccounts,
        tx_id: String,
    ) -> Option<Self> {
        info!(
            "[{}]\t\t\t*Mint: {}",
            "Mint".blue(),
            mint_event.mint.to_string(),
        );
        let initial_token_price = (mint_event.virtual_sol_reserves as f64 / 10f64.powi(9))
            / (mint_event.virtual_token_reserves as f64 / 10f64.powi(6));
        let initial_token_marketcap = initial_token_price * mint_event.token_total_supply as f64;

        let token_data = Self {
            token_mint: mint_event.mint,
            token_creator: mint_event.creator,
            token_total_supply: mint_event.token_total_supply / 10u64.pow(6),
            cashback_enabled: mint_event.cashback_enabled,
            token_balance: 0,
            token_price: initial_token_price,
            token_is_purchased: false,
            token_marketcap: initial_token_marketcap,
            token_volume: Some(0.0),
            token_buying_point_price: 0.0,
            pump_fun_swap_accounts: PumpFunSwapAccounts::from_mint(
                &mint_instruction_accounts,
                &mint_event,
            ),
            last_event: LastEvent {
                tx_hash: tx_id,
                last_tracked_event: TokenEvent::MintTokenEvent,
                last_activity_timestamp: mint_event.timestamp,
            },
            token_sell_status: TokenSellStatus::None,
            token_mint_timestamp: mint_event.timestamp,
            token_buy_now: false,
        };
        let _ = TOKEN_DB.upsert(mint_event.mint.clone(), token_data.clone());

        Some(token_data)
    }

}

#[derive(Debug, Clone)]
pub struct LastEvent {
    pub tx_hash: String,
    pub last_tracked_event: TokenEvent,
    pub last_activity_timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Copy)]
pub enum TokenEvent {
    MintTokenEvent,
    BuyTokenEvent,
    SellTokenEvent,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Copy)]
pub enum TokenSellStatus {
    None,
    SellTradeSubmitted,
}
