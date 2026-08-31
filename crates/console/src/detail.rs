//! Bead detail (`GET /rigs/{rig}/beads/{id}`): everything an operator reads on a task, epic,
//! reference, contract, or plan request — including the notes parsed into structure so the UI
//! renders verify blocks and operator lines instead of a wall of text.

use serde::Serialize;

/// One command inside a verify block: `$ cmd`, `[exit N | timed out | killed]`, output tail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct VerifyLine {
    pub command: String,
    pub status: String,
    pub tail: String,
}

/// A segment of a bead's notes, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum NoteSegment {
    /// `verify PASSED` / `verify FAILED` and its per-command records.
    VerifyBlock {
        passed: bool,
        commands: Vec<VerifyLine>,
    },
    /// `guidance: …` left by an operator for the next session.
    Guidance { text: String },
    /// `released: …` (no changes, harness error, blocked).
    Released { text: String },
    /// Operator actions: `resume-from:`, `operator:`, `incident resolved by operator:`.
    Operator { text: String },
    /// Anything else.
    Plain { text: String },
}

/// Cut every segment's tails to `cap` characters (the endpoint's default view).
pub(crate) fn truncate(segments: &mut [NoteSegment], cap: usize) {
    let cut = |s: &mut String| {
        if s.chars().count() > cap {
            let tail: String = s
                .chars()
                .rev()
                .take(cap)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            *s = format!("… {tail}");
        }
    };
    for seg in segments {
        match seg {
            NoteSegment::VerifyBlock { commands, .. } => {
                for c in commands {
                    cut(&mut c.tail);
                }
            }
            NoteSegment::Guidance { text }
            | NoteSegment::Released { text }
            | NoteSegment::Operator { text }
            | NoteSegment::Plain { text } => cut(text),
        }
    }
}

/// Parse a bead's notes into ordered segments. Total: every line lands in exactly one segment.
pub(crate) fn parse_notes(notes: &str) -> Vec<NoteSegment> {
    let mut out: Vec<NoteSegment> = Vec::new();
    let mut lines = notes.lines().peekable();
    let flush_plain = |out: &mut Vec<NoteSegment>, buf: &mut Vec<String>| {
        let text = buf.join("\n").trim().to_owned();
        if !text.is_empty() {
            out.push(NoteSegment::Plain { text });
        }
        buf.clear();
    };
    let mut plain: Vec<String> = Vec::new();
    while let Some(line) = lines.next() {
        if let Some(rest) = line
            .strip_prefix("verify PASSED")
            .or_else(|| line.strip_prefix("verify FAILED"))
        {
            let _ = rest;
            flush_plain(&mut out, &mut plain);
            let passed = line.starts_with("verify PASSED");
            let mut commands = Vec::new();
            // Command records follow as `$ cmd` / `[status]` / tail lines until the next
            // top-level marker or the end.
            while let Some(&next) = lines.peek() {
                if let Some(cmd) = next.strip_prefix("$ ") {
                    lines.next();
                    let status = lines
                        .peek()
                        .and_then(|l| l.strip_prefix('[').and_then(|l| l.strip_suffix(']')))
                        .map(str::to_owned);
                    if status.is_some() {
                        lines.next();
                    }
                    let mut tail = Vec::new();
                    while let Some(&t) = lines.peek() {
                        if t.starts_with("$ ") || is_marker(t) {
                            break;
                        }
                        tail.push(t.to_owned());
                        lines.next();
                    }
                    commands.push(VerifyLine {
                        command: cmd.to_owned(),
                        status: status.unwrap_or_default(),
                        tail: tail.join("\n").trim().to_owned(),
                    });
                } else if next.trim().is_empty() {
                    lines.next();
                } else {
                    break;
                }
            }
            out.push(NoteSegment::VerifyBlock { passed, commands });
        } else if let Some(text) = line.strip_prefix("guidance: ") {
            flush_plain(&mut out, &mut plain);
            out.push(NoteSegment::Guidance {
                text: text.to_owned(),
            });
        } else if let Some(text) = line.strip_prefix("released: ") {
            flush_plain(&mut out, &mut plain);
            out.push(NoteSegment::Released {
                text: text.to_owned(),
            });
        } else if line.starts_with("resume-from: ")
            || line.starts_with("operator: ")
            || line.starts_with("incident resolved by operator:")
        {
            flush_plain(&mut out, &mut plain);
            out.push(NoteSegment::Operator {
                text: line.to_owned(),
            });
        } else {
            plain.push(line.to_owned());
        }
    }
    flush_plain(&mut out, &mut plain);
    out
}

fn is_marker(line: &str) -> bool {
    line.starts_with("verify PASSED")
        || line.starts_with("verify FAILED")
        || line.starts_with("guidance: ")
        || line.starts_with("released: ")
        || line.starts_with("resume-from: ")
        || line.starts_with("operator: ")
        || line.starts_with("incident resolved by operator:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_runs_real_notes_parse_without_loss() {
        let notes = include_str!("../fixtures/task-notes.txt");
        let segs = parse_notes(notes);
        assert!(segs.len() >= 5, "{}", segs.len());
        let fails = segs
            .iter()
            .filter(|s| matches!(s, NoteSegment::VerifyBlock { passed: false, .. }))
            .count();
        assert!(fails >= 2, "fails {fails}");
        assert!(
            segs.iter()
                .any(|s| matches!(s, NoteSegment::Operator { .. }))
        );
        // Nothing substantial is dropped: the segments carry most of the input's characters
        // (Debug escaping makes exact substring checks unreliable).
        let kept: usize = segs
            .iter()
            .map(|seg| match seg {
                NoteSegment::VerifyBlock { commands, .. } => commands
                    .iter()
                    .map(|c| c.command.len() + c.status.len() + c.tail.len())
                    .sum(),
                NoteSegment::Guidance { text }
                | NoteSegment::Released { text }
                | NoteSegment::Operator { text }
                | NoteSegment::Plain { text } => text.len(),
            })
            .sum();
        assert!(kept * 2 >= notes.len(), "kept {kept} of {}", notes.len());
    }

    #[test]
    fn a_real_task_biography_parses_into_ordered_segments() {
        let notes = "verify FAILED\n$ test -n \"$DATABASE_URL\"\n[exit 0]\n\n$ cargo test --test x\n[exit 101]\n--- stderr ---\nerror: manifest path does not exist\n\nlease held by worker-1 expired at 17\nguidance: use the repo root\nverify PASSED\n$ cargo test\n[exit 0]\nok. 12 passed\nresume-from: task/x-1\nincident resolved by operator: retry\nreleased: blocked: need the client id";
        let segs = parse_notes(notes);
        let kinds: Vec<&str> = segs
            .iter()
            .map(|s| match s {
                NoteSegment::VerifyBlock { passed: true, .. } => "pass",
                NoteSegment::VerifyBlock { passed: false, .. } => "fail",
                NoteSegment::Guidance { .. } => "guidance",
                NoteSegment::Released { .. } => "released",
                NoteSegment::Operator { .. } => "operator",
                NoteSegment::Plain { .. } => "plain",
            })
            .collect();
        // Free text between blocks attaches to the last command's tail (the lease line here).
        assert_eq!(
            kinds,
            [
                "fail", "guidance", "pass", "operator", "operator", "released"
            ]
        );
        let commands = match &segs[0] {
            NoteSegment::VerifyBlock { commands, .. } => commands.clone(),
            NoteSegment::Guidance { .. }
            | NoteSegment::Released { .. }
            | NoteSegment::Operator { .. }
            | NoteSegment::Plain { .. } => Vec::new(),
        };
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[1].status, "exit 101");
        assert!(commands[1].tail.contains("lease held by worker-1"));
        assert!(commands[1].tail.contains("manifest path"));
        let mut segs = segs;
        truncate(&mut segs, 8);
        let truncated = match &segs[0] {
            NoteSegment::VerifyBlock { commands, .. } => commands.clone(),
            NoteSegment::Guidance { .. }
            | NoteSegment::Released { .. }
            | NoteSegment::Operator { .. }
            | NoteSegment::Plain { .. } => Vec::new(),
        };
        assert!(truncated[1].tail.starts_with("… "));
        assert!(parse_notes("").is_empty());
    }
}
