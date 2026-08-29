//! A2UI (`docs/references/a2ui.md`): the web console as declarative UI *intent*. The same
//! read models that feed A2A tasks become a flat list of basic-catalog components; any A2UI
//! renderer (ours, or an agent's) can show and drive them. No executable code crosses the wire.

use serde_json::{Map, Value};

use super::a2a::{A2aState, Task};

pub const VERSION: &str = "v0.9.1";
pub const CATALOG: &str = "https://a2ui.org/catalogs/basic/v0.9.1";
pub const SURFACE: &str = "console";
/// The A2A extension URI advertised on the Agent Card.
pub const EXTENSION: &str = "https://a2ui.org/a2a-extension/a2ui/v0.9.1";

fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<Map<_, _>>(),
    )
}

fn envelope(key: &str, body: Value) -> Value {
    obj([("version", VERSION.into()), (key, body)])
}

fn text(id: &str, s: &str) -> Value {
    obj([
        ("id", id.into()),
        ("component", "Text".into()),
        ("text", s.into()),
    ])
}

fn button(id: &str, label: &str, action: &str, context: Value) -> Value {
    obj([
        ("id", id.into()),
        ("component", "Button".into()),
        ("label", label.into()),
        (
            "action",
            obj([(
                "event",
                obj([("name", action.into()), ("context", context)]),
            )]),
        ),
    ])
}

fn field(id: &str, label: &str, path: &str) -> Value {
    obj([
        ("id", id.into()),
        ("component", "TextField".into()),
        ("label", label.into()),
        ("value", obj([("path", path.into())])),
    ])
}

fn container(id: &str, kind: &str, children: &[&str]) -> Value {
    obj([
        ("id", id.into()),
        ("component", kind.into()),
        (
            "children",
            Value::Array(
                children
                    .iter()
                    .map(|c| Value::String((*c).to_owned()))
                    .collect(),
            ),
        ),
    ])
}

fn factory_str<'a>(t: &'a Task, key: &str) -> &'a str {
    t.metadata
        .get("factory")
        .and_then(|f| f.get(key))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn factory_num(t: &Task, key: &str) -> u64 {
    t.metadata
        .get("factory")
        .and_then(|f| f.get(key))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// The whole console for one rig: `createSurface`, `updateComponents`, `updateDataModel`.
/// Idempotent — sending it again replaces the surface's contents.
#[must_use]
pub fn console_surface(rig: &str, tasks: &[Task]) -> Vec<Value> {
    let mut components = Vec::new();
    let mut data = Map::new();
    let mut root_children: Vec<String> = Vec::new();

    components.push(text("title", &format!("rig {rig}")));
    root_children.push("title".into());

    plan_editor(&mut components, &mut data, &mut root_children);
    epic_cards(tasks, &mut components, &mut root_children);
    inbox_cards(tasks, &mut components, &mut data, &mut root_children);

    if root_children.len() == 2 {
        components.push(text("empty", "nothing in flight"));
        root_children.push("empty".into());
    }
    components.push(button("refresh", "Refresh", "refresh", obj([])));
    root_children.push("refresh".into());
    let root_refs: Vec<&str> = root_children.iter().map(String::as_str).collect();
    components.push(container("root", "Column", &root_refs));

    vec![
        envelope(
            "createSurface",
            obj([
                ("surfaceId", SURFACE.into()),
                ("catalogId", CATALOG.into()),
                ("sendDataModel", Value::Bool(false)),
            ]),
        ),
        envelope(
            "updateComponents",
            obj([
                ("surfaceId", SURFACE.into()),
                ("components", Value::Array(components)),
            ]),
        ),
        envelope(
            "updateDataModel",
            obj([
                ("surfaceId", SURFACE.into()),
                ("path", "/".into()),
                ("value", Value::Object(data)),
            ]),
        ),
    ]
}

fn plan_editor(
    components: &mut Vec<Value>,
    data: &mut Map<String, Value>,
    root_children: &mut Vec<String>,
) {
    // Plan editor.
    components.push(field("plan_text", "Plan (what to build)", "/plan/text"));
    components.push(button(
        "plan_go",
        "Plan",
        "plan",
        obj([("text", obj([("path", "/plan/text".into())]))]),
    ));
    components.push(container("plan_row", "Row", &["plan_text", "plan_go"]));
    components.push(container("plan_card", "Card", &["plan_row"]));
    root_children.push("plan_card".into());
    data.insert("plan".into(), obj([("text", "".into())]));
}

fn epic_cards(tasks: &[Task], components: &mut Vec<Value>, root_children: &mut Vec<String>) {
    // Epics.
    for t in tasks.iter().filter(|t| factory_str(t, "kind") == "epic") {
        let id = &t.id;
        let label = format!(
            "{id}  {}  [{}/{}] {}",
            factory_str(t, "title"),
            factory_num(t, "closed"),
            factory_num(t, "tasks"),
            state_label(t.status.state)
        );
        components.push(text(&format!("{id}_text"), &label));
        let mut kids = vec![format!("{id}_text")];
        if !t.status.state.is_terminal() {
            components.push(button(
                &format!("{id}_stop"),
                "Stop",
                "stop",
                obj([("id", id.as_str().into())]),
            ));
            kids.push(format!("{id}_stop"));
        }
        let kid_refs: Vec<&str> = kids.iter().map(String::as_str).collect();
        components.push(container(&format!("{id}_row"), "Row", &kid_refs));
        components.push(container(
            &format!("{id}_card"),
            "Card",
            &[&format!("{id}_row")],
        ));
        root_children.push(format!("{id}_card"));
    }
}

fn inbox_cards(
    tasks: &[Task],
    components: &mut Vec<Value>,
    data: &mut Map<String, Value>,
    root_children: &mut Vec<String>,
) {
    let mut notes = Map::new();
    // Inbox.
    for t in tasks
        .iter()
        .filter(|t| factory_str(t, "kind") != "epic" && t.status.state == A2aState::InputRequired)
    {
        let id = &t.id;
        let question = t
            .status
            .message
            .as_ref()
            .map(super::a2a::Message::text)
            .unwrap_or_default();
        components.push(text(
            &format!("{id}_text"),
            &format!("{id}  [{}] {question}", factory_str(t, "kind")),
        ));
        let path = format!("/notes/{id}");
        components.push(field(&format!("{id}_note"), "Answer / resolution", &path));
        components.push(button(
            &format!("{id}_resolve"),
            "Resolve",
            "resolve",
            obj([
                ("id", id.as_str().into()),
                ("note", obj([("path", path.as_str().into())])),
            ]),
        ));
        components.push(container(
            &format!("{id}_col"),
            "Column",
            &[
                &format!("{id}_text"),
                &format!("{id}_note"),
                &format!("{id}_resolve"),
            ],
        ));
        components.push(container(
            &format!("{id}_card"),
            "Card",
            &[&format!("{id}_col")],
        ));
        root_children.push(format!("{id}_card"));
        notes.insert(id.clone(), "".into());
    }
    data.insert("notes".into(), Value::Object(notes));
}

fn state_label(s: A2aState) -> &'static str {
    match s {
        A2aState::Submitted => "queued",
        A2aState::Working => "working",
        A2aState::InputRequired => "needs you",
        A2aState::Completed => "done",
        A2aState::Failed => "failed",
        A2aState::Canceled => "canceled",
        A2aState::Rejected => "rejected",
    }
}

/// What a client `action` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    Plan { text: String },
    Resolve { id: String, note: String },
    Stop { id: String },
    Refresh,
}

/// An `action` message that does not map to anything.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UiActionError {
    #[error("unknown action `{name}`")]
    Unknown { name: String },
    #[error("action `{name}` needs `{field}` in its context")]
    Missing { name: String, field: &'static str },
}

fn ctx_str<'a>(
    name: &str,
    context: &'a Value,
    field: &'static str,
) -> Result<&'a str, UiActionError> {
    context
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| UiActionError::Missing {
            name: name.to_owned(),
            field,
        })
}

/// Decode `{name, context}` (paths already resolved by the client).
///
/// # Errors
/// `Unknown` names; `Missing` context fields.
pub fn parse_action(name: &str, context: &Value) -> Result<UiAction, UiActionError> {
    match name {
        "plan" => Ok(UiAction::Plan {
            text: ctx_str(name, context, "text")?.to_owned(),
        }),
        "resolve" => Ok(UiAction::Resolve {
            id: ctx_str(name, context, "id")?.to_owned(),
            note: ctx_str(name, context, "note")?.to_owned(),
        }),
        "stop" => Ok(UiAction::Stop {
            id: ctx_str(name, context, "id")?.to_owned(),
        }),
        "refresh" => Ok(UiAction::Refresh),
        other => Err(UiActionError::Unknown {
            name: other.to_owned(),
        }), // fp-allow: action names are free text from the client
    }
}

#[cfg(test)]
mod tests {
    use super::super::a2a::{Message, Part, TaskStatus};
    use super::*;

    fn task(id: &str, kind: &str, state: A2aState) -> Task {
        Task {
            id: id.into(),
            context_id: id.into(),
            status: TaskStatus {
                state,
                message: Some(Message {
                    message_id: "m".into(),
                    role: "ROLE_AGENT".into(),
                    parts: vec![Part::Text("why?".into())],
                    task_id: None,
                    context_id: None,
                }),
                timestamp: "t".into(),
            },
            metadata: obj([(
                "factory",
                obj([
                    ("kind", kind.into()),
                    ("title", format!("T {id}").into()),
                    ("closed", 1.into()),
                    ("tasks", 3.into()),
                ]),
            )]),
        }
    }

    fn component_ids(env: &Value) -> Vec<String> {
        env["updateComponents"]["components"]
            .as_array()
            .expect("components")
            .iter()
            .map(|c| c["id"].as_str().expect("id").to_owned())
            .collect()
    }

    #[test]
    fn surface_lists_epics_inbox_and_editor() {
        let tasks = vec![
            task("ep-1", "epic", A2aState::Working),
            task("ep-0", "epic", A2aState::Completed),
            task("inc-1", "incident", A2aState::InputRequired),
            task("q-2", "question", A2aState::Completed),
        ];
        let env = console_surface("toy", &tasks);
        assert_eq!(env.len(), 3);
        assert_eq!(env[0]["createSurface"]["surfaceId"], SURFACE);
        let ids = component_ids(&env[1]);
        for want in [
            "root",
            "plan_text",
            "plan_go",
            "ep-1_stop",
            "ep-0_card",
            "inc-1_resolve",
        ] {
            assert!(ids.contains(&want.to_owned()), "missing {want}");
        }
        assert!(
            !ids.contains(&"ep-0_stop".to_owned()),
            "terminal epics get no stop button"
        );
        assert!(!ids.contains(&"q-2_card".to_owned()));
        let comps = env[1]["updateComponents"]["components"]
            .as_array()
            .expect("c");
        let stop = comps.iter().find(|c| c["id"] == "ep-1_stop").expect("stop");
        assert_eq!(stop["action"]["event"]["context"]["id"], "ep-1");
        let root = comps.iter().find(|c| c["id"] == "root").expect("root");
        assert_eq!(root["component"], "Column");
        assert_eq!(env[2]["updateDataModel"]["value"]["notes"]["inc-1"], "");
        assert_eq!(env[2]["updateDataModel"]["value"]["plan"]["text"], "");
        let text = comps.iter().find(|c| c["id"] == "ep-1_text").expect("t");
        assert_eq!(text["text"], "ep-1  T ep-1  [1/3] working");
        let empty = console_surface("toy", &[]);
        assert!(component_ids(&empty[1]).contains(&"empty".to_owned()));
        for s in [
            A2aState::Submitted,
            A2aState::InputRequired,
            A2aState::Failed,
            A2aState::Canceled,
            A2aState::Rejected,
        ] {
            assert!(!state_label(s).is_empty());
        }
    }

    #[test]
    fn actions_decode_or_explain() {
        assert_eq!(
            parse_action("plan", &obj([("text", "build".into())])),
            Ok(UiAction::Plan {
                text: "build".into()
            })
        );
        assert_eq!(
            parse_action(
                "resolve",
                &obj([("id", "inc-1".into()), ("note", "ok".into())])
            ),
            Ok(UiAction::Resolve {
                id: "inc-1".into(),
                note: "ok".into()
            })
        );
        assert_eq!(
            parse_action("stop", &obj([("id", "ep-1".into())])),
            Ok(UiAction::Stop { id: "ep-1".into() })
        );
        assert_eq!(parse_action("refresh", &Value::Null), Ok(UiAction::Refresh));
        assert_eq!(
            parse_action("plan", &obj([("text", "  ".into())])),
            Err(UiActionError::Missing {
                name: "plan".into(),
                field: "text"
            })
        );
        assert_eq!(
            parse_action("dance", &Value::Null),
            Err(UiActionError::Unknown {
                name: "dance".into()
            })
        );
        assert!(
            parse_action("resolve", &obj([("id", "x".into())]))
                .unwrap_err()
                .to_string()
                .contains("note")
        );
    }
}
