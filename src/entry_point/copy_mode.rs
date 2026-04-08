use sniper::*;
use yellowstone_grpc_proto::geyser::SubscribeRequestFilterTransactions;

#[tokio::main]
pub async fn main() {
    show_bot_settings().await;
    println!("Mode: COPY TRADING");
    println!("Target Wallets: {:?}", *TARGET_WALLETS);
    println!(
        "Fixed Buy Amount: {} SOL | One-Time-Copy: {}",
        *BUY_AMOUNT_SOL, *ONE_TIME_COPY
    );

    init_nonce_cache().await;

    tokio::spawn(async {
        loop {
            recent_block_handler().await;
        }
    });

    tokio::spawn(async {
        loop {
            check_no_activity_tokens().await;
        }
    });

    let mut grpc_client = setup_grpc_client(GRPC_ENDPOINT.to_string(), GRPC_TOKEN.to_string())
        .await
        .unwrap();
    let (subscribe_tx, subscribe_rx) = grpc_client.subscribe().await.unwrap();

    // Subscribe to ALL PumpFun transactions so we receive mint events
    // (which carry cashback_enabled). Target wallet filtering is done in code.
    let subscribe_filter = SubscribeRequestFilterTransactions {
        account_include: vec![],
        account_exclude: vec![],
        account_required: vec![PUMPFUN_PROGRAM_ID.to_string()],
        vote: Some(false),
        failed: Some(false),
        signature: None,
    };

    send_subscription_request_grpc(subscribe_tx, subscribe_filter)
        .await
        .unwrap();

    let _ = process_copy_mode(subscribe_rx).await;
}
