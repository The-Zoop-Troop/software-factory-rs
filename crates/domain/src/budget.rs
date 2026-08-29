//! Per-bead budgets (ARCHITECTURE.md §1.6). Exceeding one is a state transition, not a loop.

use crate::counts::{Attempts, Tokens};
use crate::time::Duration;

/// Limits a task may consume before it becomes an incident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Budget {
    /// Total LLM tokens across all attempts.
    pub tokens: Tokens,
    /// Wall clock across all attempts.
    pub wall_clock: Duration,
    /// Number of worker attempts (claims that reach verification).
    pub attempts: Attempts,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            tokens: Tokens::new(400_000),
            wall_clock: Duration::from_minutes(45),
            attempts: Attempts::new(3),
        }
    }
}

/// What a task has consumed so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Usage {
    pub tokens: Tokens,
    pub wall_clock: Duration,
    pub attempts: Attempts,
}

impl Usage {
    #[must_use]
    pub fn add_tokens(self, n: Tokens) -> Self {
        Self {
            tokens: self.tokens + n,
            ..self
        }
    }

    #[must_use]
    pub fn add_wall_clock(self, d: Duration) -> Self {
        Self {
            wall_clock: Duration::from_seconds(
                self.wall_clock.seconds().saturating_add(d.seconds()),
            ),
            ..self
        }
    }

    #[must_use]
    pub fn add_attempt(self) -> Self {
        Self {
            attempts: self.attempts.incr(),
            ..self
        }
    }
}

/// Which limit was blown. Reported on the incident bead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum BudgetExceeded {
    #[error("token budget exceeded: used {used} of {limit}")]
    Tokens { used: Tokens, limit: Tokens },
    #[error("wall-clock budget exceeded: used {used}s of {limit}s")]
    WallClock { used: u64, limit: u64 },
    #[error("attempt budget exhausted: {used} of {limit}")]
    Attempts { used: Attempts, limit: Attempts },
}

impl Budget {
    /// `Ok(())` while `usage` is within limits; the first exceeded limit otherwise
    /// (attempts checked first, since it's the one that means "the model keeps failing").
    ///
    /// # Errors
    /// Returns the exceeded limit.
    pub fn check(self, usage: Usage) -> Result<(), BudgetExceeded> {
        if usage.attempts >= self.attempts {
            return Err(BudgetExceeded::Attempts {
                used: usage.attempts,
                limit: self.attempts,
            });
        }
        if usage.tokens > self.tokens {
            return Err(BudgetExceeded::Tokens {
                used: usage.tokens,
                limit: self.tokens,
            });
        }
        if usage.wall_clock > self.wall_clock {
            return Err(BudgetExceeded::WallClock {
                used: usage.wall_clock.seconds(),
                limit: self.wall_clock.seconds(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn attempts_take_precedence() {
        let b = Budget {
            tokens: Tokens::new(10),
            wall_clock: Duration::from_seconds(10),
            attempts: Attempts::new(1),
        };
        let u = Usage {
            tokens: Tokens::new(100),
            wall_clock: Duration::from_seconds(100),
            attempts: Attempts::new(1),
        };
        assert_eq!(
            b.check(u),
            Err(BudgetExceeded::Attempts {
                used: Attempts::new(1),
                limit: Attempts::new(1)
            })
        );
    }

    #[test]
    fn limits_are_inclusive_for_tokens_and_wall_clock() {
        let b = Budget {
            tokens: Tokens::new(10),
            wall_clock: Duration::from_seconds(10),
            attempts: Attempts::new(5),
        };
        let at = Usage {
            tokens: Tokens::new(10),
            wall_clock: Duration::from_seconds(10),
            attempts: Attempts::new(0),
        };
        assert_eq!(b.check(at), Ok(()), "exactly at the limit is fine");
        assert_eq!(
            b.check(at.add_tokens(Tokens::new(1))),
            Err(BudgetExceeded::Tokens {
                used: Tokens::new(11),
                limit: Tokens::new(10)
            })
        );
        assert_eq!(
            b.check(at.add_wall_clock(Duration::from_seconds(1))),
            Err(BudgetExceeded::WallClock {
                used: 11,
                limit: 10
            })
        );
        assert_eq!(
            b.check(Usage {
                attempts: Attempts::new(4),
                ..at
            }),
            Ok(())
        );
    }

    #[test]
    fn within_budget_is_ok() {
        assert_eq!(Budget::default().check(Usage::default()), Ok(()));
    }

    proptest! {
        #[test]
        fn check_is_monotone_in_usage(tokens in 0u64..1_000_000, extra in 0u64..1_000_000) {
            let b = Budget::default();
            let u = Usage { tokens: Tokens::new(tokens), ..Usage::default() };
            let more = u.add_tokens(Tokens::new(extra));
            // If the smaller usage is over budget, the larger one must be too.
            if b.check(u).is_err() { prop_assert!(b.check(more).is_err()); }
        }
    }
}
