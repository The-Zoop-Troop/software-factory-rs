//! `BdCli` tests: argument construction and raw-output decoding over a scripted runner.

use super::*;

#[test]
fn raw_bead_with_factory_meta_decodes() {
    let json = r#"{
      "id":"fac-1","title":"t","status":"open",
      "labels":["fac:kind=task"],
      "metadata":{"fac":{"version":1,"verify_bead":"fac-2",
        "base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":{"state":"open"}}}
    }"#;
    let raw: RawBead = serde_json::from_str(json).unwrap();
    let bead = Bead::try_from(raw).unwrap();
    assert_eq!(bead.kind, Some(BeadKind::Task));
    assert!(bead.meta.is_some());
}

#[test]
fn raw_bead_without_meta_is_plain() {
    let raw: RawBead =
        serde_json::from_str(r#"{"id":"fac-1","title":"t","status":"closed"}"#).unwrap();
    let bead = Bead::try_from(raw).unwrap();
    assert_eq!(bead.kind, None);
    assert_eq!(bead.status, BeadStatus::Closed);
}

#[test]
fn stderr_parses_into_each_variant() {
    let e = parse_bd_stderr(
        StoreOp::Close,
        "Error: cannot close t-1.3: blocked by open issues [t-1.2 t-1.1] (use --force to override)",
    );
    assert!(
        matches!(e, StoreError::Blocked { ref id, ref by } if id.as_ref() == "t-1.3" && by.len() == 2)
    );
    assert!(matches!(
        parse_bd_stderr(
            StoreOp::Show,
            "Error fetching x: no issue found matching \"x\""
        ),
        StoreError::Rejected {
            op: StoreOp::Show,
            ..
        }
    ));
    assert!(matches!(
        parse_bd_stderr(StoreOp::Update, "dolt: database is locked"),
        StoreError::Unavailable {
            cause: Unavailable::Locked,
            ..
        }
    ));
    assert!(matches!(
        parse_bd_stderr(StoreOp::Update, "Dolt server error"),
        StoreError::Unavailable {
            cause: Unavailable::Database,
            ..
        }
    ));
    assert!(matches!(
        parse_bd_stderr(StoreOp::Dep, "something else entirely"),
        StoreError::Rejected {
            op: StoreOp::Dep,
            ..
        }
    ));
    assert!(parse_blocked("cannot close: nothing").is_none());
}
