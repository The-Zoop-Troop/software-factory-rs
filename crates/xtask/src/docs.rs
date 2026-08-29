//! `lint-docs`: the knowledge base must be a connected, current map.
//!
//! Rules (each error names the fix):
//! - every `docs/**/*.md` is reachable by links from `AGENTS.md` (through any number of hops);
//! - every relative markdown link resolves to a file;
//! - every file under `docs/design-docs/`, `docs/references/`, `docs/product-specs/` and the
//!   top-level `docs/*.md` carries `**Status:**` and `**Verified:**` markers (index files exempt);
//! - `AGENTS.md` is at most 100 lines.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

/// Docs that must carry `Status`/`Verified` markers: top-level `docs/*.md` plus design docs,
/// references and product specs (index files, PLANS.md and the archived source post exempt).
pub(crate) fn scoped_docs(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    Ok(walk(&root.join("docs"))?
        .into_iter()
        .filter(|doc| {
            let name = doc.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let in_scope = doc.parent().is_some_and(|p| {
                p == root.join("docs")
                    || p.ends_with("design-docs")
                    || p.ends_with("references")
                    || p.ends_with("product-specs")
            });
            doc.extension().is_some_and(|e| e == "md")
                && in_scope
                && name != "index.md"
                && name != "PLANS.md"
                && name != "harness-engineering.md"
        })
        .collect())
}

pub(crate) fn lint(root: &Path) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    let agents = root.join("AGENTS.md");
    let agents_text = std::fs::read_to_string(&agents)?;
    if agents_text.lines().count() > 100 {
        errors.push(format!(
            "AGENTS.md has {} lines; it must be ≤ 100. Move detail into docs/ and link to it.",
            agents_text.lines().count()
        ));
    }

    let all_docs: BTreeSet<PathBuf> = walk(&root.join("docs"))?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();

    // Reachability: links (markdown `](path)` and backticked `docs/...md` paths) from AGENTS.md outward.
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::from([
        agents.clone(),
        root.join("ARCHITECTURE.md"),
        root.join("README.md"),
    ]);
    while let Some(file) = queue.pop_front() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let dir = file.parent().unwrap_or(root);
        for target in link_targets(&text) {
            if target.contains('*') {
                continue;
            }
            // Bare `name.md` mentions may refer to the same directory or to a top-level file.
            let candidate = if target.starts_with("docs/") || target.starts_with("skills/") {
                root.join(&target)
            } else {
                dir.join(&target)
            };
            if target == "index.md" {
                continue; // generic mention of "an index.md"; every directory index is checked by reachability
            }
            let resolved = if candidate.exists() || target.contains('/') {
                candidate
            } else if root.join(&target).exists() {
                root.join(&target)
            } else {
                root.join("docs").join(&target)
            };
            let resolved = normalize(&resolved);
            if resolved.is_dir() {
                for child in walk(&resolved)? {
                    queue.push_back(child);
                }
                continue;
            }
            if resolved.extension().is_some_and(|e| e == "md") {
                if !resolved.exists() {
                    errors.push(format!(
                        "{}: link to `{target}` does not resolve. Fix the path or create the file.",
                        rel(root, &file)
                    ));
                    continue;
                }
                queue.push_back(resolved);
            }
        }
    }
    for doc in &all_docs {
        if !seen.contains(doc) {
            errors.push(format!("{} is not reachable from AGENTS.md. Link it from AGENTS.md or the nearest index.md.", rel(root, doc)));
        }
    }

    // Headers.
    for doc in &all_docs {
        let name = doc.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let in_scope = doc.parent().is_some_and(|p| {
            p == root.join("docs")
                || p.ends_with("design-docs")
                || p.ends_with("references")
                || p.ends_with("product-specs")
        });
        if !in_scope || name == "index.md" || name == "PLANS.md" || name == "harness-engineering.md"
        {
            continue;
        }
        let text = std::fs::read_to_string(doc)?;
        for marker in ["**Status:**", "**Verified:**"] {
            if !text.contains(marker) {
                errors.push(format!("{}: missing `{marker}` line near the top. Add `- **Status:** … · **Verified:** <how and when>`.", rel(root, doc)));
            }
        }
    }

    if errors.is_empty() {
        println!("lint-docs: ok ({} docs reachable)", all_docs.len());
        Ok(())
    } else {
        anyhow::bail!(
            "lint-docs: {} problem(s)\n  - {}",
            errors.len(),
            errors.join("\n  - ")
        )
    }
}

fn link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // [text](path)
    for (i, _) in text.match_indices("](") {
        if let Some(end) = text[i + 2..].find(')') {
            let t = &text[i + 2..i + 2 + end];
            if !t.starts_with("http") && !t.starts_with('#') {
                out.push(t.split('#').next().unwrap_or(t).to_owned());
            }
        }
    }
    // `docs/...` or `docs/.../` mentions in backticks
    for (i, _) in text.match_indices("`docs/") {
        if let Some(end) = text[i + 1..].find('`') {
            out.push(text[i + 1..i + 1 + end].to_owned());
        }
    }
    // `foo.md` mentions in backticks (same directory)
    for (i, _) in text.match_indices('`') {
        if let Some(end) = text[i + 1..].find('`') {
            let t = &text[i + 1..i + 1 + end];
            if std::path::Path::new(t)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                && !t.contains(' ')
                && !t.contains('/')
            {
                out.push(t.to_owned());
            }
        }
    }
    out
}

fn walk(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            out.extend(walk(&p)?);
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other @ (std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::Normal(_)) => out.push(other),
        }
    }
    out
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}
