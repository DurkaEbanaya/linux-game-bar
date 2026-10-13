use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui};

#[derive(Clone, Copy, Debug)]
pub enum Glyph {
    Sliders,
    Activity,
    Grid,
    Settings,
    Close,
    Speaker,
    Microphone,
}
/// Original Painter strokes, on a 20×20 grid; no bitmap/brand assets.
pub fn paint(ui: &Ui, rect: Rect, glyph: Glyph, color: Color32) {
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.min.x + rect.width() * x / 20.,
            rect.min.y + rect.height() * y / 20.,
        )
    };
    let s = Stroke::new((rect.width() / 20. * 1.55).max(1.), color);
    let line = |points: &[(f32, f32)]| {
        ui.painter().add(Shape::line(
            points.iter().map(|(x, y)| p(*x, *y)).collect(),
            s,
        ));
    };
    match glyph {
        Glyph::Sliders => {
            for (y, x) in [(5., 7.), (10., 13.), (15., 9.)] {
                line(&[(3., y), (17., y)]);
                ui.painter().circle_filled(p(x, y), 2.2, color);
            }
        }
        Glyph::Activity => line(&[
            (2., 11.),
            (6., 11.),
            (9., 3.),
            (12., 17.),
            (15., 10.),
            (18., 10.),
        ]),
        Glyph::Grid => {
            for x in [4., 10., 16.] {
                line(&[(x, 3.), (x, 17.)]);
            }
            for y in [4., 10., 16.] {
                line(&[(3., y), (17., y)]);
            }
        }
        Glyph::Settings => {
            ui.painter().circle_stroke(p(10., 10.), 6.5, s);
            ui.painter().circle_stroke(p(10., 10.), 2.5, s);
        }
        Glyph::Close => {
            line(&[(5., 5.), (15., 15.)]);
            line(&[(15., 5.), (5., 15.)]);
        }
        Glyph::Speaker => {
            line(&[
                (3., 8.),
                (6., 8.),
                (11., 4.),
                (11., 16.),
                (6., 12.),
                (3., 12.),
                (3., 8.),
            ]);
            line(&[(14., 6.), (17., 9.), (17., 11.), (14., 14.)]);
        }
        Glyph::Microphone => {
            line(&[(8., 3.), (12., 3.), (12., 11.), (8., 11.), (8., 3.)]);
            line(&[(5., 8.), (5., 12.), (10., 15.), (15., 12.), (15., 8.)]);
            line(&[(10., 15.), (10., 18.)]);
        }
    }
}
