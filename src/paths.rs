//! Where every file the app owns lives on disk.
//!
//! * Windows: settings + quotes in `%APPDATA%\DoomsdayClock` (roams with the
//!   user profile), rendered wallpapers + font cache in
//!   `%LOCALAPPDATA%\DoomsdayClock` (machine-specific, safe to delete).
//! * Linux (development only): `$XDG_CONFIG_HOME`/`$XDG_CACHE_HOME`.
//! * `DOOMSDAY_CLOCK_HOME=<dir>` overrides both, so tests and screenshots
//!   never touch a real profile.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        if let Some(home) = std::env::var_os("DOOMSDAY_CLOCK_HOME") {
            let home = PathBuf::from(home);
            return Self { config_dir: home.join("config"), cache_dir: home.join("cache") };
        }
        Self { config_dir: platform_dir(true), cache_dir: platform_dir(false) }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.json")
    }

    pub fn quotes_file(&self) -> PathBuf {
        self.config_dir.join("quotes.json")
    }

    pub fn state_file(&self) -> PathBuf {
        self.config_dir.join("state.json")
    }

    pub fn font_cache_file(&self) -> PathBuf {
        self.cache_dir.join("font-cache.json")
    }

    /// Two alternating file names per monitor. Windows caches wallpapers by
    /// path, so re-setting the *same* path with new contents is sometimes
    /// ignored; flipping between `_0` and `_1` guarantees a refresh.
    pub fn wallpaper_file(&self, monitor: usize, generation: u32) -> PathBuf {
        self.cache_dir.join(format!("wallpaper_{monitor}_{}.bmp", generation % 2))
    }
}

#[cfg(windows)]
fn platform_dir(roaming: bool) -> PathBuf {
    let var = if roaming { "APPDATA" } else { "LOCALAPPDATA" };
    let base = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("DoomsdayClock")
}

#[cfg(not(windows))]
fn platform_dir(config: bool) -> PathBuf {
    let (xdg, fallback) = if config { ("XDG_CONFIG_HOME", ".config") } else { ("XDG_CACHE_HOME", ".cache") };
    let base = std::env::var_os(xdg).map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(fallback)
    });
    base.join("doomsday-clock")
}

/// Write-to-temp-then-rename, so a crash or power cut mid-write can never
/// leave a half-written (and therefore unparsable) settings file behind.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
