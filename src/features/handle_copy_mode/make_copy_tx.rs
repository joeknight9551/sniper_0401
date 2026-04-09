use crate::*;
use dashmap::DashMap;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

/// Sell the full token balance of `mint`.
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

    let amount = token_data.token_balance;

    let mut sell_data = token_data.clone();
    sell_data.token_sell_status = TokenSellStatus::SellTradeSubmitted;
    // Always use the latest creator_vault from observed buy/sell IXs
    if let Some(cv) = CREATOR_VAULT_CACHE.get(&mint) {
        sell_data.pump_fun_swap_accounts.creator_vault = *cv;
    }
    let _ = TOKEN_DB.upsert(mint, sell_data.clone());

    info!(
        "[CopySell]\t*{}\t*Mint: {}\t*SellAmount: {}\t*Balance: {}\t*creator_vault: {}\t*cashback: {} (known={})",
        reason, sell_data.token_mint, amount, sell_data.token_balance,
        sell_data.pump_fun_swap_accounts.creator_vault,
        sell_data.cashback_enabled, sell_data.cashback_known
    );

    tokio::spawn(async move {
        let sell_ix_primary = sell_data
            .pump_fun_swap_accounts
            .get_sell_ix(amount, sell_data.cashback_enabled);
        let sell_tag_primary = format!(
            "[CopySell]\t*{}\t*Mint: {}\t*Amount: {}\t*cashback={}",
            reason, sell_data.token_mint, amount, sell_data.cashback_enabled
        );

        if !sell_data.cashback_known {
            let opposite = !sell_data.cashback_enabled;
            let sell_ix_alt = sell_data
                .pump_fun_swap_accounts
                .get_sell_ix(amount, opposite);
            let sell_tag_alt = format!(
                "[CopySell]\t*{}\t*Mint: {}\t*Amount: {}\t*cashback={} (alt)",
                reason, sell_data.token_mint, amount, opposite
            );
            let alt_data = sell_data.clone();
            tokio::spawn(async move {
                let _ = confirm_no_nonce(vec![sell_ix_alt], sell_tag_alt).await;
                if let Some(mut stored) = TOKEN_DB.get(alt_data.token_mint).unwrap() {
                    stored.cashback_enabled = opposite;
                    stored.cashback_known = true;
                    let _ = TOKEN_DB.upsert(alt_data.token_mint, stored);
                }
            });
        }

        let _ = confirm(vec![sell_ix_primary], sell_tag_primary).await;
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

            info!("{}", tag);
        }
    }
}
