use egui::{
    Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Response, Stroke, TextStyle,
    Ui, Visuals,
};
use modelview_core::{DefectKind, Verdict};
use std::sync::Arc;

// Blueprint: a pure black field, white type, and one electric blue line.
pub const VOID: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const PANEL: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const ASH: Color32 = Color32::from_rgb(0x05, 0x08, 0x0c);
pub const ASH_HI: Color32 = Color32::from_rgb(0x0a, 0x12, 0x20);
pub const BONE: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const BONE_HI: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const DIM: Color32 = Color32::from_rgb(0xa7, 0xb3, 0xbf);
pub const FAINT: Color32 = Color32::from_rgb(0x5c, 0x6b, 0x78);
pub const LINE: Color32 = Color32::from_rgb(0x16, 0x1e, 0x28);
pub const LINE2: Color32 = Color32::from_rgb(0x26, 0x35, 0x45);
pub const BLUE: Color32 = Color32::from_rgb(0x2f, 0x6b, 0xff);
pub const BLUE_DIM: Color32 = Color32::from_rgb(0x0a, 0x17, 0x30);
pub const SCROLL_HOVER: Color32 = Color32::from_rgb(0x2a, 0x3b, 0x4e);
pub const SCROLL_ACTIVE: Color32 = Color32::from_rgb(0x34, 0x4b, 0x62);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x5a, 0x5f);
pub const RED_TEXT: Color32 = Color32::from_rgb(0xff, 0x8a, 0x8a);
pub const AMBER: Color32 = Color32::from_rgb(0xf5, 0xa5, 0x24);
pub const TEAL: Color32 = Color32::from_rgb(0x14, 0xb8, 0xa6);
pub const VIOLET: Color32 = Color32::from_rgb(0xa7, 0x8b, 0xfa);
pub const RED_LINE: Color32 = Color32::from_rgb(0xd9, 0x2d, 0x20);
pub const AMBER_LINE: Color32 = Color32::from_rgb(0xb4, 0x53, 0x09);
pub const TEAL_LINE: Color32 = Color32::from_rgb(0x0f, 0x76, 0x6e);
pub const VIOLET_LINE: Color32 = Color32::from_rgb(0x6d, 0x28, 0xd9);

/// One shift applied to every text job, so the whole UI scales together.
pub const TEXT_SHIFT: f32 = 3.0;

pub fn narrow() -> FontFamily {
    FontFamily::Name(Arc::from("narrow"))
}

pub fn narrow_med() -> FontFamily {
    FontFamily::Name(Arc::from("narrow_med"))
}

pub fn narrow_semi() -> FontFamily {
    FontFamily::Name(Arc::from("narrow_semi"))
}

pub fn text_job(
    text: impl Into<String>,
    size: f32,
    family: FontFamily,
    color: Color32,
    tracking: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.into(),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(size + TEXT_SHIFT, family),
            color,
            extra_letter_spacing: tracking,
            ..Default::default()
        },
    );
    job
}

pub fn label(
    ui: &mut Ui,
    text: impl Into<String>,
    size: f32,
    family: FontFamily,
    color: Color32,
    tracking: f32,
) -> Response {
    ui.add(egui::Label::new(text_job(text, size, family, color, tracking)).selectable(false))
}

pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    fonts.font_data.insert(
        "archivo".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Archivo-400.ttf"
        ))),
    );
    fonts.font_data.insert(
        "archivo-med".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Archivo-500.ttf"
        ))),
    );
    fonts.font_data.insert(
        "narrow".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/ArchivoNarrow-400.ttf"
        ))),
    );
    fonts.font_data.insert(
        "narrow-med".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/ArchivoNarrow-500.ttf"
        ))),
    );
    fonts.font_data.insert(
        "narrow-semi".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/ArchivoNarrow-600.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "archivo".into());
    let mut narrows = vec!["narrow".to_string()];
    narrows.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(Arc::from("narrow")), narrows);
    let mut narrows_med = vec!["narrow-med".to_string(), "narrow".to_string()];
    narrows_med.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(Arc::from("narrow_med")), narrows_med);
    let mut narrows_semi = vec![
        "narrow-semi".to_string(),
        "narrow-med".to_string(),
        "narrow".to_string(),
    ];
    narrows_semi.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(Arc::from("narrow_semi")), narrows_semi);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals = Visuals::dark();
        style.visuals.panel_fill = VOID;
        style.visuals.window_fill = VOID;
        style.visuals.extreme_bg_color = VOID;
        style.visuals.faint_bg_color = ASH;
        style.visuals.code_bg_color = ASH;
        style.visuals.override_text_color = Some(BONE);
        style.visuals.hyperlink_color = BLUE;
        style.visuals.warn_fg_color = AMBER;
        style.visuals.error_fg_color = RED;
        style.visuals.window_stroke = Stroke::new(1.0, LINE);
        style.visuals.window_shadow = egui::epaint::Shadow::NONE;
        style.visuals.popup_shadow = egui::epaint::Shadow::NONE;
        style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
        style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
        style.visuals.selection.bg_fill = BLUE.gamma_multiply(0.32);
        style.visuals.selection.stroke = Stroke::new(1.0, BLUE);
        style.visuals.text_cursor.stroke = Stroke::new(1.5, BLUE);
        style.visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, BONE);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::ZERO;
        style.visuals.widgets.inactive.bg_fill = LINE2;
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE2);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, BONE);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
        style.visuals.widgets.hovered.bg_fill = SCROLL_HOVER;
        style.visuals.widgets.hovered.weak_bg_fill = ASH;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BLUE);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, BONE_HI);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
        style.visuals.widgets.active.bg_fill = SCROLL_ACTIVE;
        style.visuals.widgets.active.weak_bg_fill = ASH_HI;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, BLUE);
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, BONE_HI);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::ZERO;
        style.visuals.widgets.open.bg_fill = ASH;
        style.visuals.widgets.open.weak_bg_fill = ASH;
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, BLUE);
        style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, BONE_HI);
        style.visuals.widgets.open.corner_radius = egui::CornerRadius::ZERO;
        style.visuals.widgets.inactive.expansion = 0.0;
        style.visuals.widgets.hovered.expansion = 0.0;
        style.visuals.widgets.active.expansion = 0.0;

        style.animation_time = 0.12;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.menu_margin = egui::Margin::same(8);
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating = false;
        style.spacing.scroll.foreground_color = false;
        style.spacing.scroll.bar_inner_margin = 3.0;
        style.spacing.scroll.bar_outer_margin = 6.0;

        style.text_styles = [
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(12.0, narrow())),
            (TextStyle::Small, FontId::new(11.0, narrow())),
            (TextStyle::Heading, FontId::new(18.0, narrow())),
            (TextStyle::Monospace, FontId::new(13.0, narrow())),
        ]
        .into();
    });
}

pub fn kind_color(kind: DefectKind) -> Color32 {
    match kind {
        DefectKind::OpenEdge => RED,
        DefectKind::NonManifold => AMBER,
        DefectKind::InconsistentWinding => TEAL,
        DefectKind::DegenerateFace => VIOLET,
    }
}

/// The same defect inks, one step darker for hairlines drawn on the lit mesh.
pub fn kind_line_color(kind: DefectKind) -> Color32 {
    match kind {
        DefectKind::OpenEdge => RED_LINE,
        DefectKind::NonManifold => AMBER_LINE,
        DefectKind::InconsistentWinding => TEAL_LINE,
        DefectKind::DegenerateFace => VIOLET_LINE,
    }
}

pub fn verdict_color(verdict: Verdict) -> Color32 {
    match verdict {
        Verdict::Watertight => BONE,
        Verdict::Warning => AMBER,
        Verdict::Failed => RED,
    }
}

pub fn corner_ticks(painter: &egui::Painter, rect: egui::Rect, color: Color32, len: f32) {
    let s = Stroke::new(1.0, color);
    let r = rect.shrink(0.5);
    painter.line_segment([r.left_top(), r.left_top() + egui::vec2(len, 0.0)], s);
    painter.line_segment([r.left_top(), r.left_top() + egui::vec2(0.0, len)], s);
    painter.line_segment([r.right_top(), r.right_top() - egui::vec2(len, 0.0)], s);
    painter.line_segment([r.right_top(), r.right_top() + egui::vec2(0.0, len)], s);
    painter.line_segment([r.left_bottom(), r.left_bottom() + egui::vec2(len, 0.0)], s);
    painter.line_segment([r.left_bottom(), r.left_bottom() - egui::vec2(0.0, len)], s);
    painter.line_segment(
        [r.right_bottom(), r.right_bottom() - egui::vec2(len, 0.0)],
        s,
    );
    painter.line_segment(
        [r.right_bottom(), r.right_bottom() - egui::vec2(0.0, len)],
        s,
    );
}

/// A state mark in the rail language: solid pass, dashed warning, doubled fail.
pub fn state_mark(painter: &egui::Painter, center: egui::Pos2, state: StateMark) {
    let color = match state {
        StateMark::Pass => BONE,
        StateMark::Info => DIM,
        StateMark::Warn => AMBER,
        StateMark::Fail => RED,
    };
    let s = Stroke::new(1.0, color);
    match state {
        StateMark::Fail => {
            painter.line_segment(
                [
                    center + egui::vec2(-11.0, -1.8),
                    center + egui::vec2(11.0, -1.8),
                ],
                s,
            );
            painter.line_segment(
                [
                    center + egui::vec2(-11.0, 1.8),
                    center + egui::vec2(11.0, 1.8),
                ],
                Stroke::new(1.0, color),
            );
        }
        StateMark::Warn => {
            let mut x = -11.0;
            while x < 11.0 {
                painter.line_segment(
                    [
                        center + egui::vec2(x, 0.0),
                        center + egui::vec2((x + 5.0).min(11.0), 0.0),
                    ],
                    s,
                );
                x += 7.0;
            }
        }
        _ => {
            painter.line_segment(
                [
                    center + egui::vec2(-11.0, 0.0),
                    center + egui::vec2(11.0, 0.0),
                ],
                s,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateMark {
    Pass,
    Info,
    Warn,
    Fail,
}

pub fn fmt_int(value: usize) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn fmt_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

pub fn fmt_volume(mm3: f64) -> String {
    if mm3 >= 1000.0 {
        format!("{:.2} cm³", mm3 / 1000.0)
    } else {
        format!("{:.1} mm³", mm3)
    }
}

pub fn fmt_area(mm2: f64) -> String {
    if mm2 >= 1000.0 {
        format!("{:.1} cm²", mm2 / 100.0)
    } else {
        format!("{:.1} mm²", mm2)
    }
}

pub fn fmt_ms(ms: f64) -> String {
    if ms < 1.0 {
        "<1 ms".to_string()
    } else if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}
