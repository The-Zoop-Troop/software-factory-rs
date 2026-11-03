//! What a role's child process inherits. A harness session is an agent with a shell, and a verify
//! command is the candidate's own code, so neither gets the role's secrets: the ledger password,
//! the git token, another harness's credentials, or anything shaped like a secret. A role keeps
//! them for itself, and `harden_process` stops a child reading them from `/proc`.

use tokio::process::Command;

/// Secrets the factory holds for itself, by exact name. Never passed to a child, even when
/// `RIG_SESSION_ENV` names them, because each reaches a gate: the ledger, the remote, the console.
pub const FACTORY_SECRETS: [&str; 9] = [
    "BEADS_DOLT_PASSWORD",
    "LEDGER_PASSWORD",
    "RIG_GIT_TOKEN",
    "CLAUDE_AUTH_JSON",
    "CODEX_AUTH_JSON",
    "CODEX_OAUTH_TOKEN",
    "FACTORY_TOKEN",
    "TELEGRAM_BOT_TOKEN",
    "OPENCODE_CONFIG_CONTENT",
];

/// Names a project can pass through to sessions and verify commands although they look like
/// secrets: comma- or space-separated, for example `RIG_SESSION_ENV=STRIPE_TEST_KEY`.
pub const PASS_THROUGH: &str = "RIG_SESSION_ENV";

/// Credentials each harness itself needs. A session can read its own harness's key; that key
/// reaches the provider, not a gate.
pub const CLAUDE_CREDENTIALS: [&str; 3] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
];
pub const CODEX_CREDENTIALS: [&str; 1] = ["OPENAI_API_KEY"];
pub const OPENCODE_CREDENTIALS: [&str; 1] = ["OPENCODE_API_KEY"];

/// The exact environment a child starts with.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChildEnv {
    vars: Vec<(String, String)>,
}

impl ChildEnv {
    /// This process's environment, scrubbed, keeping `keep` (a harness's own credentials).
    #[must_use]
    pub fn from_process(keep: &[&str]) -> Self {
        Self::from_vars(std::env::vars(), keep)
    }

    /// Pure: `vars` scrubbed, keeping `keep` and the names `RIG_SESSION_ENV` lists in `vars`.
    #[must_use]
    pub fn from_vars(vars: impl IntoIterator<Item = (String, String)>, keep: &[&str]) -> Self {
        let vars: Vec<(String, String)> = vars.into_iter().collect();
        let passed: Vec<String> = vars
            .iter()
            .find(|(k, _)| k == PASS_THROUGH)
            .map(|(_, v)| {
                v.split([',', ' '])
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let allowed = |name: &str| {
            !FACTORY_SECRETS.contains(&name)
                && (keep.contains(&name)
                    || passed.iter().any(|p| p == name)
                    || !secret_shaped(name))
        };
        Self {
            vars: vars.into_iter().filter(|(k, _)| allowed(k)).collect(),
        }
    }

    /// The value of `name` in this environment.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Variable names, for tests and diagnostics; values are never logged.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.vars.iter().map(|(k, _)| k.as_str()).collect()
    }

    /// Replace `cmd`'s environment with exactly this one.
    pub fn apply(&self, cmd: &mut Command) {
        cmd.env_clear()
            .envs(self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
}

/// Pure: does this name look like it holds a secret?
#[must_use]
pub fn secret_shaped(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "PRIVATE",
    ]
    .iter()
    .any(|w| upper.contains(w))
        || upper.ends_with("_KEY")
        || upper.ends_with("_AUTH_JSON")
}

/// Stop this process's children reading its memory or `/proc/<pid>/environ`: a non-dumpable
/// process's `/proc` entries belong to root, and the rig drops every capability. Children start
/// dumpable again, so this protects only the role, which holds the secrets.
///
/// # Errors
/// If the kernel refuses the change.
#[cfg(target_os = "linux")]
pub fn harden_process() -> std::io::Result<()> {
    rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable)
        .map_err(std::io::Error::from)
}

/// Outside Linux there is no `/proc` to protect.
///
/// # Errors
/// Never.
#[cfg(not(target_os = "linux"))]
pub fn harden_process() -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn the_factorys_secrets_never_reach_a_child() {
        let env = ChildEnv::from_vars(
            vars(&[
                ("PATH", "/usr/bin"),
                ("HOME", "/home/factory"),
                ("BEADS_DOLT_PASSWORD", "x"),
                ("LEDGER_PASSWORD", "x"),
                ("RIG_GIT_TOKEN", "x"),
                ("CLAUDE_AUTH_JSON", "x"),
                ("DATABASE_URL", "postgresql://postgres:5432/db"),
                ("npm_config_cache", "/work/cache/npm"),
                (PASS_THROUGH, "RIG_GIT_TOKEN"),
            ]),
            &["RIG_GIT_TOKEN"],
        );
        assert_eq!(
            env.names(),
            [
                "PATH",
                "HOME",
                "DATABASE_URL",
                "npm_config_cache",
                PASS_THROUGH
            ],
            "a factory secret stays out even when kept or passed through"
        );
    }

    #[test]
    fn a_harness_keeps_its_own_credential_and_loses_the_others() {
        let all = vars(&[
            ("ANTHROPIC_API_KEY", "a"),
            ("OPENAI_API_KEY", "b"),
            ("OPENCODE_API_KEY", "c"),
            ("AWS_SECRET_ACCESS_KEY", "d"),
        ]);
        assert_eq!(
            ChildEnv::from_vars(all.clone(), &CLAUDE_CREDENTIALS).names(),
            ["ANTHROPIC_API_KEY"]
        );
        assert_eq!(
            ChildEnv::from_vars(all.clone(), &OPENCODE_CREDENTIALS).names(),
            ["OPENCODE_API_KEY"]
        );
        assert!(ChildEnv::from_vars(all, &[]).names().is_empty());
    }

    #[test]
    fn named_variables_pass_through() {
        let env = ChildEnv::from_vars(
            vars(&[
                ("STRIPE_TEST_KEY", "k"),
                ("OTHER_TOKEN", "t"),
                (PASS_THROUGH, "STRIPE_TEST_KEY, NOT_SET"),
            ]),
            &[],
        );
        assert_eq!(env.names(), ["STRIPE_TEST_KEY", PASS_THROUGH]);
        assert_eq!(env.get("STRIPE_TEST_KEY"), Some("k"));
        assert_eq!(env.get("OTHER_TOKEN"), None);
    }

    #[test]
    fn secret_shapes() {
        for name in [
            "GH_TOKEN",
            "db_password",
            "SIGNING_KEY",
            "X_CREDENTIALS",
            "CODEX_AUTH_JSON",
        ] {
            assert!(secret_shaped(name), "{name}");
        }
        for name in [
            "PATH",
            "HOME",
            "CARGO_HOME",
            "KEYBOARD",
            "RIG_AUTH",
            "DATABASE_URL",
        ] {
            assert!(!secret_shaped(name), "{name}");
        }
    }

    #[tokio::test]
    async fn apply_replaces_the_inherited_environment() {
        let env = ChildEnv::from_vars(vars(&[("ONLY", "1"), ("PATH", "/usr/bin:/bin")]), &[]);
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "env | cut -d= -f1 | sort | tr '\\n' ' '"]);
        env.apply(&mut cmd);
        let out = cmd.output().await.unwrap();
        let names = String::from_utf8_lossy(&out.stdout);
        assert!(names.contains("ONLY"), "{names}");
        assert!(!names.contains("HOME "), "inherited HOME leaked: {names}");
    }
}
