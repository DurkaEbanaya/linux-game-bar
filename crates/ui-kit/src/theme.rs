use egui::{Color32, CornerRadius, Shadow, Stroke, Visuals};

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub theme_name: &'static str,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selection: Color32,
    pub disabled: Color32,
    pub positive: Color32,
    pub warning: Color32,
    pub radius: u8,
    pub spacing: f32,
    pub compact_height: f32,
    pub shadow: Shadow,
}
impl Tokens {
    pub fn pro_dark() -> Self {
        Self {
            theme_name: "Pro Dark",
            bg: hex(0x121820),
            surface: hex(0x1c2530),
            surface_raised: hex(0x25313e),
            border: hex(0x354350),
            text: hex(0xf2f5fa),
            muted: hex(0xa3b4c4),
            accent: hex(0x4b9dff),
            accent_text: Color32::WHITE,
            hover: hex(0x344457),
            pressed: hex(0x263950),
            selection: hex(0x274868),
            disabled: hex(0x54606d),
            positive: hex(0x64d5b1),
            warning: hex(0xfcc67b),
            radius: 5,
            spacing: 8.,
            compact_height: 30.,
            shadow: Shadow {
                offset: [0, 4],
                blur: 14,
                spread: 0,
                color: Color32::from_black_alpha(50),
            },
        }
    }
    pub fn studio_light() -> Self {
        Self {
            theme_name: "Studio Light",
            bg: hex(0xf0f3f7),
            surface: hex(0xffffff),
            surface_raised: hex(0xe5edf5),
            border: hex(0xc9d5e1),
            text: hex(0x253547),
            muted: hex(0x647990),
            accent: hex(0x1766c6),
            accent_text: Color32::WHITE,
            hover: hex(0xd8e8f7),
            pressed: hex(0xbfdcf6),
            selection: hex(0xc0ddf9),
            disabled: hex(0x9caab8),
            positive: hex(0x16856d),
            warning: hex(0xb97b20),
            radius: 7,
            spacing: 9.,
            compact_height: 32.,
            shadow: Shadow {
                offset: [0, 5],
                blur: 18,
                spread: 0,
                color: Color32::from_black_alpha(20),
            },
        }
    }
    pub fn modern_dark() -> Self {
        Self {
            theme_name: "Modern Dark",
            bg: hex(0x151521),
            surface: hex(0x242436),
            surface_raised: hex(0x303149),
            border: hex(0x46465b),
            text: hex(0xf8f6ff),
            muted: hex(0xb5b1cc),
            accent: hex(0xb095ff),
            accent_text: hex(0x161021),
            hover: hex(0x403d5b),
            pressed: hex(0x37304f),
            selection: hex(0x50426d),
            disabled: hex(0x757086),
            positive: hex(0x63d4c6),
            warning: hex(0xffc883),
            radius: 13,
            spacing: 11.,
            compact_height: 34.,
            shadow: Shadow {
                offset: [0, 7],
                blur: 23,
                spread: 0,
                color: Color32::from_black_alpha(60),
            },
        }
    }
    pub fn apply(self, ctx: &egui::Context) {
        ctx.set_theme(if self.theme_name == "Studio Light" {
            egui::Theme::Light
        } else {
            egui::Theme::Dark
        });
        let mut style = (*ctx.global_style()).clone();
        let mut v = if self.theme_name == "Studio Light" {
            Visuals::light()
        } else {
            Visuals::dark()
        };
        v.panel_fill = self.bg;
        v.window_fill = self.surface;
        v.extreme_bg_color = self.bg;
        v.widgets.noninteractive.bg_fill = self.surface;
        v.widgets.inactive.bg_fill = self.surface_raised;
        v.widgets.hovered.bg_fill = self.hover;
        v.widgets.active.bg_fill = self.pressed;
        v.selection.bg_fill = self.selection;
        v.selection.stroke = Stroke::new(1., self.accent);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.fg_stroke = Stroke::new(1., self.text);
        }
        v.window_corner_radius = CornerRadius::ZERO;
        v.window_shadow = self.shadow;
        style.visuals = v;
        style.spacing.item_spacing = egui::vec2(self.spacing, self.spacing);
        style.spacing.button_padding = egui::vec2(10., 5.);
        ctx.set_style_of(ctx.theme(), style);
    }
}
const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
