use std::sync::Mutex;

use once_cell::sync::Lazy;
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signer::Signer,
    system_instruction,
    instruction::Instruction,
};
use tokio::time::{Duration, sleep};

use crate::{RPC_CLINET, NONCE_PUBKEY, NONCE_AUTHORITY, USE_NONCE};

/// Cached nonce hash — pre-fetched and ready to use instantly.
/// Updated after every transaction send.
static CACHED_NONCE_HASH: Lazy<Mutex<Hash>> = Lazy::new(|| Mutex::new(Hash::default()));

/// Whether the nonce has been successfully initialized (first fetch done).
static NONCE_READY: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Fetch the nonce account data from RPC and extract the stored nonce hash.
async fn fetch_nonce_hash_from_rpc(nonce_pubkey: &Pubkey) -> Option<Hash> {
    match RPC_CLINET.get_account(nonce_pubkey).await {
        Ok(account) => {
            // The nonce account data is a serialized nonce::State.
            // We need to deserialize it to get the stored nonce hash.
            match bincode::deserialize::<solana_sdk::nonce::state::Versions>(&account.data) {
                Ok(versions) => {
                    let state = versions.state();
                    match state {
                        solana_sdk::nonce::State::Initialized(data) => {
                            Some(data.blockhash())
                        }
                        _ => {
                            eprintln!("[NONCE] Nonce account is not initialized!");
                            None
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[NONCE] Failed to deserialize nonce account data: {:?}", e);
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("[NONCE] Failed to fetch nonce account: {:?}", e);
            None
        }
    }
}

/// Initialize the nonce cache at startup. Retries until successful.
/// Call this once at startup before any transactions.
pub async fn init_nonce_cache() {
    if !*USE_NONCE {
        println!("[NONCE] Nonce mode disabled, skipping init.");
        return;
    }

    let nonce_pubkey = NONCE_PUBKEY.as_ref().expect("NONCE_PUBKEY not set");
    println!("[NONCE] Initializing nonce cache for account: {}", nonce_pubkey);

    loop {
        if let Some(hash) = fetch_nonce_hash_from_rpc(nonce_pubkey).await {
            {
                let mut cached = CACHED_NONCE_HASH.lock().unwrap();
                *cached = hash;
            }
            {
                let mut ready = NONCE_READY.lock().unwrap();
                *ready = true;
            }
            println!("[NONCE] Nonce cache initialized. Hash: {}", hash);
            break;
        }
        eprintln!("[NONCE] Retrying nonce fetch in 500ms...");
        sleep(Duration::from_millis(500)).await;
    }
}

/// Get the cached nonce hash. This is instant (no RPC call).
pub fn get_nonce_hash() -> Hash {
    let cached = CACHED_NONCE_HASH.lock().unwrap();
    *cached
}

/// Check if the nonce cache has been initialized.
pub fn is_nonce_ready() -> bool {
    let ready = NONCE_READY.lock().unwrap();
    *ready
}

/// Refresh the nonce cache after a transaction has been sent.
/// Call this after sending a transaction that used the nonce, because
/// the advance_nonce_account instruction will have changed the stored hash.
/// This runs in the background so it doesn't block the caller.
pub fn spawn_nonce_refresh() {
    if !*USE_NONCE {
        return;
    }
    tokio::spawn(async {
        refresh_nonce().await;
    });
}

/// Actually refresh the nonce hash from RPC.
pub async fn refresh_nonce() {
    if !*USE_NONCE {
        return;
    }

    let nonce_pubkey = match NONCE_PUBKEY.as_ref() {
        Some(pk) => *pk,
        None => return,
    };

    // Small delay to allow the advance_nonce_account to be processed on-chain
    sleep(Duration::from_millis(400)).await;

    for attempt in 0..5 {
        if let Some(hash) = fetch_nonce_hash_from_rpc(&nonce_pubkey).await {
            let changed = {
                let mut cached = CACHED_NONCE_HASH.lock().unwrap();
                if *cached != hash {
                    println!("[NONCE] Refreshed nonce hash: {} (attempt {})", hash, attempt + 1);
                    *cached = hash;
                    true
                } else {
                    false
                }
            }; // MutexGuard dropped here before any .await
            if changed {
                return;
            }
            // Hash hasn't changed yet — the advance hasn't landed. Wait and retry.
            sleep(Duration::from_millis(300)).await;
            continue;
        }
        sleep(Duration::from_millis(300)).await;
    }
    eprintln!("[NONCE] Warning: nonce hash did not change after 5 refresh attempts");
}

/// Build the advance_nonce_account instruction to prepend to transactions.
/// This MUST be the first instruction in the transaction when using durable nonce.
pub fn get_advance_nonce_ix() -> Option<Instruction> {
    if !*USE_NONCE {
        return None;
    }

    let nonce_pubkey = (*NONCE_PUBKEY).as_ref()?;
    let nonce_authority = (*NONCE_AUTHORITY).as_ref()?;

    Some(system_instruction::advance_nonce_account(
        nonce_pubkey,
        &nonce_authority.pubkey(),
    ))
}
