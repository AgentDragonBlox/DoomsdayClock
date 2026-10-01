//! Low-level painting primitives egui doesn't ship: multi-stop vertical
//! gradients with rounded corners, radial glows, and a few vector icons.
//!
//! Everything here builds plain triangle meshes, so it all lands in egui's
//! single batched draw call - no textures, no extra GPU passes. A gradient
//! with a hard "glass" edge in the middle is just two rows of vertices at
//! the same y.

use eframe::egui::{Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

pub const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub fn rgba(rgb: u32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, (alpha * 255.0).round() as u8)
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
    let f = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(f(0), f(1), f(2), f(3))
}

fn sample(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let Some(first) = stops.first() else { return Color32::TRANSPARENT };
    if t <= first.0 {
        return first.1;
    }
    for w in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t1 {
            return if t1 - t0 <= f32::EPSILON { c1 } else { mix(c0, c1, (t - t0) / (t1 - t0)) };
        }
    }
    stops.last().map_or(Color32::TRANSPARENT, |s| s.1)
}

/// Fills `rect` with a vertical multi-stop gradient. `r_top`/`r_bottom` round
/// the top/bottom corners. Two stops at the same position make a hard edge.
pub fn gradient(painter: &Painter, rect: Rect, r_top: f32, r_bottom: f32, stops: &[(f32, Color32)]) {
    painter.add(gradient_shape(rect, r_top, r_bottom, stops));
}

/// Same as [`gradient`], returned as a shape (for deferred painting).
pub fn gradient_shape(rect: Rect, r_top: f32, r_bottom: f32, stops: &[(f32, Color32)]) -> Shape {
    if rect.height() <= 0.0 || rect.width() <= 0.0 || stops.is_empty() {
        return Shape::Noop;
    }
    let half = rect.width().min(rect.height()) / 2.0;
    let (rt, rb) = (r_top.clamp(0.0, half), r_bottom.clamp(0.0, half));

    // Rows: every stop (keeps hard edges), plus corner samples for the curve.
    let mut rows: Vec<(f32, Color32)> = Vec::with_capacity(stops.len() + 16);
    if stops[0].0 > 0.0 {
        rows.push((rect.top(), stops[0].1));
    }
    rows.extend(stops.iter().map(|&(t, c)| (rect.top() + t.clamp(0.0, 1.0) * rect.height(), c)));
    if stops[stops.len() - 1].0 < 1.0 {
        rows.push((rect.bottom(), stops[stops.len() - 1].1));
    }
    const SEGMENTS: usize = 6;
    for i in 0..=SEGMENTS {
        let f = i as f32 / SEGMENTS as f32;
        for y in [rect.top() + rt * f, rect.bottom() - rb * f] {
            if !rows.iter().any(|r| (r.0 - y).abs() < 0.01) {
                rows.push((y, sample(stops, (y - rect.top()) / rect.height())));
            }
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));

    let inset = |y: f32| {
        let d_top = rect.top() + rt - y;
        let d_bot = y - (rect.bottom() - rb);
        if rt > 0.0 && d_top > 0.0 {
            rt - (rt * rt - d_top * d_top).max(0.0).sqrt()
        } else if rb > 0.0 && d_bot > 0.0 {
            rb - (rb * rb - d_bot * d_bot).max(0.0).sqrt()
        } else {
            0.0
        }
    };

    let mut mesh = Mesh::default();
    for (i, &(y, c)) in rows.iter().enumerate() {
        let x = inset(y);
        mesh.colored_vertex(pos2(rect.left() + x, y), c);
        mesh.colored_vertex(pos2(rect.right() - x, y), c);
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    Shape::mesh(mesh)
}

/// Gradient fill plus an anti-aliased border (which also hides the mesh's
/// aliased edge on curved corners).
pub fn gradient_box(painter: &Painter, rect: Rect, radius: f32, stops: &[(f32, Color32)], border: Color32) {
    gradient(painter, rect, radius, radius, stops);
    painter.rect_stroke(rect, CornerRadius::same(radius.round() as u8), Stroke::new(1.0, border), StrokeKind::Inside);
}

/// Soft elliptical glow fading to transparent - Aero's text glow and button
/// hover bloom.
pub fn glow(painter: &Painter, center: Pos2, radius: Vec2, color: Color32) {
    const N: u32 = 32;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    let clear = alpha(color, 0.0);
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(center + vec2(a.cos() * radius.x, a.sin() * radius.y), clear);
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % N);
    }
    painter.add(Shape::mesh(mesh));
}

/// Simple line icons, drawn with strokes so they scale crisply at any DPI
/// and need no icon font.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Calendar,
    Quote,
    Palette,
    Close,
    Check,
    ChevronLeft,
    ChevronRight,
    Next,
    Plus,
    Minus,
    Folder,
    Import,
}

pub fn icon(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let s = Stroke::new((rect.height() / 9.0).max(1.2), color);
    let c = rect.center();
    let r = rect.height().min(rect.width()) / 2.0;
    let p = |x: f32, y: f32| pos2(c.x + x * r, c.y + y * r);
    match icon {
        Icon::Calendar => {
            let body = Rect::from_min_max(p(-0.8, -0.6), p(0.8, 0.85));
            painter.rect_stroke(body, CornerRadius::same(2), s, StrokeKind::Middle);
            painter.line_segment([p(-0.8, -0.2), p(0.8, -0.2)], s);
            painter.line_segment([p(-0.4, -0.85), p(-0.4, -0.45)], s);
            painter.line_segment([p(0.4, -0.85), p(0.4, -0.45)], s);
            for (x, y) in [(-0.4, 0.2), (0.0, 0.2), (0.4, 0.2), (-0.4, 0.55), (0.0, 0.55)] {
                painter.circle_filled(p(x, y), s.width * 0.7, color);
            }
        }
        Icon::Quote => {
            for dx in [-0.45, 0.35] {
                painter.circle_filled(p(dx, -0.1), r * 0.28, color);
                painter.line_segment([p(dx + 0.24, -0.05), p(dx - 0.1, 0.6)], Stroke::new(s.width * 1.3, color));
            }
        }
        Icon::Palette => {
            painter.circle_stroke(c, r * 0.85, s);
            for (x, y) in [(-0.35, -0.3), (0.1, -0.5), (0.45, -0.1), (-0.45, 0.2)] {
                painter.circle_filled(p(x, y), r * 0.13, color);
            }
        }
        Icon::Close => {
            painter.line_segment([p(-0.6, -0.6), p(0.6, 0.6)], s);
            painter.line_segment([p(0.6, -0.6), p(-0.6, 0.6)], s);
        }
        Icon::Check => {
            painter
                .add(Shape::line(vec![p(-0.65, 0.0), p(-0.2, 0.5), p(0.7, -0.55)], Stroke::new(s.width * 1.3, color)));
        }
        Icon::ChevronLeft => {
            painter.add(Shape::line(vec![p(0.25, -0.6), p(-0.3, 0.0), p(0.25, 0.6)], s));
        }
        Icon::ChevronRight => {
            painter.add(Shape::line(vec![p(-0.25, -0.6), p(0.3, 0.0), p(-0.25, 0.6)], s));
        }
        Icon::Next => {
            painter.add(Shape::convex_polygon(vec![p(-0.6, -0.6), p(0.3, 0.0), p(-0.6, 0.6)], color, Stroke::NONE));
            painter.line_segment([p(0.55, -0.6), p(0.55, 0.6)], Stroke::new(s.width * 1.3, color));
        }
        Icon::Plus => {
            painter.line_segment([p(-0.65, 0.0), p(0.65, 0.0)], s);
            painter.line_segment([p(0.0, -0.65), p(0.0, 0.65)], s);
        }
        Icon::Minus => {
            painter.line_segment([p(-0.65, 0.0), p(0.65, 0.0)], s);
        }
        Icon::Folder => {
            painter.add(Shape::closed_line(
                vec![p(-0.85, -0.55), p(-0.25, -0.55), p(-0.1, -0.35), p(0.85, -0.35), p(0.85, 0.65), p(-0.85, 0.65)],
                s,
            ));
        }
        Icon::Import => {
            painter.line_segment([p(0.0, -0.75), p(0.0, 0.3)], s);
            painter.add(Shape::line(vec![p(-0.4, -0.05), p(0.0, 0.35), p(0.4, -0.05)], s));
            painter.add(Shape::line(vec![p(-0.75, 0.25), p(-0.75, 0.75), p(0.75, 0.75), p(0.75, 0.25)], s));
        }
    }
}

/// The app's logo, painted live: a dark glass orb clock reading three
/// minutes to midnight, red minute hand. Used in the Aero caption and the
/// Modern nav header (the taskbar icon is rasterised separately).
pub fn logo(painter: &Painter, center: Pos2, r: f32) {
    gradient(
        painter,
        Rect::from_center_size(center, Vec2::splat(r * 2.0)),
        r,
        r,
        &[(0.0, hex(0x3A4A63)), (0.5, hex(0x121A27)), (1.0, hex(0x05080D))],
    );
    painter.circle_stroke(center, r - 0.5, Stroke::new(1.0, hex(0x7F95B5)));
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        let (inner, w) = if i % 3 == 0 { (0.68, 1.4) } else { (0.78, 0.8) };
        let dir = vec2(a.sin(), -a.cos());
        painter.line_segment([center + dir * r * inner, center + dir * r * 0.86], Stroke::new(w, hex(0xDDE6F2)));
    }
    let hand = |frac: f32, len: f32, w: f32, col: Color32| {
        let a = frac * std::f32::consts::TAU;
        painter.line_segment([center, center + vec2(a.sin(), -a.cos()) * r * len], Stroke::new(w, col));
    };
    hand(11.95 / 12.0, 0.45, r * 0.12, hex(0xF2F5F9));
    hand(57.0 / 60.0, 0.72, r * 0.08, hex(0xFF3B30));
    painter.circle_filled(center, r * 0.09, hex(0xFF3B30));
    // Glass reflection across the top half.
    gradient(
        painter,
        Rect::from_min_max(center + vec2(-r * 0.72, -r * 0.92), center + vec2(r * 0.72, -r * 0.05)),
        r * 0.7,
        r * 0.3,
        &[(0.0, Color32::from_white_alpha(90)), (1.0, Color32::from_white_alpha(0))],
    );
}
