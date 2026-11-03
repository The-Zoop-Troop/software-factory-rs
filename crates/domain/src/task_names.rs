//! Names of task states and events: the event log, generated docs and the console use them.

use super::{Event, TaskState};

/// Every state name, in lifecycle order (for generated docs and console rendering).
pub const STATE_NAMES: [&str; 7] = [
    "open",
    "leased",
    "in_verify",
    "mergeable",
    "closed",
    "incident",
    "canceled",
];
/// Every event name (for generated docs).
pub const EVENT_NAMES: [&str; 14] = [
    "claim",
    "heartbeat",
    "submit",
    "lease_expired",
    "release",
    "provider_stopped",
    "verify_passed",
    "verify_blocked",
    "verify_failed",
    "merged",
    "merge_failed",
    "escalate",
    "cancel",
    "operator_retry",
];

impl TaskState {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Leased { .. } => "leased",
            Self::InVerify { .. } => "in_verify",
            Self::Mergeable { .. } => "mergeable",
            Self::Closed { .. } => "closed",
            Self::Incident { .. } => "incident",
            Self::Canceled { .. } => "canceled",
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        match self {
            Self::Closed { .. } | Self::Incident { .. } | Self::Canceled { .. } => true,
            Self::Open | Self::Leased { .. } | Self::InVerify { .. } | Self::Mergeable { .. } => {
                false
            }
        }
    }
}

impl Event {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Claim { .. } => "claim",
            Self::Heartbeat { .. } => "heartbeat",
            Self::Submit { .. } => "submit",
            Self::LeaseExpired { .. } => "lease_expired",
            Self::Release { .. } => "release",
            Self::ProviderStopped { .. } => "provider_stopped",
            Self::VerifyPassed => "verify_passed",
            Self::VerifyFailed { .. } => "verify_failed",
            Self::VerifyBlocked { .. } => "verify_blocked",
            Self::Merged { .. } => "merged",
            Self::MergeFailed { .. } => "merge_failed",
            Self::Escalate { .. } => "escalate",
            Self::Cancel { .. } => "cancel",
            Self::OperatorRetry { .. } => "operator_retry",
        }
    }
}
