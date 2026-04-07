use crate::*;
use futures::{SinkExt, StreamExt};
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio::time::{Duration, sleep};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterTransactions,
};

/// System program ID
const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
/// System program transfer instruction discriminator (u32 LE = 2)
const TRANSFER_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];

/// Represents an outgoing SOL transfer parsed from a transaction.
struct SolTransfer {
    to: Pubkey,
    lamports: u64,
}

/// Connect gRPC and return subscribe halves, mapping errors to String for Send safety.
async fn connect_wallet_grpc(
    wallet: Pubkey,
) -> Result<
    impl futures::Stream<Item = Result<yellowstone_grpc_proto::geyser::SubscribeUpdate, yellowstone_grpc_proto::tonic::Status>>,
    String,
> {
    let mut client = setup_grpc_client(GRPC_ENDPOINT.to_string(), GRPC_TOKEN.to_string())
        .await
        .map_err(|e| e.to_string())?;

    let (subscribe_tx, subscribe_rx) = client.subscribe().await.map_err(|e| e.to_string())?;

    send_wallet_subscription(subscribe_tx, &wallet)
        .await
        .map_err(|e| e.to_string())?;

    Ok(subscribe_rx)
}

/// Run the wallet tracker. Connects via gRPC and follows the wallet chain.
/// Sets WALLET_TRACKING_CONFIRMED when distribution threshold is met.
pub async fn start_wallet_tracker() {
    let tracking_wallet_str = &CONFIG.wallet_tracking_config.tracking_wallet;
    if tracking_wallet_str.is_empty() {
        info!("[WalletTracker] No tracking wallet configured, wallet filter disabled");
        // If no wallet configured, auto-confirm so CU pattern alone works
        WALLET_TRACKING_CONFIRMED.store(true, Ordering::SeqCst);
        return;
    }

    let start_wallet =
        Pubkey::from_str(tracking_wallet_str).expect("Invalid tracking wallet address");

    info!("[WalletTracker] Tracking wallet: {}", start_wallet);

    let min_recipients = CONFIG.wallet_tracking_config.min_distribution_recipients;
    let min_dist_ratio = CONFIG.wallet_tracking_config.min_distribution_ratio;
    let max_skip_ratio = CONFIG.wallet_tracking_config.max_skip_ratio;
    let chain_min_balance = CONFIG.wallet_tracking_config.chain_transfer_min_balance_lamports;

    // Get initial SOL balance of wallet (a) — this is X
    let initial_balance = loop {
        match RPC_CLIENT.get_balance(&start_wallet).await {
            Ok(bal) => {
                info!(
                    "[WalletTracker] Initial balance (X): {} lamports ({:.4} SOL)",
                    bal,
                    bal as f64 / 1e9
                );
                break bal;
            }
            Err(e) => {
                error!("[WalletTracker] Failed to get balance: {}, retrying...", e);
                sleep(Duration::from_millis(500)).await;
            }
        }
    };

    if initial_balance == 0 {
        info!("[WalletTracker] Wallet has 0 balance, waiting for funds...");
    }

    let mut current_wallet = start_wallet;
    let mut x_lamports = initial_balance;
    let mut distribution_recipients: HashSet<Pubkey> = HashSet::new();
    let mut distribution_total: u64 = 0;
    let mut is_distributing = false;
    let mut distribution_start_time: Option<Instant> = None;

    // Main tracking loop — reconnects on error
    loop {
        info!(
            "[WalletTracker] Subscribing to wallet: {}",
            current_wallet
        );

        let mut subscribe_rx = match connect_wallet_grpc(current_wallet).await {
            Ok(rx) => rx,
            Err(e) => {
                error!("[WalletTracker] gRPC setup failed: {}, retrying...", e);
                sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        // Process transaction stream
        while let Some(result) = subscribe_rx.next().await {
            let update = match result {
                Ok(u) => u,
                Err(e) => {
                    error!("[WalletTracker] Stream error: {}, reconnecting...", e);
                    break;
                }
            };

            // Extract transaction data from gRPC update
            let (account_keys, ixs, _inner_ixs, tx_id, pre_balances, post_balances) =
                match extract_wallet_tx_data(&update) {
                    Some(data) => data,
                    None => continue,
                };

            // Detect incoming SOL: if wallet's balance increased, update X
            let wallet_idx = account_keys.iter().position(|k| *k == current_wallet);
            if let Some(idx) = wallet_idx {
                let pre = pre_balances.get(idx).copied().unwrap_or(0);
                let post = post_balances.get(idx).copied().unwrap_or(0);
                if post > pre {
                    let received = post - pre;
                    x_lamports += received;
                    info!(
                        "[WalletTracker] Incoming: +{:.4} SOL to {} | X updated to {:.4} SOL",
                        received as f64 / 1e9,
                        current_wallet,
                        x_lamports as f64 / 1e9,
                    );
                }
            }

            // Find outgoing SOL transfers from the current wallet
            let transfers = parse_sol_transfers(&account_keys, &ixs, &current_wallet);
            if transfers.is_empty() {
                continue;
            }

            // Get current wallet's post-balance
            let wallet_post_balance = wallet_idx
                .and_then(|idx| post_balances.get(idx))
                .copied()
                .unwrap_or(0);

            let total_sent: u64 = transfers.iter().map(|t| t.lamports).sum();
            let recipient_count = transfers.len();

            info!(
                "[WalletTracker] TX: {} | Wallet: {} | Sent: {:.4} SOL to {} recipient(s) | PostBal: {:.4} SOL",
                tx_id,
                current_wallet,
                total_sent as f64 / 1e9,
                recipient_count,
                wallet_post_balance as f64 / 1e9,
            );

            if !is_distributing {
                // Find the largest transfer — if it carries the bulk of SOL,
                // treat it as a chain transfer and follow that wallet.
                // e.g. 200 SOL total: 198 to wallet B, 2 split to others → follow B.
                let largest = transfers.iter().max_by_key(|t| t.lamports).unwrap();
                let rest_total: u64 = transfers.iter()
                    .filter(|t| t.to != largest.to)
                    .map(|t| t.lamports)
                    .sum();

                // Chain transfer: the largest recipient got most of the SOL
                // and what's left (post-balance + small sends) is negligible
                let bulk_of_sol = x_lamports > 0 && largest.lamports as f64 >= x_lamports as f64 * 0.9
                    || x_lamports == 0 && largest.lamports as f64 >= total_sent as f64 * 0.9;

                if bulk_of_sol && wallet_post_balance < chain_min_balance {
                    let next_wallet = largest.to;
                    info!(
                        "[WalletTracker] Chain transfer detected: {} → {} ({:.4} SOL, ignored {:.4} SOL to {} others)",
                        current_wallet,
                        next_wallet,
                        largest.lamports as f64 / 1e9,
                        rest_total as f64 / 1e9,
                        recipient_count - 1,
                    );

                    // Update X to the amount that moved to the next wallet
                    if x_lamports == 0 {
                        x_lamports = largest.lamports;
                        info!(
                            "[WalletTracker] Updated X = {:.4} SOL",
                            x_lamports as f64 / 1e9
                        );
                    }

                    current_wallet = next_wallet;
                    // Break inner loop to reconnect gRPC with new wallet
                    break;
                }

                // Otherwise → distribution started
                if !bulk_of_sol || wallet_post_balance >= chain_min_balance {
                    is_distributing = true;
                    distribution_start_time = Some(Instant::now());

                    // If X was 0 (starting wallet had no funds initially), use pre-distribution balance
                    if x_lamports == 0 {
                        x_lamports = total_sent + wallet_post_balance;
                        info!(
                            "[WalletTracker] Updated X = {:.4} SOL (from distribution start)",
                            x_lamports as f64 / 1e9
                        );
                    }

                    info!(
                        "[WalletTracker] Distribution phase started from wallet: {}",
                        current_wallet
                    );
                }
            }

            if is_distributing {
                // Check if distribution window (1s) has expired
                let elapsed = distribution_start_time
                    .map(|t| t.elapsed())
                    .unwrap_or(Duration::ZERO);

                if elapsed > Duration::from_secs(1) {
                    let dist_ratio = if x_lamports > 0 {
                        distribution_total as f64 / x_lamports as f64
                    } else {
                        0.0
                    };
                    info!(
                        "[WalletTracker] Distribution window expired (>1s). {:.4} SOL ({:.1}%) to {} recipients — NOT confirmed",
                        distribution_total as f64 / 1e9,
                        dist_ratio * 100.0,
                        distribution_recipients.len(),
                    );

                    // Reset and go back to original wallet
                    distribution_recipients.clear();
                    distribution_total = 0;
                    is_distributing = false;
                    distribution_start_time = None;
                    current_wallet = start_wallet;
                    x_lamports = 0;
                    break;
                }

                for transfer in &transfers {
                    distribution_recipients.insert(transfer.to);
                    distribution_total += transfer.lamports;
                }

                let dist_ratio = if x_lamports > 0 {
                    distribution_total as f64 / x_lamports as f64
                } else {
                    0.0
                };

                info!(
                    "[WalletTracker] Distribution: {:.4} SOL to {} recipients ({:.1}% of X={:.4} SOL) [{:.0}ms elapsed]",
                    distribution_total as f64 / 1e9,
                    distribution_recipients.len(),
                    dist_ratio * 100.0,
                    x_lamports as f64 / 1e9,
                    elapsed.as_millis(),
                );

                // Check confirmation: enough recipients AND enough SOL distributed (within 1s)
                if distribution_recipients.len() >= min_recipients
                    && dist_ratio >= min_dist_ratio
                {
                    info!(
                        "[WalletTracker] CONFIRMED! {:.4} SOL ({:.1}%) distributed to {} wallets in {:.0}ms",
                        distribution_total as f64 / 1e9,
                        dist_ratio * 100.0,
                        distribution_recipients.len(),
                        elapsed.as_millis(),
                    );
                    WALLET_TRACKING_CONFIRMED.store(true, Ordering::SeqCst);

                    // Reset state for next cycle
                    distribution_recipients.clear();
                    distribution_total = 0;
                    is_distributing = false;
                    distribution_start_time = None;
                    // Go back to tracking the original wallet
                    current_wallet = start_wallet;
                    x_lamports = 0; // Will be refreshed on next cycle
                    break; // Reconnect with original wallet
                }

                // Reject early: distributed too little
                if dist_ratio < max_skip_ratio
                    && wallet_post_balance < chain_min_balance
                {
                    info!(
                        "[WalletTracker] Distribution too small ({:.1}% < {}%), NOT confirmed",
                        dist_ratio * 100.0,
                        max_skip_ratio * 100.0,
                    );

                    // Reset and go back to original wallet
                    distribution_recipients.clear();
                    distribution_total = 0;
                    is_distributing = false;
                    distribution_start_time = None;
                    current_wallet = start_wallet;
                    x_lamports = 0;
                    break;
                }

                // Check if wallet is now empty — distribution is done but didn't hit confirm
                if wallet_post_balance < chain_min_balance {
                    info!(
                        "[WalletTracker] Distribution done ({:.1}%) but below confirm threshold ({:.1}%)",
                        dist_ratio * 100.0,
                        min_dist_ratio * 100.0,
                    );

                    // Reset and go back to original wallet
                    distribution_recipients.clear();
                    distribution_total = 0;
                    is_distributing = false;
                    distribution_start_time = None;
                    current_wallet = start_wallet;
                    x_lamports = 0;
                    break;
                }
            }
        }
    }
}

/// Extract transaction data from a gRPC update, including pre- and post-balances.
fn extract_wallet_tx_data(
    update: &yellowstone_grpc_proto::geyser::SubscribeUpdate,
) -> Option<(
    Vec<Pubkey>,
    Vec<yellowstone_grpc_proto::prelude::CompiledInstruction>,
    Vec<yellowstone_grpc_proto::prelude::InnerInstruction>,
    String,
    Vec<u64>,
    Vec<u64>,
)> {
    let transaction_update = match &update.update_oneof {
        Some(UpdateOneof::Transaction(tx_update)) => tx_update,
        _ => return None,
    };

    let tx_info = transaction_update.transaction.as_ref()?;
    let transaction = tx_info.transaction.as_ref()?;
    let meta = tx_info.meta.as_ref()?;
    let tx_msg = transaction.message.as_ref()?;

    let mut account_keys: Vec<Pubkey> = tx_msg
        .account_keys
        .iter()
        .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok())
        .collect();

    account_keys.extend(
        meta.loaded_writable_addresses
            .iter()
            .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok()),
    );
    account_keys.extend(
        meta.loaded_readonly_addresses
            .iter()
            .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok()),
    );

    let ixs = tx_msg.instructions.clone();
    let inner_ixs = meta
        .inner_instructions
        .iter()
        .flat_map(|ix| ix.instructions.clone())
        .collect();

    let tx_id = solana_sdk::bs58::encode(&tx_info.signature).into_string();
    let pre_balances = meta.pre_balances.clone();
    let post_balances = meta.post_balances.clone();

    Some((account_keys, ixs, inner_ixs, tx_id, pre_balances, post_balances))
}

/// Parse outgoing SOL transfers (system program transfer instruction) from the tracked wallet.
fn parse_sol_transfers(
    account_keys: &[Pubkey],
    ixs: &[yellowstone_grpc_proto::prelude::CompiledInstruction],
    from_wallet: &Pubkey,
) -> Vec<SolTransfer> {
    let system_program = Pubkey::from_str(SYSTEM_PROGRAM).unwrap();
    let system_idx = match account_keys.iter().position(|k| *k == system_program) {
        Some(idx) => idx as u32,
        None => return vec![],
    };

    let mut transfers = Vec::new();

    for ix in ixs {
        if ix.program_id_index != system_idx {
            continue;
        }
        if ix.data.len() < 12 {
            continue;
        }
        // Check transfer discriminator
        if ix.data[0..4] != TRANSFER_DISCRIMINATOR {
            continue;
        }
        // instruction accounts: [from, to]
        if ix.accounts.len() < 2 {
            continue;
        }

        let from_idx = ix.accounts[0] as usize;
        let to_idx = ix.accounts[1] as usize;

        let from_key = match account_keys.get(from_idx) {
            Some(k) => k,
            None => continue,
        };

        if from_key != from_wallet {
            continue;
        }

        let to_key = match account_keys.get(to_idx) {
            Some(k) => k,
            None => continue,
        };

        let lamports = u64::from_le_bytes(ix.data[4..12].try_into().unwrap_or([0; 8]));
        if lamports > 0 {
            transfers.push(SolTransfer {
                to: *to_key,
                lamports,
            });
        }
    }

    transfers
}

/// Send a gRPC subscription for all transactions involving a specific wallet.
async fn send_wallet_subscription<T>(
    mut tx: T,
    wallet: &Pubkey,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    T: SinkExt<SubscribeRequest> + Unpin,
    <T as futures::Sink<SubscribeRequest>>::Error: std::error::Error + Send + Sync + 'static,
{
    let mut txn_filter = HashMap::new();
    txn_filter.insert(
        "wallet_tracker".to_string(),
        SubscribeRequestFilterTransactions {
            account_include: vec![wallet.to_string()],
            account_exclude: vec![],
            account_required: vec![],
            vote: Some(false),
            failed: Some(false),
            signature: None,
        },
    );

    tx.send(SubscribeRequest {
        transactions: txn_filter,
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    })
    .await?;

    Ok(())
}
