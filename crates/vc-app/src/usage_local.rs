//! Today's local `claude` CLI token usage + USD cost, read from the on-disk
//! session transcripts under `~/.claude/projects/**/*.jsonl`.
//!
//! The interactive group terminals run the user's OWN `claude` CLI (their own
//! Claude auth — §12), so their spend never touches this app's Bedrock path or
//! the CloudWatch meter. But the CLI records every assistant turn's `usage`
//! (input, output, cache-create, cache-read tokens) into per-session JSONL
//! transcripts — the exact ledger `ccusage` reads. We sum today's turns across
//! every session and price them per model (Anthropic public per-MTok rates), so
//! the KO-II meter can show the account's real spend in (near) real time.
//!
//! Privacy: only the numeric `usage` counts and the `model` id are read — never
//! a prompt, a response, or any transcript content.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

/// Today's rolled-up local CLI usage. Field names are snake_case to match the
/// TypeScript `LocalUsage` interface consumed by the meter.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LocalUsage {
    /// True when a `~/.claude/projects` ledger exists (the CLI has been used).
    pub configured: bool,
    /// Total spend today across all sessions, in USD.
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    /// input + output + cache_creation + cache_read.
    pub total_tokens: u64,
    /// Assistant turns counted today.
    pub messages: u64,
}

/// Per-MTok USD rates for one model family.
#[derive(Clone, Copy)]
struct Price {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

/// Public Anthropic pricing (USD per 1M tokens). `cache_write` is the 5-minute
/// rate (1.25× input) and `cache_read` is 0.1× input — the same split ccusage
/// uses. `<synthetic>` turns (locally-generated, no API call) cost nothing and
/// are excluded entirely.
fn price_for(model: &str) -> Option<Price> {
    let m = model.to_ascii_lowercase();
    if m.is_empty() || m == "<synthetic>" {
        return None;
    }
    if m.contains("opus") {
        Some(Price { input: 15.0, output: 75.0, cache_write: 18.75, cache_read: 1.50 })
    } else if m.contains("haiku") {
        Some(Price { input: 1.0, output: 5.0, cache_write: 1.25, cache_read: 0.10 })
    } else if m.contains("sonnet") {
        Some(Price { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 })
    } else if m.contains("claude") {
        // Unknown Claude model — price as Sonnet (mid) rather than guess high.
        Some(Price { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 })
    } else {
        None
    }
}

/// Parse an ISO-8601 UTC timestamp (`2026-09-09T00:14:31.684Z`) to epoch
/// seconds, dependency-free. Fixed-offset field extraction (the CLI always
/// emits this exact layout); returns `None` if the string is too short or a
/// field isn't numeric.
fn iso_to_epoch(ts: &str) -> Option<i64> {
    if ts.len() < 19 {
        return None;
    }
    let num = |a: usize, b: usize| ts.get(a..b).and_then(|s| s.parse::<i64>().ok());
    let year = num(0, 4)?;
    let mon = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    // Days from civil (Howard Hinnant's algorithm): 1970-01-01 == 0.
    let y = if mon <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if mon > 2 { mon - 3 } else { mon + 9 }; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

/// `~/.claude/projects` if it exists (else the CLI has never run here).
fn projects_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let p = PathBuf::from(home).join(".claude").join("projects");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// One transcript file's contribution to the day, plus the (day, mtime, len)
/// fingerprint used to skip re-parsing an unchanged file on the next poll.
#[derive(Clone, Default)]
struct FileAcc {
    /// The `since` (local-midnight epoch) this was computed for — a new day
    /// invalidates the entry.
    day: i64,
    mtime_ms: u128,
    len: u64,
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
    messages: u64,
    cost: f64,
}

/// Per-file parse cache, so a 3-second poll only re-reads the transcript that
/// actually changed (the active session's) rather than every file each tick.
fn cache() -> &'static Mutex<HashMap<PathBuf, FileAcc>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, FileAcc>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parse one transcript, summing today's priced assistant turns.
fn compute_file(path: &Path, since: i64) -> FileAcc {
    let mut a = FileAcc::default();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return a,
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Only count turns stamped today (local); the CLI timestamp is UTC.
        let in_today = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(iso_to_epoch)
            .map_or(false, |e| e >= since);
        if !in_today {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        let usage = match msg.get("usage") {
            Some(u) => u,
            None => continue,
        };
        let model = msg.get("model").and_then(|m| m.as_str()).unwrap_or("");
        let price = match price_for(model) {
            Some(p) => p,
            None => continue, // synthetic / non-Claude → no cost
        };
        let g = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let inp = g("input_tokens");
        let out = g("output_tokens");
        let cc = g("cache_creation_input_tokens");
        let cr = g("cache_read_input_tokens");
        a.input += inp;
        a.output += out;
        a.cache_creation += cc;
        a.cache_read += cr;
        a.messages += 1;
        a.cost += (inp as f64 * price.input
            + out as f64 * price.output
            + cc as f64 * price.cache_write
            + cr as f64 * price.cache_read)
            / 1_000_000.0;
    }
    a
}

/// Sum every session's today usage across `~/.claude/projects`. `since` is the
/// caller's local-midnight epoch (seconds); the backend has no local-timezone
/// info of its own. Returns an all-zero (unconfigured) result when the CLI
/// ledger doesn't exist.
pub fn scan_today(since: i64) -> LocalUsage {
    let dir = match projects_dir() {
        Some(d) => d,
        None => return LocalUsage::default(),
    };

    // Collect transcripts one level deep: projects/<project>/<session>.jsonl.
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if let Ok(rd2) = fs::read_dir(&p) {
                for e2 in rd2.flatten() {
                    let p2 = e2.path();
                    if p2.extension().map_or(false, |x| x == "jsonl") {
                        files.push(p2);
                    }
                }
            }
        }
    }

    let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    let mut total = LocalUsage {
        configured: true,
        ..Default::default()
    };
    for f in &files {
        let meta = match fs::metadata(f) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let len = meta.len();
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        // Reuse the cached parse only if the same day AND the file is byte- and
        // mtime-identical; otherwise re-parse and refresh the entry.
        let acc = match cache.get(f) {
            Some(c) if c.day == since && c.mtime_ms == mtime_ms && c.len == len => c.clone(),
            _ => {
                let mut a = compute_file(f, since);
                a.day = since;
                a.mtime_ms = mtime_ms;
                a.len = len;
                cache.insert(f.clone(), a.clone());
                a
            }
        };
        total.input_tokens += acc.input;
        total.output_tokens += acc.output;
        total.cache_creation_tokens += acc.cache_creation;
        total.cache_read_tokens += acc.cache_read;
        total.messages += acc.messages;
        total.total_cost_usd += acc.cost;
    }
    total.total_tokens = total.input_tokens
        + total.output_tokens
        + total.cache_creation_tokens
        + total.cache_read_tokens;
    total
}
