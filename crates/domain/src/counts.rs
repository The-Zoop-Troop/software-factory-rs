//! Counted quantities with a name. All are `#[serde(transparent)]` so stored metadata keeps
//! its v1 shape (`"tokens": 400000`, not `{"tokens": {...}}`).

use core::ops::Add;

macro_rules! count {
    ($(#[$doc:meta])* $name:ident($inner:ty)) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name($inner);

        impl $name {
            #[must_use]
            pub const fn new(n: $inner) -> Self {
                Self(n)
            }

            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }

            /// Saturating increment by one.
            #[must_use]
            pub const fn incr(self) -> Self {
                Self(self.0.saturating_add(1))
            }
        }

        impl Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self {
                Self(self.0.saturating_add(rhs.0))
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

count!(
    /// LLM tokens (input + output + cache, as the harness reports them).
    Tokens(u64)
);
count!(
    /// Worker attempts at a task (claims that reached verification or were released).
    Attempts(u32)
);
count!(
    /// Harness turns in a session.
    Turns(u32)
);
count!(
    /// Money in integer micro-dollars (1 USD = `1_000_000`).
    MicroUsd(u64)
);

/// Ledger priority 0 (critical) … 4 (backlog), as `bd` defines it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct Priority(u8);

/// Priority outside 0..=4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("priority {0} is not in 0..=4")]
pub struct PriorityError(pub u8);

impl Priority {
    pub const CRITICAL: Self = Self(0);
    pub const HIGH: Self = Self(1);
    pub const MEDIUM: Self = Self(2);
    pub const LOW: Self = Self(3);
    pub const BACKLOG: Self = Self(4);

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Priority {
    type Error = PriorityError;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        if n <= 4 {
            Ok(Self(n))
        } else {
            Err(PriorityError(n))
        }
    }
}

impl From<Priority> for u8 {
    fn from(p: Priority) -> Self {
        p.0
    }
}

impl core::fmt::Display for Priority {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_saturate_and_display() {
        assert_eq!(Attempts::new(u32::MAX).incr(), Attempts::new(u32::MAX));
        assert_eq!((Tokens::new(2) + Tokens::new(3)).get(), 5);
        assert_eq!(MicroUsd::new(1_500_000).to_string(), "1500000");
        assert_eq!(Turns::default(), Turns::new(0));
    }

    #[test]
    fn priority_bounds() {
        assert_eq!(Priority::try_from(4), Ok(Priority::BACKLOG));
        assert_eq!(Priority::try_from(5), Err(PriorityError(5)));
        assert_eq!(u8::from(Priority::HIGH), 1);
        assert_eq!(Priority::MEDIUM.get(), 2);
        assert_eq!(Priority::BACKLOG.to_string(), "4");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_is_transparent() {
        assert_eq!(serde_json::to_string(&Tokens::new(7)).unwrap(), "7");
        assert_eq!(
            serde_json::from_str::<Attempts>("3").unwrap(),
            Attempts::new(3)
        );
        assert!(serde_json::from_str::<Priority>("9").is_err());
    }
}
