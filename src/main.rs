//! Doomsday Clock - entry point.
//!
//! One executable, several modes, chosen by the first argument:
//!
//! | Argument        | What happens                                          |
//! |-----------------|-------------------------------------------------------|
//! | (none)          | Opens the settings window.                            |
//! | `--update`      | Silent scheduled run: rotate quote if due, refresh    |
//! |                 | wallpaper only if something changed, exit.            |
//! | `--next-quote`  | Skip to the next quote and apply it now.              |
//! | `--render F [WxH]` | Render the wallpaper to a .bmp without applying it.|
//! | `--uninstall`   | Remove the scheduled task and all settings.           |
//!
//! The headless modes never create a window or load the UI toolkit's
//! renderer, which is what keeps the daily run cheap.

// Release builds are GUI-subsystem apps: no console window flashes when
// the scheduled task runs. Debug builds keep the console for log output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod countdown;
mod fonts;
mod paths;
mod platform;
mod quotes;
mod render;
mod ui;
mod wallpaper;

use std::process::ExitCode;

use config::Config;
use fonts::FontLibrary;
use paths::Paths;
use quotes::QuoteStore;
use wallpaper::Trigger;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::resolve();
    match args.first().map(String::as_str) {
        Some("--update") => headless(&paths, false),
        Some("--next-quote") => headless(&paths, true),
        Some("--render") => render_to_file(&paths, &args[1..]),
        Some("--uninstall") => uninstall(&paths),
        _ => match ui::run(paths) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
    }
}

/// The scheduled task's entry point. Silent by design: nobody is watching
/// at 00:05, so failures become an exit code (visible in Task Scheduler's
/// "Last Run Result" column), never a dialog.
fn headless(paths: &Paths, next_quote: bool) -> ExitCode {
    platform::prepare_headless();
    let Some(cfg) = Config::load(paths) else {
        return ExitCode::SUCCESS; // Never configured: nothing to do.
    };
    let now = config::now();
    let mut store = QuoteStore::load(paths, now);
    let mut fonts = FontLibrary::new(paths.font_cache_file(), true);
    let trigger = if next_quote {
        store.advance(now);
        Trigger::Manual
    } else {
        Trigger::Scheduled
    };
    match wallpaper::update(paths, &cfg, &mut store, &mut fonts, trigger) {
        Ok(outcome) => {
            if cfg!(debug_assertions) {
                eprintln!("rendered: {}, monitors: {}", outcome.rendered, outcome.monitors);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn render_to_file(paths: &Paths, args: &[String]) -> ExitCode {
    let Some(out) = args.first() else {
        eprintln!("usage: DoomsdayClock --render <file.bmp> [WIDTHxHEIGHT]");
        return ExitCode::FAILURE;
    };
    let (w, h) = args
        .get(1)
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or_else(platform::primary_screen_size);
    let cfg = Config::load(paths).unwrap_or_default();
    let now = config::now();
    let mut store = QuoteStore::load(paths, now);
    let quote = if cfg.show_quotes { store.current(now).cloned() } else { None };
    let mut fonts = FontLibrary::new(paths.font_cache_file(), false);
    let result = wallpaper::resolve_fonts(&cfg, &mut fonts).and_then(|(hf, qf)| {
        let pm = wallpaper::render_scene(&cfg, quote.as_ref(), now.date(), &hf, &qf, w, h);
        render::write_bmp(&pm, std::path::Path::new(out)).map_err(|e| e.to_string())
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn uninstall(paths: &Paths) -> ExitCode {
    let _ = platform::remove_task();
    let _ = std::fs::remove_dir_all(&paths.config_dir);
    let _ = std::fs::remove_dir_all(&paths.cache_dir);
    ExitCode::SUCCESS
}
