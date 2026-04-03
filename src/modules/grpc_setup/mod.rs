use futures::{SinkExt};
use std::{collections::HashMap};
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient, Interceptor};
use yellowstone_grpc_proto::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterTransactions
};

pub async fn setup_grpc_client(
    grpc_endpoint: String,
    x_token: String,
) -> Result<GeyserGrpcClient<impl Interceptor>, Box<dyn std::error::Error>> {
    let client = GeyserGrpcClient::build_from_shared(grpc_endpoint)?
        .x_token(Some(x_token))?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?;

    Ok(client)
}

pub async fn send_subscription_request_grpc<T>(
    mut tx: T,
    subscribe_args: SubscribeRequestFilterTransactions,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: SinkExt<SubscribeRequest> + Unpin,
    <T as futures::Sink<SubscribeRequest>>::Error: std::error::Error + 'static,
{
    let mut accounts_filter = HashMap::new();
    accounts_filter.insert("account_monitor".to_string(), subscribe_args);

    tx.send(SubscribeRequest {
        transactions: accounts_filter,
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    })
    .await?;

    Ok(())
}
