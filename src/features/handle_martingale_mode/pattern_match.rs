use crate::*;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

/// A single pattern: a sequence of (ComputeUnitLimit, ComputeUnitPrice) values
/// that must match the first N transactions of a token.
pub type CuPattern = Vec<(u32, u64)>;

/// All patterns loaded from pattern.txt.
/// Each line in the file is one pattern (a JSON array of [limit, price] pairs).
pub static CU_PATTERNS: Lazy<Vec<CuPattern>> = Lazy::new(|| {
    load_patterns_from_file("./pattern.txt").unwrap_or_else(|e| {
        eprintln!("Failed to load pattern.txt: {}", e);
        Vec::new()
    })
});

/// Per-token tracking of Compute Budget values across consecutive transactions.
/// Key: token mint pubkey, Value: vector of (unit_limit, unit_price) per transaction.
pub static TOKEN_CU_HISTORY: Lazy<Arc<DashMap<Pubkey, Vec<(u32, u64)>>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// Load patterns from the pattern file.
/// Each line should be a JSON array like: `[[0, 0], [900000, 1000000], ...]`
/// Trailing commas are stripped before parsing.
fn load_patterns_from_file(path: &str) -> Result<Vec<CuPattern>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut patterns: Vec<CuPattern> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Remove trailing comma if present
        let line = line.trim_end_matches(',');

        // Parse as JSON array of arrays: [[u32, u64], ...]
        let parsed: Vec<Vec<u64>> = serde_json::from_str(line)?;

        let pattern: CuPattern = parsed
            .into_iter()
            .map(|pair| {
                let limit = pair.get(0).copied().unwrap_or(0) as u32;
                let price = pair.get(1).copied().unwrap_or(0);
                (limit, price)
            })
            .collect();

        if !pattern.is_empty() {
            patterns.push(pattern);
        }
    }

    info!(
        "[Pattern] Loaded {} CU patterns from pattern.txt",
        patterns.len()
    );

    Ok(patterns)
}

/// Record a transaction's Compute Budget info for a token and check if the
/// accumulated history matches any known pattern. Returns `true` if a pattern
/// match is found (meaning we should buy).
pub fn record_and_match_cu_pattern(
    mint: Pubkey,
    cu_info: ComputeBudgetInfo,
) -> bool {
    let mut entry = TOKEN_CU_HISTORY.entry(mint).or_insert_with(Vec::new);
    let history = entry.value_mut();

    // Append this transaction's CU data
    history.push((cu_info.unit_limit, cu_info.unit_price as u64));

    // Check against every loaded pattern
    for pattern in CU_PATTERNS.iter() {
        let pattern_len = pattern.len();

        // We need exactly `pattern_len` transactions recorded to compare
        if history.len() < pattern_len {
            continue;
        }

        // Compare the first `pattern_len` entries of the history with the pattern
        let first_n = &history[..pattern_len];
        let matched = first_n
            .iter()
            .zip(pattern.iter())
            .all(|((hist_limit, hist_price), (pat_limit, pat_price))| {
                *hist_limit == *pat_limit && *hist_price == *pat_price
            });

        if matched {
            info!(
                "[Pattern Match] Token {} matched CU pattern after {} txs: {:?}",
                mint, pattern_len, pattern
            );
            return true;
        }
    }

    false
}
