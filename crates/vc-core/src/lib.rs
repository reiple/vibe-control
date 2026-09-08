//! vc-core: Domain core for vibe-control
//!
//! Pure domain logic for work bundle management, resource identification,
//! matching, restore planning, and status evaluation.

pub mod error;
pub mod models;
pub mod matching;
pub mod restore;
pub mod evaluate;
pub mod normalize;
pub mod migrate;
pub mod claude_status;
pub mod summarize;
pub mod deliver;

pub use error::{CoreError, Result};
pub use models::{WorkBundle, Resource, ResourceIdentity, ResourceKind, ResourceStatus, BundleId, ResourceId, SessionCompletion, AppSettings, AnalysisCache, CacheEntry};

// Re-export commonly used items
pub use matching::{MatchSignature, MatchResult};
pub use restore::ReopenAction;
pub use evaluate::StatusSnapshot;

// Claude Code session status (Context Control feature).
pub use claude_status::{
    aggregate_context_status, derive_run_state, resolve_sessions, ClaudeSessionStatus,
    CommandDelivery, ContextClaudeStatus, CurrentWork, DeliveryStatus, Provenance,
    SessionResolution, SessionRunState, WorkItem, WorkSummary, IDLE_THRESHOLD, MAX_RECENT_WORK,
};

// Work summarization + process liveness ports and pure logic (U2 Status Query).
pub use summarize::{
    consent_allows, parse_summary_json, should_resummarize, ProcessLivenessProbe,
    SummarizationOutcome, SummarizationRequest, SummaryTurn, WorkSummarizer,
    RESUMMARIZE_THROTTLE_SECS,
};

// Command-delivery pure decision core (U3 Command Delivery).
pub use deliver::{
    delivery_block_reason, delivery_status_after_resume, select_delivery_target, DeliveryTarget,
};
