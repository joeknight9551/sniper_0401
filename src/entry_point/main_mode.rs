use std::vec;

use sniper::*;
use yellowstone_grpc_proto::geyser::SubscribeRequestFilterTransactions;

#[tokio::main]
pub async fn main() {
    show_bot_settings().await;

    // Initialize the durable nonce cache (fetches nonce hash from RPC once).
    // This must happen before any transactions are sent.
    init_nonce_cache().await;

    tokio::spawn(async {
        loop {
            recent_block_handler().await;
        }
    });

    tokio::spawn({
        async {
            loop {
                check_no_activity_tokens().await;
            }
        }
    });

    let mut grpc_client = setup_grpc_client(GRPC_ENDPOINT.to_string(), GRPC_TOKEN.to_string())
        .await
        .unwrap();
    let (subscribe_tx, subscribe_rx) = grpc_client.subscribe().await.unwrap();
    let subscribe_pumpfun_program_id = SubscribeRequestFilterTransactions {
        account_include: vec![],
        account_exclude: vec![],
        account_required: vec![PUMPFUN_PROGRAM_ID.to_string()],
        vote: Some(false),
        failed: Some(false),
        signature: None,
    };

    send_subscription_request_grpc(subscribe_tx, subscribe_pumpfun_program_id)
        .await
        .unwrap();

    let _ = process_martingale_mode(subscribe_rx).await;
}
