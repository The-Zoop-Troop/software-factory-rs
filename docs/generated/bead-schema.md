# Bead metadata schema (generated)

- **Status:** generated · **Verified:** by `cargo xtask gen-docs --check` in CI. Do not edit by hand.

Source: `crates/domain/src/meta.rs`. Each block is the JSON stored under the named key of a bead's `metadata`.

## `metadata.fac`

```json
{
  "base": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "budget": {
    "attempts": 3,
    "tokens": 400000,
    "wall_clock": 2700
  },
  "lease_expiries": 0,
  "state": {
    "state": "open"
  },
  "usage": {
    "attempts": 0,
    "tokens": 0,
    "wall_clock": 0
  },
  "verify_bead": "fac-2",
  "version": 1
}
```

## `metadata.fac_verify`

```json
{
  "commands": [
    "cargo test"
  ],
  "task": "fac-1",
  "timeout": 1200,
  "version": 1
}
```

## `metadata.fac_merge`

```json
{
  "branch": "task/fac-1",
  "head": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "task": "fac-1",
  "version": 1
}
```

Kind labels: `fac:kind=epic`, `fac:kind=task`, `fac:kind=verify`, `fac:kind=merge`, `fac:kind=question`, `fac:kind=incident`, `fac:kind=reference`, `fac:kind=plan_request`, `fac:kind=contract`
