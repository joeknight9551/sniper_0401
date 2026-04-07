use crate::*;
use futures::FutureExt;
use futures::future::BoxFuture;
use solana_sdk::instruction::Instruction;
use solana_sdk::signature::Signature;
use tokio::time::{Duration, sleep};

#[derive(PartialEq)]
pub enum TradeType {
    Buy,
    Sell,
}

pub fn confirm(
    raw_instructions: Vec<Instruction>,
    tag: String,
) -> BoxFuture<'static, Option<Signature>> {
    async move {
        let results = match CONFIRM_SERVICE.as_str() {
            "ASTRALANE" => send_astralane_transaction(raw_instructions, tag.clone()).await,
            _ => send_zero_slot_transaction(raw_instructions, tag.clone()).await,
        };

        info!(
            "[SUBMIT]
                \t* Service: {}
                \t* Hash: {:?}
                \t* {}",
            *CONFIRM_SERVICE,
            results,
            tag.clone()
        );

        if let Some(signature_str) = results {
            if let Some(confirmed_sig) = wait_for_confirmation(&signature_str, tag.clone()).await {
                return Some(confirmed_sig);
            } else {
                return None;
            }
        }

        if let Some(result_raw) = results {
            match result_raw.parse::<Signature>() {
                Ok(sig) => {
                    success!(
                        "[CONFIRM]
                            \t* CHECK : {}
                            \t* {}",
                        solscan!(sig.to_string()),
                        tag.clone()
                    );
                    Some(sig)
                }
                Err(_) => None,
            }
        } else {
            None
        }
    }
    .boxed()
}

pub async fn wait_for_confirmation(signature_str: &str, tag: String) -> Option<Signature> {
    let trimed_clean_sig = signature_str
        .trim()
        .replace("\"", "")
        .replace("'", "")
        .replace("\n", "")
        .replace("\r", "");
    let signature = match trimed_clean_sig.parse::<Signature>() {
        Ok(sig) => sig,
        Err(_) => {
            error!(
                "[FORCE_CHECK]
                \t* Check : {}
                \t* {}
                \t* States : Invalid signature",
                solscan!(signature_str),
                tag.clone()
            );

            return None;
        }
    };

    let mut attempts = 0;

    loop {
        match RPC_CLIENT.get_signature_statuses(&[signature]).await {
            Ok(statuses) => {
                if let Some(Some(status)) = statuses.value.get(0) {
                    if status.confirmations.is_none() || status.confirmations.unwrap_or(0) > 0 {
                        success!(
                            "[FORCE_CHECK]
                            \t* Check : {}
                            \t* States : Confirmed
                            \t* {}",
                            solscan!(signature),
                            tag
                        );
                        return Some(signature);
                    }
                }
            }
            Err(_) => {}
        }

        attempts += 1;
        if attempts >= 10 {
            error!(
                "[FORCE_CHECK]
                \t* Check : https://solscan.io/tx/{}
                \t* States : Failed
                \t* {}",
                signature,
                tag.clone()
            );
            return None;
        }

        sleep(Duration::from_secs(2)).await;
    }
}