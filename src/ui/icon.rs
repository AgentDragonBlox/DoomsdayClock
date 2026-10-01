//! The taskbar/window icon, rasterised at startup with the same CPU
//! renderer as the wallpaper - so no .ico/.png asset ships in the repo.
//! Design: a dark glass orb clock at three minutes to midnight.

use eframe::egui::IconData;
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, SpreadMode, Stroke, Transform,
};

pub fn window_icon() -> IconData {
    const S: u32 = 64;
    let mut pm = Pixmap::new(S, S).expect("64x64");
    let c = S as f32 / 2.0;
    let r = c - 2.0;

    let shader = |stops: Vec<GradientStop>, y0: f32, y1: f32| {
        LinearGradient::new(Point::from_xy(c, y0), Point::from_xy(c, y1), stops, SpreadMode::Pad, Transform::identity())
    };
    let circle = |cx: f32, cy: f32, rad: f32| PathBuilder::from_circle(cx, cy, rad);

    // Orb body.
    if let (Some(path), Some(sh)) = (
        circle(c, c, r),
        shader(
            vec![
                GradientStop::new(0.0, Color::from_rgba8(0x3A, 0x4A, 0x63, 255)),
                GradientStop::new(0.5, Color::from_rgba8(0x12, 0x1A, 0x27, 255)),
                GradientStop::new(1.0, Color::from_rgba8(0x05, 0x08, 0x0D, 255)),
            ],
            c - r,
            c + r,
        ),
    ) {
        pm.fill_path(
            &path,
            &Paint { shader: sh, anti_alias: true, ..Default::default() },
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let stroke_line = |pm: &mut Pixmap, a: (f32, f32), b: (f32, f32), w: f32, rgb: (u8, u8, u8)| {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(rgb.0, rgb.1, rgb.2, 255);
            paint.anti_alias = true;
            let stroke = Stroke { width: w, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    };

    // Rim + hour ticks.
    if let Some(path) = circle(c, c, r - 0.75) {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0x7F, 0x95, 0xB5, 255);
        paint.anti_alias = true;
        pm.stroke_path(&path, &paint, &Stroke { width: 1.5, ..Default::default() }, Transform::identity(), None);
    }
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        let (inner, w) = if i % 3 == 0 { (0.62, 3.0) } else { (0.74, 1.6) };
        let (s, co) = (a.sin(), -a.cos());
        stroke_line(
            &mut pm,
            (c + s * r * inner, c + co * r * inner),
            (c + s * r * 0.85, c + co * r * 0.85),
            w,
            (0xDD, 0xE6, 0xF2),
        );
    }

    // Hands at 11:57 - minute hand in red.
    let hand = |pm: &mut Pixmap, frac: f32, len: f32, w: f32, rgb| {
        let a = frac * std::f32::consts::TAU;
        stroke_line(pm, (c, c), (c + a.sin() * r * len, c - a.cos() * r * len), w, rgb);
    };
    hand(&mut pm, 11.95 / 12.0, 0.45, 4.5, (0xF2, 0xF5, 0xF9));
    hand(&mut pm, 57.0 / 60.0, 0.74, 3.0, (0xFF, 0x3B, 0x30));
    if let Some(path) = circle(c, c, 3.2) {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0xFF, 0x3B, 0x30, 255);
        paint.anti_alias = true;
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    // Glass reflection.
    if let (Some(rect), Some(sh)) = (
        tiny_skia::Rect::from_xywh(c - r * 0.72, c - r * 0.93, r * 1.44, r * 0.9),
        shader(
            vec![
                GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 95)),
                GradientStop::new(1.0, Color::from_rgba8(255, 255, 255, 0)),
            ],
            c - r * 0.93,
            c - r * 0.03,
        ),
    ) && let Some(path) = PathBuilder::from_oval(rect)
    {
        pm.fill_path(
            &path,
            &Paint { shader: sh, anti_alias: true, ..Default::default() },
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let rgba = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    IconData { rgba, width: S, height: S }
}
