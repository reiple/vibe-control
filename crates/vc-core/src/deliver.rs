//! U3 Command Delivery — pure decision core.
//!
//! The three functions here are the whole *decision* surface for delivering a
//! command to a Context's coding session; every side effect (probing liveness,
//! reading the log, spawning the resume terminal) lives in the app/OS adapters.
//! Keeping the decisions pure makes the safety invariants property-testable
//! without any I/O (U3-P1/P2/P3):
//!
//! - never deliver to an ambiguous / foreign / absent target (FR-6 / AC-14),
//! - never deliver to a `Working` session (AC-16, busy protection),
//! - never deliver an empty command.

use crate::claude_status::{DeliveryStatus, SessionResolution, SessionRunState};

/// Outcome of choosing a delivery target from a Context's session resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryTarget {
    /// Exactly one unambiguous session — safe to consider for delivery.
    Session(String),
    /// No safe single target; carries the terminal `DeliveryStatus` to report.
    Reject(DeliveryStatus),
}

/// Pick the delivery target from a session resolution (business-rule D2 / Q1=A).
///
/// Only `Resolved(one)` yields a target. Multiple/Ambiguous sessions are
/// **never** auto-picked, merged, or fanned out (FR-6 / AC-14) — that would risk
/// sending to the wrong session; they degrade to `Ambiguous`. No session / no
/// Context degrade to `NoTarget`. Pure and total.
pub fn select_delivery_target(resolution: &SessionResolution) -> DeliveryTarget {
    match resolution {
        SessionResolution::Resolved(session_ref) => DeliveryTarget::Session(session_ref.clone()),
        SessionResolution::Multiple(_) | SessionResolution::Ambiguous => {
            DeliveryTarget::Reject(DeliveryStatus::Ambiguous)
        }
        SessionResolution::NoSession | SessionResolution::ContextNotFound => {
            DeliveryTarget::Reject(DeliveryStatus::NoTarget)
        }
    }
}

/// Reason to block delivery, if any (business-rule D3 / Q2=A, Q5=A).
///
/// - an empty/whitespace command is refused (`Failed`) — no pointless resume;
/// - a `Working` session is protected (`Busy`, AC-16);
/// - every other run state (`WaitingForUser`/`Idle`/`Inactive`/`Unknown`) is
///   allowed, because a resume one-shot starts a fresh process and never
///   interrupts an existing session (NFR-3.2). `Unknown` is allowed on purpose:
///   we do not conservatively block when liveness is merely undeterminable.
///
/// Returns `Some(status)` to block, `None` to proceed. Pure and total.
pub fn delivery_block_reason(
    run_state: SessionRunState,
    command_blank: bool,
) -> Option<DeliveryStatus> {
    if command_blank {
        return Some(DeliveryStatus::Failed);
    }
    match run_state {
        SessionRunState::Working => Some(DeliveryStatus::Busy),
        _ => None,
    }
}

/// Map the resume-one-shot outcome to a delivery status (business-rule D4 / Q4=A).
/// The mechanism always *resumes* the session to deliver, so success is
/// `Resumed`; `Delivered` is reserved for a future live-injection path. A failed
/// spawn degrades to `Failed` (NFR-2). Pure and total.
pub fn delivery_status_after_resume(resume_ok: bool) -> DeliveryStatus {
    if resume_ok {
        DeliveryStatus::Resumed
    } else {
        DeliveryStatus::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_yields_the_session() {
        assert_eq!(
            select_delivery_target(&SessionResolution::Resolved("s1".into())),
            DeliveryTarget::Session("s1".into())
        );
    }

    #[test]
    fn multiple_and_ambiguous_never_pick_a_session() {
        assert_eq!(
            select_delivery_target(&SessionResolution::Multiple(vec!["a".into(), "b".into()])),
            DeliveryTarget::Reject(DeliveryStatus::Ambiguous)
        );
        assert_eq!(
            select_delivery_target(&SessionResolution::Ambiguous),
            DeliveryTarget::Reject(DeliveryStatus::Ambiguous)
        );
    }

    #[test]
    fn no_session_and_no_context_are_no_target() {
        assert_eq!(
            select_delivery_target(&SessionResolution::NoSession),
            DeliveryTarget::Reject(DeliveryStatus::NoTarget)
        );
        assert_eq!(
            select_delivery_target(&SessionResolution::ContextNotFound),
            DeliveryTarget::Reject(DeliveryStatus::NoTarget)
        );
    }

    #[test]
    fn blank_command_is_failed_regardless_of_state() {
        for st in [
            SessionRunState::Working,
            SessionRunState::WaitingForUser,
            SessionRunState::Idle,
            SessionRunState::Inactive,
            SessionRunState::Unknown,
        ] {
            assert_eq!(delivery_block_reason(st, true), Some(DeliveryStatus::Failed));
        }
    }

    #[test]
    fn working_is_busy_protected() {
        assert_eq!(
            delivery_block_reason(SessionRunState::Working, false),
            Some(DeliveryStatus::Busy)
        );
    }

    #[test]
    fn non_working_states_allow_delivery() {
        for st in [
            SessionRunState::WaitingForUser,
            SessionRunState::Idle,
            SessionRunState::Inactive,
            SessionRunState::Unknown,
        ] {
            assert_eq!(delivery_block_reason(st, false), None);
        }
    }

    #[test]
    fn resume_outcome_maps_to_resumed_or_failed() {
        assert_eq!(delivery_status_after_resume(true), DeliveryStatus::Resumed);
        assert_eq!(delivery_status_after_resume(false), DeliveryStatus::Failed);
    }
}

#[cfg(test)]
mod pbt {
    //! Property-based tests for the U3 safety invariants (blocking: U3-P1/P2/P3).
    use super::*;
    use proptest::prelude::*;

    /// Strategy over every `SessionResolution` shape.
    fn any_resolution() -> impl Strategy<Value = SessionResolution> {
        prop_oneof![
            "[a-z0-9]{1,8}".prop_map(SessionResolution::Resolved),
            proptest::collection::vec("[a-z0-9]{1,8}", 0..5).prop_map(SessionResolution::Multiple),
            Just(SessionResolution::Ambiguous),
            Just(SessionResolution::NoSession),
            Just(SessionResolution::ContextNotFound),
        ]
    }

    fn any_run_state() -> impl Strategy<Value = SessionRunState> {
        prop_oneof![
            Just(SessionRunState::Working),
            Just(SessionRunState::WaitingForUser),
            Just(SessionRunState::Idle),
            Just(SessionRunState::Inactive),
            Just(SessionRunState::Unknown),
        ]
    }

    proptest! {
        /// U3-P1: only a `Resolved` resolution can ever produce a delivery target;
        /// multiple/ambiguous/absent never yield `Session` (no foreign/ambiguous
        /// delivery — FR-6 / AC-14). Also never panics (U3-P3).
        #[test]
        fn prop_only_resolved_yields_target(res in any_resolution()) {
            let target = select_delivery_target(&res);
            match res {
                SessionResolution::Resolved(_) => {
                    prop_assert!(matches!(target, DeliveryTarget::Session(_)));
                }
                _ => {
                    prop_assert!(matches!(target, DeliveryTarget::Reject(_)));
                }
            }
        }

        /// U3-P2: `Working` is always blocked `Busy`, and any blank command is
        /// always blocked `Failed` — regardless of state (AC-16). Never panics.
        #[test]
        fn prop_working_and_blank_are_blocked(st in any_run_state(), blank in any::<bool>()) {
            let reason = delivery_block_reason(st, blank);
            if blank {
                prop_assert_eq!(reason, Some(DeliveryStatus::Failed));
            } else if st == SessionRunState::Working {
                prop_assert_eq!(reason, Some(DeliveryStatus::Busy));
            } else {
                prop_assert_eq!(reason, None);
            }
        }

        /// U3-P3: the resume-outcome mapping is total and deterministic.
        #[test]
        fn prop_resume_mapping_is_deterministic(ok in any::<bool>()) {
            let a = delivery_status_after_resume(ok);
            let b = delivery_status_after_resume(ok);
            prop_assert_eq!(a, b);
            prop_assert_eq!(a, if ok { DeliveryStatus::Resumed } else { DeliveryStatus::Failed });
        }
    }
}
