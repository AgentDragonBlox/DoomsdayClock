//! Draws the wallpaper: black canvas, huge countdown headline, optional
//! wrapped quote underneath. CPU-only (tiny-skia), so the scheduled run
//! never initialises a GPU, a window, or a UI toolkit.
//!
//! Text pipeline, kept deliberately small:
//! 1. map chars -> glyph ids (with sane substitutes for missing glyphs),
//! 2. lay out with advance widths + `kern` table pairs,
//! 3. append every glyph outline of a line into ONE path,
//! 4. fill that path once with anti-aliasing.
//!
//! One fill per line instead of one per glyph keeps the rasteriser's setup
//! cost constant no matter how long the quote is.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Rect, SpreadMode, Transform,
};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::fonts::LoadedFont;

pub struct Scene<'a> {
    pub headline: &'a str,
    pub color: [u8; 3],
    pub quote: Option<(&'a str, &'a str)>,
}

pub struct Fonts<'a> {
    pub headline: &'a LoadedFont,
    pub quote: &'a LoadedFont,
}

/// Max lines a quote may wrap to before its font size is reduced.
const MAX_QUOTE_LINES: usize = 5;

pub fn render(scene: &Scene, fonts: &Fonts, width: u32, height: u32) -> Pixmap {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("non-zero size");
    pixmap.fill(Color::BLACK);
    let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);

    let head_face = fonts.headline.face();
    let head = Line::shape(&head_face, scene.headline);

    // Headline size is solved directly (width is linear in font size), no
    // trial-and-error search needed: as wide as 84% of the screen allows,
    // capped by a share of the screen height.
    let max_head_px = if scene.quote.is_some() { h * 0.15 } else { h * 0.2 };
    let head_px = if head.width_units > 0.0 {
        ((w * 0.84) * head_face.units_per_em() as f32 / head.width_units).min(max_head_px)
    } else {
        max_head_px
    };
    let head_metrics = Metrics::of(&head_face, head_px);

    // Quote block: wrap, shrinking the font until it fits the line budget.
    let quote_face = fonts.quote.face();
    let mut quote_block = None;
    if let Some((text, author)) = scene.quote {
        let decorated = format!("\u{201C}{text}\u{201D}");
        let max_w = w * 0.62;
        let mut px = (h * 0.03).max(10.0);
        let min_px = (h * 0.017).max(9.0);
        let lines = loop {
            let lines = wrap(&quote_face, &decorated, px, max_w);
            if lines.len() <= MAX_QUOTE_LINES || px <= min_px {
                break lines;
            }
            px *= 0.9;
        };
        let author = (!author.is_empty()).then(|| Line::shape(&quote_face, &format!("\u{2014} {author}")));
        quote_block = Some((lines, author, px));
    }

    // Vertical layout: centre the whole group (headline + rule + quote).
    let head_h = head_metrics.ascent + head_metrics.descent;
    let mut total = head_h;
    let (gap, rule_gap) = (h * 0.045, h * 0.035);
    if let Some((lines, author, px)) = &quote_block {
        let m = Metrics::of(&quote_face, *px);
        total += gap + rule_gap + lines.len() as f32 * m.line_height;
        if author.is_some() {
            total += m.line_height * 1.35;
        }
    }
    let mut y = (h - total) / 2.0;

    let color = scene.color;
    head.fill(
        &mut pixmap,
        &head_face,
        head_px,
        (w - head.width_px(&head_face, head_px)) / 2.0,
        y + head_metrics.ascent,
        color,
    );
    y += head_h;

    if let Some((lines, author, px)) = quote_block {
        let m = Metrics::of(&quote_face, px);
        y += gap;
        draw_rule(&mut pixmap, w / 2.0, y, w * 0.14, crate::countdown::dim(color, 0.6));
        y += rule_gap;
        let quote_color = crate::countdown::dim(color, 0.80);
        for line in &lines {
            line.fill(
                &mut pixmap,
                &quote_face,
                px,
                (w - line.width_px(&quote_face, px)) / 2.0,
                y + m.ascent,
                quote_color,
            );
            y += m.line_height;
        }
        if let Some(author) = author {
            y += m.line_height * 0.35;
            let apx = px * 0.82;
            let am = Metrics::of(&quote_face, apx);
            author.fill(
                &mut pixmap,
                &quote_face,
                apx,
                (w - author.width_px(&quote_face, apx)) / 2.0,
                y + am.ascent,
                crate::countdown::dim(color, 0.55),
            );
        }
    }

    pixmap
}

/// A thin horizontal rule that fades out at both ends.
fn draw_rule(pixmap: &mut Pixmap, cx: f32, y: f32, half_w: f32, c: [u8; 3]) {
    let Some(rect) = Rect::from_xywh(cx - half_w, y, half_w * 2.0, (pixmap.height() as f32 / 1080.0).max(1.0)) else {
        return;
    };
    let solid = Color::from_rgba8(c[0], c[1], c[2], 255);
    let clear = Color::from_rgba8(c[0], c[1], c[2], 0);
    let stops = vec![GradientStop::new(0.0, clear), GradientStop::new(0.5, solid), GradientStop::new(1.0, clear)];
    if let Some(shader) = LinearGradient::new(
        Point::from_xy(cx - half_w, y),
        Point::from_xy(cx + half_w, y),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let paint = Paint { shader, anti_alias: true, ..Default::default() };
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

struct Metrics {
    ascent: f32,
    descent: f32,
    line_height: f32,
}

impl Metrics {
    fn of(face: &Face, px: f32) -> Self {
        let s = px / face.units_per_em() as f32;
        let ascent = face.ascender() as f32 * s;
        let descent = -(face.descender() as f32) * s;
        Self { ascent, descent, line_height: (ascent + descent + face.line_gap() as f32 * s) * 1.08 }
    }
}

/// A run of positioned glyphs, in font units.
struct Line {
    glyphs: Vec<(GlyphId, f32)>,
    width_units: f32,
}

impl Line {
    fn shape(face: &Face, text: &str) -> Self {
        let mut glyphs = Vec::with_capacity(text.len());
        let mut x = 0.0;
        let mut prev: Option<GlyphId> = None;
        for ch in text.chars() {
            let gid = glyph_for(face, ch);
            if let Some(p) = prev {
                x += kerning(face, p, gid);
            }
            glyphs.push((gid, x));
            x += face.glyph_hor_advance(gid).unwrap_or(0) as f32;
            prev = Some(gid);
        }
        Self { glyphs, width_units: x }
    }

    fn width_px(&self, face: &Face, px: f32) -> f32 {
        self.width_units * px / face.units_per_em() as f32
    }

    fn fill(&self, pixmap: &mut Pixmap, face: &Face, px: f32, x: f32, baseline: f32, c: [u8; 3]) {
        let scale = px / face.units_per_em() as f32;
        let mut sink = Sink { pb: PathBuilder::new(), scale, ox: 0.0, oy: baseline };
        for &(gid, gx) in &self.glyphs {
            sink.ox = x + gx * scale;
            face.outline_glyph(gid, &mut sink);
        }
        if let Some(path) = sink.pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(c[0], c[1], c[2], 255);
            paint.anti_alias = true;
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// Greedy word wrap. Words longer than a line are left whole (they just
/// overflow slightly) rather than being broken mid-word.
fn wrap(face: &Face, text: &str, px: f32, max_w: f32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if !current.is_empty() && Line::shape(face, &candidate).width_px(face, px) > max_w {
            lines.push(Line::shape(face, &current));
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(Line::shape(face, &current));
    }
    lines
}

/// Typographic characters many fonts lack get a plain substitute rather
/// than rendering as an empty box.
fn glyph_for(face: &Face, ch: char) -> GlyphId {
    if let Some(g) = face.glyph_index(ch) {
        return g;
    }
    let substitute = match ch {
        '\u{201C}' | '\u{201D}' | '\u{201E}' => '"',
        '\u{2018}' | '\u{2019}' => '\'',
        '\u{2014}' | '\u{2013}' => '-',
        '\u{2026}' => '.',
        '\u{00A0}' => ' ',
        _ => '?',
    };
    face.glyph_index(substitute).unwrap_or(GlyphId(0))
}

fn kerning(face: &Face, left: GlyphId, right: GlyphId) -> f32 {
    let Some(kern) = face.tables().kern else { return 0.0 };
    kern.subtables
        .into_iter()
        .filter(|st| st.horizontal && !st.variable)
        .find_map(|st| st.glyphs_kerning(left, right))
        .map_or(0.0, |v| v as f32)
}

/// Bridges ttf-parser's outline callbacks into a tiny-skia path, flipping
/// the font's y-up coordinates into the image's y-down space.
struct Sink {
    pb: PathBuilder,
    scale: f32,
    ox: f32,
    oy: f32,
}

impl Sink {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        (self.ox + x * self.scale, self.oy - y * self.scale)
    }
}

impl OutlineBuilder for Sink {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.p(x, y);
        self.pb.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.p(x, y);
        self.pb.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.p(x1, y1);
        let (x, y) = self.p(x, y);
        self.pb.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.p(x1, y1);
        let (x2, y2) = self.p(x2, y2);
        let (x, y) = self.p(x, y);
        self.pb.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

/// Writes a 24-bit BMP. Hand-rolled (~25 lines) instead of pulling in an
/// image-codec crate: BMP is the one format every Windows wallpaper API has
/// accepted since Windows 95, and the file never leaves our cache folder.
pub fn write_bmp(pixmap: &Pixmap, path: &std::path::Path) -> std::io::Result<()> {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let row = (w * 3 + 3) & !3;
    let image_size = row * h;
    let mut out = Vec::with_capacity(54 + image_size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + image_size) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes()); // positive = bottom-up rows
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]); // BI_RGB, uncompressed
    out.extend_from_slice(&(image_size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]); // resolution + palette fields: unused
    let data = pixmap.data();
    for y in (0..h).rev() {
        let start = out.len();
        for px in data[y * w * 4..(y + 1) * w * 4].chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        out.resize(start + row, 0);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{FontLibrary, Role};

    fn fonts() -> (LoadedFont, LoadedFont) {
        let dir = std::env::temp_dir().join(format!("ddc-render-test-{}", std::process::id()));
        let mut lib = FontLibrary::new(dir.join("c.json"), false);
        (
            lib.resolve("Gill Sans Nova Ultra Bold", Role::Countdown).unwrap(),
            lib.resolve("Gill Sans Nova", Role::Quote).unwrap(),
        )
    }

    #[test]
    fn renders_text_centred_and_inside_canvas() {
        let (hf, qf) = fonts();
        let scene = Scene {
            headline: "47 Days left till 25-12-2026",
            color: [255, 255, 255],
            quote: Some((
                "Well begun is half done, and the other half is mostly showing up again tomorrow.",
                "Aristotle",
            )),
        };
        let pm = render(&scene, &Fonts { headline: &hf, quote: &qf }, 1280, 720);
        let lit: Vec<(u32, u32)> = (0..720u32)
            .flat_map(|y| (0..1280u32).map(move |x| (x, y)))
            .filter(|&(x, y)| pm.pixel(x, y).unwrap().red() > 128)
            .collect();
        assert!(lit.len() > 5_000, "expected visible text, got {} lit pixels", lit.len());
        let (minx, maxx) = (lit.iter().map(|p| p.0).min().unwrap(), lit.iter().map(|p| p.0).max().unwrap());
        assert!(minx > 40 && maxx < 1240, "text should keep a margin: {minx}..{maxx}");
        let centre = (minx + maxx) as i32 / 2;
        assert!((centre - 640).abs() < 20, "text should be horizontally centred, centre={centre}");
    }

    #[test]
    fn bmp_header_is_valid() {
        let pm = Pixmap::new(3, 2).unwrap();
        let path = std::env::temp_dir().join(format!("ddc-{}.bmp", std::process::id()));
        write_bmp(&pm, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..2], b"BM");
        assert_eq!(bytes.len(), 54 + 12 * 2); // 3px*3B = 9 -> padded to 12 per row
        let _ = std::fs::remove_file(path);
    }
}
