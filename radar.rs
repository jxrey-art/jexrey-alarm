//! The "temporal radar" — an abstract, original visualization of the
//! alarm calendar (explicitly *not* a geographic map, per the brief).
//!
//! Encoding:
//!   * **angle** = the alarm's clock time, mapped like a 24h clock face
//!     (00:00 at the top, clockwise) — so an alarm's position rotates
//!     around the dial the same way its time of day would on a clock.
//!   * **radius** = how soon it will actually fire (minutes from now,
//!     clamped to a 12-hour window) — so urgency reads as "distance from
//!     the center", independent of what angle it happens to sit at.
//!
//! A slow decorative sweep line animates continuously so the panel always
//! reads as "alive", and a short static tick marks the current-time
//! heading.

use crate::alarm::{Alarm, AlarmState};
use crate::theme::Palette;
use chrono::{NaiveDateTime, Timelike};
use egui::{pos2, vec2, Color32, FontFamily, FontId, Rect, Sense, Stroke, Ui};
use std::f32::consts::{PI, TAU};

const WINDOW_MINUTES: f32 = 12.0 * 60.0;

pub fn draw(ui: &mut Ui, alarms: &[Alarm], now: NaiveDateTime, elapsed_secs: f32) {
    let available = ui.available_size();
    let side = available.x.min(available.y).max(140.0);
    let (rect, response) = ui.allocate_exact_size(vec2(available.x, side), Sense::hover());
    let center = pos2(rect.center().x, rect.center().y);
    let outer_r = side * 0.46;
    let painter = ui.painter_at(rect);

    // Background.
    painter.rect_filled(rect, egui::Rounding::same(2.0), Color32::from_rgb(4, 7, 7));

    // Concentric rings + labels.
    for step in 1..=4 {
        let r = outer_r * (step as f32) / 4.0;
        painter.circle_stroke(center, r, Stroke::new(1.0, Palette::BORDER));
        let label = format!("+{}H", step * 3);
        painter.text(
            pos2(center.x + 4.0, center.y - r),
            egui::Align2::LEFT_BOTTOM,
            label,
            FontId::new(9.0, FontFamily::Monospace),
            Palette::TEXT_FAINT,
        );
    }

    // Crosshair grid (cardinal + diagonal), matching the reference ambiance.
    for i in 0..8 {
        let a = (i as f32) * PI / 4.0;
        let dir = vec2(a.cos(), a.sin());
        painter.line_segment(
            [center, center + dir * outer_r],
            Stroke::new(1.0, Palette::BORDER),
        );
    }
    painter.circle_stroke(center, outer_r, Stroke::new(1.3, Palette::GREEN_DIM));

    // Decorative rotating sweep (period ~6s), drawn as a fading wedge trail.
    let sweep_angle = (elapsed_secs / 6.0) * TAU - PI / 2.0;
    let trail_segments = 26;
    for i in 0..trail_segments {
        let t = i as f32 / trail_segments as f32;
        let a = sweep_angle - t * 0.9;
        let alpha = ((1.0 - t) * 70.0) as u8;
        let dir = vec2(a.cos(), a.sin());
        painter.line_segment(
            [center, center + dir * outer_r],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(57, 255, 136, alpha)),
        );
    }

    // Current-time heading tick (a real, meaningful marker: where "now" sits
    // on the 24h dial).
    let now_minutes = (now.hour() * 60 + now.minute()) as f32;
    let now_angle = clock_angle(now_minutes);
    let now_dir = vec2(now_angle.cos(), now_angle.sin());
    painter.line_segment(
        [
            center + now_dir * (outer_r - 10.0),
            center + now_dir * (outer_r + 6.0),
        ],
        Stroke::new(2.0, Palette::CYAN),
    );

    // Alarm points.
    for alarm in alarms {
        let Some(next) = alarm.next_occurrence(now) else {
            continue;
        };
        let minutes_of_day = (alarm.hour * 60 + alarm.minute) as f32;
        let angle = clock_angle(minutes_of_day);
        let minutes_until = (next - now).num_minutes().max(0) as f32;
        let radius_fraction = (minutes_until / WINDOW_MINUTES).clamp(0.02, 1.0);
        let r = outer_r * radius_fraction;
        let dir = vec2(angle.cos(), angle.sin());
        let point = center + dir * r;

        let (color, pulse) = if alarm.state == AlarmState::Ringing {
            (Palette::RED, true)
        } else if minutes_until <= 15.0 {
            (Palette::AMBER, true)
        } else {
            (Palette::GREEN, false)
        };

        let dot_radius = if pulse {
            3.0 + 1.4 * ((elapsed_secs * 5.0).sin().abs())
        } else {
            2.6
        };

        painter.line_segment(
            [center, point],
            Stroke::new(1.0, color.gamma_multiply(0.35)),
        );
        painter.circle_filled(point, dot_radius, color);
        painter.circle_stroke(point, dot_radius + 2.5, Stroke::new(1.0, color.gamma_multiply(0.5)));

        let hover_rect = Rect::from_center_size(point, vec2(14.0, 14.0));
        if hover_rect.contains(response.hover_pos().unwrap_or(pos2(-1.0, -1.0))) {
            painter.text(
                point + vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{} {}", alarm.time_label(), alarm.name.to_uppercase()),
                FontId::new(10.5, FontFamily::Monospace),
                Palette::TEXT,
            );
        }
    }

    // Center "NOW" marker.
    painter.circle_filled(center, 3.2, Palette::CYAN);
    painter.text(
        center + vec2(0.0, 12.0),
        egui::Align2::CENTER_TOP,
        "NOW",
        FontId::new(10.0, FontFamily::Monospace),
        Palette::CYAN,
    );
}

/// Map a minute-of-day value (0..1440) onto a radar angle in radians, with
/// 00:00 pointing straight up and time increasing clockwise.
fn clock_angle(minutes_of_day: f32) -> f32 {
    (minutes_of_day / 1440.0) * TAU - PI / 2.0
}
