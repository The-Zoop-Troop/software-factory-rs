//! Claim leases. A dead worker's lease expires and the bead returns to `open`.

use crate::ids::AgentId;
use crate::time::{Duration, Timestamp};

/// Who holds a bead and until when.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lease {
    pub holder: AgentId,
    pub claimed_at: Timestamp,
    pub expires: Timestamp,
}

impl Lease {
    #[must_use]
    pub fn grant(holder: AgentId, now: Timestamp, ttl: Duration) -> Self {
        Self {
            holder,
            claimed_at: now,
            expires: now + ttl,
        }
    }

    #[must_use]
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expires
    }

    /// Extend the lease from `now` (not from the old expiry) so a stalled heartbeat
    /// cannot accumulate credit.
    #[must_use]
    pub fn renew(self, now: Timestamp, ttl: Duration) -> Self {
        Self {
            expires: now + ttl,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> AgentId {
        AgentId::try_new("worker-1").unwrap()
    }

    #[test]
    fn expires_at_boundary() {
        let l = Lease::grant(
            agent(),
            Timestamp::from_unix_seconds(0),
            Duration::from_seconds(10),
        );
        assert!(!l.is_expired(Timestamp::from_unix_seconds(9)));
        assert!(l.is_expired(Timestamp::from_unix_seconds(10)));
    }

    #[test]
    fn renew_is_from_now() {
        let l = Lease::grant(
            agent(),
            Timestamp::from_unix_seconds(0),
            Duration::from_seconds(10),
        );
        let l = l.renew(Timestamp::from_unix_seconds(5), Duration::from_seconds(10));
        assert_eq!(l.expires, Timestamp::from_unix_seconds(15));
        assert_eq!(l.claimed_at, Timestamp::from_unix_seconds(0));
    }
}
