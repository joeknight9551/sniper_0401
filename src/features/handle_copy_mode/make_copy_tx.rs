use crate::*;
use dashmap::DashMap;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

/// Sell the full balance of `mint`.
/// Used by both the 4.8s timeout path and 180% TP path.
/// No-op if the position is already closed or a sell is already in flight.
pub fn copy_sell_token(mint: Pubkey, reason: String) {
    let token_data = match TOKEN_DB.get(mint).unwrap() {
        Some(data) => data,
        None => return,
    };

    if !token_data.token_is_purchased
        || token_data.token_balance == 0
        || token_data.token_sell_status != TokenSellStatus::None
    {
        return;
    }

    let mut sell_data = token_data.clone();
    sell_data.token_sell_status = TokenSellStatus::SellTradeSubmitted;
    sell_data
        .pump_fun_swap_accounts
        .update_creator_vault(&sell_data.token_creator);
    let _ = TOKEN_DB.upsert(mint, sell_data.clone());

    info!(
        "[CopySell]\t*{}\t*Mint: {}\t*Balance: {}",
        reason, sell_data.token_mint, sell_data.token_balance
    );

    tokio::spawn(async move {
        let sell_ix = sell_data
            .pump_fun_swap_accounts
            .get_sell_ix(sell_data.token_balance, sell_data.cashback_enabled);
        let sell_tag = format!(
            "[CopySell]\t*{}\t*Mint: {}\t*Amount: {}",
            reason, sell_data.token_mint, sell_data.token_balance
        );
        let _ = confirm(vec![sell_ix], sell_tag).await;
    });
}

pub async fn make_copy_tx(trade_token_data_map: &DashMap<Pubkey, TokenDatabaseSchema>) {
    for trade_token_data in trade_token_data_map.iter() {
        let mut token_data = trade_token_data.value().clone();

        if token_data.token_buy_now {
            token_data.token_buy_now = false;
            token_data.token_is_purchased = true;
            let _ = TOKEN_DB.upsert(token_data.token_mint, token_data.clone());

            let buy_lamports = *BUY_AMOUNT_SOL * 1e9;

            let mut ix: Vec<Instruction> = Vec::new();
            let create_ata_ix = token_data
                .pump_fun_swap_accounts
                .get_create_ata_idempotent_ix();
            let buy_ix = token_data
                .pump_fun_swap_accounts
                .get_buy_ix(buy_lamports, token_data.token_price);

            ix.push(create_ata_ix);
            ix.push(buy_ix);

            let tag = format!(
                "[CopyBuy]\t*Mint: {}\t*MC: {}\t*Amount: {} SOL",
                token_data.pump_fun_swap_accounts.mint,
                token_data.token_marketcap,
                *BUY_AMOUNT_SOL
            );

            // Spawn confirm FIRST — before any logging — to minimise latency.
            let ix_clone = ix.clone();
            let tag_clone = tag.clone();
            tokio::spawn(async move {
                let _ = confirm(ix_clone, tag_clone).await;
            });

            // Spawn 4.8s timeout sell — exits the position after 4.8 seconds.
            // After sleeping, poll up to 2s for our buy event to be processed
            // (gRPC may deliver it a little after the TX lands on-chain).
            let mint = token_data.token_mint;
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(4800)).await;

                // Wait up to 2s for token_balance to be populated from our buy event.
                let mut poll = 0u8;
                loop {
                    match TOKEN_DB.get(mint).unwrap() {
                        None => return,                          // token already removed (sold)
                        Some(d) if d.token_balance > 0 => break, // balance ready
                        _ => {}
                    }
                    poll += 1;
                    if poll >= 20 {
                        // Buy likely failed on-chain; nothing to sell.
                        info!("[CopySell][Timeout] balance still 0 after 2s extra wait, skipping {}", mint);
                        return;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }

                copy_sell_token(mint, "Timeout4.8s".to_string());
            });

            info!("{}", tag);
        }
    }
}
