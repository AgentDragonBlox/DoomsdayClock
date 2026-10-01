//! The two skins as data: colours, type ramp, metrics. Widgets read from a
//! `Theme` and never hard-code a colour, so re-theming either skin is a
//! matter of editing the tables below.
//!
//! Aero colours are sampled from Windows 7's own controls (push buttons,
//! Explorer selection, Control Panel task pane, progress bars). Modern
//! colours follow the WinUI 3 / Fluent 2 token values Windows 11 uses.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, ThemePreference, Visuals, vec2,
};

use super::paint::{alpha, hex, mix, rgba};
use crate::config::Skin;
use crate::platform::{self, UiFont};

#[derive(Clone, Debug)]
pub struct Theme {
    pub skin: Skin,
    pub dark: bool,
    pub accent: Color32,
    pub c: Colors,
}

#[derive(Clone, Debug)]
pub struct Colors {
    pub text: Color32,
    pub text_dim: Color32,
    pub heading: Color32,
    pub link: Color32,
    /// Opaque base used when there's no OS backdrop.
    pub base: Color32,
    pub card: Color32,
    pub card_border: Color32,
    pub field_bg: Color32,
    pub field_border: Color32,
    pub divider: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub on_accent: Color32,
}

impl Theme {
    pub fn new(skin: Skin, dark: bool, system_accent: Option<[u8; 3]>) -> Self {
        let dark = dark && skin == Skin::Modern; // Aero is always the light glass look.
        let accent = match (skin, system_accent) {
            (Skin::Aero, _) => hex(0x3C7FB1),
            (Skin::Modern, Some([r, g, b])) if dark => mix(Color32::from_rgb(r, g, b), Color32::WHITE, 0.35),
            (Skin::Modern, Some([r, g, b])) => Color32::from_rgb(r, g, b),
            (Skin::Modern, None) if dark => hex(0x4CC2FF),
            (Skin::Modern, None) => hex(0x005FB8),
        };
        let c = match (skin, dark) {
            (Skin::Aero, _) => Colors {
                text: hex(0x1E1E1E),
                text_dim: hex(0x5A6779),
                heading: hex(0x1E3287),
                link: hex(0x0066CC),
                base: hex(0xFFFFFF),
                card: hex(0xFFFFFF),
                card_border: hex(0xCCD9E6),
                field_bg: hex(0xFFFFFF),
                field_border: hex(0xABADB3),
                divider: hex(0xD5DFE5),
                hover: hex(0xEBF3FD),
                selected: hex(0xCCE4FC),
                danger: hex(0xC62828),
                success: hex(0x1E8C1E),
                warn: hex(0xB07800),
                on_accent: Color32::WHITE,
            },
            (Skin::Modern, false) => Colors {
                text: hex(0x1A1A1A),
                text_dim: hex(0x5F5F5F),
                heading: hex(0x1A1A1A),
                link: accent,
                base: hex(0xF3F3F3),
                card: rgba(0xFFFFFF, 0.72),
                card_border: rgba(0x000000, 0.07),
                field_bg: rgba(0xFFFFFF, 0.75),
                field_border: rgba(0x000000, 0.10),
                divider: rgba(0x000000, 0.08),
                hover: rgba(0x000000, 0.04),
                selected: rgba(0x000000, 0.06),
                danger: hex(0xC42B1C),
                success: hex(0x0F7B0F),
                warn: hex(0x9D5D00),
                on_accent: Color32::WHITE,
            },
            (Skin::Modern, true) => Colors {
                text: hex(0xFFFFFF),
                text_dim: hex(0xC5C5C5),
                heading: hex(0xFFFFFF),
                link: accent,
                base: hex(0x202020),
                card: rgba(0xFFFFFF, 0.05),
                card_border: rgba(0x000000, 0.25),
                field_bg: rgba(0xFFFFFF, 0.06),
                field_border: rgba(0xFFFFFF, 0.09),
                divider: rgba(0xFFFFFF, 0.08),
                hover: rgba(0xFFFFFF, 0.06),
                selected: rgba(0xFFFFFF, 0.08),
                danger: hex(0xFF99A4),
                success: hex(0x6CCB5F),
                warn: hex(0xFCE100),
                on_accent: hex(0x000000),
            },
        };
        Self { skin, dark, accent, c }
    }

    pub fn aero(&self) -> bool {
        self.skin == Skin::Aero
    }

    // -- Type ramp ---------------------------------------------------------

    pub fn body(&self) -> FontId {
        FontId::new(if self.aero() { 13.0 } else { 14.0 }, FontFamily::Proportional)
    }

    pub fn small(&self) -> FontId {
        FontId::new(if self.aero() { 11.5 } else { 12.0 }, FontFamily::Proportional)
    }

    pub fn strong(&self) -> FontId {
        FontId::new(if self.aero() { 13.0 } else { 14.0 }, semi())
    }

    pub fn subtitle(&self) -> FontId {
        FontId::new(if self.aero() { 15.0 } else { 18.0 }, semi())
    }

    pub fn title(&self) -> FontId {
        FontId::new(if self.aero() { 20.0 } else { 28.0 }, semi())
    }

    pub fn display(&self, size: f32) -> FontId {
        FontId::new(size, light())
    }

    // -- Metrics ------------------------------------------------------------

    pub fn radius(&self) -> f32 {
        if self.aero() { 3.0 } else { 4.0 }
    }

    pub fn card_radius(&self) -> f32 {
        if self.aero() { 5.0 } else { 8.0 }
    }

    pub fn control_height(&self) -> f32 {
        if self.aero() { 26.0 } else { 32.0 }
    }

    /// Pushes this theme into egui's built-in widgets (text fields,
    /// scrollbars, drag values, popups), which we don't repaint ourselves.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        v.override_text_color = Some(self.c.text);
        // Text fields paint their own background; this is only the
        // scrollbar track (visible in Aero, invisible in Modern).
        v.extreme_bg_color = if self.aero() { hex(0xF0F0F0) } else { Color32::TRANSPARENT };
        v.faint_bg_color = self.c.hover;
        v.selection.bg_fill = alpha(self.accent, if self.dark { 0.45 } else { 0.30 });
        v.selection.stroke = Stroke::new(1.0, self.accent);
        v.hyperlink_color = self.c.link;
        v.window_fill = if self.dark { hex(0x2C2C2C) } else { hex(0xF9F9F9) };
        v.panel_fill = Color32::TRANSPARENT;
        v.window_stroke = Stroke::new(1.0, self.c.card_border);
        v.text_cursor.stroke = Stroke::new(if self.aero() { 1.0 } else { 1.5 }, self.c.text);
        let scrollbar = if self.aero() { hex(0xC2C9D3) } else { alpha(self.c.text_dim, 0.6) };
        for (w, fill) in [
            (&mut v.widgets.inactive, scrollbar),
            (&mut v.widgets.hovered, mix(scrollbar, self.accent, 0.4)),
            (&mut v.widgets.active, self.accent),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.fg_stroke = Stroke::new(1.0, self.c.text);
            w.bg_stroke = Stroke::new(1.0, self.c.field_border);
            w.corner_radius = egui::CornerRadius::same(self.radius() as u8);
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.c.divider);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.c.text);

        let aero = self.aero();
        ctx.all_styles_mut(|s| {
            s.visuals = v.clone();
            s.spacing.item_spacing = vec2(8.0, 8.0);
            s.spacing.button_padding = vec2(10.0, 4.0);
            s.spacing.interact_size.y = if aero { 22.0 } else { 28.0 };
            s.spacing.scroll.bar_width = if aero { 9.0 } else { 6.0 };
            s.spacing.scroll.floating = !aero;
            s.spacing.scroll.foreground_color = false;
            s.interaction.selectable_labels = false;
        });
        ctx.set_theme(if self.dark { ThemePreference::Dark } else { ThemePreference::Light });
    }
}

pub fn semi() -> FontFamily {
    FontFamily::Name("semi".into())
}

pub fn light() -> FontFamily {
    FontFamily::Name("light".into())
}

/// Loads the system UI fonts for a skin: Segoe UI for Aero, Segoe UI
/// Variable for Modern. Nothing is bundled in the exe - every Windows
/// install already has these.
pub fn install_fonts(ctx: &egui::Context, skin: Skin) {
    let (regular, semibold) = match skin {
        Skin::Aero => (UiFont::Regular, UiFont::Semibold),
        Skin::Modern => (UiFont::ModernRegular, UiFont::ModernSemibold),
    };
    let mut defs = FontDefinitions::empty();
    let mut load = |key: &str, kind: UiFont| -> bool {
        let Some(bytes) = platform::ui_font(kind).and_then(|p| std::fs::read(p).ok()) else { return false };
        defs.font_data.insert(key.to_owned(), Arc::new(FontData::from_owned(bytes)));
        true
    };
    let have_regular = load("regular", regular);
    let have_semi = load("semi", semibold);
    let have_light = load("light", UiFont::Light);
    let have_sym = load("sym", UiFont::Symbols);

    let chain = |first: (&str, bool)| -> Vec<String> {
        [first, ("regular", have_regular), ("sym", have_sym)]
            .into_iter()
            .filter(|(_, ok)| *ok)
            .map(|(k, _)| k.to_owned())
            .fold(Vec::new(), |mut v, k| {
                if !v.contains(&k) {
                    v.push(k);
                }
                v
            })
    };
    defs.families.insert(FontFamily::Proportional, chain(("regular", have_regular)));
    defs.families.insert(FontFamily::Monospace, chain(("regular", have_regular)));
    defs.families.insert(semi(), chain(("semi", have_semi)));
    defs.families.insert(light(), chain(("light", have_light)));
    ctx.set_fonts(defs);
}
