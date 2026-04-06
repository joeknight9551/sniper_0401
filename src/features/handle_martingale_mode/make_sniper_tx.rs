use crate::*;
use colored::*;
use dashmap::DashMap;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use std::time::Instant;
use tokio::time::{Duration, sleep};

pub async fn make_sniper_tx(trade_token_data_map: &DashMap<Pubkey, TokenDatabaseSchema>) {
    for trade_token_data in trade_token_data_map.iter() {
        let mut token_data = trade_token_data.value().clone();

        if token_data.token_buy_now {
            // Skip buy if in cooldown after 3 consecutive wins
            let skip = BUYS_TO_SKIP.load(std::sync::atomic::Ordering::SeqCst);
            if skip > 0 {
                BUYS_TO_SKIP.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                info!(
                    "[Streak] Skipping buy for {} — {} skip(s) remaining",
                    token_data.token_mint,
                    skip - 1
                );
                token_data.token_buy_now = false;
                let _ = TOKEN_DB.upsert(token_data.token_mint, token_data.clone());
                continue;
            }

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

            // Schedule timeout sell using per-pattern holding time.
            // If price hits take-profit before this, update_status will trigger the sell
            // and this timeout will find token_balance == 0 and skip.
            let sell_token_data = token_data.clone();
            let buy_price = token_data.token_price;
            let holding_time_secs = token_data.token_holding_time_secs;
            tokio::spawn(async move {
                sleep(Duration::from_secs(holding_time_secs)).await;

                // Re-read latest token data from DB
                if let Ok(Some(mut latest_data)) = TOKEN_DB.get(sell_token_data.token_mint) {
                    if latest_data.token_balance > 0
                        && latest_data.token_sell_status == TokenSellStatus::None
                    {
                        latest_data
                            .pump_fun_swap_accounts
                            .update_creator_vault(&latest_data.token_creator);

                        let sell_ix = latest_data
                            .pump_fun_swap_accounts
                            .get_sell_ix(latest_data.token_balance, latest_data.cashback_enabled);

                        let sell_tag = format!(
                            "[SELL]\t*{}s timeout\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
                            holding_time_secs,
                            latest_data.pump_fun_swap_accounts.mint,
                            latest_data.token_marketcap,
                            latest_data.token_balance,
                            buy_price,
                            latest_data.token_price,
                        );

                        info!(
                            "[SELL]\t*{}s timeout\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
                            holding_time_secs,
                            latest_data.pump_fun_swap_accounts.mint,
                            latest_data.token_marketcap,
                            latest_data.token_balance,
                            buy_price,
                            latest_data.token_price,
                        );

                        let _ = confirm(vec![sell_ix], sell_tag).await;

                        // Record trade outcome for win-streak tracking
                        let profitable = latest_data.token_price > buy_price;
                        record_trade_outcome(profitable);
                    }
                }
            });
        }
    }
}
