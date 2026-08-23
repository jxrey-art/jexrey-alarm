//! Small, original custom-drawn widgets shared across panels: a bordered
//! titled panel ("• SECTION TITLE"), a blocky segmented gauge bar
//! (`███████░░░░░ 51%`), a two-column status row, and a status badge pill.

use crate::theme::Palette;
use egui::{vec2, Color32, FontFamily, FontId, Rounding, Sense, Stroke, Ui};

/// A bordered panel with a small "• TITLE" header, matching the
/// multi-panel dashboard architecture the brief asks for.
pub fn section<R>(
    ui: &mut Ui,
    title: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = egui::Frame::none()
        .fill(Palette::PANEL)
        .stroke(Stroke::new(1.0, Palette::BORDER))
        .rounding(Rounding::same(2.0))
        .inner_margin(egui::Margin::same(10.0));

    frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(Palette::GREEN_DIM, "\u{2022}");
                ui.colored_label(
                    Palette::CYAN,
                    egui::RichText::new(title)
                        .font(FontId::new(12.5, FontFamily::Monospace))
                        .strong(),
                );
            });
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);
            add_contents(ui)
        })
        .inner
}

/// `LABEL   ███████░░░░░  51%` — an ASCII-bar-styled gauge, drawn as real
/// filled/empty segments rather than literal block glyphs so it scales
/// cleanly at any font size.
pub fn segmented_bar(ui: &mut Ui, label: &str, fraction: f32, value_text: &str, color: Color32) {
    let fraction = fraction.clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        ui.add_sized(
            [72.0, 16.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .color(Palette::TEXT_DIM)
                    .font(FontId::new(11.5, FontFamily::Monospace)),
            ),
        );

        let segments = 22;
        let filled = ((segments as f32) * fraction).round() as usize;
        let (rect, _resp) =
            ui.allocate_exact_size(vec2(segments as f32 * 7.0, 14.0), Sense::hover());
        let painter = ui.painter();
        for i in 0..segments {
            let x0 = rect.left() + i as f32 * 7.0;
            let seg_rect =
                egui::Rect::from_min_size(egui::pos2(x0, rect.top()), vec2(5.0, rect.height()));
            let fill = if i < filled {
                color
            } else {
                Palette::PANEL_RAISED
            };
            painter.rect_filled(seg_rect, Rounding::ZERO, fill);
        }

        ui.add_space(6.0);
        ui.colored_label(
            color,
            egui::RichText::new(value_text).font(FontId::new(11.5, FontFamily::Monospace)),
        );
    });
}

/// `LABEL                    VALUE` two-column status line.
pub fn status_row(ui: &mut Ui, label: &str, value: &str, value_color: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [150.0, 15.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .color(Palette::TEXT_DIM)
                    .font(FontId::new(12.0, FontFamily::Monospace)),
            ),
        );
        ui.colored_label(
            value_color,
            egui::RichText::new(value).font(FontId::new(12.0, FontFamily::Monospace)),
        );
    });
}

/// Small bracketed status pill, e.g. `[ACTIVE]`, `[OFF]`, `[RINGING]`.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color))
        .rounding(Rounding::same(2.0))
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.colored_label(
                color,
                egui::RichText::new(text)
                    .font(FontId::new(11.0, FontFamily::Monospace))
                    .strong(),
            );
        });
}

/// A small filled/hollow dot indicator, e.g. the `● ONLINE` marker.
pub fn dot(ui: &mut Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), size * 0.5, color);
}
