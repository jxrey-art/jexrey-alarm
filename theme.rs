//! Visual identity: near-black backgrounds, thin bordered panels, a
//! phosphor-green / cyan accent pair, and monospace type throughout — the
//! "operations center" ambiance requested in the brief, built from
//! scratch as an original palette and layout (not a copy of any reference
//! image).

use egui::{Color32, FontFamily, FontId, Rounding, Stroke, Style, TextStyle, Visuals};

pub struct Palette;

impl Palette {
    pub const VOID: Color32 = Color32::from_rgb(6, 10, 10);
    pub const PANEL: Color32 = Color32::from_rgb(10, 16, 16);
    pub const PANEL_RAISED: Color32 = Color32::from_rgb(13, 20, 20);
    pub const BORDER: Color32 = Color32::from_rgb(28, 46, 42);
    pub const BORDER_BRIGHT: Color32 = Color32::from_rgb(45, 92, 78);

    pub const GREEN: Color32 = Color32::from_rgb(57, 255, 136);
    pub const GREEN_DIM: Color32 = Color32::from_rgb(31, 110, 74);
    pub const CYAN: Color32 = Color32::from_rgb(77, 224, 255);
    pub const CYAN_DIM: Color32 = Color32::from_rgb(30, 95, 107);
    pub const AMBER: Color32 = Color32::from_rgb(255, 176, 32);
    pub const RED: Color32 = Color32::from_rgb(255, 66, 66);

    pub const TEXT: Color32 = Color32::from_rgb(200, 238, 222);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(92, 122, 112);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(52, 72, 66);
}

pub fn install(ctx: &egui::Context) {
    let mut style = Style::default();
    style.visuals = build_visuals();

    let mono = FontId::new(13.0, FontFamily::Monospace);
    let mono_small = FontId::new(11.0, FontFamily::Monospace);
    let mono_heading = FontId::new(17.0, FontFamily::Monospace);
    let mono_button = FontId::new(13.0, FontFamily::Monospace);

    style
        .text_styles
        .insert(TextStyle::Heading, mono_heading);
    style.text_styles.insert(TextStyle::Body, mono.clone());
    style
        .text_styles
        .insert(TextStyle::Monospace, mono.clone());
    style
        .text_styles
        .insert(TextStyle::Button, mono_button);
    style.text_styles.insert(TextStyle::Small, mono_small);

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(10.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.indent = 14.0;

    ctx.set_style(style);
}

fn build_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.override_text_color = Some(Palette::TEXT);
    v.window_fill = Palette::PANEL;
    v.panel_fill = Palette::VOID;
    v.faint_bg_color = Palette::PANEL_RAISED;
    v.extreme_bg_color = Color32::from_rgb(4, 6, 6);
    v.code_bg_color = Palette::PANEL_RAISED;
    v.window_stroke = Stroke::new(1.0, Palette::BORDER_BRIGHT);
    v.window_rounding = Rounding::same(2.0);
    v.menu_rounding = Rounding::same(2.0);

    v.widgets.noninteractive.bg_fill = Palette::PANEL;
    v.widgets.noninteractive.weak_bg_fill = Palette::PANEL_RAISED;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Palette::BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Palette::TEXT_DIM);
    v.widgets.noninteractive.rounding = Rounding::same(1.0);

    v.widgets.inactive.bg_fill = Palette::PANEL_RAISED;
    v.widgets.inactive.weak_bg_fill = Palette::PANEL_RAISED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, Palette::TEXT);
    v.widgets.inactive.rounding = Rounding::same(1.0);

    v.widgets.hovered.bg_fill = Color32::from_rgb(18, 34, 30);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(18, 34, 30);
    v.widgets.hovered.bg_stroke = Stroke::new(1.2, Palette::GREEN_DIM);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Palette::GREEN);
    v.widgets.hovered.rounding = Rounding::same(1.0);

    v.widgets.active.bg_fill = Color32::from_rgb(15, 40, 32);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(15, 40, 32);
    v.widgets.active.bg_stroke = Stroke::new(1.4, Palette::GREEN);
    v.widgets.active.fg_stroke = Stroke::new(1.2, Palette::GREEN);
    v.widgets.active.rounding = Rounding::same(1.0);

    v.selection.bg_fill = Palette::GREEN_DIM;
    v.selection.stroke = Stroke::new(1.0, Palette::GREEN);

    v.hyperlink_color = Palette::CYAN;
    v.error_fg_color = Palette::RED;
    v.warn_fg_color = Palette::AMBER;

    v
}
