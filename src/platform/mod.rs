//! Everything OS-specific sits behind this module, with one implementation
//! per platform exposing the same functions. The rest of the app never
//! writes `#[cfg(windows)]`.
//!
//! * `windows.rs`  - the real thing: per-monitor wallpaper via
//!   `IDesktopWallpaper`, Task Scheduler, DWM Mica/Acrylic, file dialogs.
//! * `fallback.rs` - a do-nothing-harmful stand-in so the app builds, runs
//!   and can be screenshotted on Linux during development.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod fallback;
#[cfg(not(windows))]
pub use self::fallback::*;

/// One physical display, in physical pixels.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Monitor {
    /// Stable device path from Windows (used to target that monitor).
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// Font files for the settings window itself (not the wallpaper).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiFont {
    /// Segoe UI - the Windows 7 system font.
    Regular,
    Semibold,
    Light,
    /// Segoe UI Variable - the Windows 11 system font.
    ModernRegular,
    ModernSemibold,
    /// Glyph coverage for arrows, check marks and similar symbols.
    Symbols,
}
