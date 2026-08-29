//! `lint-fp`: the mechanical sweep from `skills/rust-fp-skill/references/code-review.md` §1,
//! as a CI gate. A hit is allowed only with a `// fp-allow: <reason>` comment on the same or the
//! previous line; every unexplained hit is reported with the rule that forbids it.

use std::path::{Path, PathBuf};

struct Rule {
    name: &'static str,
    /// Substring or simple pattern tested per line.
    hit: fn(&str) -> bool,
    /// Which crate roots the rule applies to.
    scope: &'static [&'static str],
    fix: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        name: "partial: unwrap/expect",
        hit: |l| l.contains(".unwrap()") || l.contains(".expect("),
        scope: &["domain", "app"],
        fix: "return a typed error; `expect` only with an `invariant:` proof in binaries",
    },
    Rule {
        name: "partial: panic/todo/unimplemented/unreachable",
        hit: |l| {
            ["panic!(", "todo!(", "unimplemented!(", "unreachable!("]
                .iter()
                .any(|k| l.contains(k))
        },
        scope: &["domain", "app"],
        fix: "model the case as an error variant",
    },
    Rule {
        name: "error: Box<dyn Error>",
        hit: |l| l.contains("Box<dyn Error") || l.contains("Box<dyn std::error::Error"),
        scope: &["domain", "app", "infra", "factory", "stewardd"],
        fix: "name the error enum (thiserror)",
    },
    Rule {
        name: "error: Result<_, String>",
        hit: |l| l.contains(", String>") && l.contains("Result<"),
        scope: &["domain", "app", "infra", "factory", "stewardd"],
        fix: "errors are data: a thiserror enum with a payload",
    },
    Rule {
        name: "error: String payload variant",
        hit: |l| {
            let t = l.trim();
            t.ends_with("(String),") && t.chars().next().is_some_and(char::is_uppercase)
        },
        scope: &["domain", "app", "infra"],
        fix: "carry actionable fields (ids, counts, paths), not prose",
    },
    Rule {
        name: "error: anyhow outside binaries",
        hit: |l| l.contains("anyhow::") || l.contains("anyhow!("),
        scope: &["domain", "app", "infra"],
        fix: "anyhow only in main.rs",
    },
    Rule {
        name: "error: infra error type in app/domain",
        hit: |l| {
            [
                "sqlx::Error",
                "reqwest::Error",
                "serde_json::Error",
                "std::io::Error",
            ]
            .iter()
            .any(|k| l.contains(k))
        },
        scope: &["domain", "app"],
        fix: "translate at the adapter into a domain error",
    },
    Rule {
        name: "boundary: `as` numeric cast",
        hit: |l| {
            [
                " as u64",
                " as i64",
                " as u32",
                " as usize",
                " as f64",
                " as u8",
            ]
            .iter()
            .any(|k| l.contains(k))
        },
        scope: &["domain", "app", "infra"],
        fix: "u64::try_from(n)? — decode once at the boundary",
    },
    Rule {
        name: "boundary: substring error classification",
        hit: |l| {
            l.contains(".contains(\"")
                && (l.contains("msg") || l.contains("stderr") || l.contains("lower"))
        },
        scope: &["infra"],
        fix: "parse stderr once into a typed variant; branch on the variant",
    },
    Rule {
        name: "money: float in domain/app",
        hit: |l| l.contains(": f64") || l.contains(": f32"),
        scope: &["domain", "app"],
        fix: "integer minor units (e.g. MicroUsd)",
    },
    Rule {
        name: "capability: clock/random not injected",
        hit: |l| {
            [
                "SystemTime::now",
                "Instant::now",
                "Utc::now",
                "rand::random",
                "thread_rng",
            ]
            .iter()
            .any(|k| l.contains(k))
        },
        scope: &["domain", "app", "infra", "factory", "stewardd"],
        fix: "inject a Clock / IdGen; only crates/infra/src/clock.rs may call these",
    },
    Rule {
        name: "exhaustiveness: catch-all arm",
        hit: |l| l.trim_start().starts_with("_ =>"),
        scope: &["domain", "app"],
        fix: "name every variant so a new one breaks the build",
    },
    Rule {
        name: "discard: let _ = on a Result",
        hit: |l| {
            l.trim_start().starts_with("let _ = ")
                && !l.contains("write!")
                && !l.contains("writeln!")
        },
        scope: &["domain", "app", "infra"],
        fix: "handle it, map it to a typed outcome, or justify with fp-allow",
    },
    Rule {
        name: "test doubles: mockall",
        hit: |l| l.contains("mockall"),
        scope: &["domain", "app", "infra", "factory", "stewardd"],
        fix: "hand-written fakes of your own traits",
    },
];

pub(crate) fn lint(root: &Path) -> anyhow::Result<()> {
    let mut hits = Vec::new();
    for rule in RULES {
        for krate in rule.scope {
            for file in rust_files(&root.join("crates").join(krate).join("src"))? {
                let text = std::fs::read_to_string(&file)?;
                let lines: Vec<&str> = text.lines().collect();
                let mut in_tests = false;
                for (i, line) in lines.iter().enumerate() {
                    if line.contains("#[cfg(") && line.contains("test") {
                        in_tests = true;
                    }
                    let is_clock_adapter =
                        file.ends_with("clock.rs") && rule.name.starts_with("capability");
                    let is_comment = line.trim_start().starts_with("//");
                    if in_tests || is_comment || is_clock_adapter || !(rule.hit)(line) {
                        continue;
                    }
                    let prev = i
                        .checked_sub(1)
                        .and_then(|p| lines.get(p))
                        .copied()
                        .unwrap_or("");
                    if line.contains("fp-allow:") || prev.contains("fp-allow:") {
                        continue;
                    }
                    hits.push(format!(
                        "{}:{}: [{}] {}\n      fix: {}",
                        rel(root, &file),
                        i + 1,
                        rule.name,
                        line.trim(),
                        rule.fix
                    ));
                }
            }
        }
    }
    if hits.is_empty() {
        println!("lint-fp: ok");
        Ok(())
    } else {
        anyhow::bail!(
            "lint-fp: {} unexplained hit(s) (justify with `// fp-allow: <why>` or fix)\n  - {}",
            hits.len(),
            hits.join("\n  - ")
        )
    }
}

fn rust_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            out.extend(rust_files(&p)?);
        } else if p.extension().is_some_and(|e| e == "rs")
            && !p.to_string_lossy().ends_with("_tests.rs")
            && !p.ends_with("testing.rs")
        {
            out.push(p);
        }
    }
    Ok(out)
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}
