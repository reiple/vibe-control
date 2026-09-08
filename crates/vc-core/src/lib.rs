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

pub use error::{CoreError, Result};
pub use models::{WorkBundle, Resource, ResourceIdentity, ResourceKind, ResourceStatus, BundleId, ResourceId, SessionCompletion, AppSettings};

// Re-export commonly used items
pub use matching::{MatchSignature, MatchResult};
pub use restore::ReopenAction;
pub use evaluate::StatusSnapshot;
