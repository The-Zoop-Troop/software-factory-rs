//! The protected-branch guard of the Integrator.
use super::*;
use crate::testing::{FakeRepo, FakeRunner, FakeStore, FixedClock, MemorySink};

#[tokio::test]
async fn a_protected_integration_branch_is_refused_before_anything_runs() {
    let store = FakeStore::default();
    let repo = FakeRepo::default();
    let runner = FakeRunner::default();
    let clock = FixedClock(domain::Timestamp::from_unix_seconds(0));
    let log = MemorySink::default();
    let main = BranchName::try_new("main").expect("branch");
    let cfg = IntegrateConfig {
        main: main.clone(),
        remote: Some("origin".into()),
        checks: vec![],
        check_timeout: Duration::from_minutes(1),
        protected: vec![main.clone(), BranchName::try_new("master").expect("branch")],
    };
    let err = integrate_once(&store, &repo, &runner, &clock, &log, &cfg, "i")
        .await
        .expect_err("refused");
    assert_eq!(err, IntegratorError::ProtectedBranch { branch: main });
    assert!(err.to_string().contains("protected"));
    assert!(log.events().await.is_empty());
}
