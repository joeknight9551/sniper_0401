use crate::*;
use crate::{
    BuyEvent, BuyInstructionAccounts, MintEvent, MintInstructionAccounts, SellEvent,
    SellInstructionAccounts,
};
use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::Ordering;
/// Fire tiered take-profit partial sells for copy-mode positions.
///   Level 0 → 1: sell 20% at 1.2× buy price
///   Level 1 → 2: sell 30% at 1.8× buy price
/// Called after every price update (buy/sell events) for tokens we hold.
fn check_copy_take_profit(token_data: &TokenDatabaseSchema) {
    if !token_data.token_is_purchased
        || token_data.token_balance == 0
        || token_data.token_buying_point_price == 0.0
        || token_data.token_sell_status != TokenSellStatus::None
        || !token_data.skip_tp_sl  // only copy-mode tokens have skip_tp_sl = true
        || token_data.mirror_only  // mirror-only tokens sell only when target sells
        || token_data.tp_sell_level >= 2  // both TP tiers already fired
    {
        return;
    }

    // Tier 2: 1.8× buy price → sell 30%  (check higher tier first)
    if token_data.tp_sell_level == 1
        && token_data.token_price >= token_data.token_buying_point_price * 1.8
    {
        let sell_amount = token_data.token_balance * 3 / 10;
        if sell_amount == 0 {
            return;
        }
        info!(
            "[CopyTP] 180% TP hit — selling 30% | Mint: {} | BuyPrice: {:.6} | CurrentPrice: {:.6} | Amount: {}",
            token_data.token_mint,
            token_data.token_buying_point_price,
            token_data.token_price,
            sell_amount,
        );
        if let Some(mut stored) = TOKEN_DB.get(token_data.token_mint).unwrap() {
            stored.tp_sell_level = 2;
            let _ = TOKEN_DB.upsert(token_data.token_mint, stored);
        }
        copy_sell_token(token_data.token_mint, "TP180%_30pct".to_string(), sell_amount);
        return;
    }

    // Tier 1: 1.2× buy price → sell 20%
    if token_data.tp_sell_level == 0
        && token_data.token_price >= token_data.token_buying_point_price * 1.2
    {
        let sell_amount = token_data.token_balance / 5;
        if sell_amount == 0 {
            return;
        }
        info!(
            "[CopyTP] 120% TP hit — selling 20% | Mint: {} | BuyPrice: {:.6} | CurrentPrice: {:.6} | Amount: {}",
            token_data.token_mint,
            token_data.token_buying_point_price,
            token_data.token_price,
            sell_amount,
        );
        if let Some(mut stored) = TOKEN_DB.get(token_data.token_mint).unwrap() {
            stored.tp_sell_level = 1;
            let _ = TOKEN_DB.upsert(token_data.token_mint, stored);
        }
        copy_sell_token(token_data.token_mint, "TP120%_20pct".to_string(), sell_amount);
    }
}

pub async fn handle_copy_event(
    trade_data: (
        Vec<MintEvent>,
        Vec<BuyEvent>,
        Vec<SellEvent>,
        Vec<MintInstructionAccounts>,
        Vec<BuyInstructionAccounts>,
        Vec<SellInstructionAccounts>,
    ),
    tx_id: String,
) -> DashMap<Pubkey, TokenDatabaseSchema> {
    let (
        mint_events,
        buy_events,
        sell_events,
        mint_ixs_accounts,
        buy_ixs_accounts,
        sell_ixs_accounts,
    ) = trade_data;

    let return_data: DashMap<Pubkey, TokenDatabaseSchema> = DashMap::new();

    // Dummy CU info — copy mode does not use CU pattern matching
    let cu_dummy = ComputeBudgetInfo::default();

    // ── Mint events ───────────────────────────────────────────────────────────
    for (i, mint_event) in mint_events.iter().enumerate() {
        if !mint_event.is_mayhem_mode {
            if let Some(token_data) = TokenDatabaseSchema::new_from_mint(
                mint_event.clone(),
                mint_ixs_accounts[i].clone(),
                tx_id.to_string(),
            )
            .await
            {
                return_data.insert(token_data.token_mint, token_data);
            }
        }
    }

    // ── Sell events ───────────────────────────────────────────────────────────
    // Two sell triggers checked here:
    //   1. 120% TP on price update → sell 80% of balance
    //   2. Target wallet sold while we still hold → sell remaining
    for sell_event in sell_events.iter() {
        let is_target_sell = TARGET_WALLETS.iter().any(|w| *w == sell_event.user.to_string());

        if let Some(mut token_data) = TOKEN_DB.get(sell_event.mint).unwrap() {
            // Update cashback_enabled and creator_vault from the observed sell IX accounts
            // before update_status_from_sell_event potentially deletes the record.
            if token_data.token_is_purchased && token_data.token_balance > 0 {
                if let Some(sell_ix) = sell_ixs_accounts.iter().find(|s| s.mint == sell_event.mint) {
                    let mut changed = false;
                    if token_data.cashback_enabled != sell_ix.cashback_enabled {
                        token_data.cashback_enabled = sell_ix.cashback_enabled;
                        token_data.cashback_known = true;
                        changed = true;
                    }
                    if token_data.pump_fun_swap_accounts.creator_vault != sell_ix.creator_vault {
                        token_data.pump_fun_swap_accounts.creator_vault = sell_ix.creator_vault;
                        changed = true;
                    }
                    if changed {
                        let _ = TOKEN_DB.upsert(sell_event.mint, token_data.clone());
                    }
                }

                // If a target wallet sells while we still hold, follow immediately.
                // Sell full balance if no partial TP happened, or remaining half if it did.
                if is_target_sell {
                    info!(
                        "[CopyMode] Target {} sold {} — following sell (tp_sell_level={})",
                        sell_event.user, sell_event.mint, token_data.tp_sell_level
                    );
                    copy_sell_token(sell_event.mint, "TargetSell".to_string(), 0);
                    // Re-read from DB so update_status_from_sell_event doesn't
                    // overwrite the SellTradeSubmitted status with a stale clone.
                    if let Some(fresh) = TOKEN_DB.get(sell_event.mint).unwrap() {
                        token_data = fresh;
                    }
                }
            }

            if let Some(updated) = update_status_from_sell_event(
                token_data.clone(),
                sell_event.clone(),
                tx_id.to_string(),
                cu_dummy,
            ) {
                // Only check TP if we didn't just fire a target-sell for this token.
                if !is_target_sell {
                    check_copy_take_profit(&updated);
                }
                return_data.insert(updated.token_mint, updated);
            }
        }
    }

    // ── Buy events ────────────────────────────────────────────────────────────
    for buy_event in buy_events.iter() {
        let is_target = TARGET_WALLETS.iter().any(|w| *w == buy_event.user.to_string());

        if let Some(token_data) = TOKEN_DB.get(buy_event.mint).unwrap() {
            let mut updated = update_status_from_buy_event(
                token_data.clone(),
                buy_event.clone(),
                tx_id.to_string(),
                cu_dummy,
            );

            // Keep creator_vault fresh from observed buy IX accounts
            if updated.token_is_purchased {
                if let Some(buy_ix) = buy_ixs_accounts.iter().find(|a| a.mint == buy_event.mint) {
                    if updated.pump_fun_swap_accounts.creator_vault != buy_ix.creator_vault {
                        updated.pump_fun_swap_accounts.creator_vault = buy_ix.creator_vault;
                        let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());
                    }
                }
            }

            // Check 180% TP on every price update for held tokens
            check_copy_take_profit(&updated);

            if is_target
                && !updated.token_is_purchased
                && !(*ONE_TIME_COPY && COPIED_MINTS.contains(&buy_event.mint))
            {
                if SKIP_NEXT_BUY.load(Ordering::SeqCst) {
                    SKIP_NEXT_BUY.store(false, Ordering::SeqCst);
                    info!(
                        "[CopyMode][Skip] Skipping {} due to 2 consecutive losses",
                        buy_event.mint
                    );
                } else if let Some(buy_ix) =
                    buy_ixs_accounts.iter().find(|a| a.mint == buy_event.mint)
                {
                    let mut queued = updated.clone();
                    queued.pump_fun_swap_accounts =
                        PumpFunSwapAccounts::from_target_buy(buy_ix.clone());
                    queued.token_buy_now = true;
                    queued.skip_tp_sl = true;
                    queued.mirror_only = MIRROR_WALLETS.iter().any(|w| *w == buy_event.user.to_string());
                    // Fill cashback from cache if still unknown
                    if !queued.cashback_known {
                        if let Some(val) = CASHBACK_CACHE.get(&buy_event.mint) {
                            queued.cashback_enabled = *val;
                            queued.cashback_known = true;
                        }
                    }
                    if *ONE_TIME_COPY {
                        COPIED_MINTS.insert(buy_event.mint);
                    }
                    info!(
                        "[CopyMode] Target {} bought {} — queuing buy of {} SOL | creator_vault: {}",
                        buy_event.user, buy_event.mint, *BUY_AMOUNT_SOL,
                        queued.pump_fun_swap_accounts.creator_vault
                    );
                    let _ = TOKEN_DB.upsert(queued.token_mint, queued.clone());
                    return_data.insert(queued.token_mint, queued);
                    continue;
                } else {
                    let mut queued = updated.clone();
                    queued.pump_fun_swap_accounts.update_creator_vault(&buy_event.creator);
                    queued.token_buy_now = true;
                    queued.skip_tp_sl = true;
                    queued.mirror_only = MIRROR_WALLETS.iter().any(|w| *w == buy_event.user.to_string());
                    if *ONE_TIME_COPY {
                        COPIED_MINTS.insert(buy_event.mint);
                    }
                    info!(
                        "[CopyMode][Fallback] Target {} bought {} — queuing buy using mint-time accounts",
                        buy_event.user, buy_event.mint
                    );
                    let _ = TOKEN_DB.upsert(queued.token_mint, queued.clone());
                    return_data.insert(queued.token_mint, queued);
                    continue;
                }
            }
            return_data.insert(updated.token_mint, updated);
        } else if is_target {
            // Token not yet in DB (no mint event seen) — create a minimal record from the buy IX
            let already_copied = *ONE_TIME_COPY && COPIED_MINTS.contains(&buy_event.mint);
            if !already_copied {
                if SKIP_NEXT_BUY.load(Ordering::SeqCst) {
                    SKIP_NEXT_BUY.store(false, Ordering::SeqCst);
                } else if let Some(buy_ix) =
                    buy_ixs_accounts.iter().find(|a| a.mint == buy_event.mint)
                {
                    let price = (buy_event.virtual_sol_reserves as f64 / 1e9)
                        / (buy_event.virtual_token_reserves as f64 / 1e6);
                    let swap_accounts = PumpFunSwapAccounts::from_target_buy(buy_ix.clone());
                    // Look up cashback from mint event cache
                    let (cb_enabled, cb_known) = match CASHBACK_CACHE.get(&buy_event.mint) {
                        Some(val) => (*val, true),
                        None => (false, false), // fallback: unknown → dual-send
                    };
                    let new_token = TokenDatabaseSchema {
                        token_mint: buy_event.mint,
                        token_name: String::new(),
                        token_symbol: String::new(),
                        cashback_enabled: cb_enabled,
                        cashback_known: cb_known,
                        token_creator: buy_event.creator,
                        token_total_supply: PUMP_FUN_TOKEN_TOTAL_SUPPLY,
                        token_price: price,
                        token_is_purchased: false,
                        token_balance: 0,
                        token_buying_point_price: 0.0,
                        token_marketcap: price * PUMP_FUN_TOKEN_TOTAL_SUPPLY as f64,
                        token_volume: None,
                        pump_fun_swap_accounts: swap_accounts,
                        last_event: LastEvent {
                            tx_hash: tx_id.to_string(),
                            last_tracked_event: TokenEvent::BuyTokenEvent,
                            last_activity_timestamp: buy_event.timestamp,
                        },
                        token_sell_status: TokenSellStatus::None,
                        token_mint_timestamp: buy_event.timestamp,
                        token_buy_now: true,
                        token_take_profit_pct: 0.0,
                        token_holding_time_secs: 0,
                        skip_tp_sl: true,
                        mirror_only: MIRROR_WALLETS.iter().any(|w| *w == buy_event.user.to_string()),
                        tp_sell_level: 0,
                    };
                    let _ = TOKEN_DB.upsert(buy_event.mint, new_token.clone());
                    if *ONE_TIME_COPY {
                        COPIED_MINTS.insert(buy_event.mint);
                    }
                    info!(
                        "[CopyMode][NewToken] Target {} bought {} — queuing buy of {} SOL | creator_vault: {}",
                        buy_event.user, buy_event.mint, *BUY_AMOUNT_SOL,
                        new_token.pump_fun_swap_accounts.creator_vault
                    );
                    return_data.insert(buy_event.mint, new_token);
                }
            }
        }
    }

    return_data
}
