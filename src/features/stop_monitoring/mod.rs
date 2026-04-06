use crate::*;
use chrono::Utc;
use solana_sdk::pubkey::Pubkey;
use tokio::time::{Duration, sleep};

pub async fn check_no_activity_tokens() {
    if *STOP_NO_ACTIVITY_TOKEN_MONITORING {
        let keys: Vec<Pubkey> = TOKEN_DB
            .map
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for token_key in keys {
            if let Some(token_data) = TOKEN_DB.get(token_key).unwrap() {
                // Skip purchased tokens — their sell is handled by the
                // per-pattern holding-time timeout in make_sniper_tx.
                if token_data.token_is_purchased && token_data.token_balance > 0 {
                    continue;
                }

                if Utc::now().timestamp() - token_data.last_event.last_activity_timestamp
                    >= *NO_ACTIVITY_TIME
                {
                    let instruction = {
                        alert!(
                            "[Stop-Tracking]\t\t*Mint: {}\t*No activity in last {} seconds",
                            token_key,
                            *NO_ACTIVITY_TIME
                        );
                        let _ = TOKEN_DB.delete(token_key);

                        (vec![], "".to_string())
                    };

                    let (ix, tag) = instruction;

                    if !ix.is_empty() {
                        let ix_clone = ix.clone();
                        let tag_clone = tag.clone();                 
                        tokio::spawn(async move {
                            let _ = confirm(ix_clone, tag_clone).await;
                        });
                    }
                }
            }
        }
    }

    sleep(Duration::from_millis(500)).await;
}