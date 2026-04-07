use crate::*;
use crate::{
    BuyEvent, BuyInstructionAccounts, MintEvent, MintInstructionAccounts, SellEvent,
    SellInstructionAccounts,
};
use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::Ordering;

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
        _sell_ixs_accounts,
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

    // ── Sell events FIRST ─────────────────────────────────────────────────────
    // Process sells before buys so that if a target wallet sells token A and buys
    // token B in the same transaction (or same block), the position lock is already
    // released before we evaluate the buy.
    for sell_event in sell_events.iter() {
        let is_target_sell =
            TARGET_WALLETS.iter().any(|w| *w == sell_event.user.to_string());

        if let Some(token_data) = TOKEN_DB.get(sell_event.mint).unwrap() {
            if let Some(mut updated) = update_status_from_sell_event(
                token_data.clone(),
                sell_event.clone(),
                tx_id.to_string(),
                cu_dummy,
            ) {
                // If a target wallet sold and we hold this token, follow the sell immediately
                if is_target_sell
                    && updated.token_is_purchased
                    && updated.token_balance > 0
                    && updated.token_sell_status == TokenSellStatus::None
                {
                    updated.token_sell_status = TokenSellStatus::SellTradeSubmitted;
                    updated
                        .pump_fun_swap_accounts
                        .update_creator_vault(&updated.token_creator);
                    let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());

                    // Release the position lock immediately so a concurrent buy event
                    // (same block or next stream update) can proceed without waiting
                    // for our sell transaction to confirm.

                    info!(
                        "[CopyMode] Target {} sold {} — following sell of {} tokens, position unlocked",
                        sell_event.user, sell_event.mint, updated.token_balance
                    );

                    let mut sell_data = updated.clone();
                    tokio::spawn(async move {
                        let sell_ix = sell_data.pump_fun_swap_accounts.get_sell_ix(
                            sell_data.token_balance,
                            sell_data.cashback_enabled,
                        );
                        let sell_tag = format!(
                            "[CopySell]\t*Mint: {}\t*Amount: {}",
                            sell_data.token_mint, sell_data.token_balance
                        );
                        let _ = confirm(vec![sell_ix], sell_tag).await;
                    });
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
                    updated.pump_fun_swap_accounts =
                        PumpFunSwapAccounts::from_target_buy(buy_ix.clone());
                    updated.token_buy_now = true;
                    updated.skip_tp_sl = true;
                    if *ONE_TIME_COPY {
                        COPIED_MINTS.insert(buy_event.mint);
                    }
                    info!(
                        "[CopyMode] Target {} bought {} — queuing buy of {} SOL",
                        buy_event.user, buy_event.mint, *BUY_AMOUNT_SOL
                    );
                    let _ = TOKEN_DB.upsert(updated.token_mint, updated.clone());
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
                    let new_token = TokenDatabaseSchema {
                        token_mint: buy_event.mint,
                        token_name: String::new(),
                        token_symbol: String::new(),
                        cashback_enabled: false,
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
                    };
                    let _ = TOKEN_DB.upsert(buy_event.mint, new_token.clone());
                    if *ONE_TIME_COPY {
                        COPIED_MINTS.insert(buy_event.mint);
                    }
                    info!(
                        "[CopyMode][NewToken] Target {} bought {} — queuing buy of {} SOL",
                        buy_event.user, buy_event.mint, *BUY_AMOUNT_SOL
                    );
                    return_data.insert(buy_event.mint, new_token);
                }
            }
        }
    }

    return_data
}
