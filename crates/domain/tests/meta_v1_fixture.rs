//! Stored ledger metadata is a public boundary: every shape ever written must keep decoding.
//! `fixtures/meta_v1.json` is what rigs wrote on 2026-08-28/29 (v1). Re-encoding must be
//! byte-for-byte stable for the full task blob, so tightening never rewrites stored data.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use domain::{FactoryMeta, MergeMeta, TaskState, VerifyMeta};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/meta_v1.json")).unwrap()
}

#[test]
fn v1_task_metadata_decodes_and_reencodes_identically() {
    let raw = fixture()["fac"].clone();
    let meta: FactoryMeta = serde_json::from_value(raw.clone()).unwrap();
    assert!(matches!(meta.state, TaskState::Closed { .. }));
    assert_eq!(meta.usage.tokens.get(), 7362);
    assert_eq!(
        serde_json::to_value(&meta).unwrap(),
        raw,
        "re-encoding v1 must not change stored data"
    );
}

#[test]
fn v1_task_metadata_without_optional_fields_decodes() {
    let meta: FactoryMeta = serde_json::from_value(fixture()["fac_leased"].clone()).unwrap();
    assert!(matches!(meta.state, TaskState::Leased { .. }));
    assert_eq!(
        meta.budget,
        domain::Budget::default(),
        "missing budget takes the default"
    );
}

#[test]
fn v1_verify_and_merge_metadata_decode() {
    let v: VerifyMeta = serde_json::from_value(fixture()["fac_verify"].clone()).unwrap();
    assert_eq!(v.commands.len(), 2);
    assert_eq!(v.timeout.seconds(), 1200);
    let m: MergeMeta = serde_json::from_value(fixture()["fac_merge"].clone()).unwrap();
    assert_eq!(m.branch.as_ref(), "task/rig-v3e.3");
    assert_eq!(serde_json::to_value(&m).unwrap(), fixture()["fac_merge"]);
}
