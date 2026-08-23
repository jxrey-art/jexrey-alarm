//! The scheduler is deliberately *not* a background OS thread. `eframe`
//! already guarantees `App::update` gets called on a steady cadence as long
//! as the app keeps requesting a repaint (see `app.rs`, which always calls
//! `ctx.request_repaint_after(..)`), so driving the alarm check from the
//! main update loop keeps the whole alarm state machine single-threaded —
//! no `Arc<Mutex<Vec<Alarm>>>`, no risk of a race between the UI editing an
//! alarm and a background thread firing it at the same instant.
//!
//! The only real cost of a naive "check every frame" approach would be
//! doing the same work dozens of times per rendered second, so `tick`
//! throttles itself to at most once per wall-clock second.

use crate::alarm::{Alarm, AlarmState};
use crate::audio::{AudioEngine, PlaybackKind};
use chrono::NaiveDateTime;

pub enum SchedulerEvent {
    Triggered {
        alarm_id: u64,
        name: String,
        playback: Result<PlaybackKind, String>,
    },
}

pub struct Scheduler {
    last_checked_second: Option<NaiveDateTime>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            last_checked_second: None,
        }
    }

    /// Advance the scheduler to `now`. Mutates any alarms that just became
    /// due (flips them to `Ringing`, starts their audio loop) and returns
    /// the list of events that happened, for the event log / UI to react to.
    pub fn tick(
        &mut self,
        alarms: &mut [Alarm],
        audio: &mut AudioEngine,
        now: NaiveDateTime,
    ) -> Vec<SchedulerEvent> {
        let mut events = Vec::new();

        if self.last_checked_second == Some(now) {
            return events;
        }
        self.last_checked_second = Some(now);

        for alarm in alarms.iter_mut() {
            if alarm.should_trigger(now) {
                alarm.state = AlarmState::Ringing;
                alarm.last_triggered_on = Some(now.date());
                let playback = audio.play_loop(alarm.id, alarm.sound_path.as_deref());
                events.push(SchedulerEvent::Triggered {
                    alarm_id: alarm.id,
                    name: alarm.name.clone(),
                    playback,
                });
            }
        }

        events
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Soonest upcoming trigger across every alarm, used for the top bar's
/// `NEXT ALARM`, the radar, and the Active Operations panel.
pub fn next_alarm(alarms: &[Alarm], from: NaiveDateTime) -> Option<(&Alarm, NaiveDateTime)> {
    alarms
        .iter()
        .filter_map(|a| a.next_occurrence(from).map(|dt| (a, dt)))
        .min_by_key(|(_, dt)| *dt)
}
