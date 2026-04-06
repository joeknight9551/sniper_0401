use crate::*;
use crate::{
    BuyEvent, BuyInstructionAccounts, MintEvent, MintInstructionAccounts, SellEvent,
    SellInstructionAccounts,
};
use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;

pub async fn handle_sniper_event(
    trade_data: (
        Vec<MintEvent>,
        Vec<BuyEvent>,
        Vec<SellEvent>,
        Vec<MintInstructionAccounts>,
        Vec<BuyInstructionAccounts>,
        Vec<SellInstructionAccounts>,
    ),
    tx_id: String,
    cu_info: ComputeBudgetInfo,
) -> DashMap<Pubkey, TokenDatabaseSchema> {
    let (
        mint_events,
        buy_events,
        sell_events,
        mint_ixs_accounts,
        _buy_ixs_accounts,
        _sell_ixs_accounts,
    ) = trade_data;
    let return_data: DashMap<Pubkey, TokenDatabaseSchema> = DashMap::new();

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

    for (_i, buy_event) in buy_events.iter().enumerate() {
        if let Some(token_data) = TOKEN_DB.get(buy_event.mint).unwrap() {
            let mut updated_token_data: TokenDatabaseSchema = update_status_from_buy_event(
                token_data.clone(),
                buy_event.clone(),
                tx_id.to_string(),
                cu_info,
            );

            // Record CU pattern for this buy transaction on the token
            if let Some(config) = record_and_match_cu_pattern(buy_event.mint, cu_info) {
                if !updated_token_data.token_is_purchased {
                    // Check per-pattern skip-after-loss
                    let should_skip = PATTERN_SKIP_NEXT
                        .get(&config.pattern_index)
                        .map(|v| *v)
                        .unwrap_or(false);
                    if should_skip {
                        // Clear the skip flag — next match will buy
                        PATTERN_SKIP_NEXT.insert(config.pattern_index, false);
                        info!(
                            "[Skip] Pattern #{} skipped due to previous loss | Mint: {}",
                            config.pattern_index, buy_event.mint
                        );
                    } else {
                        updated_token_data.token_take_profit_pct = config.take_profit_pct;
                        updated_token_data.token_holding_time_secs = config.holding_time_secs;
                        updated_token_data.token_pattern_index = config.pattern_index;
                        updated_token_data.token_buy_now = true;
                        let _ = TOKEN_DB.upsert(updated_token_data.token_mint, updated_token_data.clone());
                    }
                }
            }

            return_data.insert(updated_token_data.token_mint, updated_token_data);
        }
    }

    for (_i, sell_event) in sell_events.iter().enumerate() {
        if let Some(token_data) = TOKEN_DB.get(sell_event.mint).unwrap() {
            if let Some(mut updated_token_data) = update_status_from_sell_event(
                token_data.clone(),
                sell_event.clone(),
                tx_id.to_string(),
                cu_info,
            ) {
                // Record CU pattern for this sell transaction on the token
                if let Some(config) = record_and_match_cu_pattern(sell_event.mint, cu_info) {
                    if !updated_token_data.token_is_purchased {
                        // Check per-pattern skip-after-loss
                        let should_skip = PATTERN_SKIP_NEXT
                            .get(&config.pattern_index)
                            .map(|v| *v)
                            .unwrap_or(false);
                        if should_skip {
                            PATTERN_SKIP_NEXT.insert(config.pattern_index, false);
                            info!(
                                "[Skip] Pattern #{} skipped due to previous loss | Mint: {}",
                                config.pattern_index, sell_event.mint
                            );
                        } else {
                            updated_token_data.token_take_profit_pct = config.take_profit_pct;
                            updated_token_data.token_holding_time_secs = config.holding_time_secs;
                            updated_token_data.token_pattern_index = config.pattern_index;
                            updated_token_data.token_buy_now = true;
                            let _ = TOKEN_DB.upsert(updated_token_data.token_mint, updated_token_data.clone());
                        }
                    }
                }

                return_data.insert(updated_token_data.token_mint, updated_token_data);
            }
        }
    }
    return_data
}
