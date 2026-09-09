//! U2 Status Query — edge adapters and the pure logic that wires the domain
//! ports to the running system.
//!
//! - [`SystemProbe`] implements [`ProcessLivenessProbe`] over the OS crates.
//! - [`CloudWorkSummarizer`] implements [`WorkSummarizer`] over the Bedrock
//!   client in [`crate::claude`] (the same path the prompt console uses).
//! - The free functions here are pure/deterministic so the gating and status
//!   assembly can be property-tested without any I/O (U2-P2/P3/P5/P6).
//!
//! Privacy (NFR-4 / §12): the bearer token flows only from the AppState helpers
//! into [`crate::claude::send_message`]; it is never logged, returned to the UI,
//! or written anywhere. Summarization is only ever *dispatched* when
//! [`should_dispatch`] passes (which requires explicit consent).

use std::time::Duration;

use vc_core::{
    consent_allows, derive_run_state, parse_summary_json, should_resummarize, CacheEntry,
    ClaudeSessionStatus, CurrentWork, ProcessLivenessProbe, SessionCompletion, SessionRunState,
    SummarizationOutcome, SummarizationRequest, SummaryTurn, WorkSummarizer, WorkSummary,
};
use vc_sessions::SessionSnapshot;

use crate::claude::{self, ChatMsg};

/// Max conversation turns handed to the summarizer per request (business-rules
/// M1: a minimal excerpt, not the whole history). The prior summary carries the
/// older context, so only the tail is needed for an incremental re-summary.
const MAX_SUMMARY_TURNS: usize = 40;

// ---------------------------------------------------------------------------
// Process liveness adapter (port impl)
// ---------------------------------------------------------------------------

/// Real [`ProcessLivenessProbe`]: a coarse, machine-level check for any running
/// Claude Code process, delegating to the OS crate for the current platform.
pub struct SystemProbe;

impl ProcessLivenessProbe for SystemProbe {
    fn any_claude_process_running(&self) -> Option<bool> {
        any_claude_process_running()
    }
}

/// Platform-dispatched, read-only liveness probe. `None` on any platform without
/// a probe (or on probe failure) so the caller degrades to `Unknown`.
pub fn any_claude_process_running() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::claude_process_running()
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::claude_process_running()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Cloud summarizer adapter (port impl)
// ---------------------------------------------------------------------------

/// Real [`WorkSummarizer`]: wraps the Bedrock client. Holds the resolved token /
/// region / model as owned values so it is `Send + 'static` and can run inside a
/// detached background task (design decision Q1=A). The token lives only here in
/// memory for the duration of the call — never logged or returned.
pub struct CloudWorkSummarizer {
    pub token: String,
    pub region: String,
    pub model: String,
}

impl WorkSummarizer for CloudWorkSummarizer {
    async fn summarize(&self, request: SummarizationRequest) -> SummarizationOutcome {
        let prompt = build_summary_prompt(&request);
        let messages = vec![ChatMsg {
            role: "user".to_string(),
            content: prompt,
        }];
        // main's send_message now returns token usage alongside the text; the
        // background summarizer doesn't meter tokens, so drop the usage here and
        // keep outcome_from_reply operating on the reply text alone.
        let reply = claude::send_message(&self.token, &self.region, &self.model, &messages).await;
        outcome_from_reply(reply.map(|(text, _usage)| text))
    }
}

/// Map a raw LLM reply (or transport error) to an outcome. Pure and total:
/// any error or unparseable/empty reply degrades to `Insufficient` (NFR-2 /
/// business-rules M8, U2-P5) — never a panic, never a partial guess.
pub fn outcome_from_reply(reply: Result<String, String>) -> SummarizationOutcome {
    match reply {
        Ok(text) => match parse_summary_json(&text) {
            Some(summary) => SummarizationOutcome::Summarized(summary),
            None => SummarizationOutcome::Insufficient,
        },
        Err(_) => SummarizationOutcome::Insufficient,
    }
}

/// Build the single structured-JSON summarization prompt (design decision Q3=A /
/// tech-stack §3). Instructs strict JSON, fact/inferred separation, and no
/// guessing (business-rules M7). The prior summary and the new turns are the only
/// content included (minimal excerpt, M1).
pub fn build_summary_prompt(request: &SummarizationRequest) -> String {
    let mut s = String::new();
    s.push_str(
        "You summarize a Claude Code coding session for a dashboard. \
Respond with STRICT JSON only — no prose, no markdown, no code fence. Schema:\n\
{\"recent_work\":[{\"title\":string,\"provenance\":\"fact\"|\"inferred\",\"detail\":string?}],\
\"latest_work\":{\"title\":string,\"provenance\":\"fact\"|\"inferred\",\"detail\":string?}|null}\n\
Rules: recent_work has at most 5 items, newest first. \
Use \"fact\" only for things directly present in the log (edits, commands, tool calls, user messages); \
use \"inferred\" for interpretations. Do NOT invent work that isn't evidenced — \
if there is nothing to report, return {\"recent_work\":[],\"latest_work\":null}.\n\n",
    );

    if let Some(prior) = &request.prior_summary {
        if let Ok(prior_json) = serde_json::to_string(prior) {
            s.push_str("Prior summary (extend incrementally; keep still-relevant items):\n");
            s.push_str(&prior_json);
            s.push_str("\n\n");
        }
    }

    s.push_str("New conversation turns since the prior summary:\n");
    if request.new_turns.is_empty() {
        s.push_str("(none)\n");
    } else {
        for turn in &request.new_turns {
            s.push_str(&turn.role);
            s.push_str(": ");
            s.push_str(&turn.content);
            s.push('\n');
        }
    }
    s
}

/// Extract the minimal turn excerpt (the tail) from a snapshot for summarization.
pub fn snapshot_turns(snapshot: &SessionSnapshot) -> Vec<SummaryTurn> {
    let conv = &snapshot.conversation;
    let start = conv.len().saturating_sub(MAX_SUMMARY_TURNS);
    conv[start..]
        .iter()
        .map(|t| SummaryTurn {
            role: t.role.clone(),
            content: t.content.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Pure gating + status assembly
// ---------------------------------------------------------------------------

/// Decide whether to dispatch a background summarization for one session
/// (design decisions Q1=A/Q2=A). All four guards must hold: consent+key
/// (privacy gate), not already in-flight (dedup), and the throttle/new-content
/// rule. Pure and deterministic — this is the single choke point that makes
/// "no consent ⇒ zero external calls" (U2-P2) and "unchanged ⇒ zero calls"
/// (U2-P3) statically true, since the caller only ever spawns for a `true`.
pub fn should_dispatch(
    consent: bool,
    key_present: bool,
    throttle_entry: Option<&CacheEntry>,
    now: u64,
    has_new_content: bool,
    in_flight_already: bool,
) -> bool {
    consent_allows(consent, key_present)
        && !in_flight_already
        && should_resummarize(throttle_entry, now, has_new_content)
}

/// Assemble a single session's status from injected facts (business-rules §6 /
/// design decision FD-Q1=A). Pure: `now` and `is_running` are values, so this is
/// deterministic and never touches the clock, filesystem, or process table
/// (U2-P6). `run_state` comes from the domain's `derive_run_state`; the cached
/// summary (if any) supplies recent/latest work; `current_work` is only surfaced
/// while `Working` (business-rules M6).
pub fn build_session_status(
    session_id: &str,
    working_directory: &str,
    is_running: Option<bool>,
    completion: SessionCompletion,
    last_activity: Option<u64>,
    now: u64,
    cached_summary: Option<&WorkSummary>,
) -> ClaudeSessionStatus {
    let since_last_activity =
        Duration::from_secs(now.saturating_sub(last_activity.unwrap_or(0)));
    let run_state = derive_run_state(completion, is_running, since_last_activity);

    let (recent_work, latest_work) = match cached_summary {
        Some(s) => (s.recent_work.clone(), s.latest_work.clone()),
        None => (Vec::new(), None),
    };

    let current_work = if run_state == SessionRunState::Working {
        match &latest_work {
            Some(item) => CurrentWork::Known(item.clone()),
            None => CurrentWork::Unknown,
        }
    } else {
        CurrentWork::Unknown
    };

    ClaudeSessionStatus {
        session_id: session_id.to_string(),
        working_directory: working_directory.to_string(),
        is_running,
        last_activity,
        run_state,
        current_work,
        recent_work,
        latest_work,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vc_core::{Provenance, WorkItem};

    fn item(title: &str) -> WorkItem {
        WorkItem {
            title: title.to_string(),
            provenance: Provenance::Fact,
            detail: None,
        }
    }

    // --- should_dispatch: the privacy/throttle choke point (U2-P2/P3) ---

    #[test]
    fn no_consent_never_dispatches() {
        // Even with fresh content, no consent ⇒ no dispatch (⇒ 0 summarizer calls).
        assert!(!should_dispatch(false, true, None, 1000, true, false));
    }

    #[test]
    fn no_key_never_dispatches() {
        assert!(!should_dispatch(true, false, None, 1000, true, false));
    }

    #[test]
    fn unchanged_never_dispatches() {
        assert!(!should_dispatch(true, true, None, 1000, false, false));
    }

    #[test]
    fn in_flight_is_not_respawned() {
        assert!(!should_dispatch(true, true, None, 1000, true, true));
    }

    #[test]
    fn dispatches_when_all_guards_pass() {
        assert!(should_dispatch(true, true, None, 1000, true, false));
    }

    #[test]
    fn throttled_within_30s() {
        let e = CacheEntry {
            analyzed_at: 1000,
            ..Default::default()
        };
        assert!(!should_dispatch(true, true, Some(&e), 1010, true, false)); // 10s < 30s
        assert!(should_dispatch(true, true, Some(&e), 1040, true, false)); // 40s ≥ 30s
    }

    // --- outcome mapping (U2-P5) ---

    #[test]
    fn transport_error_is_insufficient() {
        assert_eq!(
            outcome_from_reply(Err("network down".into())),
            SummarizationOutcome::Insufficient
        );
    }

    #[test]
    fn unparseable_reply_is_insufficient() {
        assert_eq!(
            outcome_from_reply(Ok("sorry, I can't help".into())),
            SummarizationOutcome::Insufficient
        );
    }

    #[test]
    fn valid_reply_is_summarized() {
        let reply = r#"{"recent_work":[{"title":"Edited lib.rs","provenance":"fact"}]}"#;
        match outcome_from_reply(Ok(reply.into())) {
            SummarizationOutcome::Summarized(s) => {
                assert_eq!(s.recent_work[0].title, "Edited lib.rs");
            }
            SummarizationOutcome::Insufficient => panic!("expected Summarized"),
        }
    }

    // --- build_session_status (pure, U2-P6 building block) ---

    #[test]
    fn liveness_none_is_unknown() {
        let st = build_session_status("s", "/w", None, SessionCompletion::Waiting, Some(100), 200, None);
        assert_eq!(st.run_state, SessionRunState::Unknown);
        assert_eq!(st.current_work, CurrentWork::Unknown);
    }

    #[test]
    fn not_running_is_inactive() {
        let st = build_session_status("s", "/w", Some(false), SessionCompletion::NotWaiting, Some(100), 100, None);
        assert_eq!(st.run_state, SessionRunState::Inactive);
    }

    #[test]
    fn working_surfaces_current_work_from_latest() {
        let summary = WorkSummary {
            recent_work: vec![item("A"), item("B")],
            latest_work: Some(item("A")),
        };
        // running + not waiting + recent activity (0s) ⇒ Working.
        let st = build_session_status("s", "/w", Some(true), SessionCompletion::NotWaiting, Some(100), 100, Some(&summary));
        assert_eq!(st.run_state, SessionRunState::Working);
        assert_eq!(st.current_work, CurrentWork::Known(item("A")));
        assert_eq!(st.recent_work.len(), 2);
    }

    #[test]
    fn idle_hides_current_work() {
        let summary = WorkSummary {
            recent_work: vec![item("A")],
            latest_work: Some(item("A")),
        };
        // running + not waiting + stale (60s) ⇒ Idle ⇒ current_work hidden (M6).
        let st = build_session_status("s", "/w", Some(true), SessionCompletion::NotWaiting, Some(100), 160, Some(&summary));
        assert_eq!(st.run_state, SessionRunState::Idle);
        assert_eq!(st.current_work, CurrentWork::Unknown);
        // …but recent_work is still shown.
        assert_eq!(st.recent_work.len(), 1);
    }

    #[test]
    fn waiting_preserved_when_running() {
        let st = build_session_status("s", "/w", Some(true), SessionCompletion::Waiting, Some(0), 1_000_000, None);
        assert_eq!(st.run_state, SessionRunState::WaitingForUser);
    }
}

#[cfg(test)]
mod port_doubles {
    //! Test doubles for the ports (design decision Q3=A). Kept here so both the
    //! call-count invariants and future orchestration tests can share them.
    use super::*;
    use std::cell::Cell;

    /// A [`WorkSummarizer`] double that records how many times it was invoked and
    /// returns a fixed outcome — used to assert "0 external calls" invariants.
    pub struct FakeSummarizer {
        pub calls: Cell<usize>,
        pub reply: SummarizationOutcome,
    }

    impl FakeSummarizer {
        pub fn new(reply: SummarizationOutcome) -> Self {
            Self {
                calls: Cell::new(0),
                reply,
            }
        }
    }

    impl WorkSummarizer for FakeSummarizer {
        async fn summarize(&self, _request: SummarizationRequest) -> SummarizationOutcome {
            self.calls.set(self.calls.get() + 1);
            self.reply.clone()
        }
    }

    /// A fixed-answer [`ProcessLivenessProbe`] double.
    pub struct FakeProbe(pub Option<bool>);

    impl ProcessLivenessProbe for FakeProbe {
        fn any_claude_process_running(&self) -> Option<bool> {
            self.0
        }
    }

    #[test]
    fn fake_probe_is_total_and_fixed() {
        assert_eq!(FakeProbe(Some(true)).any_claude_process_running(), Some(true));
        assert_eq!(FakeProbe(None).any_claude_process_running(), None);
    }

    #[test]
    fn fake_summarizer_counts_calls() {
        // Exercised via block_on so the async port method is actually driven,
        // proving the double honors the trait (used to assert 0-call invariants
        // where the gate blocks dispatch).
        let fake = FakeSummarizer::new(SummarizationOutcome::Insufficient);
        assert_eq!(fake.calls.get(), 0);
        let out = tauri::async_runtime::block_on(fake.summarize(SummarizationRequest::default()));
        assert_eq!(out, SummarizationOutcome::Insufficient);
        assert_eq!(fake.calls.get(), 1);
    }
}
