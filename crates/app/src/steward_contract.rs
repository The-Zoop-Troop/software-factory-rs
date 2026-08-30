//! Contract artifacts: when an epic closes, what it landed — commit range, changed public
//! surface, and the plan text — written as a `contract` bead on the epic's rig so a downstream
//! planner (another rig included) can build on what actually landed, not on what was asked.

use std::fmt::Write as _;

use domain::{BeadId, BeadKind, BranchName, Priority, Sha, Title};

use crate::bead::{Bead, NewBead};
use crate::ports::{BeadStore, DiffSummary, Repo, RepoError, StoreError};

/// Where to read what landed: the rig's repository and its integration branch.
#[derive(Clone, Copy)]
pub struct ContractSource<'a> {
    pub repo: &'a dyn Repo,
    pub main: &'a BranchName,
}

impl core::fmt::Debug for ContractSource<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ContractSource")
            .field("main", &self.main)
            .finish_non_exhaustive()
    }
}

/// Writing a contract failed; the epic is closed regardless.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// The commit range an epic's tasks covered: the earliest task base to the integration head.
#[must_use]
pub fn range(children: &[Bead], head: Sha) -> Option<(Sha, Sha)> {
    let base = children
        .iter()
        .filter(|c| c.kind == Some(BeadKind::Task))
        .find_map(|c| c.meta.as_ref().map(|m| m.base.clone()))?;
    Some((base, head))
}

/// The contract text: range, size, files, public surface, then the plan that asked for it.
#[must_use]
pub fn render(
    epic: &Bead,
    children: &[Bead],
    base: &Sha,
    head: &Sha,
    diff: &DiffSummary,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Contract: {}\n", epic.title);
    let _ = writeln!(
        out,
        "Landed on the integration branch as `{base}..{head}` — {} files, +{} −{}.\n",
        diff.files.len(),
        diff.insertions,
        diff.deletions
    );
    if !diff.files.is_empty() {
        out.push_str("## Files\n");
        for f in &diff.files {
            let _ = writeln!(out, "- `{f}`");
        }
        out.push('\n');
    }
    if !diff.added_surface.is_empty() {
        out.push_str("## Public surface added\n```\n");
        for l in &diff.added_surface {
            let _ = writeln!(out, "{l}");
        }
        out.push_str("```\n\n");
    }
    out.push_str("## Tasks\n");
    for c in children.iter().filter(|c| c.kind == Some(BeadKind::Task)) {
        let landed = c
            .meta
            .as_ref()
            .and_then(|m| match &m.state {
                domain::TaskState::Closed { merged } => Some(merged.to_string()),
                domain::TaskState::Open
                | domain::TaskState::Leased { .. }
                | domain::TaskState::InVerify { .. }
                | domain::TaskState::Mergeable { .. }
                | domain::TaskState::Incident { .. } => None,
            })
            .unwrap_or_else(|| "not landed".to_owned());
        let _ = writeln!(out, "- {} — {} ({landed})", c.id, c.title);
    }
    out.push_str("\n## Plan\n");
    out.push_str(&epic.description);
    out.push('\n');
    for r in children
        .iter()
        .filter(|c| c.kind == Some(BeadKind::Reference))
    {
        let _ = writeln!(out, "\n### Reference\n{}", r.description);
    }
    out
}

/// Write the contract bead for a just-closed epic. Created closed: it is context, not work.
///
/// # Errors
/// `Store` when the ledger refuses; `Repo` when the range cannot be summarised.
pub async fn write(
    store: &dyn BeadStore,
    source: ContractSource<'_>,
    epic: &Bead,
    children: &[Bead],
) -> Result<Option<BeadId>, ContractError> {
    let head = source.repo.head_of(source.main).await?;
    let Some((base, head)) = range(children, head) else {
        return Ok(None);
    };
    let diff = source.repo.diff_summary(&base, &head).await?;
    let id = store
        .create(NewBead {
            title: Title::derived(&format!("contract: {}", epic.title)),
            description: render(epic, children, &base, &head, &diff),
            kind: BeadKind::Contract,
            priority: Priority::LOW,
            parent: Some(epic.id.clone()),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await?;
    store.close(&id, "contract artifact, not work").await?;
    Ok(Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeRepo, FakeStore};
    use domain::{Attempts, Budget, FactoryMeta, TaskState, Usage};

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).expect("id")
    }
    fn sha(c: char) -> Sha {
        Sha::try_new(std::iter::repeat_n(c, 40).collect::<String>().as_str()).expect("sha")
    }

    #[tokio::test]
    async fn a_closed_epic_gets_a_contract_with_range_surface_tasks_and_plan() {
        let store = FakeStore::default();
        store.seed_epic(id("e-1"), &[]).await;
        store
            .seed_reference(id("e-1.0"), id("e-1"), "Use POSIX sh.")
            .await;
        store
            .seed_task(
                id("e-1.1"),
                FactoryMeta {
                    verify_bead: id("e-1.2"),
                    base: sha('a'),
                    budget: Budget::default(),
                    usage: Usage::default(),
                    lease_expiries: Attempts::new(0),
                    state: TaskState::Closed { merged: sha('b') },
                },
            )
            .await;
        store.set_parent(&id("e-1.1"), &id("e-1")).await;
        let repo = FakeRepo {
            commit_head: Some(sha('c')),
            summary: DiffSummary {
                files: vec!["src/lib.rs".into()],
                insertions: 10,
                deletions: 2,
                added_surface: vec!["pub fn balance()".into()],
            },
            ..FakeRepo::default()
        };
        let main = BranchName::try_new("main").expect("branch");
        let epic = store.show(&id("e-1")).await.expect("epic");
        let children = store.children(&id("e-1")).await.expect("children");
        let written = write(
            &store,
            ContractSource {
                repo: &repo,
                main: &main,
            },
            &epic,
            &children,
        )
        .await
        .expect("write")
        .expect("some");
        let bead = store.show(&written).await.expect("contract");
        assert_eq!(bead.kind, Some(BeadKind::Contract));
        assert_eq!(bead.status, crate::bead::BeadStatus::Closed);
        assert_eq!(bead.parent.as_ref(), Some(&id("e-1")));
        let d = &bead.description;
        assert!(
            d.contains(&format!(
                "`{}..{}`",
                sha('a'),
                repo.commit_head.clone().expect("head")
            )) || d.contains(&sha('a').to_string())
        );
        assert!(d.contains("pub fn balance()") && d.contains("src/lib.rs"));
        assert!(d.contains("e-1.1") && d.contains("Use POSIX sh."));
        // No task children → nothing to contract.
        store.seed_epic(id("e-2"), &[]).await;
        let e2 = store.show(&id("e-2")).await.expect("e2");
        assert_eq!(
            write(
                &store,
                ContractSource {
                    repo: &repo,
                    main: &main
                },
                &e2,
                &[]
            )
            .await
            .expect("ok"),
            None
        );
    }
}
