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

    // For copy-mode tokens (skip_tp_sl=true) the creator_vault was copied directly
    // from the target's buy IX and is the ground truth — never overwrite it from
    // event data, which may carry a stale/different creator.
    if !token_data.skip_tp_sl {
        token_data.token_creator = buy_event.creator;
        token_data
            .pump_fun_swap_accounts
            .update_creator_vault(&buy_event.creator);
    }

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
    // Preserve sell-related fields from the latest DB state to avoid
    // overwriting concurrent modifications (e.g. async TP sell status reset).
    if let Some(current) = TOKEN_DB.get(buy_event.mint).unwrap() {
        token_data.token_sell_status = current.token_sell_status;
        token_data.tp_sell_level = current.tp_sell_level;
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

    // For copy-mode tokens (skip_tp_sl=true) the creator_vault was copied directly
    // from the target's buy IX and is the ground truth — never overwrite it from
    // event data, which may carry a stale/different creator.
    if !token_data.skip_tp_sl {
        token_data.token_creator = sell_event.creator;
        token_data
            .pump_fun_swap_accounts
            .update_creator_vault(&sell_event.creator);
    }

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
            // Preserve sell-related fields from the latest DB state.
            if let Some(current) = TOKEN_DB.get(sell_event.mint).unwrap() {
                token_data.token_sell_status = current.token_sell_status;
                token_data.tp_sell_level = current.tp_sell_level;
            }
            let _ = TOKEN_DB.upsert(sell_event.mint.clone(), token_data.clone());
            Some(token_data.clone())
        } else {
            let _ = TOKEN_DB.delete(sell_event.mint.clone());
            None
        }
    } else {
        // Preserve sell-related fields from the latest DB state.
        if let Some(current) = TOKEN_DB.get(sell_event.mint).unwrap() {
            token_data.token_sell_status = current.token_sell_status;
            token_data.tp_sell_level = current.tp_sell_level;
        }
        let _ = TOKEN_DB.upsert(sell_event.mint.clone(), token_data.clone());
        // Check take-profit and stop-loss on every price update for tokens we hold
        check_take_profit(&token_data);
        check_stop_loss(&token_data);
        Some(token_data.clone())
    }
}

/// Tiered take-profit check. Each tier sells a percentage of the *current* balance.
/// tp_sell_level tracks which tiers have already fired (bitmask-style using level number).
///
/// Tier 1: price >= buy_price × 1.5  → sell 30%
/// Tier 2: price >= buy_price × 3.0  → sell 20%
/// Tier 3: price >= buy_price × 7.5  → sell 50%
fn check_take_profit(token_data: &TokenDatabaseSchema) {
    if token_data.skip_tp_sl
        || !token_data.token_is_purchased
        || token_data.token_balance == 0
        || token_data.token_buying_point_price == 0.0
        || token_data.token_sell_status != TokenSellStatus::None
    {
        return;
    }

    let buy_price = token_data.token_buying_point_price;
    let current_price = token_data.token_price;
    let level = token_data.tp_sell_level;

    // Determine which tier to trigger (only one per price update, lowest untriggered first)
    let (tier_level, multiplier, sell_pct): (u8, f64, f64) =
        if level < 1 && current_price >= buy_price * 1.5 {
            (1, 1.5, 30.0)
        } else if level < 2 && current_price >= buy_price * 3.0 {
            (2, 3.0, 20.0)
        } else if level < 3 && current_price >= buy_price * 7.5 {
            (3, 7.5, 50.0)
        } else {
            return;
        };

    let sell_amount = (token_data.token_balance as f64 * sell_pct / 100.0) as u64;
    if sell_amount == 0 {
        return;
    }

    info!(
        "[TP{} HIT] {:.0}× reached! BuyPrice: {:.6} → CurrentPrice: {:.6} | Selling {}% ({} tokens) | Mint: {}",
        tier_level,
        multiplier,
        buy_price,
        current_price,
        sell_pct,
        sell_amount,
        token_data.token_mint,
    );

    // Mark as sell submitted to prevent duplicate sells
    let mut updated = token_data.clone();
    updated.token_sell_status = TokenSellStatus::SellTradeSubmitted;
    updated.tp_sell_level = tier_level;
    let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());

    // Build and send sell tx asynchronously
    let sell_data = updated.clone();
    tokio::spawn(async move {
        let mut data = sell_data;
        data.pump_fun_swap_accounts
            .update_creator_vault(&data.token_creator);

        let sell_ix = data
            .pump_fun_swap_accounts
            .get_sell_ix(sell_amount, data.cashback_enabled);

        let sell_tag = format!(
            "[SELL]\t*TP{} ({:.0}×)\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
            tier_level,
            multiplier,
            data.pump_fun_swap_accounts.mint,
            data.token_marketcap,
            sell_amount,
            data.token_buying_point_price,
            data.token_price,
        );

        info!(
            "[SELL]\t*TP{} ({:.0}×)\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
            tier_level,
            multiplier,
            data.pump_fun_swap_accounts.mint,
            data.token_marketcap,
            sell_amount,
            data.token_buying_point_price,
            data.token_price,
        );

        let _ = confirm(vec![sell_ix], sell_tag).await;

        // Reset sell status so next tier can fire; keep tp_sell_level updated
        if let Ok(Some(mut current)) = TOKEN_DB.get(data.token_mint) {
            current.token_sell_status = TokenSellStatus::None;
            let _ = TOKEN_DB.upsert(data.token_mint, current);
        }

        // TP hit = profit, reset consecutive loss counter
        info!("[P&L] PROFIT (TP{} hit) | Mint: {}", tier_level, data.pump_fun_swap_accounts.mint);
        CONSECUTIVE_LOSSES.store(0, Ordering::SeqCst);
    });
}

/// Stop loss: sell ALL tokens when price drops below 0.7 × buy price (30% loss).
/// Also: if TP2 (3×) was hit but TP3 (7.5×) was not, sell ALL when price drops to 1.2× buy.
fn check_stop_loss(token_data: &TokenDatabaseSchema) {
    if token_data.skip_tp_sl
        || !token_data.token_is_purchased
        || token_data.token_balance == 0
        || token_data.token_buying_point_price == 0.0
        || token_data.token_sell_status != TokenSellStatus::None
    {
        return;
    }

    let buy_price = token_data.token_buying_point_price;
    let current_price = token_data.token_price;
    let level = token_data.tp_sell_level;

    // Trailing stop after TP2: if hit 3× but not 7.5×, sell all at 1.2×
    let (triggered, label) = if level >= 2 && level < 3 && current_price <= buy_price * 1.2 {
        (true, "Trailing SL (hit 3× → dropped to 1.2×)")
    } else if current_price < buy_price * 0.7 {
        (true, "SL (< 0.7×)")
    } else {
        (false, "")
    };

    if !triggered {
        return;
    }

    info!(
        "[{}] BuyPrice: {:.6} → CurrentPrice: {:.6} | Mint: {}",
        label,
        buy_price,
        current_price,
        token_data.token_mint,
    );

    // Mark as sell submitted to prevent duplicate sells
    let mut updated = token_data.clone();
    updated.token_sell_status = TokenSellStatus::SellTradeSubmitted;
    let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());

    // Build and send sell tx asynchronously
    let sell_data = updated.clone();
    let sell_label = label.to_string();
    tokio::spawn(async move {
        let mut data = sell_data;
        data.pump_fun_swap_accounts
            .update_creator_vault(&data.token_creator);

        let sell_ix = data
            .pump_fun_swap_accounts
            .get_sell_ix(data.token_balance, data.cashback_enabled);

        let sell_tag = format!(
            "[SELL]\t*{}\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
            sell_label,
            data.pump_fun_swap_accounts.mint,
            data.token_marketcap,
            data.token_balance,
            data.token_buying_point_price,
            data.token_price,
        );

        info!(
            "[SELL]\t*{}\t*MINT: {}\t*MC: {}\t*AMOUNT: {}\t*BuyPrice: {:.6}\t*SellPrice: {:.6}",
            sell_label,
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
        info!("[P&L] LOSS ({}) | Mint: {} | Consecutive losses: {}", sell_label, data.pump_fun_swap_accounts.mint, new_count);
        if new_count >= 2 {
            SKIP_NEXT_BUY.store(true, Ordering::SeqCst);
            CONSECUTIVE_LOSSES.store(0, Ordering::SeqCst);
            info!("[P&L] 2 consecutive losses — will skip next token");
        }
    });
}
