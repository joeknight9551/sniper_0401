use crate::*;
use colored::*;
use dashmap::DashMap;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use std::time::Instant;

pub async fn make_sniper_tx(trade_token_data_map: &DashMap<Pubkey, TokenDatabaseSchema>) {
    for trade_token_data in trade_token_data_map.iter() {
        let mut token_data = trade_token_data.value().clone();

        if token_data.token_buy_now {
            token_data.token_buy_now = false;
            token_data.token_is_purchased = true;
            let _ = TOKEN_DB.upsert(token_data.token_mint, token_data.clone());
            let sniper_buy_amount = *BUY_AMOUNT_SOL as f64 * 10f64.powi(9);
            info!(
                "[Buy Exact] Sending exact SOL amount: {} lamports ({} SOL)",
                sniper_buy_amount as u64,
                *BUY_AMOUNT_SOL
            );
            let build_tx_start = Instant::now();
            let mut ix: Vec<Instruction> = Vec::new();
            let create_ata_ix = token_data
                .pump_fun_swap_accounts
                .get_create_ata_idempotent_ix();
            let buy_ix = token_data
                .pump_fun_swap_accounts
                .get_buy_ix(sniper_buy_amount, token_data.token_price);

            ix.push(create_ata_ix);
            ix.push(buy_ix);

            let building_tx_time = build_tx_start.elapsed();
            println!(
                "{}",
                format!(
                    "{}: {}",
                    "Building tx took:".blue(),
                    format_elapsed_time(building_tx_time).blue()
                )
            );

            let tag = format!(
                "[Buy]\t*Mint: {}\t*MC: {}\t*Amount: {} SOL",
                token_data.pump_fun_swap_accounts.mint, token_data.token_marketcap, *BUY_AMOUNT_SOL
            );

            info!(
                "[Buy]\t*Mint: {}\t*MC: {}\t*Amount: {} SOL",
                token_data.pump_fun_swap_accounts.mint, token_data.token_marketcap, *BUY_AMOUNT_SOL
            );

            // Send the buy transaction
            let buy_ix_clone = ix.clone();
            let buy_tag_clone = tag.clone();
            tokio::spawn(async move {
                let _ = confirm(buy_ix_clone, buy_tag_clone).await;
            });
        }
    }
}
