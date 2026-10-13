use crate::{
    icons::{self, Glyph},
    theme::Tokens,
};
use egui::{Align2, FontId, Response, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
}
pub fn button(ui: &mut Ui, t: &Tokens, label: &str, kind: Kind, enabled: bool) -> Response {
    let width = (label.chars().count() as f32 * 7.1 + 24.).max(55.);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, t.compact_height),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if !enabled {
        t.surface
    } else if response.is_pointer_button_down_on() {
        t.pressed
    } else if response.hovered() {
        t.hover
    } else {
        match kind {
            Kind::Primary => t.accent,
            Kind::Secondary => t.surface_raised,
            Kind::Ghost => t.surface,
        }
    };
    let fg = if !enabled {
        t.disabled
    } else if matches!(kind, Kind::Primary) {
        t.accent_text
    } else {
        t.text
    };
    ui.painter().rect_filled(rect, t.radius as f32, fill);
    ui.painter().rect_stroke(
        rect,
        t.radius as f32,
        Stroke::new(
            1.,
            if response.has_focus() {
                t.accent
            } else {
                t.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        fg,
    );
    response
}
pub fn icon_button(ui: &mut Ui, t: &Tokens, glyph: Glyph, selected: bool, hint: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(t.compact_height), Sense::click());
    let fill = if response.is_pointer_button_down_on() {
        t.pressed
    } else if selected {
        t.selection
    } else if response.hovered() {
        t.hover
    } else {
        t.surface
    };
    ui.painter().rect_filled(rect, t.radius as f32, fill);
    if selected || response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            t.radius as f32,
            Stroke::new(1., t.accent),
            egui::StrokeKind::Inside,
        );
    }
    icons::paint(
        ui,
        rect.shrink(6.),
        glyph,
        if selected { t.accent } else { t.text },
    );
    response.on_hover_text(hint)
}
