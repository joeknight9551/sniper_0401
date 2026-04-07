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
    pub take_profit_pct: f64,
    pub holding_time_secs: u64,
}

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
///
/// When a shorter pattern matches but a longer pattern could still match
/// (current history is a valid prefix), we wait for more events before triggering.
pub fn record_and_match_cu_pattern(
    mint: Pubkey,
    cu_info: ComputeBudgetInfo,
) -> Option<MatchedPatternConfig> {
    let mut entry = TOKEN_CU_HISTORY.entry(mint).or_insert_with(Vec::new);
    let history = entry.value_mut();

    // Append this transaction's CU data
    history.push((cu_info.unit_limit, cu_info.unit_price as u64));

    let mut best_match: Option<&CuPatternEntry> = None;
    let mut has_longer_potential = false;

    for pattern in CU_PATTERNS.iter() {
        let pattern_len = pattern.cu_pairs.len();

        if history.len() >= pattern_len {
            // Check if the first `pattern_len` entries match this pattern
            let first_n = &history[..pattern_len];
            let matched = first_n
                .iter()
                .zip(pattern.cu_pairs.iter())
                .all(|((hist_limit, hist_price), (pat_limit, pat_price))| {
                    *hist_limit == *pat_limit && *hist_price == *pat_price
                });

            if matched {
                // Keep the longest matching pattern
                if best_match.is_none()
                    || pattern_len > best_match.unwrap().cu_pairs.len()
                {
                    best_match = Some(pattern);
                }
            }
        } else {
            // history.len() < pattern_len — check if history is a valid prefix of this pattern
            let prefix = &pattern.cu_pairs[..history.len()];
            let is_prefix = history
                .iter()
                .zip(prefix.iter())
                .all(|((hist_limit, hist_price), (pat_limit, pat_price))| {
                    *hist_limit == *pat_limit && *hist_price == *pat_price
                });

            if is_prefix {
                has_longer_potential = true;
            }
        }
    }

    // If a longer pattern could still match, wait for more events
    if has_longer_potential {
        if let Some(m) = &best_match {
            info!(
                "[Pattern] Token {} matched {} CU pairs but longer pattern possible — waiting",
                mint, m.cu_pairs.len()
            );
        }
        return None;
    }

    if let Some(matched_pattern) = best_match {
        info!(
            "[Pattern Match] Token {} matched CU pattern after {} txs: {:?} | TP: {}% | Hold: {}s",
            mint,
            matched_pattern.cu_pairs.len(),
            matched_pattern.cu_pairs,
            matched_pattern.take_profit_pct,
            matched_pattern.holding_time_secs
        );
        return Some(MatchedPatternConfig {
            take_profit_pct: matched_pattern.take_profit_pct,
            holding_time_secs: matched_pattern.holding_time_secs,
        });
    }

    None
}
