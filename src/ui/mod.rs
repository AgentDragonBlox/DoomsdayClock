//! The settings window.
//!
//! Structure:
//! * `App` owns all state and the two layouts (`aero_layout`,
//!   `modern_layout`). The layouts differ only in *chrome* - frame, title
//!   bar, navigation, footer placement.
//! * The three pages (`countdown_page`, `quotes_page`, `appearance_page`)
//!   are shared by both skins and only ever call skinned widgets.
//!
//! Performance notes:
//! * egui runs in reactive mode: no input, no repaint. An idle window
//!   uses no CPU.
//! * Scanning installed fonts (100-300 ms on Windows) happens on a
//!   background thread at startup, so the window appears instantly.
//! * The live preview is re-rendered only when something that affects it
//!   changes (hashed key), at thumbnail size.
//! * The quote library list is virtualised: only visible rows are laid out.

mod calendar;
mod icon;
mod paint;
mod theme;
mod widgets;

use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, UiBuilder, Vec2,
    ViewportCommand, pos2, vec2,
};

use crate::config::{self, ColorMode, Config, Rotation, Scheme, Skin};
use crate::countdown;
use crate::fonts::{FontLibrary, LoadedFont, Role};
use crate::paths::Paths;
use crate::platform::{self, WindowFx};
use crate::quotes::QuoteStore;
use crate::wallpaper::{self, Trigger};
use paint::{Icon, alpha, gradient, gradient_box, hex, mix, rgba};
use theme::Theme;
use widgets::Kind;

const WIN_W: f32 = 1000.0;
const WIN_H: f32 = 720.0;

pub fn run(paths: Paths) -> Result<(), String> {
    let saved = Config::load(&paths);
    let first_run = saved.is_none();
    let cfg = saved.unwrap_or_default();
    let viewport = egui::ViewportBuilder::default()
        .with_title("Doomsday Clock")
        .with_app_id("doomsday-clock")
        .with_inner_size([WIN_W, WIN_H])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_decorations(cfg.skin == Skin::Modern)
        .with_transparent(true)
        .with_icon(std::sync::Arc::new(icon::window_icon()));
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Doomsday Clock", options, Box::new(move |cc| Ok(Box::new(App::new(cc, paths, cfg, first_run)))))
        .map_err(|e| e.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Countdown,
    Quotes,
    Appearance,
}

impl Page {
    const ALL: [(Page, Icon, &'static str); 3] = [
        (Page::Countdown, Icon::Calendar, "Countdown"),
        (Page::Quotes, Icon::Quote, "Quotes"),
        (Page::Appearance, Icon::Palette, "Appearance"),
    ];

    fn subtitle(self) -> &'static str {
        match self {
            Page::Countdown => "Choose your deadline and how the countdown looks.",
            Page::Quotes => "A line of motivation under the countdown, rotated on your schedule.",
            Page::Appearance => "Switch between the Windows 7 Aero and Windows 11 looks.",
        }
    }
}

struct FontsReady {
    lib: FontLibrary,
    head: Option<LoadedFont>,
    quote: Option<LoadedFont>,
    families: Vec<String>,
}

enum FontState {
    Loading(Receiver<FontsReady>),
    Ready(Box<FontsReady>),
    Failed,
}

impl FontState {
    /// Blocks until the background font scan finishes (only used by actions
    /// that need real fonts right now, e.g. Save & Activate).
    fn wait(&mut self) -> Option<&mut FontsReady> {
        if let FontState::Loading(rx) = self {
            *self = match rx.recv() {
                Ok(ready) => FontState::Ready(Box::new(ready)),
                Err(_) => FontState::Failed,
            };
        }
        match self {
            FontState::Ready(r) => Some(r),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RotKind {
    Daily,
    Weekly,
    Monthly,
    Custom,
}

struct Toast {
    text: String,
    error: bool,
    shown: Instant,
}

pub struct App {
    paths: Paths,
    saved: Config,
    draft: Config,
    first_run: bool,
    store: QuoteStore,
    fonts: FontState,
    page: Page,

    theme: Theme,
    applied: Option<(Skin, bool, bool)>,
    fonts_skin: Skin,
    fx: Option<WindowFx>,
    glass_active: bool,
    accent: Option<[u8; 3]>,

    preview: Option<(u64, TextureHandle)>,
    screen: (u32, u32),
    calendar: calendar::Calendar,

    font_bufs: [String; 2],
    new_quote: String,
    new_author: String,
    search: String,
    custom_hours: u32,

    task_installed: bool,
    toast: Option<Toast>,
    close_armed: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, paths: Paths, cfg: Config, first_run: bool) -> Self {
        let now = config::now();
        let store = QuoteStore::load(&paths, now);

        // Font discovery off the UI thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let (cache, head_req, quote_req) =
            (paths.font_cache_file(), cfg.countdown_font.clone(), cfg.quote_font.clone());
        std::thread::spawn(move || {
            let mut lib = FontLibrary::new(cache, false);
            let head = lib.resolve(&head_req, Role::Countdown);
            let quote = lib.resolve(&quote_req, Role::Quote);
            let families = lib.family_names();
            lib.save_cache();
            let _ = tx.send(FontsReady { lib, head, quote, families });
            ctx.request_repaint();
        });

        let custom_hours = match cfg.rotation {
            Rotation::EveryHours(h) => h,
            _ => 6,
        };
        let accent = platform::accent_color();
        // Must happen before the first frame: egui has no built-in fonts in
        // this build (they'd add ~1 MB to the exe for no benefit on Windows).
        theme::install_fonts(&cc.egui_ctx, cfg.skin);
        Self {
            font_bufs: [cfg.countdown_font.clone(), cfg.quote_font.clone()],
            draft: cfg.clone(),
            saved: cfg.clone(),
            first_run,
            store,
            fonts: FontState::Loading(rx),
            page: start_page(),
            theme: Theme::new(cfg.skin, false, accent),
            applied: None,
            fonts_skin: cfg.skin,
            fx: WindowFx::attach(cc),
            glass_active: false,
            accent,
            preview: None,
            screen: platform::primary_screen_size(),
            calendar: Default::default(),
            new_quote: String::new(),
            new_author: String::new(),
            search: String::new(),
            custom_hours,
            task_installed: platform::task_installed(),
            toast: None,
            close_armed: false,
            paths,
        }
    }

    // -- State helpers --------------------------------------------------------

    fn poll_fonts(&mut self) {
        if let FontState::Loading(rx) = &self.fonts {
            match rx.try_recv() {
                Ok(ready) => self.fonts = FontState::Ready(Box::new(ready)),
                Err(TryRecvError::Disconnected) => self.fonts = FontState::Failed,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    /// The draft as it would be saved: a new target date restarts the
    /// fade-to-red from today.
    fn effective_draft(&self) -> Config {
        let mut cfg = self.draft.clone();
        cfg.rotation = cfg.rotation.sanitized();
        cfg.created_date = if cfg.target_date != self.saved.target_date || self.first_run {
            config::today()
        } else {
            self.saved.created_date
        };
        cfg
    }

    fn dirty(&self) -> bool {
        self.first_run || self.effective_draft() != self.saved
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast { text: text.into(), error, shown: Instant::now() });
    }

    /// Theme preferences apply (and persist) immediately - they're about the
    /// app itself, not the wallpaper.
    fn set_ui_pref(&mut self, f: impl Fn(&mut Config)) {
        f(&mut self.saved);
        f(&mut self.draft);
        if !self.first_run {
            let _ = self.saved.save(&self.paths);
        }
    }

    fn sync_theme(&mut self, ctx: &egui::Context) {
        let skin = self.saved.skin;
        let dark = skin == Skin::Modern
            && match self.saved.scheme {
                Scheme::System => ctx.system_theme() == Some(egui::Theme::Dark),
                Scheme::Light => false,
                Scheme::Dark => true,
            };
        let want = (skin, dark, self.saved.glass_effects);
        if self.applied == Some(want) {
            return;
        }
        if self.fonts_skin != skin {
            // Font changes take effect from the next frame; both skins
            // define the same family names, so this frame still renders.
            theme::install_fonts(ctx, skin);
            self.fonts_skin = skin;
            ctx.send_viewport_cmd(ViewportCommand::Decorations(skin == Skin::Modern));
        }
        self.glass_active = self.fx.as_ref().is_some_and(|fx| fx.apply(skin, dark, self.saved.glass_effects));
        self.theme = Theme::new(skin, dark, self.accent);
        self.theme.apply(ctx);
        self.applied = Some(want);
    }

    // -- Actions --------------------------------------------------------------

    fn save_and_activate(&mut self) {
        let cfg = self.effective_draft();
        if let Err(e) = cfg.save(&self.paths) {
            return self.toast(format!("Couldn't save settings: {e}"), true);
        }
        self.saved = cfg.clone();
        self.draft = cfg.clone();
        self.first_run = false;
        let paths = self.paths.clone();
        let Some(ready) = self.fonts.wait() else {
            return self.toast("No usable fonts were found on this PC.", true);
        };
        let result = wallpaper::update(&paths, &cfg, &mut self.store, &mut ready.lib, Trigger::Manual);
        if let Err(e) = result {
            return self.toast(e, true);
        }
        let task = std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|exe| platform::install_task(&exe, cfg.rotation));
        self.task_installed = platform::task_installed();
        match task {
            Ok(()) => self.toast("Wallpaper updated. It will refresh by itself from now on.", false),
            Err(e) => self.toast(format!("Wallpaper updated, but scheduling failed: {e}"), true),
        }
    }

    fn disable_updates(&mut self) {
        match platform::remove_task() {
            Ok(()) => self.toast("Background updates are off. Your current wallpaper stays as it is.", false),
            Err(e) => self.toast(e, true),
        }
        self.task_installed = platform::task_installed();
    }

    fn next_quote(&mut self) {
        let now = config::now();
        self.store.advance(now);
        let _ = self.store.save(&self.paths);
        if self.task_installed && self.saved.show_quotes {
            let (paths, cfg) = (self.paths.clone(), self.saved.clone());
            let result = match self.fonts.wait() {
                Some(ready) => {
                    wallpaper::update(&paths, &cfg, &mut self.store, &mut ready.lib, Trigger::Manual).map(|_| ())
                }
                None => Err("No usable fonts were found on this PC.".into()),
            };
            match result {
                Ok(()) => self.toast("Next quote is on your desktop.", false),
                Err(e) => self.toast(e, true),
            }
        }
    }

    fn add_quote(&mut self) {
        match self.store.add(&self.new_quote, &self.new_author, config::now()) {
            Ok(_) => {
                let _ = self.store.save(&self.paths);
                self.new_quote.clear();
                self.new_author.clear();
                self.toast("Added. It's first in line for the next rotation.", false);
            }
            Err(e) => self.toast(e.to_string(), true),
        }
    }

    fn import_quotes(&mut self) {
        let Some(path) = platform::pick_json_file() else { return };
        let result = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| self.store.import_json(&text, config::now()));
        match result {
            Ok(r) => {
                let _ = self.store.save(&self.paths);
                let mut msg = format!("Imported {} new quote{}", r.added, if r.added == 1 { "" } else { "s" });
                if r.duplicates > 0 {
                    msg += &format!(", skipped {} already in your library", r.duplicates);
                }
                if r.invalid > 0 {
                    msg += &format!(", ignored {} empty or too long", r.invalid);
                }
                self.toast(msg + ".", false);
            }
            Err(e) => self.toast(e, true),
        }
    }

    fn commit_font(&mut self, role: Role) {
        let i = if role == Role::Countdown { 0 } else { 1 };
        let request = self.font_bufs[i].trim().to_string();
        match role {
            Role::Countdown => self.draft.countdown_font = request.clone(),
            Role::Quote => self.draft.quote_font = request.clone(),
        }
        if let FontState::Ready(r) = &mut self.fonts {
            let font = r.lib.resolve(&request, role);
            r.lib.save_cache();
            match role {
                Role::Countdown => r.head = font,
                Role::Quote => r.quote = font,
            }
        }
    }

    fn preview_texture(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
        let cfg = self.effective_draft();
        let now = config::now();
        let quote = if cfg.show_quotes { self.store.current(now).cloned() } else { None };
        let FontState::Ready(r) = &self.fonts else { return None };
        let (head, qf) = (r.head.as_ref()?, r.quote.as_ref()?);

        let mut h = std::collections::hash_map::DefaultHasher::new();
        (cfg.target_date, cfg.created_date, cfg.color_mode, now.date(), &head.family, &qf.family, self.screen)
            .hash(&mut h);
        quote.as_ref().map(|q| q.id).hash(&mut h);
        let key = h.finish();
        if let Some((k, tex)) = &self.preview
            && *k == key
        {
            return Some(tex.clone());
        }
        let w = 640u32;
        let hgt = (w as f32 * self.screen.1 as f32 / self.screen.0.max(1) as f32).round() as u32;
        let pm = wallpaper::render_scene(&cfg, quote.as_ref(), now.date(), head, qf, w, hgt);
        let img = egui::ColorImage::from_rgba_premultiplied([w as usize, hgt as usize], pm.data());
        let tex = ctx.load_texture("wallpaper-preview", img, egui::TextureOptions::LINEAR);
        self.preview = Some((key, tex.clone()));
        Some(tex)
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.glass_active { [0.0; 4] } else { self.theme.c.base.to_normalized_gamma_f32() }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_fonts();
        self.sync_theme(&ctx);

        // Unsaved-changes guard on close: first close warns, second closes.
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.first_run && !self.close_armed {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.close_armed = true;
            self.toast("You have unsaved changes. Close again to discard them.", true);
        }

        let th = self.theme.clone();
        let win = ui.max_rect();
        match th.skin {
            Skin::Aero => self.aero_layout(ui, &th, win),
            Skin::Modern => self.modern_layout(ui, &th, win),
        }

        if let Some(day) = self.calendar.show(&ctx, &th, self.draft.target_date, config::today()) {
            self.draft.target_date = day;
        }
        self.toast_ui(&ctx, &th, win);
    }
}

// ---------------------------------------------------------------------------
// Chrome: Windows 7 Aero
// ---------------------------------------------------------------------------

const AERO_CAPTION: f32 = 30.0;
const AERO_BANNER: f32 = 70.0;
const AERO_BORDER: f32 = 8.0;

impl App {
    fn aero_layout(&mut self, ui: &mut Ui, th: &Theme, win: Rect) {
        let p = ui.painter().clone();
        let ctx = ui.ctx().clone();

        // 1. Glass frame. Translucent over real Acrylic when available,
        //    otherwise an opaque painted rendition of Aero "Sky".
        let tint: [(f32, Color32); 3] = if self.glass_active {
            [(0.0, rgba(0xA9CBF0, 0.55)), (0.4, rgba(0x7DA8DC, 0.45)), (1.0, rgba(0x6390CB, 0.55))]
        } else {
            [(0.0, hex(0xB7D3F1)), (0.35, hex(0x8DB4E2)), (1.0, hex(0x6E9AD0))]
        };
        gradient(&p, win, 0.0, 0.0, &tint);
        // Diagonal light streaks - the signature Windows 7 glass sheen.
        for (x, w, a) in
            [(0.06, 70.0, 0.16), (0.13, 22.0, 0.20), (0.62, 110.0, 0.10), (0.71, 30.0, 0.14), (0.9, 60.0, 0.10)]
        {
            let x0 = win.left() + win.width() * x;
            let h = win.height();
            let poly = vec![
                pos2(x0, win.top()),
                pos2(x0 + w, win.top()),
                pos2(x0 + w - h * 0.55, win.bottom()),
                pos2(x0 - h * 0.55, win.bottom()),
            ];
            p.add(egui::Shape::convex_polygon(poly, rgba(0xFFFFFF, a), Stroke::NONE));
        }
        // Soft sheen across the top edge.
        gradient(
            &p,
            Rect::from_min_size(win.min, vec2(win.width(), 46.0)),
            0.0,
            0.0,
            &[(0.0, rgba(0xFFFFFF, 0.35)), (1.0, rgba(0xFFFFFF, 0.0))],
        );
        p.rect_stroke(win, CornerRadius::ZERO, Stroke::new(1.0, rgba(0x000000, 0.55)), StrokeKind::Inside);
        p.rect_stroke(win.shrink(1.0), CornerRadius::ZERO, Stroke::new(1.0, rgba(0xFFFFFF, 0.55)), StrokeKind::Inside);

        // 2. Caption: logo, glowing title, caption buttons, drag area.
        let caption = Rect::from_min_size(win.min, vec2(win.width(), AERO_CAPTION));
        paint::logo(&p, pos2(win.left() + 18.0, caption.center().y + 1.0), 8.5);
        glow_text(
            &p,
            pos2(win.left() + 33.0, caption.center().y + 1.0),
            Align2::LEFT_CENTER,
            "Doomsday Clock",
            th.body(),
            hex(0x000000),
        );

        let drag_zone = Rect::from_min_max(win.min, pos2(win.right() - 90.0, win.top() + AERO_CAPTION + AERO_BANNER));
        let drag = ui.interact(drag_zone, egui::Id::new("aero-drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        self.aero_caption_buttons(ui, win);

        // 3. Extended glass banner: live countdown summary.
        let banner = Rect::from_min_size(
            pos2(win.left() + AERO_BORDER, caption.bottom()),
            vec2(win.width() - AERO_BORDER * 2.0, AERO_BANNER),
        );
        self.countdown_summary(&p, th, banner);

        // 4. Client area.
        let client = Rect::from_min_max(
            pos2(win.left() + AERO_BORDER, banner.bottom()),
            pos2(win.right() - AERO_BORDER, win.bottom() - AERO_BORDER),
        );
        p.rect_filled(client, CornerRadius::ZERO, hex(0xFFFFFF));
        p.rect_stroke(
            client.expand(1.0),
            CornerRadius::ZERO,
            Stroke::new(1.0, rgba(0x1E3A5F, 0.55)),
            StrokeKind::Inside,
        );

        let footer = Rect::from_min_max(pos2(client.left(), client.bottom() - 48.0), client.max);
        let pane = Rect::from_min_max(client.min, pos2(client.left() + 196.0, footer.top()));

        // Control Panel-style task pane.
        gradient(&p, pane, 0.0, 0.0, &[(0.0, hex(0xF7FAFE)), (1.0, hex(0xDAE6F4))]);
        p.line_segment([pane.right_top(), pane.right_bottom()], Stroke::new(1.0, hex(0xC5D5E8)));
        let nav_rect = pane.shrink2(vec2(10.0, 14.0));
        ui.scope_builder(UiBuilder::new().max_rect(nav_rect), |ui| {
            ui.label(egui::RichText::new("Settings").font(th.small()).color(th.c.text_dim));
            ui.add_space(2.0);
            self.nav(ui, th);
            ui.add_space(14.0);
            ui.label(egui::RichText::new("See also").font(th.small()).color(th.c.text_dim));
            if link(ui, th, "Open settings folder").clicked() {
                platform::open_folder(&self.paths.config_dir);
            }
        });

        // Command area, Windows 7 dialog style.
        gradient(&p, footer, 0.0, 0.0, &[(0.0, hex(0xF0F0F0)), (1.0, hex(0xE9ECF0))]);
        p.line_segment([footer.left_top(), footer.right_top()], Stroke::new(1.0, hex(0xDFDFDF)));
        self.footer(ui, th, footer.shrink2(vec2(14.0, 10.0)));

        let page = Rect::from_min_max(pos2(pane.right() + 1.0, client.top()), pos2(client.right(), footer.top()));
        self.page_area(ui, th, page.shrink2(vec2(22.0, 16.0)));
    }

    fn aero_caption_buttons(&mut self, ui: &mut Ui, win: Rect) {
        let ctx = ui.ctx().clone();
        let close = Rect::from_min_size(pos2(win.right() - 6.0 - 46.0, win.top() + 1.0), vec2(46.0, 20.0));
        let min = Rect::from_min_size(pos2(close.left() - 27.0, close.top()), vec2(27.0, 20.0));
        for (rect, is_close) in [(min, false), (close, true)] {
            let resp = ui.interact(rect, egui::Id::new(("caption", is_close)), Sense::click());
            let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
            let down = resp.is_pointer_button_down_on();
            let p = ui.painter();
            let (normal, hover): (widgets::Stops, widgets::Stops) = if is_close {
                (
                    [(0.0, hex(0xE8A7A0)), (0.5, hex(0xD16B5F)), (0.5, hex(0xBE3A2C)), (1.0, hex(0xD9624F))],
                    [(0.0, hex(0xF7C4BB)), (0.5, hex(0xEC6A57)), (0.5, hex(0xDB2F1A)), (1.0, hex(0xF58A72))],
                )
            } else {
                (
                    [
                        (0.0, rgba(0xFFFFFF, 0.55)),
                        (0.5, rgba(0xFFFFFF, 0.25)),
                        (0.5, rgba(0xFFFFFF, 0.08)),
                        (1.0, rgba(0xFFFFFF, 0.30)),
                    ],
                    [(0.0, hex(0xC4E0F7)), (0.5, hex(0x69AAE3)), (0.5, hex(0x2F7CC8)), (1.0, hex(0x67BBF6))],
                )
            };
            let mut stops: widgets::Stops = std::array::from_fn(|i| (normal[i].0, mix(normal[i].1, hover[i].1, t)));
            if down {
                for s in &mut stops {
                    s.1 = mix(s.1, Color32::BLACK, 0.18);
                }
            }
            let (rt, rb) = (0.0, 4.0);
            gradient(p, rect, rt, rb, &stops);
            let cr = if is_close {
                CornerRadius { nw: 0, ne: 0, sw: 0, se: 4 }
            } else {
                CornerRadius { nw: 0, ne: 0, sw: 4, se: 0 }
            };
            p.rect_stroke(rect, cr, Stroke::new(1.0, rgba(0x0B1D33, 0.75)), StrokeKind::Inside);
            p.rect_stroke(rect.shrink(1.0), cr, Stroke::new(1.0, rgba(0xFFFFFF, 0.45)), StrokeKind::Inside);
            if t > 0.0 {
                let col = if is_close { rgba(0xFF8A70, 0.7 * t) } else { rgba(0x9FD8FF, 0.7 * t) };
                paint::glow(
                    &p.with_clip_rect(rect),
                    pos2(rect.center().x, rect.bottom()),
                    vec2(rect.width() * 0.6, 12.0),
                    col,
                );
            }
            // White glyphs with a dark outline, exactly like Windows 7's.
            if is_close {
                let glyph = Rect::from_center_size(rect.center(), vec2(9.0, 8.0));
                paint::icon(p, Icon::Close, glyph.expand(0.8), rgba(0x0B1D33, 0.85));
                paint::icon(p, Icon::Close, glyph, Color32::WHITE);
            } else {
                let bar = Rect::from_center_size(rect.center() + vec2(0.0, 3.0), vec2(9.0, 3.0));
                p.rect_filled(bar.expand(1.0), CornerRadius::same(1), rgba(0x0B1D33, 0.85));
                p.rect_filled(bar, CornerRadius::ZERO, Color32::WHITE);
            }
            if resp.clicked() {
                ctx.send_viewport_cmd(if is_close { ViewportCommand::Close } else { ViewportCommand::Minimized(true) });
            }
        }
    }

    /// Big days-left number, deadline, and progress bar. Lives in the Aero
    /// glass banner, or at the top of the Modern navigation pane.
    fn countdown_summary(&self, p: &egui::Painter, th: &Theme, rect: Rect) {
        let today = config::today();
        let cfg = self.effective_draft();
        let days = countdown::days_left(cfg.target_date, today);
        let frac = countdown::progress(cfg.created_date, cfg.target_date, today);
        let number = if days >= 0 { days.to_string() } else { "0".into() };
        let unit = match days {
            1 => "day left until",
            d if d < 0 => "days left - deadline passed",
            _ => "days left until",
        };
        let date = cfg.target_date.format("%A, %-d %B %Y").to_string();

        if th.aero() {
            let num_rect = glow_text(
                p,
                pos2(rect.left() + 10.0, rect.center().y - 2.0),
                Align2::LEFT_CENTER,
                &number,
                th.display(46.0),
                hex(0x0B1D33),
            );
            let x = num_rect.right() + 12.0;
            glow_text(p, pos2(x, rect.center().y - 11.0), Align2::LEFT_CENTER, unit, th.body(), hex(0x1B2D45));
            glow_text(p, pos2(x, rect.center().y + 9.0), Align2::LEFT_CENTER, &date, th.subtitle(), hex(0x0B1D33));
            let bar = Rect::from_min_size(pos2(rect.right() - 300.0, rect.center().y - 4.0), vec2(286.0, 15.0));
            widgets::progress(p, th, bar, frac);
            glow_text(
                p,
                pos2(bar.left(), bar.top() - 11.0),
                Align2::LEFT_CENTER,
                &format!("{:.0}% of the countdown has passed", frac * 100.0),
                th.small(),
                hex(0x1B2D45),
            );
        } else {
            p.text(pos2(rect.left(), rect.top()), Align2::LEFT_TOP, &number, th.display(48.0), th.c.text);
            p.text(pos2(rect.left(), rect.top() + 58.0), Align2::LEFT_TOP, unit, th.small(), th.c.text_dim);
            p.text(pos2(rect.left(), rect.top() + 76.0), Align2::LEFT_TOP, &date, th.strong(), th.c.text);
            widgets::progress(
                p,
                th,
                Rect::from_min_size(pos2(rect.left(), rect.top() + 104.0), vec2(rect.width(), 6.0)),
                frac,
            );
        }
    }

    // -----------------------------------------------------------------------
    // Chrome: Windows 11
    // -----------------------------------------------------------------------

    fn modern_layout(&mut self, ui: &mut Ui, th: &Theme, win: Rect) {
        let p = ui.painter().clone();
        if !self.glass_active {
            p.rect_filled(win, CornerRadius::ZERO, th.c.base);
        }
        let nav_w = 240.0;
        let nav = Rect::from_min_max(win.min, pos2(win.left() + nav_w, win.bottom()));
        paint::logo(&p, pos2(nav.left() + 26.0, nav.top() + 24.0), 9.0);
        p.text(pos2(nav.left() + 44.0, nav.top() + 24.0), Align2::LEFT_CENTER, "Doomsday Clock", th.small(), th.c.text);
        self.countdown_summary(
            &p,
            th,
            Rect::from_min_size(pos2(nav.left() + 18.0, nav.top() + 52.0), vec2(nav_w - 36.0, 112.0)),
        );

        ui.scope_builder(
            UiBuilder::new().max_rect(Rect::from_min_max(
                pos2(nav.left() + 6.0, nav.top() + 186.0),
                pos2(nav.right() - 8.0, nav.bottom() - 8.0),
            )),
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                self.nav(ui, th);
            },
        );

        // The content "layer" floating over Mica, as in Windows 11 Settings.
        let layer = Rect::from_min_max(pos2(nav.right(), win.top()), win.max);
        let fill = match (self.glass_active, th.dark) {
            (true, false) => rgba(0xFFFFFF, 0.5),
            (true, true) => rgba(0x3A3A3A, 0.3),
            (false, false) => hex(0xF9F9F9),
            (false, true) => hex(0x272727),
        };
        let cr = CornerRadius { nw: 8, ne: 0, sw: 0, se: 0 };
        p.rect_filled(layer, cr, fill);
        p.rect_stroke(layer.expand(1.0), cr, Stroke::new(1.0, th.c.card_border), StrokeKind::Inside);

        let footer = Rect::from_min_max(pos2(layer.left(), layer.bottom() - 64.0), layer.max);
        p.line_segment([footer.left_top(), footer.right_top()], Stroke::new(1.0, th.c.divider));
        self.footer(ui, th, footer.shrink2(vec2(28.0, 14.0)));
        self.page_area(
            ui,
            th,
            Rect::from_min_max(layer.min + vec2(32.0, 22.0), pos2(layer.right() - 32.0, footer.top())),
        );
    }

    // -----------------------------------------------------------------------
    // Shared chrome pieces
    // -----------------------------------------------------------------------

    fn nav(&mut self, ui: &mut Ui, th: &Theme) {
        for (page, icon, label) in Page::ALL {
            if widgets::nav_item(ui, th, icon, label, self.page == page).clicked() {
                self.page = page;
                self.calendar.open = false;
            }
        }
    }

    fn footer(&mut self, ui: &mut Ui, th: &Theme, rect: Rect) {
        let p = ui.painter();
        let (dot, text) = if self.dirty() {
            (
                th.c.warn,
                if self.first_run {
                    "Not activated yet - press Save & Activate to set your wallpaper."
                } else {
                    "You have unsaved changes."
                },
            )
        } else if self.task_installed {
            (th.c.success, "Active - the wallpaper refreshes itself daily and at sign-in.")
        } else {
            (th.c.text_dim, "Saved, but background updates are off.")
        };
        p.circle_filled(pos2(rect.left() + 5.0, rect.center().y), 4.5, dot);
        if th.aero() {
            p.circle_stroke(
                pos2(rect.left() + 5.0, rect.center().y),
                4.5,
                Stroke::new(1.0, mix(dot, Color32::BLACK, 0.35)),
            );
            p.circle_filled(pos2(rect.left() + 3.8, rect.center().y - 1.4), 1.4, alpha(Color32::WHITE, 0.8));
        }
        p.text(pos2(rect.left() + 18.0, rect.center().y), Align2::LEFT_CENTER, text, th.body(), th.c.text);

        ui.scope_builder(
            UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                if widgets::button(ui, th, "Save & Activate", None, Kind::Primary).clicked() {
                    self.save_and_activate();
                }
                if self.dirty() && !self.first_run && widgets::button(ui, th, "Revert", None, Kind::Normal).clicked() {
                    self.draft = self.saved.clone();
                    self.font_bufs = [self.saved.countdown_font.clone(), self.saved.quote_font.clone()];
                    self.commit_font(Role::Countdown);
                    self.commit_font(Role::Quote);
                }
            },
        );
    }

    fn page_area(&mut self, ui: &mut Ui, th: &Theme, rect: Rect) {
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            let title = Page::ALL.iter().find(|(p, ..)| *p == self.page).map_or("", |(.., l)| *l);
            ui.label(egui::RichText::new(title).font(th.title()).color(th.c.heading));
            ui.label(egui::RichText::new(self.page.subtitle()).font(th.body()).color(th.c.text_dim));
            ui.add_space(if th.aero() { 6.0 } else { 10.0 });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let total = ui.available_width();
                let gap = 16.0;
                let left_w = ((total - gap) * 0.52).floor();
                let right_w = total - gap - left_w;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.vertical(|ui| {
                        ui.set_width(left_w);
                        ui.spacing_mut().item_spacing.y = 12.0;
                        match self.page {
                            Page::Countdown => self.countdown_left(ui, th),
                            Page::Quotes => self.quotes_left(ui, th),
                            Page::Appearance => self.appearance_left(ui, th),
                        }
                    });
                    ui.vertical(|ui| {
                        ui.set_width(right_w);
                        ui.spacing_mut().item_spacing.y = 12.0;
                        match self.page {
                            Page::Countdown => self.countdown_right(ui, th),
                            Page::Quotes => self.quotes_right(ui, th),
                            Page::Appearance => self.appearance_right(ui, th),
                        }
                    });
                });
            });
        });
    }

    fn toast_ui(&mut self, ctx: &egui::Context, th: &Theme, win: Rect) {
        let Some(toast) = &self.toast else { return };
        let age = toast.shown.elapsed().as_secs_f32();
        const LIFE: f32 = 5.0;
        if age > LIFE {
            self.toast = None;
            self.close_armed = false;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(if (0.25..=LIFE - 0.4).contains(&age) { 250 } else { 16 }));
        let a = (age / 0.2).min(1.0).min((LIFE - age) / 0.4);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("toast")));
        let font = th.body();
        let galley = painter.layout(toast.text.clone(), font, th.c.text, 340.0);
        let size = galley.size() + vec2(48.0, 24.0);
        let bottom = if th.aero() { win.bottom() - AERO_BORDER - 58.0 } else { win.bottom() - 74.0 };
        let rect = Rect::from_min_size(pos2(win.right() - size.x - 24.0, bottom - size.y + (1.0 - a) * 12.0), size);
        let accent = if toast.error { th.c.danger } else { th.c.success };
        painter.add(
            egui::epaint::Shadow { offset: [0, 4], blur: 18, spread: 0, color: alpha(Color32::BLACK, 0.25 * a) }
                .as_shape(rect, CornerRadius::same(6)),
        );
        if th.aero() {
            // Windows 7 notification balloon: pale yellow-white gradient.
            gradient_box(
                &painter,
                rect,
                4.0,
                &[(0.0, alpha(hex(0xFFFFFF), a)), (1.0, alpha(hex(0xE9EEF5), a))],
                alpha(hex(0x768AA8), a),
            );
        } else {
            painter.rect_filled(
                rect,
                CornerRadius::same(8),
                alpha(if th.dark { hex(0x2C2C2C) } else { hex(0xFBFBFB) }, a),
            );
            painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, th.c.card_border), StrokeKind::Inside);
        }
        let dot = pos2(rect.left() + 18.0, rect.top() + 12.0 + galley.rows.first().map_or(8.0, |r| r.height() / 2.0));
        painter.circle_filled(dot, 7.0, alpha(accent, a));
        paint::icon(
            &painter,
            if toast.error { Icon::Close } else { Icon::Check },
            Rect::from_center_size(dot, Vec2::splat(8.0)),
            alpha(Color32::WHITE, a),
        );
        painter.galley(rect.min + vec2(34.0, 12.0), galley, alpha(th.c.text, a));
    }
}

/// `DOOMSDAY_START_PAGE=quotes|appearance` opens on that page (debug builds
/// only - used to take documentation screenshots).
fn start_page() -> Page {
    if cfg!(debug_assertions) {
        match std::env::var("DOOMSDAY_START_PAGE").as_deref() {
            Ok("quotes") => return Page::Quotes,
            Ok("appearance") => return Page::Appearance,
            _ => {}
        }
    }
    Page::Countdown
}

/// Windows 7 caption text: dark text on a soft white bloom so it reads on
/// any glass colour.
fn glow_text(
    p: &egui::Painter,
    pos: egui::Pos2,
    align: Align2,
    text: &str,
    font: egui::FontId,
    color: Color32,
) -> Rect {
    let galley = p.layout_no_wrap(text.to_owned(), font, color);
    let rect = align.anchor_size(pos, galley.size());
    paint::glow(p, rect.center(), vec2(rect.width() * 0.62 + 10.0, rect.height() * 0.9), rgba(0xFFFFFF, 0.7));
    p.galley(rect.min, galley, color);
    rect
}

fn link(ui: &mut Ui, th: &Theme, text: &str) -> egui::Response {
    let resp =
        ui.add(egui::Label::new(egui::RichText::new(text).font(th.body()).color(th.c.link)).sense(Sense::click()));
    if resp.hovered() {
        ui.painter().line_segment([resp.rect.left_bottom(), resp.rect.right_bottom()], Stroke::new(1.0, th.c.link));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

impl App {
    fn countdown_left(&mut self, ui: &mut Ui, th: &Theme) {
        let today = config::today();
        widgets::card(ui, th, Some("Deadline"), |ui| {
            let label = self.draft.target_date.format("%A, %-d %B %Y").to_string();
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), th.control_height() + 4.0), Sense::click());
            let hovered = resp.hovered() || self.calendar.open;
            let p = ui.painter();
            if th.aero() {
                let stops = if hovered {
                    [(0.0, hex(0xEAF6FD)), (0.5, hex(0xD9F0FC)), (0.5, hex(0xBEE6FD)), (1.0, hex(0xA7D9F5))]
                } else {
                    [(0.0, hex(0xFFFFFF)), (0.5, hex(0xF7F7F7)), (0.5, hex(0xEDEDED)), (1.0, hex(0xF5F5F5))]
                };
                gradient_box(p, rect, 3.0, &stops, if hovered { hex(0x3C7FB1) } else { hex(0xABADB3) });
            } else {
                p.rect_filled(rect, CornerRadius::same(4), if hovered { th.c.hover } else { th.c.field_bg });
                p.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, th.c.field_border), StrokeKind::Inside);
            }
            p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, &label, th.strong(), th.c.text);
            paint::icon(
                p,
                Icon::Calendar,
                Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(15.0)),
                if th.aero() { hex(0x2D5FA8) } else { th.c.text_dim },
            );
            if resp.clicked() {
                if self.calendar.open {
                    self.calendar.open = false;
                } else {
                    self.calendar.open_at(rect.left_bottom() + vec2(0.0, 4.0), self.draft.target_date);
                }
            }
            let days = countdown::days_left(self.draft.target_date, today);
            let since = self.effective_draft().created_date;
            widgets::caption(
                ui,
                th,
                &format!(
                    "{days} days from today. The fade-to-red runs from {} to the deadline.",
                    since.format("%-d %b %Y")
                ),
            );
        });

        widgets::card(ui, th, Some("Countdown colour"), |ui| {
            widgets::choice(
                ui,
                th,
                &mut self.draft.color_mode,
                &[(ColorMode::StaticWhite, "Always white"), (ColorMode::FadeToRed, "Fade to red")],
            );
            widgets::caption(
                ui,
                th,
                match self.draft.color_mode {
                    ColorMode::StaticWhite => "The countdown stays crisp white every day.",
                    ColorMode::FadeToRed => "Starts white and warms day by day, reaching full red on the deadline.",
                },
            );
        });

        widgets::card(ui, th, Some("Fonts"), |ui| {
            self.font_row(ui, th, Role::Countdown, "Countdown");
            ui.add_space(4.0);
            self.font_row(ui, th, Role::Quote, "Quote");
            widgets::caption(ui, th, "Type a font name (or paste a path to a .ttf/.otf file) and press Enter.");
        });
    }

    fn font_row(&mut self, ui: &mut Ui, th: &Theme, role: Role, label: &str) {
        let i = if role == Role::Countdown { 0 } else { 1 };
        ui.label(egui::RichText::new(label).font(th.strong()).color(th.c.text));
        let w = ui.available_width();
        let mut buf = std::mem::take(&mut self.font_bufs[i]);
        let resp = widgets::text_field(ui, th, &mut buf, "Font family name", w, 1);
        self.font_bufs[i] = buf;
        let committed = resp.lost_focus();
        if committed {
            self.commit_font(role);
        }

        // Autocomplete from installed families while typing.
        if resp.has_focus()
            && self.font_bufs[i].trim().len() >= 2
            && let FontState::Ready(r) = &self.fonts
        {
            let q = self.font_bufs[i].trim().to_lowercase();
            let matches: Vec<String> = r
                .families
                .iter()
                .filter(|f| f.to_lowercase().contains(&q) && f.to_lowercase() != q)
                .take(5)
                .cloned()
                .collect();
            for m in matches {
                if ui.add(egui::Button::new(egui::RichText::new(&m).font(th.small())).frame(false)).clicked() {
                    self.font_bufs[i] = m;
                    self.commit_font(role);
                }
            }
        }

        let status = match &self.fonts {
            FontState::Loading(_) => ("Looking through installed fonts...".to_string(), th.c.text_dim),
            FontState::Failed => ("No fonts could be loaded.".to_string(), th.c.danger),
            FontState::Ready(r) => match if role == Role::Countdown { &r.head } else { &r.quote } {
                Some(f) if f.exact => (format!("Installed: {}", f.family), th.c.success),
                Some(f) => (format!("Not installed here - using {} instead", f.family), th.c.warn),
                None => ("No usable font".to_string(), th.c.danger),
            },
        };
        ui.label(egui::RichText::new(status.0).font(th.small()).color(status.1));
    }

    fn countdown_right(&mut self, ui: &mut Ui, th: &Theme) {
        let ctx = ui.ctx().clone();
        let tex = self.preview_texture(&ctx);
        let screen = self.screen;
        widgets::card(ui, th, Some("Live preview"), |ui| {
            let w = ui.available_width();
            let h = (w * screen.1 as f32 / screen.0.max(1) as f32).round();
            if th.aero() {
                // A glossy Windows 7-era monitor around the preview.
                let bezel_h = h + 24.0;
                let (rect, _) = ui.allocate_exact_size(vec2(w, bezel_h + 22.0), Sense::hover());
                let bezel = Rect::from_min_size(rect.min, vec2(w, bezel_h));
                let p = ui.painter();
                gradient_box(
                    p,
                    bezel,
                    6.0,
                    &[(0.0, hex(0x4B4F55)), (0.06, hex(0x2A2D31)), (1.0, hex(0x0E0F11))],
                    hex(0x050505),
                );
                gradient(
                    p,
                    Rect::from_min_size(bezel.min + vec2(2.0, 2.0), vec2(w - 4.0, bezel_h * 0.45)),
                    5.0,
                    0.0,
                    &[(0.0, rgba(0xFFFFFF, 0.16)), (1.0, rgba(0xFFFFFF, 0.0))],
                );
                let screen_rect = bezel.shrink(12.0);
                draw_preview(ui, th, screen_rect, tex.as_ref());
                let neck = Rect::from_min_size(pos2(bezel.center().x - 14.0, bezel.bottom()), vec2(28.0, 12.0));
                gradient(ui.painter(), neck, 0.0, 0.0, &[(0.0, hex(0x1C1E21)), (1.0, hex(0x3A3D42))]);
                let foot = Rect::from_min_size(pos2(bezel.center().x - 60.0, neck.bottom()), vec2(120.0, 8.0));
                gradient_box(
                    ui.painter(),
                    foot,
                    4.0,
                    &[(0.0, hex(0x5A5E64)), (0.5, hex(0x2E3135)), (1.0, hex(0x1A1C1F))],
                    hex(0x0A0A0A),
                );
            } else {
                let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                draw_preview(ui, th, rect, tex.as_ref());
            }
            widgets::caption(
                ui,
                th,
                &format!(
                    "Primary display, {} x {}. Every monitor gets its own correctly sized image.",
                    screen.0, screen.1
                ),
            );
        });
    }

    fn quotes_left(&mut self, ui: &mut Ui, th: &Theme) {
        widgets::card(ui, th, Some("On the wallpaper"), |ui| {
            widgets::toggle(ui, th, &mut self.draft.show_quotes, "Show a quote under the countdown");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Change the quote").font(th.strong()).color(th.c.text));
            let mut kind = match self.draft.rotation {
                Rotation::Daily => RotKind::Daily,
                Rotation::Weekly => RotKind::Weekly,
                Rotation::Monthly => RotKind::Monthly,
                Rotation::EveryHours(_) => RotKind::Custom,
            };
            if widgets::choice(
                ui,
                th,
                &mut kind,
                &[
                    (RotKind::Daily, "Daily"),
                    (RotKind::Weekly, "Weekly"),
                    (RotKind::Monthly, "Monthly"),
                    (RotKind::Custom, "Custom"),
                ],
            ) {
                self.draft.rotation = match kind {
                    RotKind::Daily => Rotation::Daily,
                    RotKind::Weekly => Rotation::Weekly,
                    RotKind::Monthly => Rotation::Monthly,
                    RotKind::Custom => Rotation::EveryHours(self.custom_hours),
                };
            }
            if kind == RotKind::Custom {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Every").font(th.body()));
                    let changed =
                        widgets::stepper(ui, th, &mut self.custom_hours, Rotation::MIN_HOURS, Rotation::MAX_HOURS);
                    ui.label(
                        egui::RichText::new(if self.custom_hours == 1 { "hour" } else { "hours" }).font(th.body()),
                    );
                    if changed {
                        self.draft.rotation = Rotation::EveryHours(self.custom_hours);
                    }
                });
                widgets::caption(
                    ui,
                    th,
                    "An hour is the minimum: anything faster would wake the PC constantly for no real benefit.",
                );
            } else {
                widgets::caption(
                    ui,
                    th,
                    "Changes with the countdown just after midnight, and whenever you sign in late.",
                );
            }
        });

        widgets::card(ui, th, Some("Showing now"), |ui| {
            let now = config::now();
            let current = self.store.current(now).cloned();
            match current {
                Some(q) => {
                    ui.label(
                        egui::RichText::new(format!("\u{201C}{}\u{201D}", q.text)).font(th.body()).color(th.c.text),
                    );
                    if !q.author.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("\u{2014} {}", q.author)).font(th.small()).color(th.c.text_dim),
                        );
                    }
                }
                None if self.store.is_empty() => widgets::caption(ui, th, "Your library is empty - add a quote below."),
                None => widgets::caption(ui, th, "Quotes are switched off."),
            }
            ui.horizontal(|ui| {
                if widgets::button(ui, th, "Next quote", Some(Icon::Next), Kind::Normal).clicked() {
                    self.next_quote();
                }
                let queued = self.store.queued_new().len();
                if queued > 0 {
                    widgets::caption(
                        ui,
                        th,
                        &format!("{queued} new quote{} queued to show first", if queued == 1 { "" } else { "s" }),
                    );
                }
            });
        });

        widgets::card(ui, th, Some("Add a quote"), |ui| {
            let w = ui.available_width();
            widgets::text_field(ui, th, &mut self.new_quote, "Type a quote...", w, 3);
            ui.horizontal(|ui| {
                let aw = ui.available_width() - 104.0;
                widgets::text_field(ui, th, &mut self.new_author, "Author (optional)", aw, 1);
                if widgets::button(ui, th, "Add", Some(Icon::Plus), Kind::Primary).clicked() {
                    self.add_quote();
                }
            });
            ui.horizontal(|ui| {
                if widgets::button(ui, th, "Import JSON...", Some(Icon::Import), Kind::Normal).clicked() {
                    self.import_quotes();
                }
                if widgets::button(ui, th, "Open quotes.json", Some(Icon::Folder), Kind::Normal).clicked() {
                    platform::open_folder(&self.paths.config_dir);
                }
            });
            widgets::caption(
                ui,
                th,
                "Imports merge into your library: duplicates are skipped, new quotes go to the front of the queue.",
            );
        });
    }

    fn quotes_right(&mut self, ui: &mut Ui, th: &Theme) {
        let total = self.store.len();
        widgets::card(ui, th, Some(&format!("Library - {total} quotes")), |ui| {
            let w = ui.available_width();
            widgets::text_field(ui, th, &mut self.search, "Search quotes or authors", w, 1);
            let q = self.search.trim().to_lowercase();
            let rows: Vec<usize> = self
                .store
                .quotes()
                .iter()
                .enumerate()
                .filter(|(_, x)| {
                    q.is_empty() || x.text.to_lowercase().contains(&q) || x.author.to_lowercase().contains(&q)
                })
                .map(|(i, _)| i)
                .rev() // newest first
                .collect();
            let row_h = 54.0;
            let current = self.store.current_id();
            let mut remove = None;
            egui::ScrollArea::vertical().id_salt("library").max_height(420.0).auto_shrink([false, true]).show_rows(
                ui,
                row_h,
                rows.len(),
                |ui, range| {
                    for &idx in &rows[range] {
                        let quote = &self.store.quotes()[idx];
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());
                        let p = ui.painter();
                        let is_current = current == Some(quote.id);
                        if is_current || resp.hovered() {
                            if th.aero() {
                                gradient_box(
                                    p,
                                    rect.shrink(1.0),
                                    3.0,
                                    &[(0.0, hex(0xFAFBFD)), (1.0, hex(0xEBF3FD))],
                                    hex(0xB8D6FB),
                                );
                            } else {
                                p.rect_filled(rect.shrink(1.0), CornerRadius::same(4), th.c.hover);
                            }
                        }
                        // Right-hand cluster, vertically centred: [badges] [x].
                        let mut badge_x = rect.right() - 36.0;
                        let badge_y = rect.center().y;
                        if is_current {
                            badge_x = widgets::badge(p, th, pos2(badge_x, badge_y), "ON SCREEN").left() - 4.0;
                        }
                        if self.store.is_queued_new(quote.id) {
                            badge_x = widgets::badge(p, th, pos2(badge_x, badge_y), "NEW").left() - 4.0;
                        }
                        let has_author = !quote.author.is_empty();
                        let text_w = (badge_x - rect.left() - 14.0).max(80.0);
                        let mut job = egui::text::LayoutJob::simple(quote.text.clone(), th.body(), th.c.text, text_w);
                        job.wrap.max_rows = if has_author { 1 } else { 2 };
                        job.wrap.overflow_character = Some('\u{2026}');
                        let g = p.layout_job(job);
                        let text_top = if has_author { rect.top() + 8.0 } else { rect.center().y - g.size().y / 2.0 };
                        p.galley(pos2(rect.left() + 8.0, text_top), g, th.c.text);
                        if has_author {
                            p.text(
                                pos2(rect.left() + 8.0, rect.top() + 29.0),
                                Align2::LEFT_TOP,
                                &quote.author,
                                th.small(),
                                th.c.text_dim,
                            );
                        }
                        let del = Rect::from_center_size(
                            pos2(rect.right() - 18.0, rect.center().y),
                            Vec2::splat(if th.aero() { 22.0 } else { 26.0 }),
                        );
                        if ui
                            .put(del, |ui: &mut Ui| widgets::icon_button(ui, th, Icon::Close, "Remove from library"))
                            .clicked()
                        {
                            remove = Some(quote.id);
                        }
                    }
                },
            );
            if let Some(id) = remove {
                self.store.remove(id);
                let _ = self.store.save(&self.paths);
            }
            if rows.is_empty() {
                widgets::caption(ui, th, "No quotes match your search.");
            }
        });
    }

    fn appearance_left(&mut self, ui: &mut Ui, th: &Theme) {
        widgets::card(ui, th, Some("Window style"), |ui| {
            let w = ui.available_width();
            let tile_w = (w - 12.0) / 2.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for (skin, label) in [(Skin::Aero, "Aero"), (Skin::Modern, "Modern")] {
                    let (rect, resp) = ui.allocate_exact_size(vec2(tile_w, 150.0), Sense::click());
                    let selected = self.saved.skin == skin;
                    skin_tile(ui.painter(), th, rect, skin, selected, resp.hovered(), label);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
                        self.set_ui_pref(|c| c.skin = skin);
                    }
                }
            });
            widgets::caption(
                ui,
                th,
                "Applies instantly. Only changes this window - the wallpaper looks the same either way.",
            );
        });

        widgets::card(ui, th, Some("Light or dark"), |ui| {
            if th.aero() {
                widgets::caption(ui, th, "Aero is always the classic light glass. Switch to Modern for dark mode.");
            } else {
                let mut scheme = self.saved.scheme;
                if widgets::choice(
                    ui,
                    th,
                    &mut scheme,
                    &[(Scheme::System, "System"), (Scheme::Light, "Light"), (Scheme::Dark, "Dark")],
                ) {
                    self.set_ui_pref(|c| c.scheme = scheme);
                }
            }
        });

        widgets::card(ui, th, Some("Glass"), |ui| {
            let mut glass = self.saved.glass_effects;
            if widgets::toggle(ui, th, &mut glass, "Use real translucent glass") {
                self.set_ui_pref(|c| c.glass_effects = glass);
            }
            let status = if !self.saved.glass_effects {
                "Off - surfaces are painted solid.".to_string()
            } else if self.glass_active {
                format!("Live: {} behind this window.", if th.aero() { "Acrylic blur" } else { "Mica" })
            } else {
                "Needs Windows 11 (22H2 or newer). This PC gets the painted look instead.".to_string()
            };
            widgets::caption(ui, th, &status);
        });
    }

    fn appearance_right(&mut self, ui: &mut Ui, th: &Theme) {
        widgets::card(ui, th, Some("Background updates"), |ui| {
            let schedule = match self.saved.rotation {
                Rotation::EveryHours(h) => format!(
                    "Runs at 00:05, at every sign-in, and every {h} hour{} for the quote.",
                    if h == 1 { "" } else { "s" }
                ),
                _ => "Runs at 00:05 and at every sign-in. Nothing stays running in between.".to_string(),
            };
            ui.label(
                egui::RichText::new(if self.task_installed { "On" } else { "Off" })
                    .font(th.subtitle())
                    .color(if self.task_installed { th.c.success } else { th.c.text_dim }),
            );
            widgets::caption(ui, th, &schedule);
            ui.horizontal(|ui| {
                if self.task_installed {
                    if widgets::button(ui, th, "Turn off", None, Kind::Danger).clicked() {
                        self.disable_updates();
                    }
                } else if widgets::button(ui, th, "Save & Activate", None, Kind::Primary).clicked() {
                    self.save_and_activate();
                }
            });
        });

        widgets::card(ui, th, Some("Your files"), |ui| {
            widgets::caption(ui, th, &format!("Settings and quotes: {}", self.paths.config_dir.display()));
            widgets::caption(ui, th, &format!("Rendered wallpapers: {}", self.paths.cache_dir.display()));
            if widgets::button(ui, th, "Open settings folder", Some(Icon::Folder), Kind::Normal).clicked() {
                platform::open_folder(&self.paths.config_dir);
            }
            widgets::caption(ui, th, concat!("Doomsday Clock ", env!("CARGO_PKG_VERSION"), " - MIT licensed"));
        });
    }
}

fn draw_preview(ui: &mut Ui, th: &Theme, rect: Rect, tex: Option<&TextureHandle>) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(if th.aero() { 1 } else { 6 }), Color32::BLACK);
    match tex {
        Some(tex) => {
            egui::Image::new(tex).corner_radius(CornerRadius::same(if th.aero() { 1 } else { 6 })).paint_at(ui, rect);
        }
        None => {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Loading fonts...", th.small(), hex(0x9AA4B2));
        }
    }
    if !th.aero() {
        ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, th.c.card_border), StrokeKind::Outside);
    }
}

/// Miniature of each skin, for the style picker.
fn skin_tile(p: &egui::Painter, th: &Theme, rect: Rect, skin: Skin, selected: bool, hovered: bool, label: &str) {
    let border = if selected {
        th.accent
    } else if hovered {
        mix(th.c.card_border, th.accent, 0.5)
    } else {
        th.c.card_border
    };
    p.rect_stroke(
        rect,
        CornerRadius::same(th.card_radius() as u8),
        Stroke::new(if selected { 2.0 } else { 1.0 }, border),
        StrokeKind::Inside,
    );
    let art = Rect::from_min_size(rect.min + vec2(14.0, 12.0), vec2(rect.width() - 28.0, rect.height() - 52.0));
    match skin {
        Skin::Aero => {
            gradient(p, art, 3.0, 3.0, &[(0.0, hex(0xC9DCF1)), (1.0, hex(0x8EB1DA))]);
            p.rect_stroke(art, CornerRadius::same(3), Stroke::new(1.0, hex(0x3D5878)), StrokeKind::Inside);
            let client = Rect::from_min_max(art.min + vec2(5.0, 22.0), art.max - vec2(5.0, 5.0));
            p.rect_filled(client, CornerRadius::ZERO, Color32::WHITE);
            let pane = Rect::from_min_max(client.min, pos2(client.left() + client.width() * 0.3, client.bottom()));
            gradient(p, pane, 0.0, 0.0, &[(0.0, hex(0xF7FAFE)), (1.0, hex(0xDAE6F4))]);
            let close = Rect::from_min_size(pos2(art.right() - 26.0, art.top() + 1.0), vec2(20.0, 10.0));
            gradient(
                p,
                close,
                0.0,
                2.0,
                &[(0.0, hex(0xE8A7A0)), (0.5, hex(0xD16B5F)), (0.5, hex(0xBE3A2C)), (1.0, hex(0xD9624F))],
            );
            let bar = Rect::from_min_size(
                pos2(pane.right() + 8.0, client.top() + 10.0),
                vec2(client.right() - pane.right() - 16.0, 7.0),
            );
            gradient(
                p,
                bar,
                1.0,
                1.0,
                &[(0.0, hex(0xB6EFAB)), (0.45, hex(0x46D041)), (0.5, hex(0x16B016)), (1.0, hex(0x37C931))],
            );
            let btn = Rect::from_min_size(pos2(client.right() - 34.0, client.bottom() - 14.0), vec2(28.0, 9.0));
            gradient_box(
                p,
                btn,
                2.0,
                &[(0.0, hex(0x8EC3F4)), (0.5, hex(0x4A93DF)), (0.5, hex(0x1F66BD)), (1.0, hex(0x3A8BE0))],
                hex(0x123F78),
            );
        }
        Skin::Modern => {
            p.rect_filled(art, CornerRadius::same(6), hex(0xE9EDF2));
            let layer = Rect::from_min_max(pos2(art.left() + art.width() * 0.3, art.top() + 8.0), art.max);
            p.rect_filled(layer, CornerRadius { nw: 6, ne: 0, sw: 0, se: 6 }, hex(0xFAFAFA));
            for i in 0..3 {
                let item = Rect::from_min_size(
                    pos2(art.left() + 6.0, art.top() + 16.0 + i as f32 * 12.0),
                    vec2(art.width() * 0.3 - 12.0, 8.0),
                );
                p.rect_filled(item, CornerRadius::same(2), if i == 0 { hex(0xDCE1E7) } else { hex(0xE9EDF2) });
                if i == 0 {
                    p.rect_filled(
                        Rect::from_min_size(item.min + vec2(0.0, 1.5), vec2(2.0, 5.0)),
                        CornerRadius::same(1),
                        th.accent,
                    );
                }
            }
            let card = Rect::from_min_size(layer.min + vec2(8.0, 10.0), vec2(layer.width() - 16.0, 24.0));
            p.rect_filled(card, CornerRadius::same(4), Color32::WHITE);
            p.rect_stroke(card, CornerRadius::same(4), Stroke::new(1.0, hex(0xE5E5E5)), StrokeKind::Inside);
            let btn = Rect::from_min_size(pos2(layer.right() - 34.0, layer.bottom() - 14.0), vec2(28.0, 9.0));
            p.rect_filled(btn, CornerRadius::same(2), th.accent);
        }
    }
    let label_pos = pos2(rect.left() + 34.0, rect.bottom() - 20.0);
    let r = p.text(label_pos, Align2::LEFT_CENTER, label, th.strong(), th.c.text);
    let era = if skin == Skin::Aero { "Windows 7" } else { "Windows 11" };
    p.text(pos2(r.right() + 6.0, label_pos.y), Align2::LEFT_CENTER, era, th.small(), th.c.text_dim);
    let c = pos2(rect.left() + 20.0, label_pos.y);
    p.circle_stroke(c, 7.0, Stroke::new(1.0, if selected { th.accent } else { th.c.text_dim }));
    if selected {
        p.circle_filled(c, 4.0, th.accent);
    }
}
