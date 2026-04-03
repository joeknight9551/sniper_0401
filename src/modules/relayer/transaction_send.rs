use crate::*;
use base64;
use serde_json::json;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
    signer::Signer, system_instruction, transaction::Transaction,
};
use std::str::FromStr;
use std::time::Instant;


pub fn init_http_client() {
    let _client = &HTTP_CLIENT;
}


pub async fn send_sender_transaction(
    raw_instructions: Vec<Instruction>,
    tag: String,
) -> Option<String> {
    let start_time = Instant::now();
    let (cu, priority_fee_micro_lamport, third_party_fee) = *PRIORITY_FEE;

    let mut total_instruction = Vec::new();

    // If nonce mode is enabled, the advance_nonce_account instruction MUST be first.
    if *USE_NONCE {
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
    //tip ix
    let tip_receiver = Pubkey::from_str("9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta").unwrap();
    let tip_transfer_instruction = system_instruction::transfer(
        &WALLET_PUB_KEY,                           // Sender's public key
        &tip_receiver,                            // Tip receiver's public key
        (third_party_fee * 10f64.powi(9)) as u64, // Amount to transfer as a tip (0.001 SOL in this case)
    );
    total_instruction.push(tip_transfer_instruction);
    let mut transaction = Transaction::new_with_payer(&total_instruction, Some(&WALLET_PUB_KEY));
    info!("Total ix build took: {:?}", start_time.elapsed());

    // Choose blockhash and signers based on nonce mode
    if *USE_NONCE && is_nonce_ready() {
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

    info!("Signing and serializing took: {:?}", start_time.elapsed());

    // Build the JSON-RPC request
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": format!("{}", chrono::Utc::now().timestamp_millis()),
        "method": "sendTransaction",
        "params": [
            base64_encoded_transaction,
            {
                "encoding": "base64",
                "skipPreflight": true,
                "maxRetries": 0
            }
        ]
    });
    info!("TX making: {:?}", start_time.elapsed());
    let tx_submission_start = Instant::now();
    let response = HTTP_CLIENT
        .post("https://rpc.ny.shyft.to/?api_key=rMoG4rcm8MbzVcPF")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await;
    match response {
        Ok(response_data) => {
            println!("{:?}", response_data);
            let response_json: serde_json::Value = response_data.json().await.unwrap();
            if let Some(result) = response_json.get("result") {
                println!(
                    "Transaction(sender) submission took: {:?}",
                    tx_submission_start.elapsed()
                );
                info!(
                    "[SUBMIT]
                        \t* Service: Sender
                        \t* Hash: {:?}
                        \t* {}",
                    result,
                    tag.clone()
                );
                // Refresh nonce in background so it's ready for next tx
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