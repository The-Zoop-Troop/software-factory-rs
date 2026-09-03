# Task state machine (generated)

- **Status:** generated · **Verified:** by `cargo xtask gen-docs --check` in CI. Do not edit by hand.

Source: `crates/domain/src/task.rs`.

## States

- `open`
- `leased`
- `in_verify`
- `mergeable`
- `closed`
- `incident`

## Events

- `claim`
- `heartbeat`
- `submit`
- `lease_expired`
- `release`
- `verify_passed`
- `verify_failed`
- `merged`
- `merge_failed`
- `escalate`

## Constants

- Lease expiries before a lease-storm incident: `3`
- Consecutive blocked releases before a release-loop incident: `2`
- Default budget: tokens `400000`, wall clock `2700s`, attempts `3`
