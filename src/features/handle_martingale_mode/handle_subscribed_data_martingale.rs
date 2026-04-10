use crate::*;
use futures::StreamExt;
use std::{sync::atomic::Ordering};
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, tonic::Status};

pub async fn process_martingale_mode<S>(mut stream: S) -> Result<(), Box<dyn std::error::Error>>
where
    S: StreamExt<Item = Result<SubscribeUpdate, Status>> + Unpin,
{
    // Eagerly initialize creator wallet whitelist on startup
    let _ = &*CREATOR_WALLETS;

    while let Some(result) = stream.next().await {
        match result {
            Ok(update) => {
                if AUTO_TURN_OFF.load(Ordering::Relaxed) {
                    break;
                };
                extract_transaction_data(&update);
                let (account_keys, ixs, inner_ixs, tx_id, _signers) =
                    if let Some(data) = extract_transaction_data(&update) {
                        data
                    } else {
                        continue;
                    };

                // Extract Compute Budget info (SetComputeUnitLimit + SetComputeUnitPrice)
                let cu_info = extract_compute_budget(&ixs, &account_keys);

                let ix_info =
                    filter_by_program_id(ixs, inner_ixs, account_keys.clone(), PUMPFUN_PROGRAM_ID)
                        .unwrap();
                let trade_data = get_trade_info(ix_info, account_keys.clone());

                let token_data_map = handle_sniper_event(trade_data, tx_id, cu_info).await;

                make_sniper_tx(&token_data_map).await;
            }

            Err(e) => {
                println!("Stream error: {}", e);
            }
        }
    }

    Ok(())
}
