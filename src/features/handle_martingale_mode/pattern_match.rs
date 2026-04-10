use crate::*;
use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::str::FromStr;

/// Whitelisted creator wallet addresses loaded from pattern.txt (one pubkey per line).
pub static CREATOR_WALLETS: Lazy<HashSet<Pubkey>> = Lazy::new(|| {
    load_creator_wallets("./pattern.txt").unwrap_or_else(|e| {
        eprintln!("Failed to load pattern.txt: {}", e);
        HashSet::new()
    })
});

fn load_creator_wallets(path: &str) -> Result<HashSet<Pubkey>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut wallets = HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match Pubkey::from_str(line) {
            Ok(pk) => {
                info!("[Pattern] Loaded creator wallet: {}", pk);
                wallets.insert(pk);
            }
            Err(e) => {
                eprintln!("[Pattern] Invalid pubkey '{}': {}", line, e);
            }
        }
    }

    info!(
        "[Pattern] Loaded {} creator wallets from pattern.txt",
        wallets.len()
    );

    Ok(wallets)
}

/// Check if a creator pubkey is in the whitelist.
pub fn is_creator_whitelisted(creator: &Pubkey) -> bool {
    CREATOR_WALLETS.contains(creator)
}
