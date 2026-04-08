use borsh::BorshDeserialize;
use crate::*;
use futures::StreamExt;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, tonic::Status};

/// Cache creator_vault + cashback from observed buy/sell/mint IX accounts.
/// Called for target/own-wallet TXs after full parsing.
#[inline]
fn cache_from_trade_data(trade_data: &(
    Vec<MintEvent>,
    Vec<BuyEvent>,
    Vec<SellEvent>,
    Vec<MintInstructionAccounts>,
    Vec<BuyInstructionAccounts>,
    Vec<SellInstructionAccounts>,
)) {
    for mint_event in &trade_data.0 {
        CASHBACK_CACHE.insert(mint_event.mint, mint_event.cashback_enabled);
    }
    for buy_ix in &trade_data.4 {
        CREATOR_VAULT_CACHE.insert(buy_ix.mint, buy_ix.creator_vault);
    }
    for sell_ix in &trade_data.5 {
        CREATOR_VAULT_CACHE.insert(sell_ix.mint, sell_ix.creator_vault);
    }
}

/// Lightweight cache pass for non-relevant TXs.
/// Extracts only mint→cashback and mint→creator_vault from raw IX data
/// WITHOUT Borsh-deserializing TradeEvents (the expensive part).
#[inline]
fn cache_lightweight(ix_infos: &[InstructionRawData], account_keys: &[Pubkey]) {
    let mint_event_prefix: Vec<u8> = [
        &PUMP_FUN_EVENT_LOG_DISCRIMINATOR[..],
        &PUMP_FUN_MINT_EVENT_DISCRIMINATOR[..],
    ].concat();

    for info in ix_infos {
        if info.data.starts_with(&mint_event_prefix) && info.data.len() > 16 {
            // Borsh-deserialize only MintEvent (rare — most TXs are trades, not mints)
            if let Ok(mint_event) = MintEvent::deserialize(&mut &info.data[16..]) {
                CASHBACK_CACHE.insert(mint_event.mint, mint_event.cashback_enabled);
            }
        } else if (info.data.starts_with(&PUMP_FUN_BUY_DISCRIMINATOR)
            || info.data.starts_with(&PUMP_FUN_BUY_EXACT_SOL_IN_DISCRIMINATOR))
            && info.accounts.len() >= 10
        {
            let mint = account_keys[info.accounts[2] as usize];
            let creator_vault = account_keys[info.accounts[9] as usize];
            CREATOR_VAULT_CACHE.insert(mint, creator_vault);
        } else if info.data.starts_with(&PUMP_FUN_SELL_DISCRIMINATOR)
            && info.accounts.len() >= 9
        {
            let mint = account_keys[info.accounts[2] as usize];
            let creator_vault = account_keys[info.accounts[8] as usize];
            CREATOR_VAULT_CACHE.insert(mint, creator_vault);
        }
    }
}

pub async fn process_copy_mode<S>(mut stream: S) -> Result<(), Box<dyn std::error::Error>>
where
    S: StreamExt<Item = Result<SubscribeUpdate, Status>> + Unpin,
{
    // Pre-parse target wallet pubkeys once for fast byte-level comparison
    // instead of base58-encoding every signer on every TX.
    let mut watched_pubkeys: HashSet<Pubkey> = TARGET_WALLETS
        .iter()
        .filter_map(|w| w.parse::<Pubkey>().ok())
        .collect();
    watched_pubkeys.insert(*WALLET_PUB_KEY);

    while let Some(result) = stream.next().await {
        match result {
            Ok(update) => {
                if AUTO_TURN_OFF.load(Ordering::Relaxed) {
                    break;
                }

                let (account_keys, ixs, inner_ixs, tx_id, signers) =
                    if let Some(data) = extract_transaction_data(&update) {
                        data
                    } else {
                        continue;
                    };

                // Check relevance FIRST — before any expensive parsing.
                // Compare raw Pubkey bytes (32-byte memcmp) instead of base58 strings.
                let involves_us = signers.iter().any(|s| watched_pubkeys.contains(s));

                let ix_info =
                    match filter_by_program_id(ixs, inner_ixs, account_keys.clone(), PUMPFUN_PROGRAM_ID) {
                        Ok(info) => info,
                        Err(_) => continue,
                    };

                if !involves_us {
                    // Lightweight cache: no Borsh deserialize of TradeEvents
                    cache_lightweight(&ix_info, &account_keys);
                    continue;
                }

                // Full parse only for target/own-wallet TXs
                let trade_data = get_trade_info(ix_info, account_keys);
                cache_from_trade_data(&trade_data);

                // Spawn processing so the stream loop is never blocked waiting
                // for DB lookups, TX building or network I/O.
                tokio::spawn(async move {
                    let token_data_map = handle_copy_event(trade_data, tx_id).await;
                    make_copy_tx(&token_data_map).await;
                });
            }

            Err(e) => {
                println!("Stream error: {}", e);
            }
        }
    }

    Ok(())
}
