//! Application state and the full dashboard layout: top status bar, alarm
//! grid + creation panel, temporal radar, local system monitor, system
//! telemetry, active operations, event log, and the full-screen alarm mode
//! that takes over when something is ringing.

use crate::alarm::{Alarm, AlarmState, RepeatMode, Weekday};
use crate::audio::{AudioEngine, PlaybackKind};
use crate::config::{self, ConfigFile};
use crate::event_log::{EventLog, Severity};
use crate::radar;
use crate::scheduler::{self, Scheduler, SchedulerEvent};
use crate::system_info::SystemMonitor;
use crate::theme::Palette;
use crate::widgets;
use chrono::{Local, NaiveDate, Timelike};
use eframe::egui;
use egui::{vec2, Align2, Color32, FontFamily, FontId, RichText, Stroke};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatKind {
    Once,
    Daily,
    Custom,
}

struct AlarmForm {
    editing_id: Option<u64>,
    name: String,
    hour: u32,
    minute: u32,
    sound_path: Option<String>,
    enabled: bool,
    repeat_kind: RepeatKind,
    custom_days: [bool; 7], // Mon..Sun, matches alarm::Weekday::ALL order
}

impl AlarmForm {
    fn blank() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            hour: 7,
            minute: 0,
            sound_path: None,
            enabled: true,
            repeat_kind: RepeatKind::Once,
            custom_days: [false; 7],
        }
    }

    fn from_alarm(alarm: &Alarm) -> Self {
        let (repeat_kind, custom_days) = match &alarm.repeat {
            RepeatMode::Once => (RepeatKind::Once, [false; 7]),
            RepeatMode::Daily => (RepeatKind::Daily, [false; 7]),
            RepeatMode::Custom(days) => {
                let mut flags = [false; 7];
                for (i, wd) in Weekday::ALL.iter().enumerate() {
                    if days.contains(wd) {
                        flags[i] = true;
                    }
                }
                (RepeatKind::Custom, flags)
            }
        };
        Self {
            editing_id: Some(alarm.id),
            name: alarm.name.clone(),
            hour: alarm.hour,
            minute: alarm.minute,
            sound_path: alarm.sound_path.clone(),
            enabled: alarm.enabled,
            repeat_kind,
            custom_days,
        }
    }

    fn build_repeat(&self) -> RepeatMode {
        match self.repeat_kind {
            RepeatKind::Once => RepeatMode::Once,
            RepeatKind::Daily => RepeatMode::Daily,
            RepeatKind::Custom => {
                let days: Vec<Weekday> = Weekday::ALL
                    .iter()
                    .zip(self.custom_days.iter())
                    .filter(|(_, on)| **on)
                    .map(|(d, _)| *d)
                    .collect();
                if days.is_empty() {
                    RepeatMode::Once
                } else {
                    RepeatMode::Custom(days)
                }
            }
        }
    }
}

pub struct JexreyApp {
    alarms: Vec<Alarm>,
    next_id: u64,
    scheduler: Scheduler,
    audio: AudioEngine,
    log: EventLog,
    sys_monitor: SystemMonitor,
    start_instant: Instant,

    show_alarm_form: bool,
    form: AlarmForm,
    pending_delete: Option<u64>,

    alarm_mode_active: bool,
    launch_focus_sent: bool,
}

impl JexreyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&_cc.egui_ctx);

        let cfg = config::load();
        let mut log = EventLog::new();
        log.push(Severity::Sys, "JEXREY ALARM CONTROL SYSTEM initialized");
        log.push(
            Severity::Sys,
            format!("Configuration loaded ({} alarm(s))", cfg.alarms.len()),
        );

        let audio = AudioEngine::new();
        if audio.is_available() {
            log.push(Severity::Audio, "Audio engine ready");
        } else {
            log.push(
                Severity::Warn,
                "No audio output device detected — alarms will still trigger visually",
            );
        }
        log.push(Severity::Sys, "Scheduler online — monitoring active");

        let next_id = cfg.next_id.max(
            cfg.alarms
                .iter()
                .map(|a| a.id + 1)
                .max()
                .unwrap_or(1),
        );

        Self {
            alarms: cfg.alarms,
            next_id,
            scheduler: Scheduler::new(),
            audio,
            log,
            sys_monitor: SystemMonitor::spawn(),
            start_instant: Instant::now(),
            show_alarm_form: false,
            form: AlarmForm::blank(),
            pending_delete: None,
            alarm_mode_active: false,
            launch_focus_sent: false,
        }
    }

    fn persist(&mut self) {
        let cfg = ConfigFile {
            version: 1,
            next_id: self.next_id,
            alarms: self.alarms.clone(),
        };
        if let Err(e) = config::save(&cfg) {
            self.log
                .push(Severity::Warn, format!("Failed to save configuration: {e}"));
        }
    }

    fn open_new_alarm_form(&mut self) {
        self.form = AlarmForm::blank();
        self.show_alarm_form = true;
    }

    fn open_edit_alarm_form(&mut self, id: u64) {
        if let Some(alarm) = self.alarms.iter().find(|a| a.id == id) {
            self.form = AlarmForm::from_alarm(alarm);
            self.show_alarm_form = true;
        }
    }

    fn save_form(&mut self) {
        let name = if self.form.name.trim().is_empty() {
            "ALARM".to_string()
        } else {
            self.form.name.trim().to_uppercase()
        };
        let repeat = self.form.build_repeat();

        if let Some(id) = self.form.editing_id {
            if let Some(alarm) = self.alarms.iter_mut().find(|a| a.id == id) {
                alarm.name = name.clone();
                alarm.hour = self.form.hour.min(23);
                alarm.minute = self.form.minute.min(59);
                alarm.sound_path = self.form.sound_path.clone();
                alarm.enabled = self.form.enabled;
                alarm.repeat = repeat;
                self.log
                    .push(Severity::Alarm, format!("{name} reconfigured"));
            }
        } else {
            let id = self.next_id;
            self.next_id += 1;
            let mut alarm = Alarm::new(id, name.clone(), self.form.hour, self.form.minute);
            alarm.sound_path = self.form.sound_path.clone();
            alarm.enabled = self.form.enabled;
            alarm.repeat = repeat;
            self.log.push(
                Severity::Alarm,
                format!("{} scheduled for {:02}:{:02}", name, alarm.hour, alarm.minute),
            );
            self.alarms.push(alarm);
        }

        self.show_alarm_form = false;
        self.persist();
    }

    fn duplicate_alarm(&mut self, id: u64) {
        if let Some(source) = self.alarms.iter().find(|a| a.id == id).cloned() {
            let new_id = self.next_id;
            self.next_id += 1;
            let mut copy = source;
            copy.id = new_id;
            copy.name = format!("{} (COPY)", copy.name);
            copy.state = AlarmState::Scheduled;
            copy.last_triggered_on = None;
            self.log
                .push(Severity::Alarm, format!("{} duplicated", copy.name));
            self.alarms.push(copy);
            self.persist();
        }
    }

    fn delete_alarm(&mut self, id: u64) {
        if let Some(pos) = self.alarms.iter().position(|a| a.id == id) {
            let alarm = self.alarms.remove(pos);
            self.audio.stop(id);
            self.log
                .push(Severity::Alarm, format!("{} deleted", alarm.name));
            self.persist();
        }
        self.pending_delete = None;
    }

    fn toggle_enabled(&mut self, id: u64) {
        if let Some(alarm) = self.alarms.iter_mut().find(|a| a.id == id) {
            alarm.enabled = !alarm.enabled;
            let state = if alarm.enabled { "ENABLED" } else { "DISABLED" };
            self.log
                .push(Severity::Alarm, format!("{} {}", alarm.name, state));
        }
        self.persist();
    }

    fn stop_alarm(&mut self, id: u64) {
        self.audio.stop(id);
        if let Some(alarm) = self.alarms.iter_mut().find(|a| a.id == id) {
            alarm.state = AlarmState::Completed;
            self.log
                .push(Severity::Alarm, format!("{} STOPPED BY USER", alarm.name));
        }
        self.persist();
    }

    fn stop_all_ringing(&mut self) {
        let ringing_ids: Vec<u64> = self
            .alarms
            .iter()
            .filter(|a| a.state == AlarmState::Ringing)
            .map(|a| a.id)
            .collect();
        for id in ringing_ids {
            self.stop_alarm(id);
        }
        self.audio.stop_all();
    }

    fn process_scheduler_events(&mut self, events: Vec<SchedulerEvent>) {
        for event in events {
            match event {
                SchedulerEvent::Triggered {
                    alarm_id,
                    name,
                    playback,
                } => {
                    self.log.push(
                        Severity::Alarm,
                        format!("ALM-{alarm_id:04} {name} TRIGGERED"),
                    );
                    match playback {
                        Ok(PlaybackKind::File(path)) => {
                            let file_name = std::path::Path::new(&path)
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or(path);
                            self.log.push(
                                Severity::Audio,
                                format!("LOOP PLAYBACK STARTED ({file_name})"),
                            );
                        }
                        Ok(PlaybackKind::FallbackBeep(reason)) => {
                            self.log.push(
                                Severity::Warn,
                                format!("Falling back to system beep — {reason}"),
                            );
                            self.log
                                .push(Severity::Audio, "LOOP PLAYBACK STARTED (fallback beep)");
                        }
                        Err(reason) => {
                            self.log.push(
                                Severity::Warn,
                                format!("Audio playback failed — {reason}"),
                            );
                        }
                    }
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let wants_kb = ctx.wants_keyboard_input();
        let any_ringing = self.alarms.iter().any(|a| a.state == AlarmState::Ringing);

        ctx.input(|i| {
            if any_ringing
                && (i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Space))
            {
                // Handled after input() borrow ends, see below.
            }
        });

        let stop_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Space)
        });
        if any_ringing && stop_pressed {
            self.stop_all_ringing();
        }

        let new_pressed = ctx.input(|i| i.key_pressed(egui::Key::N));
        if new_pressed && !wants_kb && !self.show_alarm_form && !any_ringing {
            self.open_new_alarm_form();
        }
    }
}

impl eframe::App for JexreyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keep ticking even when idle/minimized, without burning CPU: a
        // half-second cadence is far tighter than the one-minute
        // granularity alarms actually need.
        ctx.request_repaint_after(std::time::Duration::from_millis(66));

        // Truncate to whole seconds: the scheduler throttles itself by
        // comparing consecutive timestamps for equality, which would never
        // match on sub-second (nanosecond) precision.
        let raw_now = Local::now().naive_local();
        let now_local = raw_now.with_nanosecond(0).unwrap_or(raw_now);
        let today: NaiveDate = now_local.date();

        let events = self
            .scheduler
            .tick(&mut self.alarms, &mut self.audio, now_local);
        self.process_scheduler_events(events);

        self.handle_shortcuts(ctx);

        let any_ringing = self.alarms.iter().any(|a| a.state == AlarmState::Ringing);
        if any_ringing != self.alarm_mode_active {
            self.alarm_mode_active = any_ringing;
            if any_ringing {
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::AlwaysOnTop,
                ));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::Normal,
                ));
            }
        }
        if !self.launch_focus_sent {
            self.launch_focus_sent = true;
        }

        let elapsed_secs = self.start_instant.elapsed().as_secs_f32();

        self.draw_top_bar(ctx, now_local, today, any_ringing);
        self.draw_right_column(ctx, now_local, elapsed_secs);
        self.draw_central(ctx, today, any_ringing);
        self.draw_alarm_form_window(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
    }
}

impl JexreyApp {
    fn draw_top_bar(
        &mut self,
        ctx: &egui::Context,
        now_local: chrono::NaiveDateTime,
        today: NaiveDate,
        any_ringing: bool,
    ) {
        let alarm_count = self.alarms.len();
        let active_count = self.alarms.iter().filter(|a| a.enabled).count();
        let next = scheduler::next_alarm(&self.alarms, now_local);

        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::none()
                    .fill(if any_ringing {
                        Color32::from_rgb(24, 8, 8)
                    } else {
                        Palette::PANEL
                    })
                    .stroke(Stroke::new(1.0, Palette::BORDER_BRIGHT))
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Palette::CYAN,
                        RichText::new("\u{25C6} JEXREY ALARM SYSTEM")
                            .font(FontId::new(15.0, FontFamily::Monospace))
                            .strong(),
                    );

                    ui.add_space(18.0);
                    widgets::dot(
                        ui,
                        if any_ringing { Palette::RED } else { Palette::GREEN },
                        8.0,
                    );
                    ui.colored_label(
                        if any_ringing { Palette::RED } else { Palette::GREEN },
                        if any_ringing { "ALARM ACTIVE" } else { "ONLINE" },
                    );

                    ui.add_space(18.0);
                    ui.colored_label(Palette::TEXT_DIM, "SYSTEM TIME");
                    ui.colored_label(
                        Palette::TEXT,
                        now_local.format("%H:%M:%S").to_string(),
                    );

                    ui.add_space(18.0);
                    ui.colored_label(Palette::TEXT_DIM, "NEXT ALARM");
                    ui.colored_label(
                        Palette::CYAN,
                        match next {
                            Some((a, _)) => a.time_label(),
                            None => "----".to_string(),
                        },
                    );

                    ui.add_space(18.0);
                    ui.colored_label(Palette::TEXT_DIM, "ALARMS");
                    ui.colored_label(Palette::TEXT, format!("{active_count:02}/{alarm_count:02}"));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::badge(
                            ui,
                            if any_ringing { "ALERT" } else { "OPERATIONAL" },
                            if any_ringing { Palette::RED } else { Palette::GREEN },
                        );
                        ui.colored_label(Palette::TEXT_DIM, "STATUS");
                        let _ = today;
                    });
                });
            });
    }

    fn draw_right_column(
        &mut self,
        ctx: &egui::Context,
        now_local: chrono::NaiveDateTime,
        elapsed_secs: f32,
    ) {
        egui::SidePanel::right("right_column")
            .resizable(true)
            .default_width(340.0)
            .width_range(280.0..=460.0)
            .frame(
                egui::Frame::none()
                    .fill(Palette::VOID)
                    .inner_margin(egui::Margin::same(10.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        widgets::section(ui, "TEMPORAL RADAR", |ui| {
                            radar::draw(ui, &self.alarms, now_local, elapsed_secs);
                        });
                        ui.add_space(8.0);
                        self.draw_local_system(ui, now_local);
                        ui.add_space(8.0);
                        self.draw_telemetry(ui);
                        ui.add_space(8.0);
                        self.draw_active_operations(ui, now_local);
                    });
            });
    }

    fn draw_local_system(&self, ui: &mut egui::Ui, now_local: chrono::NaiveDateTime) {
        let snap = self.sys_monitor.latest();
        let uptime = self.start_instant.elapsed();
        let uptime_str = format_duration(uptime.as_secs());
        widgets::section(ui, "LOCAL SYSTEM", |ui| {
            widgets::status_row(ui, "Platform", &snap.os_name, Palette::TEXT);
            if !snap.os_version.is_empty() {
                widgets::status_row(ui, "OS Version", &snap.os_version, Palette::TEXT_DIM);
            }
            if !snap.host_name.is_empty() {
                widgets::status_row(ui, "Device", &snap.host_name, Palette::TEXT);
            }
            widgets::status_row(
                ui,
                "Processors",
                &format!("{} logical", snap.cpu_count),
                Palette::TEXT,
            );
            widgets::status_row(
                ui,
                "Memory",
                &format!("{} / {} MB", snap.memory_used_mb, snap.memory_total_mb),
                Palette::TEXT,
            );
            let rect = ui.available_rect_before_wrap();
            widgets::status_row(
                ui,
                "Window Size",
                &format!("{} x {}", rect.width() as i32, rect.height() as i32),
                Palette::TEXT,
            );
            widgets::status_row(
                ui,
                "Local Time",
                &now_local.format("%H:%M:%S").to_string(),
                Palette::TEXT,
            );
            widgets::status_row(ui, "App Uptime", &uptime_str, Palette::TEXT);
            widgets::status_row(ui, "Status", "ONLINE", Palette::GREEN);
        });
    }

    fn draw_telemetry(&self, ui: &mut egui::Ui) {
        let snap = self.sys_monitor.latest();
        let cpu_frac = (snap.cpu_usage_percent / 100.0).clamp(0.0, 1.0);
        let mem_frac = if snap.memory_total_mb > 0 {
            snap.memory_used_mb as f32 / snap.memory_total_mb as f32
        } else {
            0.0
        };
        let audio_ready = self.audio.is_available();
        let playing = self.audio.any_playing();

        widgets::section(ui, "SYSTEM TELEMETRY", |ui| {
            widgets::segmented_bar(
                ui,
                "CPU",
                cpu_frac,
                &format!("{:>3.0}%", snap.cpu_usage_percent),
                Palette::GREEN,
            );
            widgets::segmented_bar(
                ui,
                "MEMORY",
                mem_frac,
                &format!("{:>3.0}%", mem_frac * 100.0),
                Palette::CYAN,
            );
            widgets::segmented_bar(
                ui,
                "AUDIO",
                if audio_ready { 1.0 } else { 0.0 },
                if !audio_ready {
                    "OFFLINE"
                } else if playing {
                    "PLAYING"
                } else {
                    "READY"
                },
                if audio_ready { Palette::GREEN } else { Palette::RED },
            );
            widgets::segmented_bar(
                ui,
                "SCHEDULER",
                1.0,
                "ONLINE",
                Palette::GREEN,
            );
        });
    }

    fn draw_active_operations(&self, ui: &mut egui::Ui, now_local: chrono::NaiveDateTime) {
        let next = scheduler::next_alarm(&self.alarms, now_local);
        let playing = self.audio.any_playing();
        widgets::section(ui, "ACTIVE OPERATIONS", |ui| {
            widgets::status_row(ui, "Scheduler", "ONLINE", Palette::GREEN);
            widgets::status_row(ui, "Alarm Monitor", "ACTIVE", Palette::GREEN);
            widgets::status_row(
                ui,
                "Audio Engine",
                &if playing {
                    format!("PLAYING x{}", self.audio.playing_count())
                } else {
                    "READY".to_string()
                },
                if playing { Palette::AMBER } else { Palette::GREEN },
            );
            widgets::status_row(ui, "Event Logger", "ACTIVE", Palette::GREEN);
            widgets::status_row(ui, "System Clock", "SYNC", Palette::GREEN);
            widgets::status_row(
                ui,
                "Next Trigger",
                &match next {
                    Some((a, _)) => a.time_label(),
                    None => "----".to_string(),
                },
                Palette::CYAN,
            );
        });
    }

    fn draw_central(&mut self, ctx: &egui::Context, today: NaiveDate, any_ringing: bool) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Palette::VOID)
                    .inner_margin(egui::Margin::same(10.0)),
            )
            .show(ctx, |ui| {
                if any_ringing {
                    self.draw_alarm_mode(ui, ctx);
                } else {
                    ui.horizontal(|ui| {
                        ui.set_height(ui.available_height() * 0.55);
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width());
                            self.draw_alarm_grid(ui, today);
                        });
                    });
                    ui.add_space(8.0);
                    self.draw_event_log(ui);
                }
            });
    }

    fn draw_alarm_grid(&mut self, ui: &mut egui::Ui, today: NaiveDate) {
        let mut edit_target: Option<u64> = None;
        let mut duplicate_target: Option<u64> = None;
        let mut toggle_target: Option<u64> = None;
        let mut delete_target: Option<u64> = None;

        widgets::section(ui, "ALARM CONTROL", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("+ NEW ALARM").strong())
                    .clicked()
                {
                    self.open_new_alarm_form();
                }
                ui.add_space(8.0);
                ui.colored_label(
                    Palette::TEXT_FAINT,
                    "[N] new   [ESC/SPACE] stop ringing",
                );
            });
            ui.add_space(6.0);
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.alarms.is_empty() {
                        ui.add_space(10.0);
                        ui.colored_label(
                            Palette::TEXT_DIM,
                            "NO ALARMS CONFIGURED — PRESS + NEW ALARM TO BEGIN",
                        );
                        return;
                    }

                    let mut sorted: Vec<&Alarm> = self.alarms.iter().collect();
                    sorted.sort_by_key(|a| (a.hour, a.minute));

                    for alarm in sorted {
                        let (badge_text, badge_color_kind) = alarm.badge(today);
                        let badge_color = match badge_color_kind {
                            crate::alarm::BadgeColor::Ok => Palette::GREEN,
                            crate::alarm::BadgeColor::Alert => Palette::RED,
                            crate::alarm::BadgeColor::Dim => Palette::TEXT_DIM,
                        };

                        egui::Frame::none()
                            .fill(Palette::PANEL_RAISED)
                            .stroke(Stroke::new(1.0, Palette::BORDER))
                            .rounding(egui::Rounding::same(2.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [56.0, 18.0],
                                        egui::Label::new(
                                            RichText::new(alarm.time_label())
                                                .font(FontId::new(15.0, FontFamily::Monospace))
                                                .color(Palette::TEXT)
                                                .strong(),
                                        ),
                                    );
                                    widgets::badge(ui, badge_text, badge_color);
                                    ui.add_sized(
                                        [150.0, 18.0],
                                        egui::Label::new(
                                            RichText::new(&alarm.name)
                                                .font(FontId::new(12.5, FontFamily::Monospace))
                                                .color(Palette::TEXT),
                                        ),
                                    );
                                    ui.add_sized(
                                        [72.0, 18.0],
                                        egui::Label::new(
                                            RichText::new(alarm.repeat.label())
                                                .font(FontId::new(11.5, FontFamily::Monospace))
                                                .color(Palette::CYAN),
                                        ),
                                    );
                                    ui.add_sized(
                                        [130.0, 18.0],
                                        egui::Label::new(
                                            RichText::new(alarm.sound_label())
                                                .font(FontId::new(11.0, FontFamily::Monospace))
                                                .color(Palette::TEXT_DIM),
                                        ),
                                    );

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if self.pending_delete == Some(alarm.id) {
                                                if ui
                                                    .button(
                                                        RichText::new("CONFIRM")
                                                            .color(Palette::RED),
                                                    )
                                                    .clicked()
                                                {
                                                    delete_target = Some(alarm.id);
                                                }
                                                if ui.button("CANCEL").clicked() {
                                                    delete_target = None;
                                                    toggle_target = None;
                                                    self.pending_delete = None;
                                                }
                                            } else {
                                                if ui.small_button("DEL").clicked() {
                                                    self.pending_delete = Some(alarm.id);
                                                }
                                                if ui.small_button("DUP").clicked() {
                                                    duplicate_target = Some(alarm.id);
                                                }
                                                if ui.small_button("EDIT").clicked() {
                                                    edit_target = Some(alarm.id);
                                                }
                                                let toggle_label =
                                                    if alarm.enabled { "ON" } else { "OFF" };
                                                if ui.small_button(toggle_label).clicked() {
                                                    toggle_target = Some(alarm.id);
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
        });

        if let Some(id) = edit_target {
            self.open_edit_alarm_form(id);
        }
        if let Some(id) = duplicate_target {
            self.duplicate_alarm(id);
        }
        if let Some(id) = toggle_target {
            self.toggle_enabled(id);
        }
        if let Some(id) = delete_target {
            self.delete_alarm(id);
        }
    }

    fn draw_event_log(&mut self, ui: &mut egui::Ui) {
        widgets::section(ui, "EVENT LOG", |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    for entry in self.log.entries() {
                        let color = match entry.severity {
                            Severity::Sys => Palette::CYAN,
                            Severity::Alarm => Palette::GREEN,
                            Severity::Audio => Palette::AMBER,
                            Severity::Warn => Palette::RED,
                        };
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                Palette::TEXT_FAINT,
                                RichText::new(&entry.timestamp)
                                    .font(FontId::new(11.0, FontFamily::Monospace)),
                            );
                            ui.colored_label(
                                color,
                                RichText::new(format!("[{}]", entry.severity.tag()))
                                    .font(FontId::new(11.0, FontFamily::Monospace)),
                            );
                            ui.colored_label(
                                Palette::TEXT,
                                RichText::new(&entry.message)
                                    .font(FontId::new(11.0, FontFamily::Monospace)),
                            );
                        });
                    }
                });
        });
    }

    fn draw_alarm_mode(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let elapsed = self.start_instant.elapsed().as_secs_f32();
        let pulse = (elapsed * 4.0).sin().abs();
        let border_color = Color32::from_rgb(
            255,
            (60.0 + pulse * 60.0) as u8,
            (40.0 + pulse * 20.0) as u8,
        );

        egui::Frame::none()
            .fill(Color32::from_rgb(18, 6, 6))
            .stroke(Stroke::new(3.0, border_color))
            .rounding(egui::Rounding::same(3.0))
            .inner_margin(egui::Margin::same(28.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.colored_label(
                        border_color,
                        RichText::new("!!! ALARM TRIGGERED !!!")
                            .font(FontId::new(30.0, FontFamily::Monospace))
                            .strong(),
                    );
                    ui.add_space(18.0);

                    let ringing: Vec<Alarm> = self
                        .alarms
                        .iter()
                        .filter(|a| a.state == AlarmState::Ringing)
                        .cloned()
                        .collect();

                    for alarm in &ringing {
                        ui.add_space(6.0);
                        ui.colored_label(
                            Palette::TEXT,
                            RichText::new(&alarm.name)
                                .font(FontId::new(22.0, FontFamily::Monospace))
                                .strong(),
                        );
                        ui.colored_label(
                            Palette::AMBER,
                            RichText::new(alarm.time_label())
                                .font(FontId::new(16.0, FontFamily::Monospace)),
                        );
                        ui.colored_label(Palette::TEXT_DIM, "PLAYBACK LOOP ACTIVE");
                        ui.add_space(6.0);
                        let stop_btn = egui::Button::new(
                            RichText::new("STOP ALARM")
                                .font(FontId::new(20.0, FontFamily::Monospace))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(180, 30, 30))
                        .min_size(vec2(260.0, 46.0));
                        if ui.add(stop_btn).clicked() {
                            self.stop_alarm(alarm.id);
                        }
                        ui.add_space(14.0);
                        ui.separator();
                    }

                    if ringing.len() > 1 {
                        ui.add_space(6.0);
                        let stop_all_btn = egui::Button::new(
                            RichText::new("STOP ALL ALARMS")
                                .font(FontId::new(16.0, FontFamily::Monospace))
                                .strong(),
                        )
                        .min_size(vec2(220.0, 34.0));
                        if ui.add(stop_all_btn).clicked() {
                            self.stop_all_ringing();
                        }
                    }

                    ui.add_space(8.0);
                    ui.colored_label(
                        Palette::TEXT_FAINT,
                        "Press ESC or SPACE to silence",
                    );
                });
            });

        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn draw_alarm_form_window(&mut self, ctx: &egui::Context) {
        if !self.show_alarm_form {
            return;
        }

        let is_edit = self.form.editing_id.is_some();
        let title = if is_edit { "EDIT ALARM" } else { "+ NEW ALARM" };

        let mut open = true;
        let mut save_requested = false;
        let mut cancel_requested = false;

        egui::Window::new(title)
            .id(egui::Id::new("alarm_form_window"))
            .collapsible(false)
            .resizable(false)
            .fixed_size(vec2(360.0, 0.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(Palette::PANEL)
                    .stroke(Stroke::new(1.5, Palette::CYAN_DIM)),
            )
            .open(&mut open)
            .show(ctx, |ui| {
                ui.colored_label(Palette::TEXT_DIM, "ALARM NAME");
                ui.text_edit_singleline(&mut self.form.name);
                ui.add_space(6.0);

                ui.colored_label(Palette::TEXT_DIM, "TIME");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.form.hour).clamp_range(0..=23).prefix("H "));
                    ui.add(egui::DragValue::new(&mut self.form.minute).clamp_range(0..=59).prefix("M "));
                });
                ui.add_space(6.0);

                ui.colored_label(Palette::TEXT_DIM, "SOUND");
                ui.horizontal(|ui| {
                    let label = self
                        .form
                        .sound_path
                        .as_ref()
                        .and_then(|p| std::path::Path::new(p).file_name())
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "SYSTEM BEEP (no file)".to_string());
                    ui.colored_label(Palette::TEXT, label);
                    if ui.button("BROWSE...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
                            .pick_file()
                        {
                            self.form.sound_path = Some(path.to_string_lossy().to_string());
                        }
                    }
                    if self.form.sound_path.is_some() && ui.small_button("CLEAR").clicked() {
                        self.form.sound_path = None;
                    }
                });
                ui.add_space(6.0);

                ui.colored_label(Palette::TEXT_DIM, "REPEAT");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.form.repeat_kind, RepeatKind::Once, "ONCE");
                    ui.selectable_value(&mut self.form.repeat_kind, RepeatKind::Daily, "DAILY");
                    ui.selectable_value(&mut self.form.repeat_kind, RepeatKind::Custom, "CUSTOM");
                });
                if self.form.repeat_kind == RepeatKind::Custom {
                    ui.horizontal(|ui| {
                        for (i, wd) in Weekday::ALL.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.form.custom_days[i],
                                true,
                                wd.letter(),
                            );
                        }
                    });
                }
                ui.add_space(6.0);

                ui.checkbox(&mut self.form.enabled, "ENABLED");
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("SAVE").strong()))
                        .clicked()
                    {
                        save_requested = true;
                    }
                    if ui.button("CANCEL").clicked() {
                        cancel_requested = true;
                    }
                });
            });

        if save_requested {
            self.save_form();
        }
        if cancel_requested || !open {
            self.show_alarm_form = false;
        }
    }
}

fn format_duration(total_secs: u64) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

