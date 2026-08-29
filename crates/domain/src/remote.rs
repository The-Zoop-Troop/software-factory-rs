//! Remote control: who may do what to which rig, and how much a rig may spend.
//! Pure types; the console (an adapter) authenticates tokens and enforces these.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ClientId, MicroUsd, RigName, Tokens};

/// A verb a remote client may be granted on a rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum Scope {
    /// Read tasks, subscribe to events.
    Watch,
    /// Submit plans and stop epics.
    Plan,
    /// Answer questions and resolve incidents.
    Resolve,
    /// Everything, including registry changes.
    Admin,
}

impl Scope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Watch => "watch",
            Self::Plan => "plan",
            Self::Resolve => "resolve",
            Self::Admin => "admin",
        }
    }
}

/// Unknown scope name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown scope `{0}` (watch|plan|resolve|admin)")]
pub struct UnknownScope(pub String);

impl core::str::FromStr for Scope {
    type Err = UnknownScope;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "watch" => Ok(Self::Watch),
            "plan" => Ok(Self::Plan),
            "resolve" => Ok(Self::Resolve),
            "admin" => Ok(Self::Admin),
            other => Err(UnknownScope(other.to_owned())),
        }
    }
}

/// An authenticated client and what it may do. Built by the console's authenticator from a
/// verified token; the core never sees the token itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub client: ClientId,
    /// Rig → scopes. `Admin` on a rig implies every scope on it.
    pub grants: BTreeMap<RigName, BTreeSet<Scope>>,
}

impl Principal {
    #[must_use]
    pub fn allows(&self, rig: &RigName, scope: Scope) -> bool {
        self.grants
            .get(rig)
            .is_some_and(|s| s.contains(&scope) || s.contains(&Scope::Admin))
    }

    /// Rigs the client may at least watch.
    #[must_use]
    pub fn visible_rigs(&self) -> Vec<&RigName> {
        self.grants
            .iter()
            .filter(|(r, _)| self.allows(r, Scope::Watch))
            .map(|(r, _)| r)
            .collect()
    }
}

/// A missing grant, with everything an audit line needs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("client `{client}` lacks `{}` on rig `{rig}`", scope.as_str())]
pub struct Forbidden {
    pub client: ClientId,
    pub rig: RigName,
    pub scope: Scope,
}

/// Require `scope` on `rig`, or produce the audit-ready refusal.
///
/// # Errors
/// `Forbidden` when the principal has neither `scope` nor `Admin` on the rig.
pub fn require(principal: &Principal, rig: &RigName, scope: Scope) -> Result<(), Forbidden> {
    if principal.allows(rig, scope) {
        Ok(())
    } else {
        Err(Forbidden {
            client: principal.client.clone(),
            rig: rig.clone(),
            scope,
        })
    }
}

/// Caps on what a rig may consume, checked before accepting new work. `None` = uncapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RigBudget {
    pub max_tokens: Option<Tokens>,
    pub max_usd: Option<MicroUsd>,
}

/// What a rig has consumed so far, as summed from its ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RigSpend {
    pub tokens: Tokens,
    pub usd: MicroUsd,
}

/// Which cap a rig has exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "kind", rename_all = "snake_case")
)]
pub enum RigBudgetExceeded {
    #[error("rig token cap reached: {spent} of {cap}")]
    Tokens { spent: Tokens, cap: Tokens },
    #[error("rig spend cap reached: {spent} of {cap} micro-USD")]
    Usd { spent: MicroUsd, cap: MicroUsd },
}

impl RigBudget {
    /// New work is admitted while spend is strictly below every cap.
    ///
    /// # Errors
    /// The first exhausted cap, tokens checked before USD.
    pub fn admit(self, spent: RigSpend) -> Result<(), RigBudgetExceeded> {
        if let Some(cap) = self.max_tokens
            && spent.tokens >= cap
        {
            return Err(RigBudgetExceeded::Tokens {
                spent: spent.tokens,
                cap,
            });
        }
        if let Some(cap) = self.max_usd
            && spent.usd >= cap
        {
            return Err(RigBudgetExceeded::Usd {
                spent: spent.usd,
                cap,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(s: &str) -> RigName {
        RigName::try_new(s).expect("valid rig")
    }

    fn principal(grants: &[(&str, &[Scope])]) -> Principal {
        Principal {
            client: ClientId::try_new("cli").expect("valid"),
            grants: grants
                .iter()
                .map(|(r, s)| (rig(r), s.iter().copied().collect()))
                .collect(),
        }
    }

    #[test]
    fn scope_roundtrip_and_unknown() {
        for s in [Scope::Watch, Scope::Plan, Scope::Resolve, Scope::Admin] {
            assert_eq!(s.as_str().parse::<Scope>(), Ok(s));
        }
        assert_eq!(
            "root".parse::<Scope>(),
            Err(UnknownScope("root".to_owned()))
        );
    }

    #[test]
    fn admin_implies_everything_on_that_rig_only() {
        let p = principal(&[("toy", &[Scope::Admin]), ("api", &[Scope::Watch])]);
        assert!(p.allows(&rig("toy"), Scope::Resolve));
        assert!(!p.allows(&rig("api"), Scope::Plan));
        assert!(!p.allows(&rig("web"), Scope::Watch));
        assert_eq!(p.visible_rigs(), vec![&rig("api"), &rig("toy")]);
        let err = require(&p, &rig("api"), Scope::Plan).expect_err("forbidden");
        assert_eq!(err.scope, Scope::Plan);
        assert!(err.to_string().contains("lacks `plan` on rig `api`"));
    }

    #[test]
    fn visible_rigs_excludes_grants_without_watch() {
        let p = principal(&[("toy", &[Scope::Plan])]);
        assert!(p.visible_rigs().is_empty());
    }

    #[test]
    fn budget_admits_below_caps_and_refuses_at_them() {
        let b = RigBudget {
            max_tokens: Some(Tokens::new(100)),
            max_usd: Some(MicroUsd::new(5)),
        };
        assert_eq!(b.admit(RigSpend::default()), Ok(()));
        assert_eq!(
            b.admit(RigSpend {
                tokens: Tokens::new(100),
                usd: MicroUsd::new(0)
            }),
            Err(RigBudgetExceeded::Tokens {
                spent: Tokens::new(100),
                cap: Tokens::new(100)
            })
        );
        assert_eq!(
            b.admit(RigSpend {
                tokens: Tokens::new(1),
                usd: MicroUsd::new(9)
            }),
            Err(RigBudgetExceeded::Usd {
                spent: MicroUsd::new(9),
                cap: MicroUsd::new(5)
            })
        );
        assert_eq!(RigBudget::default().admit(RigSpend::default()), Ok(()));
    }

    #[test]
    fn ids_validate() {
        assert!(RigName::try_new("Toy").is_err());
        assert!(RigName::try_new("toy-1").is_ok());
        assert!(ClientId::try_new("").is_err());
        assert!(ClientId::try_new("bot@tg").is_ok());
    }
}
