//! Claude Code session status: domain types + pure derivation logic.
//!
//! This module owns the *types* and the *pure, deterministic* functions that
//! map a work bundle (a "Context") and per-session facts into a run state and a
//! context-level status. It never reads the system clock, the filesystem, or the
//! process table — those live at the edges (vc-sessions / vc-app) and are passed
//! in as values (design decision Q1=A). That keeps this logic total and
//! property-testable (PBT-03: determinism, invariants).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{ResourceKind, SessionCompletion, WorkBundle};

/// Threshold separating an actively-*Working* session from a running-but-*Idle*
/// one, measured by time since the last log activity (design decision Q1=B).
pub const IDLE_THRESHOLD: Duration = Duration::from_secs(30);

/// Five-state run status layered on top of the existing 3-state
/// `SessionCompletion` plus OS process liveness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionRunState {
    /// Process running + recent log activity (< IDLE_THRESHOLD) + not waiting.
    Working,
    /// Process running + waiting for the user's input.
    WaitingForUser,
    /// Process running but idle (no recent activity) + not waiting.
    Idle,
    /// Process not running (terminated / never started).
    Inactive,
    /// Cannot determine liveness (probe unavailable) or insufficient info.
    Unknown,
}

/// Evidence class for a summarized work item (FR-3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    /// Directly present in the log (tool call, file edit, command, user message).
    Fact,
    /// An interpretation/grouping produced by summarization.
    Inferred,
}

/// A single unit of summarized work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub title: String,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Recent work list (most recent first, capped) + the single latest item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSummary {
    /// Most recent semantic units, newest at index 0, at most `MAX_RECENT_WORK`.
    #[serde(default)]
    pub recent_work: Vec<WorkItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_work: Option<WorkItem>,
}

/// Upper bound on `recent_work` length (design decision Q4=A).
pub const MAX_RECENT_WORK: usize = 5;

/// What the session is doing *right now* (FR-2.6); `Unknown` when undeterminable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurrentWork {
    Known(WorkItem),
    Unknown,
}

impl Default for CurrentWork {
    fn default() -> Self {
        CurrentWork::Unknown
    }
}

/// Status of a single Claude Code session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeSessionStatus {
    pub session_id: String,
    pub working_directory: String,
    /// Process liveness probe result; `None` = could not determine.
    pub is_running: Option<bool>,
    /// Unix seconds of the last log activity; `None` if unknown.
    pub last_activity: Option<u64>,
    pub run_state: SessionRunState,
    pub current_work: CurrentWork,
    #[serde(default)]
    pub recent_work: Vec<WorkItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_work: Option<WorkItem>,
}

/// Context-level (aggregated) status for a work bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextClaudeStatus {
    pub context_ref: String,
    /// Per-session statuses — always provided individually.
    pub claude_sessions: Vec<ClaudeSessionStatus>,
    #[serde(default)]
    pub recent_work: Vec<WorkItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_work: Option<WorkItem>,
    pub current_status: SessionRunState,
    pub current_work: CurrentWork,
    pub waiting_for_user: bool,
    pub last_activity: Option<u64>,
    pub session_resolution: SessionResolution,
}

/// Result of resolving a Context to its coding session(s) (FR-1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionResolution {
    /// Exactly one coding session.
    Resolved(String),
    /// Multiple coding sessions — kept distinct, never merged.
    Multiple(Vec<String>),
    /// Coding resources exist but no target can be determined (FR-1.5).
    Ambiguous,
    /// The Context has no coding session resources.
    NoSession,
    /// The Context itself was not found.
    ContextNotFound,
}

/// Delivery outcome for a command sent to a session (U3 uses this; type owned here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    Delivered,
    Resumed,
    Busy,
    Ambiguous,
    NoTarget,
    Failed,
}

/// Result of a command-delivery attempt (§10.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDelivery {
    pub context_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_session: Option<String>,
    pub delivery_status: DeliveryStatus,
    pub current_status: SessionRunState,
    pub current_work: CurrentWork,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Pure logic
// ---------------------------------------------------------------------------

/// Map `(completion, is_running, since_last_activity)` to a run state.
///
/// Decision table (business-rules §1):
/// - R1 `is_running = Some(false)`            → Inactive (any completion)
/// - R2 `Some(true)` + `Waiting`              → WaitingForUser
/// - R3 `Some(true)` + not-Waiting + `< 30s`  → Working
/// - R4 `Some(true)` + not-Waiting + `>= 30s` → Idle
/// - R5 `is_running = None`                   → Unknown (any completion)
///
/// Pure and deterministic: no clock access, no I/O.
pub fn derive_run_state(
    completion: SessionCompletion,
    is_running: Option<bool>,
    since_last_activity: Duration,
) -> SessionRunState {
    match is_running {
        // R5: liveness undeterminable ⇒ we cannot claim any running sub-state.
        None => SessionRunState::Unknown,
        // R1: process not running ⇒ Inactive regardless of the log.
        Some(false) => SessionRunState::Inactive,
        Some(true) => match completion {
            // R2: running + waiting for the user.
            SessionCompletion::Waiting => SessionRunState::WaitingForUser,
            // R3 / R4: running, not waiting ⇒ Working/Idle by recency.
            SessionCompletion::NotWaiting | SessionCompletion::Unknown => {
                if since_last_activity < IDLE_THRESHOLD {
                    SessionRunState::Working
                } else {
                    SessionRunState::Idle
                }
            }
        },
    }
}

/// Pick the representative session index: newest `last_activity` wins; ties are
/// broken by the lexicographically smallest `session_id` (deterministic).
fn representative_index(sessions: &[ClaudeSessionStatus]) -> Option<usize> {
    if sessions.is_empty() {
        return None;
    }
    let mut best = 0usize;
    for i in 1..sessions.len() {
        let cur = &sessions[i];
        let cur_best = &sessions[best];
        let cur_la = cur.last_activity.unwrap_or(0);
        let best_la = cur_best.last_activity.unwrap_or(0);
        let takes = cur_la > best_la
            || (cur_la == best_la && cur.session_id < cur_best.session_id);
        if takes {
            best = i;
        }
    }
    Some(best)
}

/// Aggregate per-session statuses into a context-level status (business-rules §3,
/// design decision Q2=A). Deterministic; representative = newest activity, ties by
/// smallest session_id. `waiting_for_user` is an OR; `last_activity` is the max;
/// `recent_work` is merged newest-session-first and capped at `MAX_RECENT_WORK`.
pub fn aggregate_context_status(
    context_ref: impl Into<String>,
    resolution: SessionResolution,
    sessions: Vec<ClaudeSessionStatus>,
) -> ContextClaudeStatus {
    let context_ref = context_ref.into();
    let rep = representative_index(&sessions);

    let waiting_for_user = sessions
        .iter()
        .any(|s| s.run_state == SessionRunState::WaitingForUser);
    let last_activity = sessions.iter().filter_map(|s| s.last_activity).max();

    let (current_status, current_work, latest_work) = match rep {
        Some(i) => (
            sessions[i].run_state,
            sessions[i].current_work.clone(),
            sessions[i].latest_work.clone(),
        ),
        None => (SessionRunState::Unknown, CurrentWork::Unknown, None),
    };

    // Merge recent_work newest-session-first, preserving each session's order.
    let mut ordered: Vec<usize> = (0..sessions.len()).collect();
    ordered.sort_by(|&a, &b| {
        let la = sessions[a].last_activity.unwrap_or(0);
        let lb = sessions[b].last_activity.unwrap_or(0);
        lb.cmp(&la)
            .then_with(|| sessions[a].session_id.cmp(&sessions[b].session_id))
    });
    let mut recent_work: Vec<WorkItem> = Vec::new();
    for &idx in &ordered {
        for item in &sessions[idx].recent_work {
            if recent_work.len() >= MAX_RECENT_WORK {
                break;
            }
            recent_work.push(item.clone());
        }
        if recent_work.len() >= MAX_RECENT_WORK {
            break;
        }
    }

    ContextClaudeStatus {
        context_ref,
        claude_sessions: sessions,
        recent_work,
        latest_work,
        current_status,
        current_work,
        waiting_for_user,
        last_activity,
        session_resolution: resolution,
    }
}

/// Resolve a Context (work bundle) to its coding session reference(s) (FR-1).
///
/// Only the stable `descriptor` of a `CodingSession` resource identifies a
/// session — working-directory similarity is never used to guess a link
/// (FR-1.2, AC-4). Terminated sessions still registered as resources are
/// included (FR-1.4, AC-3); their state resolves to `Inactive` downstream.
pub fn resolve_sessions(bundle: &WorkBundle) -> SessionResolution {
    let coding: Vec<&crate::Resource> = bundle
        .resources
        .iter()
        .filter(|r| r.kind == ResourceKind::CodingSession)
        .collect();

    if coding.is_empty() {
        return SessionResolution::NoSession;
    }

    let refs: Vec<String> = coding
        .iter()
        .map(|r| r.identity.descriptor.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();

    match refs.len() {
        // Coding resources exist but none yield a usable session ref ⇒ ambiguous.
        0 => SessionResolution::Ambiguous,
        1 => SessionResolution::Resolved(refs.into_iter().next().unwrap()),
        _ => SessionResolution::Multiple(refs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Resource, ResourceIdentity};

    fn coding_resource(descriptor: &str) -> Resource {
        Resource::new(
            "session",
            ResourceKind::CodingSession,
            ResourceIdentity {
                kind: ResourceKind::CodingSession,
                descriptor: descriptor.to_string(),
                hint: None,
                reopen_info: None,
            },
        )
    }

    fn folder_resource() -> Resource {
        Resource::new(
            "folder",
            ResourceKind::Folder,
            ResourceIdentity {
                kind: ResourceKind::Folder,
                descriptor: "/some/path".to_string(),
                hint: None,
                reopen_info: None,
            },
        )
    }

    fn status(id: &str, run_state: SessionRunState, last_activity: Option<u64>) -> ClaudeSessionStatus {
        ClaudeSessionStatus {
            session_id: id.to_string(),
            working_directory: "/w".to_string(),
            is_running: Some(true),
            last_activity,
            run_state,
            current_work: CurrentWork::Unknown,
            recent_work: vec![],
            latest_work: None,
        }
    }

    // --- derive_run_state decision table (R1..R5) ---

    #[test]
    fn r1_not_running_is_inactive() {
        for c in [SessionCompletion::Waiting, SessionCompletion::NotWaiting, SessionCompletion::Unknown] {
            assert_eq!(
                derive_run_state(c, Some(false), Duration::from_secs(0)),
                SessionRunState::Inactive
            );
        }
    }

    #[test]
    fn r5_none_is_unknown() {
        for c in [SessionCompletion::Waiting, SessionCompletion::NotWaiting, SessionCompletion::Unknown] {
            assert_eq!(
                derive_run_state(c, None, Duration::from_secs(0)),
                SessionRunState::Unknown
            );
        }
    }

    #[test]
    fn r2_running_waiting_is_waiting_for_user() {
        assert_eq!(
            derive_run_state(SessionCompletion::Waiting, Some(true), Duration::from_secs(999)),
            SessionRunState::WaitingForUser
        );
    }

    #[test]
    fn r3_r4_working_idle_boundary_at_30s() {
        // < 30s → Working
        assert_eq!(
            derive_run_state(SessionCompletion::NotWaiting, Some(true), Duration::from_secs(29)),
            SessionRunState::Working
        );
        // exactly 30s → Idle (boundary is inclusive of Idle)
        assert_eq!(
            derive_run_state(SessionCompletion::NotWaiting, Some(true), Duration::from_secs(30)),
            SessionRunState::Idle
        );
        // >= 30s → Idle
        assert_eq!(
            derive_run_state(SessionCompletion::Unknown, Some(true), Duration::from_secs(60)),
            SessionRunState::Idle
        );
    }

    // --- aggregate_context_status ---

    #[test]
    fn aggregate_representative_is_newest_activity() {
        let sessions = vec![
            status("a", SessionRunState::Idle, Some(100)),
            status("b", SessionRunState::Working, Some(200)),
        ];
        let ctx = aggregate_context_status("ctx", SessionResolution::Multiple(vec!["a".into(), "b".into()]), sessions);
        assert_eq!(ctx.current_status, SessionRunState::Working); // b is newest
        assert_eq!(ctx.last_activity, Some(200));
    }

    #[test]
    fn aggregate_tie_break_min_session_id() {
        let sessions = vec![
            status("b", SessionRunState::Idle, Some(100)),
            status("a", SessionRunState::Working, Some(100)), // tie → min id "a" wins
        ];
        let ctx = aggregate_context_status("ctx", SessionResolution::Multiple(vec!["b".into(), "a".into()]), sessions);
        assert_eq!(ctx.current_status, SessionRunState::Working);
    }

    #[test]
    fn aggregate_waiting_is_or() {
        let sessions = vec![
            status("a", SessionRunState::Idle, Some(100)),
            status("b", SessionRunState::WaitingForUser, Some(50)),
        ];
        let ctx = aggregate_context_status("ctx", SessionResolution::Multiple(vec!["a".into(), "b".into()]), sessions);
        assert!(ctx.waiting_for_user);
    }

    #[test]
    fn aggregate_recent_work_capped_at_five() {
        let mut a = status("a", SessionRunState::Working, Some(100));
        a.recent_work = (0..4)
            .map(|i| WorkItem { title: format!("a{i}"), provenance: Provenance::Fact, detail: None })
            .collect();
        let mut b = status("b", SessionRunState::Idle, Some(90));
        b.recent_work = (0..4)
            .map(|i| WorkItem { title: format!("b{i}"), provenance: Provenance::Fact, detail: None })
            .collect();
        let ctx = aggregate_context_status("ctx", SessionResolution::Multiple(vec!["a".into(), "b".into()]), vec![a, b]);
        assert_eq!(ctx.recent_work.len(), MAX_RECENT_WORK);
        // Newest session (a) contributes first.
        assert_eq!(ctx.recent_work[0].title, "a0");
    }

    #[test]
    fn aggregate_empty_is_unknown() {
        let ctx = aggregate_context_status("ctx", SessionResolution::NoSession, vec![]);
        assert_eq!(ctx.current_status, SessionRunState::Unknown);
        assert!(!ctx.waiting_for_user);
        assert_eq!(ctx.last_activity, None);
    }

    // --- resolve_sessions ---

    #[test]
    fn resolve_no_session() {
        let mut b = WorkBundle::new("ctx");
        b.add_resource(folder_resource());
        assert_eq!(resolve_sessions(&b), SessionResolution::NoSession);
    }

    #[test]
    fn resolve_single() {
        let mut b = WorkBundle::new("ctx");
        b.add_resource(coding_resource("claude-code:abc"));
        assert_eq!(resolve_sessions(&b), SessionResolution::Resolved("claude-code:abc".into()));
    }

    #[test]
    fn resolve_multiple() {
        let mut b = WorkBundle::new("ctx");
        b.add_resource(coding_resource("s1"));
        b.add_resource(coding_resource("s2"));
        assert_eq!(resolve_sessions(&b), SessionResolution::Multiple(vec!["s1".into(), "s2".into()]));
    }

    #[test]
    fn resolve_ambiguous_when_descriptors_blank() {
        let mut b = WorkBundle::new("ctx");
        b.add_resource(coding_resource("   "));
        assert_eq!(resolve_sessions(&b), SessionResolution::Ambiguous);
    }
}

#[cfg(test)]
mod pbt {
    use super::*;
    use proptest::prelude::*;

    // --- domain generators (PBT-07: never raw primitives alone) ---

    fn completion_strategy() -> impl Strategy<Value = SessionCompletion> {
        prop_oneof![
            Just(SessionCompletion::Waiting),
            Just(SessionCompletion::NotWaiting),
            Just(SessionCompletion::Unknown),
        ]
    }

    fn is_running_strategy() -> impl Strategy<Value = Option<bool>> {
        prop_oneof![Just(None), Just(Some(true)), Just(Some(false))]
    }

    // Include boundary values around IDLE_THRESHOLD (30s).
    fn since_strategy() -> impl Strategy<Value = Duration> {
        prop_oneof![
            Just(Duration::from_secs(0)),
            Just(Duration::from_secs(29)),
            Just(Duration::from_secs(30)),
            Just(Duration::from_secs(31)),
            (0u64..100_000).prop_map(Duration::from_secs),
        ]
    }

    fn work_item_strategy() -> impl Strategy<Value = WorkItem> {
        (".*", prop_oneof![Just(Provenance::Fact), Just(Provenance::Inferred)], proptest::option::of(".*"))
            .prop_map(|(title, provenance, detail)| WorkItem { title, provenance, detail })
    }

    fn run_state_strategy() -> impl Strategy<Value = SessionRunState> {
        prop_oneof![
            Just(SessionRunState::Working),
            Just(SessionRunState::WaitingForUser),
            Just(SessionRunState::Idle),
            Just(SessionRunState::Inactive),
            Just(SessionRunState::Unknown),
        ]
    }

    fn status_strategy() -> impl Strategy<Value = ClaudeSessionStatus> {
        (
            "[a-z0-9]{1,8}",
            run_state_strategy(),
            proptest::option::of(0u64..1_000_000),
            proptest::collection::vec(work_item_strategy(), 0..8),
        )
            .prop_map(|(session_id, run_state, last_activity, recent_work)| ClaudeSessionStatus {
                session_id,
                working_directory: "/w".to_string(),
                is_running: Some(true),
                last_activity,
                run_state,
                current_work: CurrentWork::Unknown,
                recent_work,
                latest_work: None,
            })
    }

    proptest! {
        // P1: is_running=Some(false) ⇒ Inactive.
        #[test]
        fn p1_inactive(c in completion_strategy(), s in since_strategy()) {
            prop_assert_eq!(derive_run_state(c, Some(false), s), SessionRunState::Inactive);
        }

        // P2: is_running=None ⇒ Unknown.
        #[test]
        fn p2_unknown(c in completion_strategy(), s in since_strategy()) {
            prop_assert_eq!(derive_run_state(c, None, s), SessionRunState::Unknown);
        }

        // P3: is_running=Some(true) ⇒ ∈ {Working, WaitingForUser, Idle}.
        #[test]
        fn p3_running_substates(c in completion_strategy(), s in since_strategy()) {
            let rs = derive_run_state(c, Some(true), s);
            prop_assert!(matches!(rs, SessionRunState::Working | SessionRunState::WaitingForUser | SessionRunState::Idle));
        }

        // P4: monotonic — for running + non-Waiting, increasing `since` never
        // turns Idle back into Working.
        #[test]
        fn p4_monotonic(a in since_strategy(), b in since_strategy()) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let s_lo = derive_run_state(SessionCompletion::NotWaiting, Some(true), lo);
            let s_hi = derive_run_state(SessionCompletion::NotWaiting, Some(true), hi);
            // If the earlier (smaller since) is Idle, the later must not be Working.
            if s_lo == SessionRunState::Idle {
                prop_assert_ne!(s_hi, SessionRunState::Working);
            }
        }

        // P5: determinism.
        #[test]
        fn p5_deterministic(c in completion_strategy(), r in is_running_strategy(), s in since_strategy()) {
            prop_assert_eq!(derive_run_state(c, r, s), derive_run_state(c, r, s));
        }

        // P6: context last_activity = max(sessions.last_activity).
        #[test]
        fn p6_last_activity_is_max(sessions in proptest::collection::vec(status_strategy(), 0..6)) {
            let expected = sessions.iter().filter_map(|s| s.last_activity).max();
            let ctx = aggregate_context_status("c", SessionResolution::NoSession, sessions);
            prop_assert_eq!(ctx.last_activity, expected);
        }

        // P7: waiting_for_user is an OR over sessions.
        #[test]
        fn p7_waiting_is_or(sessions in proptest::collection::vec(status_strategy(), 0..6)) {
            let expected = sessions.iter().any(|s| s.run_state == SessionRunState::WaitingForUser);
            let ctx = aggregate_context_status("c", SessionResolution::NoSession, sessions);
            prop_assert_eq!(ctx.waiting_for_user, expected);
        }

        // P8: recent_work length <= MAX_RECENT_WORK.
        #[test]
        fn p8_recent_work_capped(sessions in proptest::collection::vec(status_strategy(), 0..6)) {
            let ctx = aggregate_context_status("c", SessionResolution::NoSession, sessions);
            prop_assert!(ctx.recent_work.len() <= MAX_RECENT_WORK);
        }

        // P9: representative selection is deterministic (same input → same output).
        #[test]
        fn p9_aggregate_deterministic(sessions in proptest::collection::vec(status_strategy(), 0..6)) {
            let a = aggregate_context_status("c", SessionResolution::NoSession, sessions.clone());
            let b = aggregate_context_status("c", SessionResolution::NoSession, sessions);
            prop_assert_eq!(a, b);
        }

        // P11: AnalysisCache round-trips through serde (see analysis_cache tests too).
        #[test]
        fn p11_work_summary_roundtrip(recent in proptest::collection::vec(work_item_strategy(), 0..6)) {
            let ws = WorkSummary { recent_work: recent, latest_work: None };
            let json = serde_json::to_string(&ws).unwrap();
            let back: WorkSummary = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(ws, back);
        }
    }
}
