//! Coverage history — the report is the product, so every finished match appends one line to a
//! JSONL log outside the repo. That turns single-run coverage into a trend you can actually use:
//! "is my robust coverage improving across scenarios, and where do the gaps keep recurring?"
//!
//! Location: $WARGAME_HISTORY, else ~/.local/share/purple-range/coverage-history.jsonl.
//! Append-only + best-effort: a logging failure never affects a match.

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::state::GameState;

pub fn history_path() -> PathBuf {
    if let Ok(p) = std::env::var("WARGAME_HISTORY") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/share/purple-range/coverage-history.jsonl")
}

/// Build the record for a finished match (also what the `/api/history` rows look like).
pub fn record_value(state: &GameState, winner: &str, mode: &str, rounds: u32) -> Value {
    let cov = state.coverage();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    json!({
        "ts": ts,
        "scenario": state.scenario,
        "seed": state.seed,
        "mode": mode,
        "winner": winner,
        "rounds": rounds,
        "coverage_pct": cov.coverage_pct,
        "robust_pct": cov.robust_pct,
        "techniques_fired": cov.techniques_fired,
        "techniques_detected": cov.techniques_detected,
        "gaps": cov.gaps,
        "overfit": cov.overfit,
        "noisy": cov.noisy,
        "mttd_rounds": cov.mttd_rounds,
    })
}

/// Append one finished-match record. Best-effort: never panics, never blocks a match on I/O.
pub fn record(state: &GameState, winner: &str, mode: &str, rounds: u32) {
    let rec = record_value(state, winner, mode, rounds);
    let path = history_path();
    if let Some(dir) = path.parent() {
        let _ = create_dir_all(dir);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", rec);
    }
}

/// The most recent `n` records, newest first. Tolerates a missing file / bad lines.
pub fn recent(n: usize) -> Vec<Value> {
    let path = history_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect();
    rows.reverse();
    rows.truncate(n);
    rows
}
