//! Orchestrates one wallpaper update: rotate the quote if due, skip if
//! nothing visible changed, otherwise render once per distinct monitor size
//! and hand the files to Windows.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use tiny_skia::Pixmap;

use crate::config::{self, Config};
use crate::countdown;
use crate::fonts::{FontLibrary, LoadedFont, Role};
use crate::paths::Paths;
use crate::platform::{self, Monitor};
use crate::quotes::{Quote, QuoteStore};
use crate::render::{self, Fonts, Scene};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trigger {
    /// The scheduled background run: rotates quotes when due, and skips all
    /// work if the wallpaper would come out identical.
    Scheduled,
    /// "Save & Activate" or "Next quote" in the app: always re-applies.
    Manual,
}

#[derive(Debug)]
pub struct Outcome {
    pub rendered: bool,
    pub monitors: usize,
}

pub fn update(
    paths: &Paths,
    cfg: &Config,
    store: &mut QuoteStore,
    fonts: &mut FontLibrary,
    trigger: Trigger,
) -> Result<Outcome, String> {
    let now = config::now();
    let today = now.date();

    if cfg.show_quotes && trigger == Trigger::Scheduled && cfg.rotation.is_due(store.last_rotated(), now) {
        store.advance(now);
    }
    let quote = if cfg.show_quotes { store.current(now).cloned() } else { None };
    let monitors = platform::monitors();

    let key = render_key(cfg, today, quote.as_ref(), &monitors);
    if trigger == Trigger::Scheduled && key == store.state.last_render_key {
        // Nothing on screen would change (e.g. an hourly run mid-day with a
        // daily quote): no render, no disk write, no wallpaper API call.
        store.save(paths).map_err(|e| e.to_string())?;
        return Ok(Outcome { rendered: false, monitors: monitors.len() });
    }

    let (head_font, quote_font) = resolve_fonts(cfg, fonts)?;
    let generation = store.state.wallpaper_generation.wrapping_add(1);

    // Monitors that share a resolution share one rendered file.
    let mut by_size: HashMap<(u32, u32), PathBuf> = HashMap::new();
    let mut targets = Vec::with_capacity(monitors.len());
    for (i, m) in monitors.iter().enumerate() {
        let path = match by_size.get(&(m.width, m.height)) {
            Some(p) => p.clone(),
            None => {
                let pixmap = render_scene(cfg, quote.as_ref(), today, &head_font, &quote_font, m.width, m.height);
                let path = paths.wallpaper_file(i, generation);
                render::write_bmp(&pixmap, &path).map_err(|e| format!("Couldn't write the wallpaper image: {e}"))?;
                by_size.insert((m.width, m.height), path.clone());
                path
            }
        };
        targets.push((m.clone(), path));
    }

    platform::apply_wallpapers(&targets)?;

    // Remove the previous generation's files now that Windows has the new ones.
    for i in 0..monitors.len() {
        let _ = std::fs::remove_file(paths.wallpaper_file(i, generation.wrapping_add(1)));
    }

    store.state.last_render_key = key;
    store.state.wallpaper_generation = generation;
    store.save(paths).map_err(|e| format!("Couldn't save state: {e}"))?;
    fonts.save_cache();
    Ok(Outcome { rendered: true, monitors: monitors.len() })
}

pub fn resolve_fonts(cfg: &Config, fonts: &mut FontLibrary) -> Result<(LoadedFont, LoadedFont), String> {
    let head = fonts.resolve(&cfg.countdown_font, Role::Countdown).ok_or("No usable fonts are installed.")?;
    let quote = fonts.resolve(&cfg.quote_font, Role::Quote).ok_or("No usable fonts are installed.")?;
    Ok((head, quote))
}

/// The same renderer the real wallpaper uses, at any size (the app's live
/// preview calls this at thumbnail size).
pub fn render_scene(
    cfg: &Config,
    quote: Option<&Quote>,
    today: chrono::NaiveDate,
    head_font: &LoadedFont,
    quote_font: &LoadedFont,
    width: u32,
    height: u32,
) -> Pixmap {
    let headline = countdown::headline(cfg.target_date, today);
    let color = countdown::text_color(cfg.color_mode, cfg.created_date, cfg.target_date, today);
    let scene = Scene { headline: &headline, color, quote: quote.map(|q| (q.text.as_str(), q.author.as_str())) };
    render::render(&scene, &Fonts { headline: head_font, quote: quote_font }, width, height)
}

fn render_key(cfg: &Config, today: chrono::NaiveDate, quote: Option<&Quote>, monitors: &[Monitor]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (cfg.target_date, cfg.created_date, cfg.color_mode, &cfg.countdown_font, &cfg.quote_font, today).hash(&mut h);
    quote.map(|q| (q.id, &q.text, &q.author)).hash(&mut h);
    for m in monitors {
        (&m.id, m.width, m.height).hash(&mut h);
    }
    h.finish()
}
