//! The one place the system clock is read.

use app::Clock;
use app::domain::Timestamp;

/// Wall clock backed by `std::time`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[allow(
        clippy::disallowed_methods,
        clippy::disallowed_types,
        reason = "this IS the clock adapter; the ban makes it the only caller"
    )]
    fn now(&self) -> Timestamp {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        Timestamp::from_unix_seconds(secs)
    }
}
