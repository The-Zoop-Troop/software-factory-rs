//! Time as values. The domain never reads a clock; it is handed a `Timestamp`.

use core::fmt;
use core::ops::Add;

/// Seconds since the Unix epoch, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Timestamp(i64);

impl Timestamp {
    #[must_use]
    pub const fn from_unix_seconds(secs: i64) -> Self {
        Self(secs)
    }

    #[must_use]
    pub const fn unix_seconds(self) -> i64 {
        self.0
    }

    /// Elapsed time from `earlier` to `self`; zero if `earlier` is later.
    #[must_use]
    pub fn since(self, earlier: Self) -> Duration {
        Duration::from_seconds(u64::try_from(self.0.saturating_sub(earlier.0)).unwrap_or(0))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A non-negative span of whole seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Duration(u64);

impl Duration {
    #[must_use]
    pub const fn from_seconds(secs: u64) -> Self {
        Self(secs)
    }

    #[must_use]
    pub const fn from_minutes(mins: u64) -> Self {
        Self(mins.saturating_mul(60))
    }

    #[must_use]
    pub const fn seconds(self) -> u64 {
        self.0
    }
}

impl Add<Duration> for Timestamp {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self {
        let secs = i64::try_from(rhs.0).unwrap_or(i64::MAX);
        Self(self.0.saturating_add(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_is_saturating() {
        let a = Timestamp::from_unix_seconds(100);
        let b = Timestamp::from_unix_seconds(160);
        assert_eq!(b.since(a), Duration::from_seconds(60));
        assert_eq!(a.since(b), Duration::from_seconds(0));
    }

    #[test]
    fn unix_seconds_and_display_roundtrip() {
        let t = Timestamp::from_unix_seconds(1_700_000_123);
        assert_eq!(t.unix_seconds(), 1_700_000_123);
        assert_eq!(t.to_string(), "1700000123");
        assert_eq!(Timestamp::from_unix_seconds(-5).unix_seconds(), -5);
    }

    #[test]
    fn add_duration() {
        let a = Timestamp::from_unix_seconds(100);
        assert_eq!(
            a + Duration::from_minutes(1),
            Timestamp::from_unix_seconds(160)
        );
    }
}
