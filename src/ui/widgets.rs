//! Skinned widgets. Each function draws the Windows 7 version or the
//! Windows 11 version of the same control from one call site, so page code
//! never branches on the skin.
//!
//! Hover/press states animate over ~120 ms (Windows 7 buttons cross-faded
//! their glow too). egui only repaints while an animation is running, so
//! an idle window still costs zero CPU.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind,
    TextEdit, Ui, UiBuilder, Vec2, epaint::RectShape, pos2, vec2,
};

use super::paint::{self, Icon, alpha, gradient, gradient_box, gradient_shape, hex, mix, rgba};
use super::theme::Theme;

pub type Stops = [(f32, Color32); 4];

fn lerp_stops(a: &Stops, b: &Stops, t: f32) -> Stops {
    std::array::from_fn(|i| (a[i].0, mix(a[i].1, b[i].1, t)))
}

fn hover_t(ui: &Ui, resp: &Response) -> f32 {
    ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Primary,
    Danger,
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

pub fn button(ui: &mut Ui, th: &Theme, label: &str, icon: Option<Icon>, kind: Kind) -> Response {
    let font = if kind == Kind::Normal { th.body() } else { th.strong() };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
    let min_w = if th.aero() { 84.0 } else { 96.0 };
    let size = vec2((galley.size().x + icon_w + 28.0).max(min_w), th.control_height());
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let t = hover_t(ui, &resp);
    let down = resp.is_pointer_button_down_on();
    let painter = ui.painter();
    let fg = if th.aero() {
        paint_aero_button(painter, rect, kind, t, down)
    } else {
        paint_modern_button(painter, th, rect, kind, t, down)
    };

    let content_w = galley.size().x + icon_w;
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(icon) = icon {
        paint::icon(painter, icon, Rect::from_center_size(pos2(x + 7.0, rect.center().y), Vec2::splat(13.0)), fg);
        x += icon_w;
    }
    let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0);
    if th.aero() && kind != Kind::Normal {
        // Etched text on glossy buttons: a dark shadow 1px below.
        painter.galley(text_pos + vec2(0.0, 1.0), galley.clone(), alpha(Color32::BLACK, 0.35));
    }
    painter.galley(text_pos, galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Windows 7 push buttons. Grey glass for normal buttons, glossy blue/red
/// "orb" style for primary/destructive actions. Returns the text colour.
fn paint_aero_button(p: &Painter, rect: Rect, kind: Kind, t: f32, down: bool) -> Color32 {
    let (normal, hover, pressed, border, border_hover, fg): (Stops, Stops, Stops, u32, u32, Color32) = match kind {
        Kind::Normal => (
            [(0.0, hex(0xF2F2F2)), (0.5, hex(0xEBEBEB)), (0.5, hex(0xDDDDDD)), (1.0, hex(0xCFCFCF))],
            [(0.0, hex(0xEAF6FD)), (0.5, hex(0xD9F0FC)), (0.5, hex(0xBEE6FD)), (1.0, hex(0xA7D9F5))],
            [(0.0, hex(0xE5F4FC)), (0.5, hex(0xC4E5F6)), (0.5, hex(0x98D1EF)), (1.0, hex(0x68B3DB))],
            0x707070,
            0x3C7FB1,
            hex(0x1E1E1E),
        ),
        Kind::Primary => (
            [(0.0, hex(0x8EC3F4)), (0.5, hex(0x4A93DF)), (0.5, hex(0x1F66BD)), (1.0, hex(0x3A8BE0))],
            [(0.0, hex(0xA9D4FA)), (0.5, hex(0x64A9EE)), (0.5, hex(0x2C7AD6)), (1.0, hex(0x56A6F5))],
            [(0.0, hex(0x6CA6DE)), (0.5, hex(0x3576C2)), (0.5, hex(0x174F96)), (1.0, hex(0x2A6CB5))],
            0x123F78,
            0x0E3566,
            Color32::WHITE,
        ),
        Kind::Danger => (
            [(0.0, hex(0xF2A9A9)), (0.5, hex(0xDE5A5A)), (0.5, hex(0xC22B2B)), (1.0, hex(0xD64747))],
            [(0.0, hex(0xF8C0C0)), (0.5, hex(0xEC7070)), (0.5, hex(0xD63A3A)), (1.0, hex(0xEC6262))],
            [(0.0, hex(0xD98C8C)), (0.5, hex(0xC24646)), (0.5, hex(0x9E1D1D)), (1.0, hex(0xB83636))],
            0x6E1414,
            0x5A0F0F,
            Color32::WHITE,
        ),
    };
    let stops = if down { pressed } else { lerp_stops(&normal, &hover, t) };
    let border = mix(hex(border), hex(border_hover), t);
    gradient_box(p, rect, 3.0, &stops, border);
    // Inner 1px highlight: the bevel that makes Win7 buttons look raised.
    p.rect_stroke(
        rect.shrink(1.0),
        CornerRadius::same(2),
        Stroke::new(1.0, alpha(Color32::WHITE, if down { 0.25 } else { 0.6 })),
        StrokeKind::Inside,
    );
    if t > 0.0 && !down {
        // Hover bloom rising from the bottom edge.
        let glow_col = if kind == Kind::Normal { rgba(0xFFFFFF, 0.55 * t) } else { rgba(0xFFFFFF, 0.35 * t) };
        paint::glow(
            &p.with_clip_rect(rect.shrink(1.0)),
            pos2(rect.center().x, rect.bottom()),
            vec2(rect.width() * 0.5, rect.height() * 0.6),
            glow_col,
        );
    }
    fg
}

fn paint_modern_button(p: &Painter, th: &Theme, rect: Rect, kind: Kind, t: f32, down: bool) -> Color32 {
    let r = CornerRadius::same(th.radius() as u8);
    let (fill, fg) = match kind {
        Kind::Normal => {
            let base = if th.dark { rgba(0xFFFFFF, 0.06) } else { rgba(0xFFFFFF, 0.70) };
            let hov = if th.dark { rgba(0xFFFFFF, 0.085) } else { rgba(0xF9F9F9, 0.50) };
            (
                if down {
                    if th.dark { rgba(0xFFFFFF, 0.03) } else { rgba(0xF9F9F9, 0.30) }
                } else {
                    mix(base, hov, t)
                },
                if down { th.c.text_dim } else { th.c.text },
            )
        }
        Kind::Primary => {
            let hov = if th.dark { mix(th.accent, Color32::BLACK, 0.1) } else { mix(th.accent, Color32::WHITE, 0.1) };
            (if down { alpha(th.accent, 0.8) } else { mix(th.accent, hov, t) }, th.c.on_accent)
        }
        Kind::Danger => {
            let base = if th.dark { hex(0xC42B1C) } else { th.c.danger };
            (if down { alpha(base, 0.8) } else { mix(base, mix(base, Color32::WHITE, 0.1), t) }, Color32::WHITE)
        }
    };
    p.rect_filled(rect, r, fill);
    // Fluent "elevation" border: faint all round, a touch darker at the bottom.
    let edge = if kind == Kind::Normal { th.c.card_border } else { alpha(Color32::WHITE, 0.08) };
    p.rect_stroke(rect, r, Stroke::new(1.0, edge), StrokeKind::Inside);
    if !down {
        let shade = if th.dark { rgba(0x000000, 0.25) } else { rgba(0x000000, 0.12) };
        p.line_segment(
            [pos2(rect.left() + 3.0, rect.bottom() - 0.5), pos2(rect.right() - 3.0, rect.bottom() - 0.5)],
            Stroke::new(1.0, shade),
        );
    }
    fg
}

/// Square push button with just an icon (the spin box's - and +).
pub fn square_button(ui: &mut Ui, th: &Theme, icon: Icon, tooltip: &str) -> Response {
    let s = th.control_height();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(s), Sense::click());
    let t = hover_t(ui, &resp);
    let down = resp.is_pointer_button_down_on();
    let fg = if th.aero() {
        paint_aero_button(ui.painter(), rect, Kind::Normal, t, down)
    } else {
        paint_modern_button(ui.painter(), th, rect, Kind::Normal, t, down)
    };
    paint::icon(ui.painter(), icon, rect.shrink(s * 0.33), fg);
    resp.on_hover_text(tooltip)
}

/// Small square icon-only button (delete, calendar arrows).
pub fn icon_button(ui: &mut Ui, th: &Theme, icon: Icon, tooltip: &str) -> Response {
    let s = if th.aero() { 22.0 } else { 28.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(s), Sense::click());
    let t = hover_t(ui, &resp);
    let p = ui.painter();
    if t > 0.0 {
        if th.aero() {
            let stops = [(0.0, hex(0xFAFBFD)), (0.5, hex(0xF0F6FD)), (0.5, hex(0xE3EEFC)), (1.0, hex(0xEBF3FD))];
            gradient(&p.with_clip_rect(rect), rect, 3.0, 3.0, &stops.map(|(o, c)| (o, alpha(c, t))));
            p.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0, alpha(hex(0xB8D6FB), t)), StrokeKind::Inside);
        } else {
            p.rect_filled(rect, CornerRadius::same(4), alpha(th.c.hover, t * 2.0));
        }
    }
    let col = if icon == Icon::Close && resp.hovered() { th.c.danger } else { th.c.text_dim };
    paint::icon(p, icon, rect.shrink(s * 0.3), col);
    resp.on_hover_text(tooltip)
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

pub fn nav_item(ui: &mut Ui, th: &Theme, icon: Icon, label: &str, selected: bool) -> Response {
    let h = if th.aero() { 30.0 } else { 36.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let t = hover_t(ui, &resp);
    let p = ui.painter();
    if th.aero() {
        // Windows 7 Explorer selection: blue gradient with a crisp border.
        if selected {
            gradient_box(p, rect, 3.0, &[(0.0, hex(0xDCEBFC)), (1.0, hex(0xC1DBFC))], hex(0x7DA2CE));
            p.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(2),
                Stroke::new(1.0, alpha(Color32::WHITE, 0.6)),
                StrokeKind::Inside,
            );
        } else if t > 0.0 {
            gradient(p, rect, 3.0, 3.0, &[(0.0, alpha(hex(0xFAFBFD), t)), (1.0, alpha(hex(0xEBF3FD), t))]);
            p.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0, alpha(hex(0xB8D6FB), t)), StrokeKind::Inside);
        }
    } else {
        let fill = if selected { th.c.selected } else { alpha(th.c.hover, t * 0.04 / 0.04) };
        if selected || t > 0.0 {
            p.rect_filled(
                rect,
                CornerRadius::same(4),
                if selected { fill } else { mix(Color32::TRANSPARENT, th.c.hover, t) },
            );
        }
        if selected {
            let pill = Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, 16.0));
            p.rect_filled(pill, CornerRadius::same(2), th.accent);
        }
    }
    let fg = if th.aero() { hex(0x1E3287) } else { th.c.text };
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(15.0));
    paint::icon(p, icon, icon_rect, if th.aero() { hex(0x2D5FA8) } else { fg });
    let font = if selected && th.aero() { th.strong() } else { th.body() };
    p.text(pos2(rect.left() + 38.0, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

/// A titled container. Aero: a white group panel with a soft blue header
/// wash and a gradient rule under the title. Modern: a Fluent card.
pub fn card<R>(ui: &mut Ui, th: &Theme, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let slot = ui.painter().add(Shape::Noop);
    let pad = if th.aero() { 12.0 } else { 16.0 };
    let outer = ui.available_rect_before_wrap();
    let inner_max = Rect::from_min_max(outer.min + vec2(pad, pad), pos2(outer.right() - pad, outer.bottom()));
    let mut title_bottom = None;
    let inner = ui.scope_builder(UiBuilder::new().max_rect(inner_max), |ui| {
        if let Some(title) = title {
            let r = ui.label(egui::RichText::new(title).font(th.subtitle()).color(th.c.heading));
            title_bottom = Some(r.rect.bottom());
            ui.add_space(if th.aero() { 4.0 } else { 2.0 });
        }
        add(ui)
    });
    let rect = Rect::from_min_max(outer.min, pos2(outer.right(), inner.response.rect.bottom() + pad));
    ui.allocate_rect(rect, Sense::hover());

    let radius = th.card_radius();
    let mut shapes = Vec::new();
    if th.aero() {
        let wash = (34.0 / rect.height()).min(1.0);
        shapes.push(gradient_shape(
            rect,
            radius,
            radius,
            &[(0.0, hex(0xF3F7FC)), (wash, hex(0xFFFFFF)), (1.0, hex(0xFFFFFF))],
        ));
        shapes.push(Shape::Rect(RectShape::stroke(
            rect,
            CornerRadius::same(radius as u8),
            Stroke::new(1.0, th.c.card_border),
            StrokeKind::Inside,
        )));
        if let Some(y) = title_bottom {
            let y = y + 3.0;
            let line = Rect::from_min_max(pos2(rect.left() + pad, y), pos2(rect.right() - pad, y + 1.0));
            shapes.push(horizontal_fade(line, hex(0x7DA2CE)));
        }
    } else {
        shapes.push(Shape::Rect(RectShape::filled(rect, CornerRadius::same(radius as u8), th.c.card)));
        shapes.push(Shape::Rect(RectShape::stroke(
            rect,
            CornerRadius::same(radius as u8),
            Stroke::new(1.0, th.c.card_border),
            StrokeKind::Inside,
        )));
    }
    ui.painter().set(slot, Shape::Vec(shapes));
    inner.inner
}

/// A 1px line that is solid on the left and fades out to the right.
fn horizontal_fade(rect: Rect, color: Color32) -> Shape {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), color);
    mesh.colored_vertex(rect.right_top(), alpha(color, 0.0));
    mesh.colored_vertex(rect.right_bottom(), alpha(color, 0.0));
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    Shape::mesh(mesh)
}

pub fn caption(ui: &mut Ui, th: &Theme, text: &str) {
    ui.label(egui::RichText::new(text).font(th.small()).color(th.c.text_dim));
}

// ---------------------------------------------------------------------------
// Choices: radio groups (Aero) / segmented control (Modern)
// ---------------------------------------------------------------------------

pub fn choice<T: PartialEq + Copy>(ui: &mut Ui, th: &Theme, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    if th.aero() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            for &(v, label) in options {
                if radio(ui, th, *value == v, label).clicked() && *value != v {
                    *value = v;
                    changed = true;
                }
            }
        });
        return changed;
    }

    let h = 32.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(5), if th.dark { rgba(0x000000, 0.18) } else { rgba(0x000000, 0.04) });
    p.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, th.c.card_border), StrokeKind::Inside);
    let seg_w = (w - 4.0) / options.len() as f32;
    for (i, &(v, label)) in options.iter().enumerate() {
        let seg =
            Rect::from_min_size(pos2(rect.left() + 2.0 + i as f32 * seg_w, rect.top() + 2.0), vec2(seg_w, h - 4.0));
        let resp = ui.interact(seg, ui.id().with(("seg", i)), Sense::click());
        let selected = *value == v;
        let t = hover_t(ui, &resp);
        let p = ui.painter();
        if selected {
            p.rect_filled(seg, CornerRadius::same(4), if th.dark { rgba(0xFFFFFF, 0.09) } else { hex(0xFFFFFF) });
            p.rect_stroke(seg, CornerRadius::same(4), Stroke::new(1.0, th.c.card_border), StrokeKind::Inside);
            let pill = Rect::from_center_size(pos2(seg.center().x, seg.bottom() - 2.0), vec2(16.0, 3.0));
            p.rect_filled(pill, CornerRadius::same(2), th.accent);
        } else if t > 0.0 {
            p.rect_filled(seg, CornerRadius::same(4), mix(Color32::TRANSPARENT, th.c.hover, t));
        }
        let font = if selected { th.strong() } else { th.body() };
        p.text(
            seg.center() - vec2(0.0, 1.0),
            Align2::CENTER_CENTER,
            label,
            font,
            if selected { th.c.text } else { th.c.text_dim },
        );
        if resp.clicked() && !selected {
            *value = v;
            changed = true;
        }
    }
    changed
}

pub fn radio(ui: &mut Ui, th: &Theme, selected: bool, label: &str) -> Response {
    let font = th.body();
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, th.c.text);
    let d = if th.aero() { 13.0 } else { 20.0 };
    let size = vec2(d + 7.0 + galley.size().x, d.max(galley.size().y));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = hover_t(ui, &resp);
    let p = ui.painter();
    let c = pos2(rect.left() + d / 2.0, rect.center().y);
    if th.aero() {
        let well = Rect::from_center_size(c, Vec2::splat(d));
        gradient(
            p,
            well,
            d / 2.0,
            d / 2.0,
            &[(0.0, mix(hex(0xD6D6D6), hex(0xC5E3F7), t)), (1.0, mix(hex(0xFBFBFB), hex(0xEAF6FD), t))],
        );
        p.circle_stroke(c, d / 2.0 - 0.5, Stroke::new(1.0, mix(hex(0x8E8F8F), hex(0x3C7FB1), t)));
        if selected {
            let dot = Rect::from_center_size(c, Vec2::splat(7.0));
            gradient(p, dot, 3.5, 3.5, &[(0.0, hex(0x9CD6FA)), (0.5, hex(0x3D9CE8)), (1.0, hex(0x185DAA))]);
            p.circle_filled(c - vec2(1.0, 1.5), 1.3, alpha(Color32::WHITE, 0.8));
        }
    } else if selected {
        p.circle_filled(c, d / 2.0, th.accent);
        p.circle_filled(c, if resp.hovered() { 5.0 } else { 4.0 }, th.c.on_accent);
    } else {
        p.circle_filled(c, d / 2.0 - 0.5, mix(th.c.field_bg, th.c.hover, t));
        p.circle_stroke(c, d / 2.0 - 0.5, Stroke::new(1.0, th.c.text_dim));
    }
    p.galley(pos2(rect.left() + d + 7.0, rect.center().y - galley.size().y / 2.0), galley, th.c.text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// On/off: a glossy checkbox in Aero, a Fluent toggle switch in Modern.
pub fn toggle(ui: &mut Ui, th: &Theme, on: &mut bool, label: &str) -> bool {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), th.body(), th.c.text);
    let (bw, bh): (f32, f32) = if th.aero() { (13.0, 13.0) } else { (40.0, 20.0) };
    let size = vec2(bw + 10.0 + galley.size().x, bh.max(galley.size().y));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = hover_t(ui, &resp);
    let anim = ui.ctx().animate_bool_with_time(resp.id.with("on"), *on, 0.12);
    let p = ui.painter();
    let b = Rect::from_min_size(pos2(rect.left(), rect.center().y - bh / 2.0), vec2(bw, bh));
    if th.aero() {
        gradient(
            p,
            b,
            0.0,
            0.0,
            &[(0.0, mix(hex(0xDCDCDC), hex(0xC5E3F7), t)), (1.0, mix(hex(0xFFFFFF), hex(0xEAF6FD), t))],
        );
        p.rect_stroke(
            b,
            CornerRadius::ZERO,
            Stroke::new(1.0, mix(hex(0x8E8F8F), hex(0x3C7FB1), t)),
            StrokeKind::Inside,
        );
        if *on {
            paint::icon(p, Icon::Check, b.shrink(1.5), hex(0x2B4A80));
        }
    } else {
        let r = CornerRadius::same(10);
        if anim > 0.5 {
            p.rect_filled(b, r, th.accent);
        } else {
            p.rect_filled(b, r, mix(Color32::TRANSPARENT, th.c.hover, t));
            p.rect_stroke(b, r, Stroke::new(1.0, th.c.text_dim), StrokeKind::Inside);
        }
        let x = egui::lerp(b.left() + 10.0..=b.right() - 10.0, anim);
        let knob = if anim > 0.5 { th.c.on_accent } else { th.c.text_dim };
        p.circle_filled(pos2(x, b.center().y), if resp.hovered() { 7.0 } else { 6.0 }, knob);
    }
    p.galley(pos2(b.right() + 10.0, rect.center().y - galley.size().y / 2.0), galley, th.c.text);
    resp.clicked()
}

/// A number box with -/+ buttons (Windows "spin box"), replacing egui's
/// drag-to-change number which neither Windows 7 nor 11 users expect.
pub fn stepper(ui: &mut Ui, th: &Theme, value: &mut u32, min: u32, max: u32) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if square_button(ui, th, Icon::Minus, "Fewer").clicked() {
            *value = value.saturating_sub(1).max(min);
        }
        let mut text = value.to_string();
        let resp = text_field(ui, th, &mut text, "", 52.0, 1);
        if resp.changed() {
            if let Ok(v) = text.trim().parse::<u32>() {
                *value = v.clamp(min, max);
            } else if text.trim().is_empty() {
                *value = min;
            }
        }
        if square_button(ui, th, Icon::Plus, "More").clicked() {
            *value = (*value + 1).min(max);
        }
    });
    *value != before
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

/// Windows 7's glossy progress bar (green, then yellow, then red as the
/// deadline nears), or the thin Windows 11 accent bar.
pub fn progress(p: &Painter, th: &Theme, rect: Rect, frac: f32) {
    let frac = frac.clamp(0.0, 1.0);
    if th.aero() {
        gradient_box(p, rect, 2.0, &[(0.0, hex(0xE4E4E4)), (1.0, hex(0xF7F7F7))], hex(0xA9B3BF));
        let inner = rect.shrink(1.0);
        let bar = Rect::from_min_size(inner.min, vec2(inner.width() * frac, inner.height()));
        let stops: Stops = if frac < 0.75 {
            [(0.0, hex(0xB6EFAB)), (0.45, hex(0x46D041)), (0.5, hex(0x16B016)), (1.0, hex(0x37C931))]
        } else if frac < 0.9 {
            [(0.0, hex(0xFCEDB0)), (0.45, hex(0xF3D13C)), (0.5, hex(0xE0B40C)), (1.0, hex(0xF0CF35))]
        } else {
            [(0.0, hex(0xF7B7B7)), (0.45, hex(0xE24A4A)), (0.5, hex(0xC81A1A)), (1.0, hex(0xE23B3B))]
        };
        if bar.width() > 1.0 {
            gradient(p, bar, 1.0, 1.0, &stops);
            paint::glow(
                &p.with_clip_rect(bar),
                pos2(bar.right(), bar.center().y),
                vec2(14.0, inner.height()),
                rgba(0xFFFFFF, 0.45),
            );
        }
    } else {
        let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 1.0));
        p.rect_filled(track, CornerRadius::ZERO, alpha(th.c.text_dim, 0.5));
        let bar = Rect::from_min_size(pos2(rect.left(), rect.center().y - 1.5), vec2(rect.width() * frac, 3.0));
        let col = if frac >= 0.9 {
            th.c.danger
        } else if frac >= 0.75 {
            th.c.warn
        } else {
            th.accent
        };
        p.rect_filled(bar, CornerRadius::same(2), col);
    }
}

// ---------------------------------------------------------------------------
// Text fields
// ---------------------------------------------------------------------------

/// A themed text box. egui does the editing; we only paint the chrome
/// around it (Windows 7 sunken box / Fluent field with accent underline).
pub fn text_field(ui: &mut Ui, th: &Theme, buf: &mut String, hint: &str, width: f32, rows: usize) -> Response {
    let line_h = ui.fonts_mut(|f| f.row_height(&th.body()));
    let pad = vec2(7.0, if th.aero() { 4.0 } else { 6.0 });
    let h = (line_h * rows as f32 + pad.y * 2.0).max(th.control_height());
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let slot = ui.painter().add(Shape::Noop);
    let edit = if rows > 1 { TextEdit::multiline(buf).desired_rows(rows) } else { TextEdit::singleline(buf) }
        .frame(egui::Frame::NONE)
        .hint_text(egui::RichText::new(hint).color(alpha(th.c.text_dim, 0.8)))
        .font(th.body())
        .text_color(th.c.text)
        .margin(egui::Margin::ZERO)
        .desired_width(width - pad.x * 2.0);
    let inner = Rect::from_min_size(rect.min + pad, vec2(width - pad.x * 2.0, h - pad.y * 2.0));
    let resp = ui.put(inner, edit);
    let focused = resp.has_focus();
    let t = hover_t(ui, &resp);

    let mut shapes = Vec::new();
    if th.aero() {
        shapes.push(Shape::Rect(RectShape::filled(rect, CornerRadius::same(2), th.c.field_bg)));
        let border = if focused { hex(0x3D7BAD) } else { mix(th.c.field_border, hex(0x5794BF), t) };
        shapes.push(Shape::Rect(RectShape::stroke(
            rect,
            CornerRadius::same(2),
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        )));
        // Win7 text boxes are lit from above: a slightly darker top edge.
        shapes.push(Shape::line_segment(
            [rect.left_top() + vec2(1.0, 0.5), rect.right_top() + vec2(-1.0, 0.5)],
            Stroke::new(1.0, alpha(border, 0.9)),
        ));
    } else {
        let fill = if focused {
            if th.dark { hex(0x1F1F1F) } else { hex(0xFFFFFF) }
        } else {
            mix(th.c.field_bg, if th.dark { rgba(0xFFFFFF, 0.08) } else { rgba(0xF9F9F9, 0.9) }, t)
        };
        shapes.push(Shape::Rect(RectShape::filled(rect, CornerRadius::same(4), fill)));
        shapes.push(Shape::Rect(RectShape::stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, th.c.field_border),
            StrokeKind::Inside,
        )));
        let (w, col) = if focused { (2.0, th.accent) } else { (1.0, alpha(th.c.text_dim, 0.7)) };
        let y = rect.bottom() - w / 2.0;
        shapes
            .push(Shape::line_segment([pos2(rect.left() + 2.0, y), pos2(rect.right() - 2.0, y)], Stroke::new(w, col)));
    }
    ui.painter().set(slot, Shape::Vec(shapes));
    resp
}

pub fn badge(p: &Painter, th: &Theme, right_center: Pos2, text: &str) -> Rect {
    let font = FontId::new(10.0, super::theme::semi());
    let g = p.layout_no_wrap(text.to_owned(), font, Color32::WHITE);
    let rect = Rect::from_min_size(
        pos2(right_center.x - g.size().x - 10.0, right_center.y - 8.0),
        vec2(g.size().x + 10.0, 16.0),
    );
    if th.aero() {
        gradient_box(
            p,
            rect,
            3.0,
            &[(0.0, hex(0xFFC46B)), (0.5, hex(0xF59B1B)), (0.5, hex(0xE58200)), (1.0, hex(0xF39A1E))],
            hex(0xA35C00),
        );
    } else {
        p.rect_filled(rect, CornerRadius::same(8), th.accent);
    }
    let fg = if th.aero() { Color32::WHITE } else { th.c.on_accent };
    p.galley(pos2(rect.left() + 5.0, rect.center().y - g.size().y / 2.0), g, fg);
    rect
}
