use egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (key, paths, family) in [
        (
            "UI",
            vec![
                "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
                "/usr/share/fonts/truetype/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ],
            FontFamily::Proportional,
        ),
        (
            "Mono",
            vec![
                "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
                "/usr/share/fonts/truetype/DejaVuSansMono.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            ],
            FontFamily::Monospace,
        ),
    ] {
        if let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()) {
            fonts
                .font_data
                .insert(key.into(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, key.into());
        }
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(22.));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(12.5));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.));
    });
}
