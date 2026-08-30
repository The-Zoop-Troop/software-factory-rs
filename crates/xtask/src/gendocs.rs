//! `gen-docs`: files under `docs/generated/` are derived from code and never hand-edited.
//! `--check` fails if they are stale (CI runs it), with the command that fixes it.

#![allow(clippy::format_push_string, reason = "tooling that builds markdown")]

use std::path::Path;

pub(crate) fn generate(root: &Path, check: bool) -> anyhow::Result<()> {
    let outputs = [
        ("state-machine.md", state_machine()),
        ("bead-schema.md", bead_schema()?),
        ("cli.md", cli_reference(root)?),
        ("console-api.md", console_api(root)?),
    ];
    let dir = root.join("docs/generated");
    std::fs::create_dir_all(&dir)?;
    let mut stale = Vec::new();
    for (name, content) in outputs {
        let path = dir.join(name);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != content {
            if check {
                stale.push(name);
            } else {
                std::fs::write(&path, content)?;
                println!("wrote docs/generated/{name}");
            }
        }
    }
    if check && !stale.is_empty() {
        anyhow::bail!(
            "docs/generated is stale: {stale:?}. Run `cargo xtask gen-docs` and commit the result."
        );
    }
    if check {
        println!("gen-docs: up to date");
    }
    Ok(())
}

fn header(what: &str) -> String {
    format!(
        "# {what}\n\n- **Status:** generated · **Verified:** by `cargo xtask gen-docs --check` in CI. Do not edit by hand.\n\n"
    )
}

fn state_machine() -> String {
    let mut s = header("Task state machine (generated)");
    s.push_str("Source: `crates/domain/src/task.rs`.\n\n## States\n\n");
    for st in domain::task::STATE_NAMES {
        s.push_str(&format!("- `{st}`\n"));
    }
    s.push_str("\n## Events\n\n");
    for ev in domain::task::EVENT_NAMES {
        s.push_str(&format!("- `{ev}`\n"));
    }
    s.push_str(&format!(
        "\n## Constants\n\n- Lease expiries before a lease-storm incident: `{}`\n",
        domain::task::MAX_LEASE_EXPIRIES
    ));
    let b = domain::Budget::default();
    s.push_str(&format!(
        "- Default budget: tokens `{}`, wall clock `{}s`, attempts `{}`\n",
        b.tokens,
        b.wall_clock.seconds(),
        b.attempts
    ));
    s
}

fn bead_schema() -> anyhow::Result<String> {
    let sha = domain::Sha::try_new("a".repeat(40)).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let id = |s: &str| domain::BeadId::try_new(s).map_err(|e| anyhow::anyhow!(e.to_string()));
    let task = domain::FactoryMeta {
        verify_bead: id("fac-2")?,
        base: sha.clone(),
        budget: domain::Budget::default(),
        usage: domain::Usage::default(),
        lease_expiries: domain::Attempts::new(0),
        state: domain::TaskState::Open,
    };
    let verify = domain::VerifyMeta {
        task: id("fac-1")?,
        commands: domain::NonEmpty::singleton(
            domain::VerifyCommand::try_new("cargo test")
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        ),
        timeout: domain::Duration::from_minutes(20),
    };
    let merge = domain::MergeMeta {
        task: id("fac-1")?,
        branch: domain::BranchName::try_new("task/fac-1")
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        head: sha,
    };
    let mut s = header("Bead metadata schema (generated)");
    s.push_str("Source: `crates/domain/src/meta.rs`. Each block is the JSON stored under the named key of a bead's `metadata`.\n\n");
    for (key, value) in [
        (domain::meta::META_KEY, serde_json::to_value(&task)?),
        (
            domain::meta::VERIFY_META_KEY,
            serde_json::to_value(&verify)?,
        ),
        (domain::meta::MERGE_META_KEY, serde_json::to_value(&merge)?),
    ] {
        s.push_str(&format!(
            "## `metadata.{key}`\n\n```json\n{}\n```\n\n",
            serde_json::to_string_pretty(&value)?
        ));
    }
    s.push_str("Kind labels: ");
    s.push_str(
        &[
            "epic",
            "task",
            "verify",
            "merge",
            "question",
            "incident",
            "reference",
            "plan_request",
            "contract",
        ]
        .iter()
        .map(|k| format!("`{}{k}`", domain::BeadKind::LABEL_PREFIX))
        .collect::<Vec<_>>()
        .join(", "),
    );
    s.push('\n');
    Ok(s)
}

fn cli_reference(root: &Path) -> anyhow::Result<String> {
    let help = |args: &[&str]| -> anyhow::Result<String> {
        let out = std::process::Command::new("cargo")
            .args(["run", "-q", "-p", "factory", "--"])
            .args(args)
            .arg("--help")
            .current_dir(root)
            .output()?;
        anyhow::ensure!(
            out.status.success(),
            "factory {args:?} --help failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let mut s = header("`factory` CLI reference (generated)");
    s.push_str("Source: `crates/factory/src/cli.rs` via `factory <cmd> --help`.\n\n");
    for cmd in [
        vec![],
        vec!["doctor"],
        vec!["watch"],
        vec!["inbox"],
        vec!["plan"],
        vec!["work"],
        vec!["verify"],
        vec!["integrate"],
        vec!["bead"],
        vec!["stop"],
        vec!["telegram"],
        vec!["rig"],
    ] {
        let title = if cmd.is_empty() {
            "factory".to_owned()
        } else {
            format!("factory {}", cmd.join(" "))
        };
        s.push_str(&format!("## `{title}`\n\n```text\n{}```\n\n", help(&cmd)?));
    }
    Ok(s)
}

fn console_api(root: &Path) -> anyhow::Result<String> {
    let run = |args: &[&str]| -> anyhow::Result<String> {
        let out = std::process::Command::new("cargo")
            .args(["run", "-q", "-p", "console", "--"])
            .args(args)
            .current_dir(root)
            .output()?;
        anyhow::ensure!(
            out.status.success(),
            "console {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let mut s = header("Console A2A API (generated)");
    s.push_str("Source: `crates/console` via `console card` and `console --help`. Protocol: A2A v1 JSON-RPC binding (`docs/references/a2a.md`); every request carries `Authorization: Bearer <token>`.\n\n");
    s.push_str(
        "## Agent Card (per rig, `GET /rigs/<rig>/.well-known/agent-card.json`)\n\n```json\n",
    );
    s.push_str(&run(&[
        "card",
        "--rig",
        "toy",
        "--public-url",
        "https://console.example",
    ])?);
    s.push_str("```\n\n## Operations (`POST /rigs/<rig>/a2a`)\n\n");
    s.push_str(
        "| Method | Scope | Params | Result |\n|---|---|---|---|\n\
| `SendMessage` | `plan` | `{message: {parts: [{text}]}}` | `{task}` — the new epic |\n\
| `SendMessage` | `resolve` | `{message: {taskId, parts: [{text}]}}` | `{task}` — the resolved inbox item |\n\
| `SendMessage` | `resolve` / `plan` | `{message: {taskId, parts: [{data: {option}}, {text: note}]}}` | applies an attention option: `retry_fresh`, `retry_with_guidance`, `stop_epic`, `replan`, `answer` |\n\
| `GetTask` | `watch` | `{id}` | `Task` (epic or inbox item) |\n\
| `ListTasks` | `watch` | `{status?: \"TASK_STATE_INPUT_REQUIRED\"}` | `{tasks: [Task]}` |\n\
| `CancelTask` | `plan` | `{id}` | `Task` in `TASK_STATE_CANCELED` |\n\
| `SubscribeToTask` | `watch` | `{id}` | SSE: `{task}` then `{statusUpdate}` per event, `final: true` at a terminal state |\n\n\
Errors: HTTP 401 (no/unknown token), 403 + code -32040 (missing scope), 404 + -32001 (task/rig), 400 (-32601/-32602/-32004), -32041 (rig budget cap), -32002 (terminal task).\n\n",
    );
    s.push_str("Every `INPUT_REQUIRED` message carries a `DataPart` (`Attention`): `reason {kind, summary, detail}`, `attempts`/`tokens` `{used, limit}`, `branch`, `lastVerify` (the Verifier's last failing block), `guidance` (notes already given), and `options` (id, label, description, needsNote, destructive). `GetTask` on an epic lists `metadata.factory.children` (id, title, state, attempts, tokens, branch). Other routes: `GET /whoami` (client + grants), `GET /rigs` (visible rigs + per-rig counts), `GET /rigs/<rig>/ui` + `POST …/ui/action` (A2UI), `GET /` (the operator web app).\n\nTask states: an epic is `SUBMITTED` until a task is claimed, `WORKING` while tasks move, `INPUT_REQUIRED` while any task is in incident, `COMPLETED` when closed, `CANCELED` after `CancelTask`. Incidents and questions are their own `INPUT_REQUIRED` tasks whose `contextId` is the epic.\n\n");
    s.push_str("## CLI\n\n```text\n");
    s.push_str(&run(&["--help"])?);
    s.push_str("```\n\n```text\n");
    s.push_str(&run(&["serve", "--help"])?);
    s.push_str("```\n");
    Ok(s)
}
