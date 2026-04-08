use crate::*;
use futures::StreamExt;
use std::sync::atomic::Ordering;
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, tonic::Status};

pub async fn process_copy_mode<S>(mut stream: S) -> Result<(), Box<dyn std::error::Error>>
where
    S: StreamExt<Item = Result<SubscribeUpdate, Status>> + Unpin,
{
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

                let ix_info =
                    match filter_by_program_id(ixs, inner_ixs, account_keys.clone(), PUMPFUN_PROGRAM_ID) {
                        Ok(info) => info,
                        Err(_) => continue,
                    };

                let trade_data = get_trade_info(ix_info, account_keys);

                // Fast-path: for transactions NOT involving target wallets or our
                // own wallet, only store cashback_enabled from mint events and skip
                // the rest. This avoids spawning tasks for the huge volume of
                // unrelated PumpFun transactions.
                let involves_us = signers.iter().any(|s| {
                    let s_str = s.to_string();
                    *s == *WALLET_PUB_KEY || TARGET_WALLETS.iter().any(|w| *w == s_str)
                });

                if !involves_us {
                    // Only harvest mint events for cashback info
                    for mint_event in &trade_data.0 {
                        CASHBACK_CACHE.insert(mint_event.mint, mint_event.cashback_enabled);
                    }
                    continue;
                }

                // Also store cashback for any mint events in target/own-wallet TXs
                for mint_event in &trade_data.0 {
                    CASHBACK_CACHE.insert(mint_event.mint, mint_event.cashback_enabled);
                }

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
