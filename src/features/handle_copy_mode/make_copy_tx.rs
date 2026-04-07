use crate::*;
use colored::*;
use dashmap::DashMap;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use std::sync::atomic::Ordering;
use std::time::Instant;

pub async fn make_copy_tx(trade_token_data_map: &DashMap<Pubkey, TokenDatabaseSchema>) {
    for trade_token_data in trade_token_data_map.iter() {
        let mut token_data = trade_token_data.value().clone();

        if token_data.token_buy_now {
            // Only one position at a time
            if IS_HOLDING_POSITION.load(Ordering::SeqCst) {
                continue;
            }
            IS_HOLDING_POSITION.store(true, Ordering::SeqCst);

            token_data.token_buy_now = false;
            token_data.token_is_purchased = true;
            let _ = TOKEN_DB.upsert(token_data.token_mint, token_data.clone());

            // Always use the fixed buy_amount_sol from config
            let buy_lamports = *BUY_AMOUNT_SOL * 1e9;
            info!(
                "[CopyBuy] Sending {} lamports ({} SOL) for {}",
                buy_lamports as u64,
                *BUY_AMOUNT_SOL,
                token_data.pump_fun_swap_accounts.mint
            );

            let build_start = Instant::now();
            let mut ix: Vec<Instruction> = Vec::new();
            let create_ata_ix = token_data
                .pump_fun_swap_accounts
                .get_create_ata_idempotent_ix();
            let buy_ix = token_data
                .pump_fun_swap_accounts
                .get_buy_ix(buy_lamports, token_data.token_price);

            ix.push(create_ata_ix);
            ix.push(buy_ix);

            println!(
                "{}",
                format!(
                    "{}: {}",
                    "Building tx took:".blue(),
                    format_elapsed_time(build_start.elapsed()).blue()
                )
            );

            let tag = format!(
                "[CopyBuy]\t*Mint: {}\t*MC: {}\t*Amount: {} SOL",
                token_data.pump_fun_swap_accounts.mint,
                token_data.token_marketcap,
                *BUY_AMOUNT_SOL
            );
            info!("{}", tag);

            let ix_clone = ix.clone();
            let tag_clone = tag.clone();
            tokio::spawn(async move {
                let _ = confirm(ix_clone, tag_clone).await;
            });

            // No timeout sell — position is closed only when the target wallet sells.
        }
    }
}
