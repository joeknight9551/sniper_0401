use crate::*;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

/// A single pattern entry: CU pairs to match + per-pattern take profit and holding time.
#[derive(Debug, Clone)]
pub struct CuPatternEntry {
    pub cu_pairs: Vec<(u32, u64)>,
    pub take_profit_pct: f64,
    pub holding_time_secs: u64,
}

/// Returned when a pattern matches, carrying the pattern's trade settings.
#[derive(Debug, Clone, Copy)]
pub struct MatchedPatternConfig {
    pub pattern_index: usize,
    pub take_profit_pct: f64,
    pub holding_time_secs: u64,
}

/// Per-pattern skip-after-loss flag.
/// Key: pattern index, Value: true = skip the next match for this pattern.
pub static PATTERN_SKIP_NEXT: Lazy<Arc<DashMap<usize, bool>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// All patterns loaded from pattern.txt.
/// Each line: `[[cu_limit, cu_price], ..., take_profit_pct, holding_time_secs]`
pub static CU_PATTERNS: Lazy<Vec<CuPatternEntry>> = Lazy::new(|| {
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
/// Each line: `[[cu_limit, cu_price], ..., take_profit_pct, holding_time_secs]`
/// The last two plain numbers are take-profit percentage and holding time in seconds.
fn load_patterns_from_file(path: &str) -> Result<Vec<CuPatternEntry>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut patterns: Vec<CuPatternEntry> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Remove trailing comma if present
        let line = line.trim_end_matches(',');

        // Parse as mixed JSON array: [[u32, u64], ..., number, number]
        let parsed: Vec<serde_json::Value> = serde_json::from_str(line)?;

        // Need at least 3 elements: 1+ CU pairs + take_profit + holding_time
        if parsed.len() < 3 {
            continue;
        }

        let holding_time_secs = parsed.last().and_then(|v| v.as_u64()).unwrap_or(200);
        let take_profit_pct = parsed[parsed.len() - 2].as_f64().unwrap_or(500.0);

        let cu_pairs: Vec<(u32, u64)> = parsed[..parsed.len() - 2]
            .iter()
            .filter_map(|v| {
                let arr = v.as_array()?;
                let limit = arr.get(0)?.as_u64()? as u32;
                let price = arr.get(1)?.as_u64()?;
                Some((limit, price))
            })
            .collect();

        if !cu_pairs.is_empty() {
            info!(
                "[Pattern] CU: {:?} | TP: {}% | Hold: {}s",
                cu_pairs, take_profit_pct, holding_time_secs
            );
            patterns.push(CuPatternEntry {
                cu_pairs,
                take_profit_pct,
                holding_time_secs,
            });
        }
    }

    info!(
        "[Pattern] Loaded {} CU patterns from pattern.txt",
        patterns.len()
    );

    Ok(patterns)
}

/// Record a transaction's Compute Budget info for a token and check if the
/// accumulated history matches any known pattern. Returns the matched pattern's
/// trade config (take profit %, holding time) if found.
pub fn record_and_match_cu_pattern(
    mint: Pubkey,
    cu_info: ComputeBudgetInfo,
) -> Option<MatchedPatternConfig> {
    let mut entry = TOKEN_CU_HISTORY.entry(mint).or_insert_with(Vec::new);
    let history = entry.value_mut();

    // Append this transaction's CU data
    history.push((cu_info.unit_limit, cu_info.unit_price as u64));

    // Check against every loaded pattern
    for (idx, pattern) in CU_PATTERNS.iter().enumerate() {
        let pattern_len = pattern.cu_pairs.len();

        // We need exactly `pattern_len` transactions recorded to compare
        if history.len() < pattern_len {
            continue;
        }

        // Compare the first `pattern_len` entries of the history with the pattern
        let first_n = &history[..pattern_len];
        let matched = first_n
            .iter()
            .zip(pattern.cu_pairs.iter())
            .all(|((hist_limit, hist_price), (pat_limit, pat_price))| {
                *hist_limit == *pat_limit && *hist_price == *pat_price
            });

        if matched {
            info!(
                "[Pattern Match] Token {} matched CU pattern #{} after {} txs: {:?} | TP: {}% | Hold: {}s",
                mint, idx, pattern_len, pattern.cu_pairs, pattern.take_profit_pct, pattern.holding_time_secs
            );
            return Some(MatchedPatternConfig {
                pattern_index: idx,
                take_profit_pct: pattern.take_profit_pct,
                holding_time_secs: pattern.holding_time_secs,
            });
        }
    }

    None
}
