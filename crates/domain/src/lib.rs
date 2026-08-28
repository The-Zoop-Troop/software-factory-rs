//! Factory domain — the pure core.
//!
//! Bead conventions (ARCHITECTURE.md §3), the task state machine, budgets and
//! leases live here as types and total functions. No clock, no I/O, no async:
//! timestamps and IDs arrive as parameters.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing))]

/// Marker so the crate is non-empty until `fac-ec6.2` lands the conventions.
#[must_use]
pub const fn crate_name() -> &'static str {
    "domain"
}

#[cfg(test)]
mod tests {
    #[test]
    fn leaf_crate_compiles() {
        assert_eq!(super::crate_name(), "domain");
    }
}
