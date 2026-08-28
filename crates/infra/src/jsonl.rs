//! Append-only JSONL event log. One `FactoryEvent` per line.
#![allow(
    clippy::disallowed_types,
    reason = "a leaf Mutex around the file handle; never held across an await"
)]

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::Path;

use app::{EventSink, FactoryEvent};

/// Appends events to a file. Opened once; each record is a single `write_all` so
/// concurrent writers from different processes interleave at line granularity.
#[derive(Debug)]
pub struct JsonlSink {
    file: std::sync::Mutex<File>,
}

impl JsonlSink {
    /// # Errors
    /// If the file cannot be opened for append.
    pub fn open(path: &Path) -> Result<Self, std::io::Error> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: std::sync::Mutex::new(file),
        })
    }
}

impl EventSink for JsonlSink {
    fn record(&self, event: &FactoryEvent) {
        let Ok(mut line) = serde_json::to_string(event) else {
            tracing::error!("event not serializable");
            return;
        };
        line.push('\n');
        let Ok(mut f) = self.file.lock() else {
            tracing::error!("event log mutex poisoned");
            return;
        };
        if let Err(e) = f.write_all(line.as_bytes()) {
            tracing::error!(error = %e, "event log write failed");
        }
    }
}
