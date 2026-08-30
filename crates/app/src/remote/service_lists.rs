//! Listings beyond the live set: history (closed epics) and vanished-task catch-up.

use domain::{BeadKind, Scope};

use super::super::a2a::{A2aState, Task, epic_task};
use super::{RemoteError, Rig, authorize, get_task, list_tasks};
use crate::ports::Clock;
use domain::Principal;

/// Closed epics — the rig's history — as the ledger lists them (sort by the `epic_closed`
/// event when the log is at hand).
///
/// # Errors
/// `Unauthorized` without `watch`; store failures.
pub async fn list_history(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
) -> Result<Vec<Task>, RemoteError> {
    authorize(rig, clock, who, Scope::Watch, "ListTasks")?;
    let now = clock.now().to_string();
    let mut out = Vec::new();
    for epic in rig.store.list_closed(BeadKind::Epic).await? {
        let children = rig.store.children(&epic.id).await?;
        out.push(epic_task(&epic, &children, &now));
    }
    Ok(out)
}

/// `ListTasks` plus the tasks in `seen` that dropped out of the listing (closed epics),
/// each fetched once so a watcher observes its terminal state.
///
/// # Errors
/// As `list_tasks`.
pub async fn list_tasks_with_vanished(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    seen: &std::collections::BTreeMap<String, A2aState>,
) -> Result<Vec<Task>, RemoteError> {
    let mut tasks = list_tasks(rig, clock, who).await?;
    let listed: std::collections::BTreeSet<String> = tasks.iter().map(|t| t.id.clone()).collect();
    for id in seen
        .iter()
        .filter(|(id, state)| !listed.contains(*id) && !state.is_terminal())
        .map(|(id, _)| id)
    {
        if let Ok(t) = get_task(rig, clock, who, id).await {
            tasks.push(t);
        }
    }
    Ok(tasks)
}
