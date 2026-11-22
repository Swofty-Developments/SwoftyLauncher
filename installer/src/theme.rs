use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(0x18, 0x1b, 0x22);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(0x1f, 0x23, 0x2d);
const BORDER: Color32 = Color32::from_rgb(0x3a, 0x3f, 0x4d);
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xe9, 0xec, 0xf3);
pub(crate) const MUTED: Color32 = Color32::from_rgb(0xa3, 0xa8, 0xb7);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x8b, 0x7f, 0xd6);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0xa1, 0x96, 0xe6);
pub(crate) const ON_ACCENT: Color32 = Color32::from_rgb(0xf8, 0xf6, 0xff);
pub(crate) const ERROR: Color32 = Color32::from_rgb(0xf0, 0x6d, 0x78);

pub(crate) fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BACKGROUND;
    v.window_fill = BACKGROUND;
    v.extreme_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, ON_ACCENT);
    v.text_edit_bg_color = Some(SURFACE);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(6);
    }
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v
}
