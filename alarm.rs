//! Core data model for a single alarm and its lifecycle state machine.
//!
//! An alarm's *lifecycle* (SCHEDULED / RINGING / COMPLETED) is tracked
//! separately from its *enabled* switch, because a repeating alarm needs to
//! fall back to SCHEDULED again on the next day it is due, while a one-shot
//! alarm should stay COMPLETED once it has fired. `last_triggered_on` is the
//! single source of truth that lets us tell "already handled today" apart
//! from "due again".

use chrono::{Datelike, NaiveDate, NaiveDateTime, Weekday as ChronoWeekday};
use serde::{Deserialize, Serialize};

/// Day of week, kept as our own enum (rather than chrono's) so the JSON
/// config file has a stable, human-readable representation that does not
/// depend on an external crate's serde implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    pub fn short(self) -> &'static str {
        match self {
            Weekday::Mon => "MON",
            Weekday::Tue => "TUE",
            Weekday::Wed => "WED",
            Weekday::Thu => "THU",
            Weekday::Fri => "FRI",
            Weekday::Sat => "SAT",
            Weekday::Sun => "SUN",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Weekday::Mon => "M",
            Weekday::Tue => "T",
            Weekday::Wed => "W",
            Weekday::Thu => "T",
            Weekday::Fri => "F",
            Weekday::Sat => "S",
            Weekday::Sun => "S",
        }
    }

    pub fn from_chrono(w: ChronoWeekday) -> Weekday {
        match w {
            ChronoWeekday::Mon => Weekday::Mon,
            ChronoWeekday::Tue => Weekday::Tue,
            ChronoWeekday::Wed => Weekday::Wed,
            ChronoWeekday::Thu => Weekday::Thu,
            ChronoWeekday::Fri => Weekday::Fri,
            ChronoWeekday::Sat => Weekday::Sat,
            ChronoWeekday::Sun => Weekday::Sun,
        }
    }
}

/// How an alarm repeats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RepeatMode {
    /// Fires once, then moves to `Completed` forever.
    Once,
    /// Fires every day.
    Daily,
    /// Fires only on the listed weekdays. An empty list behaves like `Once`
    /// (the UI prevents saving an empty custom selection, but we handle it
    /// defensively here too).
    Custom(Vec<Weekday>),
}

impl RepeatMode {
    pub fn is_due(&self, today: ChronoWeekday) -> bool {
        match self {
            RepeatMode::Once => true,
            RepeatMode::Daily => true,
            RepeatMode::Custom(days) => {
                if days.is_empty() {
                    true
                } else {
                    days.contains(&Weekday::from_chrono(today))
                }
            }
        }
    }

    pub fn is_repeating(&self) -> bool {
        !matches!(self, RepeatMode::Once)
    }

    /// Compact label used in the alarm grid, e.g. `DAILY`, `ONCE`,
    /// `MON-FRI`, or `SAT SUN`.
    pub fn label(&self) -> String {
        match self {
            RepeatMode::Once => "ONCE".to_string(),
            RepeatMode::Daily => "DAILY".to_string(),
            RepeatMode::Custom(days) => {
                if days.is_empty() {
                    return "ONCE".to_string();
                }
                let mut sorted = days.clone();
                sorted.sort_by_key(|d| Weekday::ALL.iter().position(|x| x == d).unwrap_or(0));

                // Recognise the common Mon-Fri / Sat-Sun runs for a tighter label.
                let weekdays: Vec<Weekday> = Weekday::ALL[0..5].to_vec();
                let weekend: Vec<Weekday> = Weekday::ALL[5..7].to_vec();
                if sorted == weekdays {
                    return "MON-FRI".to_string();
                }
                if sorted == weekend {
                    return "SAT-SUN".to_string();
                }
                sorted
                    .iter()
                    .map(|d| d.short())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }
}

/// Lifecycle state of an alarm, independent of the `enabled` switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlarmState {
    /// Waiting for its trigger time.
    Scheduled,
    /// Currently sounding; waiting for the user to press STOP.
    Ringing,
    /// Already fired (and was stopped) today / for good, for a one-shot alarm.
    Completed,
}

impl Default for AlarmState {
    fn default() -> Self {
        AlarmState::Scheduled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: u64,
    pub name: String,
    pub hour: u32,
    pub minute: u32,
    pub enabled: bool,
    pub sound_path: Option<String>,
    pub repeat: RepeatMode,
    #[serde(default)]
    pub state: AlarmState,
    /// The calendar date this alarm last started ringing on. Used both to
    /// stop a single trigger from firing twice inside the same minute, and
    /// to know when a `Completed` repeating alarm becomes due again.
    #[serde(default)]
    pub last_triggered_on: Option<NaiveDate>,
}

impl Alarm {
    pub fn new(id: u64, name: String, hour: u32, minute: u32) -> Self {
        Self {
            id,
            name,
            hour: hour.min(23),
            minute: minute.min(59),
            enabled: true,
            sound_path: None,
            repeat: RepeatMode::Once,
            state: AlarmState::Scheduled,
            last_triggered_on: None,
        }
    }

    pub fn time_label(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    pub fn sound_label(&self) -> String {
        match &self.sound_path {
            Some(p) => std::path::Path::new(p)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "UNKNOWN.WAV".to_string()),
            None => "SYSTEM BEEP".to_string(),
        }
    }

    /// What the effective, display-facing state is right now. This folds
    /// "enabled" into the state space and auto-recovers a repeating alarm
    /// from `Completed` back to `Scheduled` once the day has rolled over,
    /// without needing a background job to mutate stored state.
    pub fn display_state(&self, today: NaiveDate) -> AlarmState {
        if self.state == AlarmState::Ringing {
            return AlarmState::Ringing;
        }
        if self.state == AlarmState::Completed {
            let still_today = self.last_triggered_on == Some(today);
            if !still_today && self.repeat.is_repeating() {
                return AlarmState::Scheduled;
            }
            return AlarmState::Completed;
        }
        AlarmState::Scheduled
    }

    pub fn badge(&self, today: NaiveDate) -> (&'static str, BadgeColor) {
        if !self.enabled {
            return ("OFF", BadgeColor::Dim);
        }
        match self.display_state(today) {
            AlarmState::Ringing => ("RINGING", BadgeColor::Alert),
            AlarmState::Completed => ("DONE", BadgeColor::Dim),
            AlarmState::Scheduled => ("ACTIVE", BadgeColor::Ok),
        }
    }

    /// Should this alarm start ringing right now?
    pub fn should_trigger(&self, now: NaiveDateTime) -> bool {
        if !self.enabled {
            return false;
        }
        if self.state == AlarmState::Ringing {
            return false;
        }
        let today = now.date();
        if self.last_triggered_on == Some(today) {
            // Already handled (rang, or was completed) today.
            return false;
        }
        if now.time().hour_() != self.hour || now.time().minute_() != self.minute {
            return false;
        }
        self.repeat.is_due(now.date().weekday())
    }

    /// Next upcoming DateTime this alarm is due, starting the search from
    /// `from` (inclusive of `from`'s own minute). Returns `None` if the
    /// alarm is disabled, permanently completed, or has no due weekday.
    pub fn next_occurrence(&self, from: NaiveDateTime) -> Option<NaiveDateTime> {
        if !self.enabled {
            return None;
        }
        if self.state == AlarmState::Completed && !self.repeat.is_repeating() {
            return None;
        }
        for day_offset in 0..8i64 {
            let candidate_date = from.date() + chrono::Duration::days(day_offset);
            if !self.repeat.is_due(candidate_date.weekday()) {
                continue;
            }
            let candidate = candidate_date.and_hms_opt(self.hour, self.minute, 0)?;
            if candidate >= from {
                // Skip a slot that is today, already fired today, and is a
                // one-shot (it will never fire again).
                if day_offset == 0
                    && self.last_triggered_on == Some(candidate_date)
                    && !self.repeat.is_repeating()
                {
                    continue;
                }
                return Some(candidate);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeColor {
    Ok,
    Alert,
    Dim,
}

/// Small helper trait so call sites read `now.time().hour_()` consistently
/// with the rest of the codebase without importing chrono::Timelike
/// everywhere. (Kept intentionally trivial.)
trait TimelikeExt {
    fn hour_(&self) -> u32;
    fn minute_(&self) -> u32;
}

impl TimelikeExt for chrono::NaiveTime {
    fn hour_(&self) -> u32 {
        use chrono::Timelike;
        self.hour()
    }
    fn minute_(&self) -> u32 {
        use chrono::Timelike;
        self.minute()
    }
}
