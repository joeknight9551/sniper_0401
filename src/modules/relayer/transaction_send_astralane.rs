use crate::*;
use base64;
use serde_json::json;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
    system_instruction, transaction::Transaction,
};
use std::str::FromStr;
use std::time::Instant;

/// Astralane tip account
const ASTRALANE_TIP_ACCOUNT: &str = "astra4uejePWneqNaJKuFFA8oonqCE1sqF6b45kDMZm";

pub async fn send_astralane_transaction(
    raw_instructions: Vec<Instruction>,
    tag: String,
    skip_nonce: bool,
) -> Option<String> {
    let start_time = Instant::now();
    let (cu, priority_fee_micro_lamport, third_party_fee) = *PRIORITY_FEE;

    let mut total_instruction = Vec::new();

    // If nonce mode is enabled AND not explicitly skipped, advance_nonce_account MUST be first.
    let use_nonce_this_tx = *USE_NONCE && !skip_nonce;
    if use_nonce_this_tx {
        if let Some(advance_ix) = get_advance_nonce_ix() {
            total_instruction.push(advance_ix);
        }
    }

    //budget compute unit limit
    total_instruction.push(ComputeBudgetInstruction::set_compute_unit_limit(cu as u32));
    //compute unit price
    total_instruction.push(ComputeBudgetInstruction::set_compute_unit_price(
        priority_fee_micro_lamport,
    ));
    //pure ix
    total_instruction.extend(raw_instructions);
    //tip ix — Astralane tip account
    let tip_receiver = Pubkey::from_str(ASTRALANE_TIP_ACCOUNT).unwrap();
    let tip_transfer_instruction = system_instruction::transfer(
        &WALLET_PUB_KEY,
        &tip_receiver,
        (third_party_fee * 10f64.powi(9)) as u64,
    );
    total_instruction.push(tip_transfer_instruction);
    let mut transaction = Transaction::new_with_payer(&total_instruction, Some(&WALLET_PUB_KEY));

    // Choose blockhash and signers based on nonce mode
    if use_nonce_this_tx && is_nonce_ready() {
        let nonce_hash = get_nonce_hash();
        let nonce_authority = NONCE_AUTHORITY
            .as_ref()
            .expect("NONCE_AUTHORITY must be set when use_nonce is true");
        transaction
            .try_sign(
                &[WALLET_KEYPAIR.insecure_clone(), nonce_authority.insecure_clone()],
                nonce_hash,
            )
            .expect("Failed to sign transaction with nonce");
    } else {
        transaction
            .try_sign(&[WALLET_KEYPAIR.insecure_clone()], get_slot())
            .expect("Failed to sign transaction");
    }

    let serialized_transaction = bincode::serialize(&transaction).unwrap();
    let base64_encoded_transaction = base64::encode(serialized_transaction);

    // Build the JSON-RPC request
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [
            base64_encoded_transaction,
            {
                "encoding": "base64",
                "skipPreflight": true,
            }
        ]
    });
    let tx_submission_start = Instant::now();
    let response = HTTP_CLIENT
        .post(&*ASTRALANE_ENDPOINT)
        .json(&request_body)
        .send()
        .await;
    match response {
        Ok(response_data) => {
            let response_json: serde_json::Value = response_data.json().await.unwrap();
            if let Some(result) = response_json.get("result") {
                info!(
                    "[SUBMIT] Service: ASTRALANE | RTT: {:?} | Total: {:?} | Hash: {:?} | {}",
                    tx_submission_start.elapsed(),
                    start_time.elapsed(),
                    result,
                    tag.clone()
                );
                spawn_nonce_refresh();
                return Some(result.to_string());
            } else {
                return None;
            }
        }
        Err(_) => {
            return None;
        }
    }
}
