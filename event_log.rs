//! A capped in-memory event log. Capped so a multi-day session can never
//! accumulate unbounded memory (brief §23: "ne pas créer des milliers de
//! logs" / no memory growth over long uptimes).

use chrono::Local;
use std::collections::VecDeque;

pub const MAX_ENTRIES: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Sys,
    Alarm,
    Audio,
    Warn,
}

impl Severity {
    pub fn tag(self) -> &'static str {
        match self {
            Severity::Sys => "SYS",
            Severity::Alarm => "ALARM",
            Severity::Audio => "AUDIO",
            Severity::Warn => "WARN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub severity: Severity,
    pub message: String,
}

pub struct EventLog {
    entries: VecDeque<LogEntry>,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_ENTRIES),
        }
    }

    pub fn push(&mut self, severity: Severity, message: impl Into<String>) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        self.entries.push_back(LogEntry {
            timestamp,
            severity,
            message: message.into(),
        });
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &LogEntry> {
        self.entries.iter()
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}
