use colored::Colorize;
use std::sync::atomic::Ordering;

use crate::*;
use crate::{BuyEvent, LastEvent, TokenDatabaseSchema, WALLET_PUB_KEY, info};

pub fn update_status_from_buy_event(
    mut token_data: TokenDatabaseSchema,
    buy_event: BuyEvent,
    tx_id: String,
    cu_info: ComputeBudgetInfo,
) -> TokenDatabaseSchema {
    let updated_token_price = (buy_event.virtual_sol_reserves as f64 / 10f64.powi(9))
        / (buy_event.virtual_token_reserves as f64 / 10f64.powi(6));
    token_data.token_price = updated_token_price;

    token_data.token_marketcap = updated_token_price * token_data.token_total_supply as f64;

    token_data.token_volume = if let Some(val) = token_data.token_volume {
        Some(val + buy_event.sol_amount as f64 / 10f64.powi(9))
    } else {
        None
    };

    // Update creator vault PDA from the event's creator (creator may have changed since mint)
    token_data.token_creator = buy_event.creator;
    token_data
        .pump_fun_swap_accounts
        .update_creator_vault(&buy_event.creator);

    token_data.last_event = LastEvent {
        tx_hash: tx_id.clone(),
        last_tracked_event: super::TokenEvent::BuyTokenEvent,
        last_activity_timestamp: buy_event.timestamp,
    };

    if buy_event.user == token_data.token_creator {
        info!(
            "{}, [{}]\t*Mint: {}\t*MC: {:.2} SOL\t{}\t*Buy Amount: {:.2} SOL\t*CU Limit: {}\t*CU Price: {}",
            "Dev Buy".blue(),
            if token_data.token_is_purchased {
                "Purchased Token"
            } else {
                "No Purchased"
            },
            token_data.token_mint,
            token_data.token_marketcap,
            match token_data.token_volume {
                Some(val) => format!("*Volume: {:.4} SOL", val),
                None => "".to_string(),
            },
            buy_event.sol_amount as f64 / 10f64.powi(9),
            cu_info.unit_limit,
            cu_info.unit_price,
        );
    } else if buy_event.user != *WALLET_PUB_KEY {
        info!(
            "{}, [{}]\t*Mint: {}\t*MC: {:.2} SOL\t{}\t*Buy Amount: {:.2} SOL\t*CU Limit: {}\t*CU Price: {}",
            "BUY".blue(),
            if token_data.token_is_purchased {
                "Purchased Token"
            } else {
                "No Purchased"
            },
            token_data.token_mint,
            token_data.token_marketcap,
            match token_data.token_volume {
                Some(val) => format!("*Volume: {:.4} SOL", val),
                None => "".to_string(),
            },
            buy_event.sol_amount as f64 / 10f64.powi(9),
            cu_info.unit_limit,
            cu_info.unit_price,
        );
    }

    if buy_event.user == *WALLET_PUB_KEY {
        info!(
            "[My tx]\t[{}]\t*Hash: {}\t*mint: {}",
            "Buy".green(),
            tx_id,
            buy_event.mint.to_string()
        );
        token_data.token_is_purchased = true;
        token_data.token_buying_point_price = (buy_event.sol_amount as f64 / 10f64.powi(9))
            / (buy_event.token_amount as f64 / 10f64.powi(6));
        token_data.token_balance += buy_event.token_amount;
    }
    let _ = TOKEN_DB.upsert(buy_event.mint.clone(), token_data.clone());
    // Check take-profit and stop-loss on every price update for tokens we hold
    check_take_profit(&token_data);
    check_stop_loss(&token_data);
    token_data.clone()
}

pub fn update_status_from_sell_event(
    mut token_data: TokenDatabaseSchema,
    sell_event: SellEvent,
    tx_id: String,
    cu_info: ComputeBudgetInfo,
) -> Option<TokenDatabaseSchema> {
    let updated_token_price = (sell_event.virtual_sol_reserves as f64 / 10f64.powi(9))
        / (sell_event.virtual_token_reserves as f64 / 10f64.powi(6));

    token_data.token_price = updated_token_price;
    token_data.token_marketcap = updated_token_price * token_data.token_total_supply as f64;

    token_data.token_volume = if let Some(val) = token_data.token_volume {
        Some(val + sell_event.sol_amount as f64 / 10f64.powi(9))
    } else {
        None
    };

    // Update creator vault PDA from the event's creator (creator may have changed since mint)
    token_data.token_creator = sell_event.creator;
    token_data
        .pump_fun_swap_accounts
        .update_creator_vault(&sell_event.creator);

    token_data.last_event = LastEvent {
        tx_hash: tx_id.clone(),
        last_tracked_event: TokenEvent::SellTokenEvent,
        last_activity_timestamp: sell_event.timestamp,
    };

    if sell_event.user == token_data.token_creator {
        info!(
            "{}, [{}]\t*Mint: {}\t*MC: {:.2} SOL\t{}\t*Sell Amount: {:.2} SOL\t*CU Limit: {}\t*CU Price: {}",
            "Dev SELL".blue(),
            if token_data.token_is_purchased {
                "Purchased Token"
            } else {
                "No Purchased"
            },
            token_data.token_mint,
            token_data.token_marketcap,
            match token_data.token_volume {
                Some(val) => format!("*Volume: {:.4} SOL", val),
                None => "".to_string(),
            },
            sell_event.sol_amount as f64 / 10f64.powi(9),
            cu_info.unit_limit,
            cu_info.unit_price,
        );
    } else if sell_event.user != *WALLET_PUB_KEY {
        info!(
            "{}, [{}]\t*Mint: {}\t*MC: {:.2} SOL\t{}\t*Sell Amount: {:.2} SOL\t*CU Limit: {}\t*CU Price: {}",
            "SELL".blue(),
            if token_data.token_is_purchased {
                "Purchased Token"
            } else {
                "No Purchased"
            },
            token_data.token_mint,
            token_data.token_marketcap,
            match token_data.token_volume {
                Some(val) => format!("*Volume: {:.4} SOL", val),
                None => "".to_string(),
            },
            sell_event.sol_amount as f64 / 10f64.powi(9),
            cu_info.unit_limit,
            cu_info.unit_price,
        );
    }

    if sell_event.user == *WALLET_PUB_KEY {
        info!(
            "[My Tx]\t[{}]\t*Hash: {}\t*mint: {}",
            "Sell".green(),
            tx_id,
            sell_event.mint.to_string()
        );
        token_data.token_balance -= sell_event.token_amount;

        if token_data.token_balance > 0 {
            let _ = TOKEN_DB.upsert(sell_event.mint.clone(), token_data.clone());
            Some(token_data.clone())
        } else {
            let _ = TOKEN_DB.delete(sell_event.mint.clone());
            None
        }
    } else {
        let _ = TOKEN_DB.upsert(sell_event.mint.clone(), token_data.clone());
        // Check take-profit and stop-loss on every price update for tokens we hold
        check_take_profit(&token_data);
        check_stop_loss(&token_data);
        Some(token_data.clone())
    }
}

/// Check if the current price has hit the per-pattern take-profit target.
/// Called on every buy/sell event price update — no polling needed.
fn check_take_profit(token_data: &TokenDatabaseSchema) {
    if token_data.skip_tp_sl
        || !token_data.token_is_purchased
        || token_data.token_balance == 0
        || token_data.token_buying_point_price == 0.0
        || token_data.token_sell_status != TokenSellStatus::None
        || token_data.token_take_profit_pct == 0.0
    {
        return;
    }

    let take_profit_multiplier = token_data.token_take_profit_pct / 100.0;
    let take_profit_price = token_data.token_buying_point_price * take_profit_multiplier;

    if token_data.token_price >= take_profit_price {
        info!(
            "[TP HIT] {}% reached! BuyPrice: {:.6} → CurrentPrice: {:.6} (target: {:.6}) | Mint: {}",
            token_data.token_take_profit_pct,
            token_data.token_buying_point_price,
            token_data.token_price,
            take_profit_price,
            token_data.token_mint,
        );

        // Mark as sell submitted to prevent duplicate sells
        let mut updated = token_data.clone();
        updated.token_sell_status = TokenSellStatus::SellTradeSubmitted;
        let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());

        // Build and send sell tx asynchronously
        let sell_data = updated.clone();
        tokio::spawn(async move {
            let mut data = sell_data;
            data.pump_fun_swap_accounts
                .update_creator_vault(&data.token_creator);

            let sell_ix = data
                .pump_fun_swap_accounts
                .get_sell_ix(data.token_balance, data.cashback_enabled);

            let sell_tag = format!(
                "[SELL]\t*{}% TP\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
                data.token_take_profit_pct,
                data.pump_fun_swap_accounts.mint,
                data.token_marketcap,
                data.token_balance,
                data.token_buying_point_price,
                data.token_price,
            );

            info!(
                "[SELL]\t*{}% TP\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
                data.token_take_profit_pct,
                data.pump_fun_swap_accounts.mint,
                data.token_marketcap,
                data.token_balance,
                data.token_buying_point_price,
                data.token_price,
            );

            let _ = confirm(vec![sell_ix], sell_tag).await;

            // Take-profit hit = profit, reset consecutive loss counter
            info!("[P&L] PROFIT (TP hit) | Mint: {}", data.pump_fun_swap_accounts.mint);
            CONSECUTIVE_LOSSES.store(0, Ordering::SeqCst);

            // Unlock position so bot can buy next token
            IS_HOLDING_POSITION.store(false, Ordering::SeqCst);
        });
    }
}

/// Check if the current price has hit the stop-loss threshold.
/// stop_loss config value is the percentage of buy price at which to sell.
/// e.g. stop_loss = 70 means sell when price drops to 70% of buy price (30% loss).
fn check_stop_loss(token_data: &TokenDatabaseSchema) {
    if token_data.skip_tp_sl
        || !token_data.token_is_purchased
        || token_data.token_balance == 0
        || token_data.token_buying_point_price == 0.0
        || token_data.token_sell_status != TokenSellStatus::None
    {
        return;
    }

    let stop_loss_pct = CONFIG.sell_setting.stop_loss;
    if stop_loss_pct == 0.0 {
        return;
    }

    let stop_loss_price = token_data.token_buying_point_price * (stop_loss_pct / 100.0);

    if token_data.token_price <= stop_loss_price {
        info!(
            "[SL HIT] {}% stop loss! BuyPrice: {:.6} → CurrentPrice: {:.6} (threshold: {:.6}) | Mint: {}",
            stop_loss_pct,
            token_data.token_buying_point_price,
            token_data.token_price,
            stop_loss_price,
            token_data.token_mint,
        );

        // Mark as sell submitted to prevent duplicate sells
        let mut updated = token_data.clone();
        updated.token_sell_status = TokenSellStatus::SellTradeSubmitted;
        let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());

        // Build and send sell tx asynchronously
        let sell_data = updated.clone();
        tokio::spawn(async move {
            let mut data = sell_data;
            data.pump_fun_swap_accounts
                .update_creator_vault(&data.token_creator);

            let sell_ix = data
                .pump_fun_swap_accounts
                .get_sell_ix(data.token_balance, data.cashback_enabled);

            let sell_tag = format!(
                "[SELL]\t*{}% SL\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
                stop_loss_pct,
                data.pump_fun_swap_accounts.mint,
                data.token_marketcap,
                data.token_balance,
                data.token_buying_point_price,
                data.token_price,
            );

            info!(
                "[SELL]\t*{}% SL\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
                stop_loss_pct,
                data.pump_fun_swap_accounts.mint,
                data.token_marketcap,
                data.token_balance,
                data.token_buying_point_price,
                data.token_price,
            );

            let _ = confirm(vec![sell_ix], sell_tag).await;

            // Stop loss = loss, increment consecutive loss counter
            let prev = CONSECUTIVE_LOSSES.fetch_add(1, Ordering::SeqCst);
            let new_count = prev + 1;
            info!("[P&L] LOSS (SL hit) | Mint: {} | Consecutive losses: {}", data.pump_fun_swap_accounts.mint, new_count);
            if new_count >= 2 {
                SKIP_NEXT_BUY.store(true, Ordering::SeqCst);
                CONSECUTIVE_LOSSES.store(0, Ordering::SeqCst);
                info!("[P&L] 2 consecutive losses — will skip next token");
            }

            // Unlock position so bot can buy next token
            IS_HOLDING_POSITION.store(false, Ordering::SeqCst);
        });
    }
}
