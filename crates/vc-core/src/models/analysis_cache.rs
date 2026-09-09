//! Incremental-analysis cache (NFR-1.2).
//!
//! Persisted to `analysis-cache.json` alongside `bundles.json`. This cache is
//! *non-critical*: if it is missing or corrupt, callers recompute from scratch —
//! it only ever affects performance, never correctness. It carries the byte
//! offset + file mtime needed to resume incremental parsing of a session log.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::claude_status::WorkSummary;

/// Per-session incremental-analysis state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub session_id: String,
    /// The Context (bundle id, as string) this session was analyzed under.
    #[serde(default)]
    pub context_ref: String,
    /// Byte offset in the session log already parsed (incremental resume point).
    pub last_analyzed_offset: u64,
    /// File mtime (unix seconds) at the time of the last analysis.
    pub last_analyzed_mtime: u64,
    /// When the analysis was taken (unix seconds).
    #[serde(default)]
    pub analyzed_at: u64,
    /// Reusable summary from the previous analysis (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_summary: Option<WorkSummary>,
}

/// The whole cache: session_id → entry. `BTreeMap` for deterministic ordering
/// (stable serialization, easier round-trip testing).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisCache {
    #[serde(default)]
    pub entries: BTreeMap<String, CacheEntry>,
}

impl AnalysisCache {
    pub fn get(&self, session_id: &str) -> Option<&CacheEntry> {
        self.entries.get(session_id)
    }

    /// Insert or replace the entry for `entry.session_id`.
    pub fn upsert(&mut self, entry: CacheEntry) {
        self.entries.insert(entry.session_id.clone(), entry);
    }

    /// Loose GC: drop entries whose session is no longer referenced by any
    /// live Context. No size cap — entries are tiny (business-rules §6 / NFR-1.2).
    pub fn gc<I, S>(&mut self, live_session_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let live: std::collections::BTreeSet<String> = live_session_ids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.entries.retain(|k, _| live.contains(k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> CacheEntry {
        CacheEntry {
            session_id: id.to_string(),
            context_ref: "ctx".to_string(),
            last_analyzed_offset: 42,
            last_analyzed_mtime: 1000,
            analyzed_at: 1001,
            cached_summary: None,
        }
    }

    #[test]
    fn upsert_and_get() {
        let mut c = AnalysisCache::default();
        c.upsert(entry("s1"));
        assert_eq!(c.get("s1").unwrap().last_analyzed_offset, 42);
        assert!(c.get("missing").is_none());
    }

    #[test]
    fn gc_drops_unreferenced() {
        let mut c = AnalysisCache::default();
        c.upsert(entry("s1"));
        c.upsert(entry("s2"));
        c.gc(["s1"]);
        assert!(c.get("s1").is_some());
        assert!(c.get("s2").is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let mut c = AnalysisCache::default();
        c.upsert(entry("s1"));
        let json = serde_json::to_string(&c).unwrap();
        let back: AnalysisCache = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
