//! Work summarization + process liveness: ports (hexagonal edges) and the
//! pure, deterministic logic around them (U2, design decisions Q3=A ports,
//! Q1=A background dispatch, Q2=A throttle/TTL).
//!
//! Like [`crate::claude_status`], this module stays *pure and side-effect free*.
//! The two traits here are **ports**: the real adapters live at the edges
//! (`vc-app` for the LLM summarizer over Bedrock, `vc-os-*` for the process
//! probe), while test doubles implement the same traits so the orchestration
//! and gating logic can be property-tested without any network or subprocess
//! (PBT U2-P2/P3/P5/P7). No LLM call is ever made from this crate.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::claude_status::{Provenance, WorkItem, WorkSummary, MAX_RECENT_WORK};
use crate::models::analysis_cache::CacheEntry;

/// Re-summarization throttle (NFR-1, Q2=A): a session is only re-summarized when
/// at least this many seconds have passed since the last analysis, *and* the log
/// actually changed. Below the threshold the cached summary is reused.
pub const RESUMMARIZE_THROTTLE_SECS: u64 = 30;

/// A minimal conversation excerpt fed to the summarizer (business-rules M1: only
/// the incremental slice + prior summary, never the whole history verbatim).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryTurn {
    pub role: String,
    pub content: String,
}

/// Input to a summarization request (design decision Q3=A / business-rules M3):
/// the prior summary (for incremental re-summary) plus the new turns.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummarizationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_summary: Option<WorkSummary>,
    #[serde(default)]
    pub new_turns: Vec<SummaryTurn>,
}

/// Outcome of a summarization attempt. `Insufficient` is the graceful-degradation
/// fallback (NFR-2 / business-rules M8): parsing failure, an empty/malformed LLM
/// reply, or a transport error all degrade to `Insufficient` rather than erroring.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummarizationOutcome {
    Summarized(WorkSummary),
    Insufficient,
}

/// Port: probe whether *any* Claude Code process is currently running on this
/// machine (coarse, machine-level liveness — design decision FD-Q1=A).
///
/// Read-only (NFR-3.3). `None` means "could not determine" (probe unavailable /
/// failed) → the caller degrades that session to `Unknown` via
/// [`crate::claude_status::derive_run_state`]. `Some(false)` (no process) →
/// `Inactive`; `Some(true)` (a process exists) preserves `WaitingForUser` for a
/// long-idle-but-live session (design decision FD-Q2=A).
pub trait ProcessLivenessProbe {
    fn any_claude_process_running(&self) -> Option<bool>;
}

/// Port: summarize a coding session's work into a [`WorkSummary`] (design
/// decision Q3=A). The real adapter wraps the Bedrock client; test doubles record
/// call counts and return fixed responses so "no external call" invariants
/// (U2-P2: no consent ⇒ 0 calls; U2-P3: unchanged ⇒ 0 calls) are deterministic.
///
/// `async` because the real adapter performs network I/O; the trait is used only
/// through concrete types (no `dyn`), so the returned future's `Send`-ness is
/// inferred at the call site and it can be spawned on the app's async runtime.
#[allow(async_fn_in_trait)]
pub trait WorkSummarizer {
    async fn summarize(&self, request: SummarizationRequest) -> SummarizationOutcome;
}

/// Consent gate (NFR-4 / §12, business-rules §2): session content may cross to an
/// external service *only* with explicit consent **and** a configured key. When
/// this returns `false` the summarizer must not be called at all — the static
/// privacy guarantee that no excerpt can leave the process (verified by U2-P2).
pub fn consent_allows(consent: bool, key_present: bool) -> bool {
    consent && key_present
}

/// Throttle decision (Q2=A, business-rules M2/§5): re-summarize only when the log
/// changed *and* enough time has elapsed since the last analysis. A never-analyzed
/// session (`entry == None`) is eligible immediately once it has new content.
/// Pure — `now` is injected, so it is deterministic (U2-P3).
pub fn should_resummarize(entry: Option<&CacheEntry>, now: u64, has_new_content: bool) -> bool {
    if !has_new_content {
        return false;
    }
    match entry {
        None => true,
        Some(e) => now.saturating_sub(e.analyzed_at) >= RESUMMARIZE_THROTTLE_SECS,
    }
}

/// Parse an LLM summary reply (strict JSON per the prompt contract) into a
/// [`WorkSummary`]. Tolerant reader (NFR-5 / business-rules M8): strips an
/// optional ```json code fence, ignores unknown fields, clamps `recent_work` to
/// `MAX_RECENT_WORK`, and returns `None` on any failure or an empty result so the
/// caller degrades to `Insufficient` (U2-P5: parsing failure never panics).
pub fn parse_summary_json(text: &str) -> Option<WorkSummary> {
    let cleaned = strip_code_fence(text);
    let v: Value = serde_json::from_str(cleaned.trim()).ok()?;

    let recent_work: Vec<WorkItem> = v
        .get("recent_work")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(parse_work_item)
                .take(MAX_RECENT_WORK)
                .collect()
        })
        .unwrap_or_default();

    let latest_work = v
        .get("latest_work")
        .and_then(|x| if x.is_null() { None } else { parse_work_item(x) })
        .or_else(|| recent_work.first().cloned());

    if recent_work.is_empty() && latest_work.is_none() {
        return None;
    }
    Some(WorkSummary {
        recent_work,
        latest_work,
    })
}

/// Parse a single work item object; `None` when it has no usable title.
fn parse_work_item(v: &Value) -> Option<WorkItem> {
    let title = v
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let provenance = match v
        .get("provenance")
        .and_then(Value::as_str)
        .unwrap_or("inferred")
        .to_ascii_lowercase()
        .as_str()
    {
        "fact" => Provenance::Fact,
        _ => Provenance::Inferred,
    };
    let detail = v
        .get("detail")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some(WorkItem {
        title,
        provenance,
        detail,
    })
}

/// Strip a leading ```json / ``` code fence (and its closing ```), if present.
/// LLMs frequently wrap JSON in a fence despite "JSON only" instructions.
fn strip_code_fence(text: &str) -> String {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Drop the (optional) language tag on the fence's first line.
        let body = rest.splitn(2, '\n').nth(1).unwrap_or("");
        let body = body.trim_end();
        let body = body.strip_suffix("```").unwrap_or(body);
        return body.trim().to_string();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_analyzed_at(at: u64) -> CacheEntry {
        CacheEntry {
            analyzed_at: at,
            ..Default::default()
        }
    }

    // --- consent gate (U2-P2) ---

    #[test]
    fn consent_gate_requires_both() {
        assert!(consent_allows(true, true));
        assert!(!consent_allows(false, true)); // no consent ⇒ blocked
        assert!(!consent_allows(true, false)); // no key ⇒ blocked
        assert!(!consent_allows(false, false));
    }

    // --- throttle (U2-P3) ---

    #[test]
    fn no_new_content_never_resummarizes() {
        assert!(!should_resummarize(None, 1000, false));
        assert!(!should_resummarize(Some(&entry_analyzed_at(0)), 1_000_000, false));
    }

    #[test]
    fn first_time_with_new_content_is_eligible() {
        assert!(should_resummarize(None, 1000, true));
    }

    #[test]
    fn throttle_boundary_is_30s() {
        let e = entry_analyzed_at(1000);
        assert!(!should_resummarize(Some(&e), 1000 + 29, true)); // < 30s
        assert!(should_resummarize(Some(&e), 1000 + 30, true)); // == 30s
        assert!(should_resummarize(Some(&e), 1000 + 60, true));
    }

    // --- tolerant JSON parse (U2-P5) ---

    #[test]
    fn parses_plain_json() {
        let text = r#"{"recent_work":[{"title":"Fixed bug","provenance":"fact","detail":"in parser"}],"latest_work":{"title":"Fixed bug","provenance":"fact"}}"#;
        let s = parse_summary_json(text).expect("parses");
        assert_eq!(s.recent_work.len(), 1);
        assert_eq!(s.recent_work[0].provenance, Provenance::Fact);
        assert_eq!(s.latest_work.unwrap().title, "Fixed bug");
    }

    #[test]
    fn strips_code_fence() {
        let text = "```json\n{\"recent_work\":[{\"title\":\"A\"}]}\n```";
        let s = parse_summary_json(text).expect("parses fenced");
        assert_eq!(s.recent_work[0].title, "A");
        // Unknown provenance string defaults to Inferred (tolerant).
        assert_eq!(s.recent_work[0].provenance, Provenance::Inferred);
    }

    #[test]
    fn clamps_recent_work_to_max() {
        let items: Vec<String> = (0..10)
            .map(|i| format!("{{\"title\":\"t{i}\"}}"))
            .collect();
        let text = format!("{{\"recent_work\":[{}]}}", items.join(","));
        let s = parse_summary_json(&text).expect("parses");
        assert_eq!(s.recent_work.len(), MAX_RECENT_WORK);
    }

    #[test]
    fn malformed_or_empty_is_none() {
        assert!(parse_summary_json("not json at all").is_none());
        assert!(parse_summary_json("").is_none());
        assert!(parse_summary_json("{}").is_none()); // no items
        assert!(parse_summary_json(r#"{"recent_work":[]}"#).is_none());
        // Items without a title are dropped → empty → None.
        assert!(parse_summary_json(r#"{"recent_work":[{"detail":"x"}]}"#).is_none());
    }
}

#[cfg(test)]
mod pbt {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // U2-P5: the summary parser never panics on arbitrary input.
        #[test]
        fn prop_parse_never_panics(text in ".*") {
            let _ = parse_summary_json(&text);
        }

        // Arbitrary bytes rendered as lossy UTF-8 also never panic.
        #[test]
        fn prop_parse_bytes_never_panics(raw in prop::collection::vec(any::<u8>(), 0..512)) {
            let text = String::from_utf8_lossy(&raw);
            let _ = parse_summary_json(&text);
        }

        // U2-P3: no new content ⇒ never re-summarize, for any clock/entry.
        #[test]
        fn prop_no_new_content_blocks(now in any::<u64>(), at in any::<u64>()) {
            let e = CacheEntry { analyzed_at: at, ..Default::default() };
            prop_assert!(!should_resummarize(Some(&e), now, false));
            prop_assert!(!should_resummarize(None, now, false));
        }

        // U2-P2: consent gate is exactly `consent AND key` (no other path opens it).
        #[test]
        fn prop_consent_is_conjunction(consent in any::<bool>(), key in any::<bool>()) {
            prop_assert_eq!(consent_allows(consent, key), consent && key);
        }

        // should_resummarize is deterministic (same inputs → same output).
        #[test]
        fn prop_resummarize_deterministic(now in any::<u64>(), at in any::<u64>(), nc in any::<bool>()) {
            let e = CacheEntry { analyzed_at: at, ..Default::default() };
            prop_assert_eq!(
                should_resummarize(Some(&e), now, nc),
                should_resummarize(Some(&e), now, nc)
            );
        }
    }
}
