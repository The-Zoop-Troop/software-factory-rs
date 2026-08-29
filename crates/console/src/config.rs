//! Boundary decoding of the registry and token files. Raw TOML shapes in, domain types out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use domain::{ClientId, MicroUsd, RigBudget, RigName, Scope, Tokens};

/// A rig as the console operates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RigSpec {
    pub name: RigName,
    /// Directory holding `.beads/` (the ledger volume).
    pub ledger: PathBuf,
    /// The rig's `events.jsonl`.
    pub events: PathBuf,
    /// Command that plans inside the rig; the plan text is appended as `--text <plan>`.
    pub plan_cmd: Vec<String>,
    pub budget: RigBudget,
}

/// A client token: the sha256 hex of the bearer value and what it grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenSpec {
    pub client: ClientId,
    pub sha256: String,
    pub grants: BTreeMap<RigName, BTreeSet<Scope>>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {detail}")]
    Parse { path: PathBuf, detail: String },
    #[error("rig `{rig}`: {detail}")]
    Rig { rig: String, detail: String },
    #[error("token for `{client}`: {detail}")]
    Token { client: String, detail: String },
}

#[derive(Debug, serde::Deserialize)]
struct RawRegistry {
    #[serde(default)]
    rig: Vec<RawRig>,
}

#[derive(Debug, serde::Deserialize)]
struct RawRig {
    name: String,
    ledger: PathBuf,
    events: PathBuf,
    plan_cmd: Vec<String>,
    #[serde(default)]
    max_tokens: Option<u64>,
    #[serde(default)]
    max_usd_micros: Option<u64>,
}

impl TryFrom<RawRig> for RigSpec {
    type Error = ConfigError;

    fn try_from(raw: RawRig) -> Result<Self, Self::Error> {
        let err = |detail: String| ConfigError::Rig {
            rig: raw.name.clone(),
            detail,
        };
        let name = RigName::try_new(&raw.name).map_err(|e| err(e.to_string()))?;
        if raw.plan_cmd.is_empty() {
            return Err(err("plan_cmd must name a command".to_owned()));
        }
        Ok(Self {
            name,
            ledger: raw.ledger,
            events: raw.events,
            plan_cmd: raw.plan_cmd,
            budget: RigBudget {
                max_tokens: raw.max_tokens.map(Tokens::new),
                max_usd: raw.max_usd_micros.map(MicroUsd::new),
            },
        })
    }
}

#[derive(Debug, serde::Deserialize)]
struct RawTokens {
    #[serde(default)]
    token: Vec<RawToken>,
}

#[derive(Debug, serde::Deserialize)]
struct RawToken {
    client: String,
    sha256: String,
    #[serde(default)]
    grants: BTreeMap<String, Vec<String>>,
}

impl TryFrom<RawToken> for TokenSpec {
    type Error = ConfigError;

    fn try_from(raw: RawToken) -> Result<Self, Self::Error> {
        let err = |detail: String| ConfigError::Token {
            client: raw.client.clone(),
            detail,
        };
        let client = ClientId::try_new(&raw.client).map_err(|e| err(e.to_string()))?;
        if raw.sha256.len() != 64 || !raw.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(err("sha256 must be 64 hex chars".to_owned()));
        }
        let grants = raw
            .grants
            .iter()
            .map(|(rig, scopes)| {
                let rig = RigName::try_new(rig).map_err(|e| err(e.to_string()))?;
                let scopes = scopes
                    .iter()
                    .map(|s| s.parse::<Scope>().map_err(|e| err(e.to_string())))
                    .collect::<Result<BTreeSet<_>, _>>()?;
                Ok((rig, scopes))
            })
            .collect::<Result<BTreeMap<_, _>, ConfigError>>()?;
        Ok(Self {
            client,
            sha256: raw.sha256.to_ascii_lowercase(),
            grants,
        })
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|e| ConfigError::Parse {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

/// # Errors
/// Unreadable, malformed, or invalid registry.
pub(crate) fn load_registry(path: &Path) -> Result<Vec<RigSpec>, ConfigError> {
    parse_registry(read::<RawRegistry>(path)?)
}

fn parse_registry(raw: RawRegistry) -> Result<Vec<RigSpec>, ConfigError> {
    let rigs = raw
        .rig
        .into_iter()
        .map(RigSpec::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = BTreeSet::new();
    for r in &rigs {
        if !seen.insert(&r.name) {
            return Err(ConfigError::Rig {
                rig: r.name.to_string(),
                detail: "listed twice".to_owned(),
            });
        }
    }
    Ok(rigs)
}

/// # Errors
/// Unreadable, malformed, or invalid token file.
pub(crate) fn load_tokens(path: &Path) -> Result<Vec<TokenSpec>, ConfigError> {
    read::<RawTokens>(path)?
        .token
        .into_iter()
        .map(TokenSpec::try_from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_decodes_and_rejects_bad_rigs() {
        let raw: RawRegistry = toml::from_str(
            r#"
            [[rig]]
            name = "toy"
            ledger = "/srv/toy/ledger"
            events = "/srv/toy/events.jsonl"
            plan_cmd = ["docker", "compose", "-p", "toy", "run", "--rm", "plan"]
            max_tokens = 5000
            "#,
        )
        .expect("toml");
        let rigs = parse_registry(raw).expect("valid");
        assert_eq!(rigs[0].name.as_ref(), "toy");
        assert_eq!(rigs[0].budget.max_tokens, Some(Tokens::new(5000)));
        assert_eq!(rigs[0].budget.max_usd, None);

        let bad: RawRegistry = toml::from_str(
            "[[rig]]\nname = \"Toy\"\nledger = \"a\"\nevents = \"b\"\nplan_cmd = [\"x\"]\n",
        )
        .expect("toml");
        assert!(matches!(parse_registry(bad), Err(ConfigError::Rig { .. })));
        let empty: RawRegistry = toml::from_str(
            "[[rig]]\nname = \"toy\"\nledger = \"a\"\nevents = \"b\"\nplan_cmd = []\n",
        )
        .expect("toml");
        assert!(
            parse_registry(empty)
                .unwrap_err()
                .to_string()
                .contains("plan_cmd")
        );
        let dup: RawRegistry = toml::from_str(
            "[[rig]]\nname = \"toy\"\nledger = \"a\"\nevents = \"b\"\nplan_cmd = [\"x\"]\n[[rig]]\nname = \"toy\"\nledger = \"a\"\nevents = \"b\"\nplan_cmd = [\"x\"]\n",
        )
        .expect("toml");
        assert!(
            parse_registry(dup)
                .unwrap_err()
                .to_string()
                .contains("twice")
        );
    }

    #[test]
    fn tokens_decode_and_reject_bad_entries() {
        let ok = RawToken {
            client: "phone".into(),
            sha256: "A".repeat(64),
            grants: BTreeMap::from([(
                "toy".to_owned(),
                vec!["watch".to_owned(), "plan".to_owned()],
            )]),
        };
        let spec = TokenSpec::try_from(ok).expect("valid");
        assert_eq!(spec.sha256, "a".repeat(64));
        assert!(spec.grants[&RigName::try_new("toy").expect("r")].contains(&Scope::Plan));
        for (client, sha, grants) in [
            ("", "a".repeat(64), BTreeMap::new()),
            ("p", "zz".into(), BTreeMap::new()),
            (
                "p",
                "a".repeat(64),
                BTreeMap::from([("BAD".to_owned(), vec![])]),
            ),
            (
                "p",
                "a".repeat(64),
                BTreeMap::from([("toy".to_owned(), vec!["root".to_owned()])]),
            ),
        ] {
            let raw = RawToken {
                client: client.into(),
                sha256: sha,
                grants,
            };
            assert!(matches!(
                TokenSpec::try_from(raw),
                Err(ConfigError::Token { .. })
            ));
        }
    }

    #[test]
    fn files_are_read_or_reported() {
        let dir = std::env::temp_dir().join(format!("console-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp");
        assert!(matches!(
            load_registry(&dir.join("missing.toml")),
            Err(ConfigError::Read { .. })
        ));
        std::fs::write(dir.join("bad.toml"), "not = [toml").expect("write");
        assert!(matches!(
            load_tokens(&dir.join("bad.toml")),
            Err(ConfigError::Parse { .. })
        ));
        std::fs::write(
            dir.join("t.toml"),
            "[[token]]\nclient = \"c\"\nsha256 = \"".to_owned() + &"b".repeat(64) + "\"\n",
        )
        .expect("write");
        assert_eq!(load_tokens(&dir.join("t.toml")).expect("ok").len(), 1);
        std::fs::write(dir.join("r.toml"), "").expect("write");
        assert!(load_registry(&dir.join("r.toml")).expect("ok").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
