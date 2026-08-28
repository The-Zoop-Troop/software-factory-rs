//! Infrastructure adapters implementing the `app` ports.
//!
//! This is the only crate allowed to shell out (`bd`, `git`, `claude`) or
//! read the system clock.
#![forbid(unsafe_code)]
#![cfg_attr(any(test, feature = "testing"),
            allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing))]

pub use app;
