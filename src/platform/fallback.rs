//! Non-Windows stand-in. Wallpapers are rendered to the cache folder but not
//! applied; the "scheduled task" is a marker file. Lets the whole app be
//! developed, tested and screenshotted on Linux.

use std::path::{Path, PathBuf};

use super::{Monitor, UiFont};
use crate::config::{Rotation, Skin};

/// `DOOMSDAY_SCREEN=2560x1440` simulates a different display.
pub fn monitors() -> Vec<Monitor> {
    let (width, height) = primary_screen_size();
    vec![Monitor { id: "primary".into(), width, height }]
}

pub fn primary_screen_size() -> (u32, u32) {
    std::env::var("DOOMSDAY_SCREEN")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1920, 1080))
}

pub fn apply_wallpapers(targets: &[(Monitor, PathBuf)]) -> Result<(), String> {
    for (m, path) in targets {
        eprintln!("[dev] would set wallpaper for {} ({}x{}) to {}", m.id, m.width, m.height, path.display());
    }
    Ok(())
}

fn marker() -> PathBuf {
    std::env::temp_dir().join("doomsday-clock-task.marker")
}

pub fn install_task(_exe: &Path, rotation: Rotation) -> Result<(), String> {
    std::fs::write(marker(), format!("{rotation:?}")).map_err(|e| e.to_string())
}

pub fn remove_task() -> Result<(), String> {
    let _ = std::fs::remove_file(marker());
    Ok(())
}

pub fn task_installed() -> bool {
    marker().exists()
}

/// `DOOMSDAY_IMPORT=/path/file.json` stands in for the open-file dialog.
pub fn pick_json_file() -> Option<PathBuf> {
    std::env::var_os("DOOMSDAY_IMPORT").map(PathBuf::from)
}

pub fn open_folder(path: &Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

pub fn extra_font_dirs() -> Vec<PathBuf> {
    Vec::new()
}

pub fn accent_color() -> Option<[u8; 3]> {
    None
}

/// Linux stand-ins for Segoe UI, used only for development screenshots.
pub fn ui_font(kind: UiFont) -> Option<PathBuf> {
    let candidates: &[&str] = match kind {
        UiFont::Regular | UiFont::ModernRegular => &[
            "/usr/share/fonts/truetype/crosextra/Carlito-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ],
        UiFont::Semibold | UiFont::ModernSemibold => &[
            "/usr/share/fonts/truetype/crosextra/Carlito-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        ],
        UiFont::Light => &["/usr/share/fonts/truetype/crosextra/Carlito-Regular.ttf"],
        UiFont::Symbols => &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"],
    };
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

pub fn prepare_headless() {}

/// Window effects (Mica/Acrylic, dark title bar) exist only on Windows.
pub struct WindowFx;

impl WindowFx {
    pub fn attach(_cc: &eframe::CreationContext<'_>) -> Option<Self> {
        None
    }

    pub fn apply(&self, _skin: Skin, _dark: bool, _glass: bool) -> bool {
        false
    }
}
