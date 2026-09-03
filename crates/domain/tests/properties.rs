//! Property tests: round-trips for every newtype, invariants for the aggregates, totality of
//! the state machine. `proptest` shrinks counterexamples; a failure here is a domain bug.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing,
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    clippy::single_match
)]

use domain::plan::{PlanDefaults, RawPlan, RawPlannedTask};
use domain::task::{IncidentReason, MAX_LEASE_EXPIRIES};
use domain::{
    AgentId, Attempts, BeadId, BranchName, Budget, Duration, Event, Lease, NonEmpty, Priority, Sha,
    Task, TaskState, Timestamp, Title, Tokens, Usage, VerifyCommand,
};
use proptest::prelude::*;

// ---- generators -------------------------------------------------------------------------------

fn bead_id() -> impl Strategy<Value = BeadId> {
    "[a-z]{1,4}-[a-z0-9]{1,6}(\\.[0-9]{1,3}){0,2}".prop_map(|s| BeadId::try_new(s).unwrap())
}
fn agent_id() -> impl Strategy<Value = AgentId> {
    "[A-Za-z0-9_.-]{1,12}".prop_map(|s| AgentId::try_new(s).unwrap())
}
fn branch() -> impl Strategy<Value = BranchName> {
    "[A-Za-z0-9][A-Za-z0-9_-]{0,8}(/[A-Za-z0-9_-]{1,8}){0,2}"
        .prop_map(|s| BranchName::try_new(s).unwrap())
}
fn sha() -> impl Strategy<Value = Sha> {
    "[0-9a-f]{40}".prop_map(|s| Sha::try_new(s).unwrap())
}
fn timestamp() -> impl Strategy<Value = Timestamp> {
    (0i64..4_000_000_000).prop_map(Timestamp::from_unix_seconds)
}
fn duration() -> impl Strategy<Value = Duration> {
    (1u64..100_000).prop_map(Duration::from_seconds)
}
fn budget() -> impl Strategy<Value = Budget> {
    (1u64..1_000_000, duration(), 1u32..10).prop_map(|(t, w, a)| Budget {
        tokens: Tokens::new(t),
        wall_clock: w,
        attempts: Attempts::new(a),
    })
}
fn task() -> impl Strategy<Value = Task> {
    (bead_id(), bead_id(), sha(), budget()).prop_map(|(id, v, base, b)| Task::new(id, v, base, b))
}
fn event(holder: AgentId) -> impl Strategy<Value = Event> {
    let h = holder.clone();
    prop_oneof![
        (timestamp(), duration()).prop_map({
            let h = h.clone();
            move |(now, ttl)| Event::Claim {
                holder: h.clone(),
                now,
                ttl,
            }
        }),
        timestamp().prop_map({
            let h = h.clone();
            move |now| Event::Heartbeat {
                holder: h.clone(),
                now,
            }
        }),
        (branch(), sha(), timestamp(), 0u64..100_000).prop_map({
            let h = h.clone();
            move |(b, s, now, t)| Event::Submit {
                holder: h.clone(),
                branch: b,
                head: s,
                now,
                tokens: Tokens::new(t),
            }
        }),
        timestamp().prop_map(|now| Event::LeaseExpired { now }),
        timestamp().prop_map({
            let h = h.clone();
            move |now| Event::Release {
                holder: h.clone(),
                now,
                note: "r".into(),
                blocked: false,
            }
        }),
        Just(Event::VerifyPassed),
        Just(Event::VerifyFailed { note: "f".into() }),
        sha().prop_map(|s| Event::Merged { merged: s }),
        Just(Event::MergeFailed { detail: "m".into() }),
        Just(Event::Escalate {
            reason: IncidentReason::Manual { detail: "e".into() }
        }),
    ]
}

// ---- newtype round-trips ---------------------------------------------------------------------

proptest! {
    #[test]
    fn ids_roundtrip_through_display(id in bead_id(), a in agent_id(), b in branch(), s in sha()) {
        let task_branch = BranchName::for_task(&id).unwrap();
        prop_assert_eq!(task_branch.as_ref(), format!("task/{id}"));
        prop_assert_eq!(BeadId::try_new(id.to_string()).unwrap(), id);
        prop_assert_eq!(AgentId::try_new(a.to_string()).unwrap(), a);
        prop_assert_eq!(BranchName::try_new(b.to_string()).unwrap(), b);
        prop_assert_eq!(Sha::try_new(s.to_string().to_uppercase()).unwrap(), s, "sha is case-normalised");
    }

    #[test]
    fn title_and_command_trim_and_bound(raw in "[ -~]{0,300}") {
        match Title::try_new(raw.clone()) {
            Ok(t) => {
                prop_assert_eq!(t.as_ref(), raw.trim());
                prop_assert!(t.as_ref().chars().count() <= 200);
            }
            Err(_) => prop_assert!(raw.trim().is_empty() || raw.trim().chars().count() > 200),
        }
        match VerifyCommand::try_new(raw.clone()) {
            Ok(c) => prop_assert_eq!(c.as_ref(), raw.trim()),
            Err(_) => prop_assert!(raw.trim().is_empty()),
        }
        let derived = Title::derived(&raw);
        prop_assert!(!derived.as_ref().is_empty() && derived.as_ref().chars().count() <= 200);
    }

    #[test]
    fn priority_accepts_exactly_0_to_4(n in 0u8..=255) {
        prop_assert_eq!(Priority::try_from(n).is_ok(), n <= 4);
    }

    #[test]
    fn nonempty_roundtrips_and_preserves_order(v in prop::collection::vec(any::<u16>(), 0..20)) {
        match NonEmpty::try_from(v.clone()) {
            Ok(n) => {
                prop_assert_eq!(n.len(), v.len());
                prop_assert_eq!(Vec::from(n), v);
            }
            Err(_) => prop_assert!(v.is_empty()),
        }
    }

    #[test]
    fn timestamp_arithmetic_is_saturating_and_monotone(a in timestamp(), b in timestamp(), d in duration()) {
        prop_assert!(a + d >= a);
        if a >= b {
            prop_assert_eq!((a.since(b)).seconds(), u64::try_from(a.unix_seconds() - b.unix_seconds()).unwrap());
        } else {
            prop_assert_eq!(a.since(b).seconds(), 0);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn counts_serde_transparent(t in any::<u64>(), a in any::<u32>()) {
        prop_assert_eq!(serde_json::to_string(&Tokens::new(t)).unwrap(), t.to_string());
        prop_assert_eq!(serde_json::from_str::<Attempts>(&a.to_string()).unwrap(), Attempts::new(a));
    }
}

// ---- aggregate invariants --------------------------------------------------------------------

proptest! {
    #[test]
    fn lease_renewal_never_extends_past_now_plus_ttl(holder in agent_id(), t0 in timestamp(), ttl in duration(), later in 0i64..100_000) {
        let lease = Lease::grant(holder, t0, ttl);
        let now = Timestamp::from_unix_seconds(t0.unix_seconds() + later);
        let renewed = lease.clone().renew(now, ttl);
        prop_assert_eq!(renewed.expires, now + ttl);
        prop_assert_eq!(renewed.claimed_at, lease.claimed_at);
        prop_assert_eq!(lease.is_expired(now), now >= lease.expires);
    }

    #[test]
    fn budget_check_is_monotone(b in budget(), u1 in (0u64..2_000_000, 0u64..200_000, 0u32..20), extra in (0u64..1_000, 0u64..1_000, 0u32..3)) {
        let u = Usage { tokens: Tokens::new(u1.0), wall_clock: Duration::from_seconds(u1.1), attempts: Attempts::new(u1.2) };
        let mut more = u.add_tokens(Tokens::new(extra.0)).add_wall_clock(Duration::from_seconds(extra.1));
        for _ in 0..extra.2 { more = more.add_attempt(); }
        if b.check(u).is_err() {
            prop_assert!(b.check(more).is_err(), "exceeding more can never un-exceed");
        }
    }

    #[test]
    fn task_apply_is_total_and_never_leaves_a_terminal_state(
        t in task(), events in agent_id().prop_flat_map(|h| prop::collection::vec(event(h), 1..12))
    ) {
        let mut cur = t;
        for ev in events {
            let was_terminal = cur.state.is_terminal();
            match cur.clone().apply(ev) {
                Ok(tr) => {
                    prop_assert!(!was_terminal, "terminal states accept no events");
                    prop_assert!(tr.task.usage.attempts <= tr.task.budget.attempts || tr.task.state.is_terminal(),
                        "attempts over budget must be an incident");
                    cur = tr.task;
                }
                Err(_) => {}
            }
        }
        // Lease storms are bounded.
        prop_assert!(cur.lease_expiries <= MAX_LEASE_EXPIRIES);
        if let TaskState::Incident { reason: IncidentReason::LeaseStorm { expiries } } = &cur.state {
            prop_assert_eq!(*expiries, MAX_LEASE_EXPIRIES);
        }
    }

    #[test]
    fn plan_validation_orders_needs_before_dependents(n in 1usize..8, edges in prop::collection::vec((0usize..8, 0usize..8), 0..12)) {
        let keys: Vec<String> = (0..n).map(|i| format!("t{i}")).collect();
        // Only forward edges (i needs j with j < i) so the plan is acyclic by construction.
        let tasks: Vec<RawPlannedTask> = (0..n).map(|i| RawPlannedTask {
            key: keys[i].clone(),
            title: format!("Task {i}"),
            description: String::new(),
            acceptance: vec![],
            verify: vec!["true".into()],
            needs: edges.iter().filter(|(a, b)| *a == i && *b < i).map(|(_, b)| keys[*b].clone()).collect(),
        }).collect();
        let plan = RawPlan { summary: "s".into(), reference: None, tasks }.validate(PlanDefaults::default()).unwrap();
        let order: Vec<String> = plan.tasks.iter().map(|t| t.key.to_string()).collect();
        for t in plan.tasks.iter() {
            let pos = order.iter().position(|k| *k == t.key.to_string()).unwrap();
            for need in &t.needs {
                let need_pos = order.iter().position(|k| *k == need.to_string()).unwrap();
                prop_assert!(need_pos < pos, "{need} must come before {}", t.key);
            }
        }
        prop_assert_eq!(plan.tasks.len(), n);
    }
}
