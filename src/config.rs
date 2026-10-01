//! User settings: one small JSON file, read by both the settings window and
//! the silent scheduled `--update` run.
//!
//! `#[serde(default)]` on every struct means a config written by an older
//! version (missing newer fields) still loads, with the new fields taking
//! their defaults - no migration code needed when a setting is added.

use chrono::{Local, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::paths::{Paths, atomic_write};

/// How the countdown text is coloured.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    #[default]
    StaticWhite,
    /// Fades linearly from white (on `created_date`) to red (on `target_date`).
    FadeToRed,
}

/// How often the wallpaper quote changes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
#[serde(tag = "kind", content = "hours", rename_all = "snake_case")]
pub enum Rotation {
    #[default]
    Daily,
    Weekly,
    Monthly,
    /// Every N hours. Clamped to `MIN_HOURS..=MAX_HOURS` - an hour is the
    /// floor on purpose: minute-level rotation would mean waking the machine
    /// (and rewriting a multi-megabyte bitmap) dozens of times per hour for
    /// no real benefit.
    EveryHours(u32),
}

impl Rotation {
    pub const MIN_HOURS: u32 = 1;
    /// Task Scheduler's repetition interval tops out at 31 days.
    pub const MAX_HOURS: u32 = 31 * 24;

    pub fn sanitized(self) -> Self {
        match self {
            Rotation::EveryHours(h) => Rotation::EveryHours(h.clamp(Self::MIN_HOURS, Self::MAX_HOURS)),
            other => other,
        }
    }

    /// Whether a new quote is due, given when the current one went up.
    ///
    /// Daily/weekly/monthly are *calendar* based (a new day/month), not
    /// "24h since last change", so the quote flips with the countdown at
    /// midnight even if the PC was asleep and the task ran late. Custom
    /// intervals are elapsed-time based with 10 minutes of slack, so a
    /// scheduler run that fires a few seconds early still counts.
    pub fn is_due(self, last: Option<NaiveDateTime>, now: NaiveDateTime) -> bool {
        use chrono::Datelike;
        let Some(last) = last else { return true };
        if now < last {
            // System clock moved backwards (manual change, bad RTC). Rotate
            // rather than freeze on one quote until the clock catches up.
            return true;
        }
        match self.sanitized() {
            Rotation::Daily => now.date() != last.date(),
            Rotation::Weekly => (now.date() - last.date()).num_days() >= 7,
            Rotation::Monthly => (now.year(), now.month()) != (last.year(), last.month()),
            Rotation::EveryHours(h) => now - last >= chrono::Duration::hours(h as i64) - chrono::Duration::minutes(10),
        }
    }
}

/// Which look the settings window uses.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Skin {
    /// Windows 7 Aero: glass frame, glossy buttons, gradients everywhere.
    #[default]
    Aero,
    /// Windows 11: Mica backdrop, flat layered cards, accent colour.
    Modern,
}

/// Light/dark choice for the Modern skin (Aero is always Aero).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    pub target_date: NaiveDate,
    /// "Day zero" for the fade-to-red gradient. Reset whenever the target
    /// date changes, so the fade always spans the countdown on screen.
    pub created_date: NaiveDate,
    pub color_mode: ColorMode,
    /// Family name (e.g. "Gill Sans Nova Ultra Bold") or a path to a
    /// .ttf/.otf file.
    pub countdown_font: String,
    pub quote_font: String,
    pub show_quotes: bool,
    pub rotation: Rotation,
    pub skin: Skin,
    pub scheme: Scheme,
    /// Use the real OS blur/Mica backdrop where Windows supports it.
    pub glass_effects: bool,
}

impl Default for Config {
    fn default() -> Self {
        let today = today();
        Self {
            target_date: today + chrono::Duration::days(30),
            created_date: today,
            color_mode: ColorMode::StaticWhite,
            countdown_font: "Gill Sans Nova Ultra Bold".into(),
            quote_font: "Gill Sans Nova".into(),
            show_quotes: true,
            rotation: Rotation::Daily,
            skin: Skin::Aero,
            scheme: Scheme::System,
            glass_effects: true,
        }
    }
}

impl Config {
    /// `None` = never configured (or unreadable): the scheduled run treats
    /// that as "nothing to do", the UI as "show defaults".
    pub fn load(paths: &Paths) -> Option<Self> {
        let text = std::fs::read_to_string(paths.config_file()).ok()?;
        let mut cfg: Config = serde_json::from_str(&text).ok()?;
        cfg.rotation = cfg.rotation.sanitized();
        Some(cfg)
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        atomic_write(&paths.config_file(), &json)
    }
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn daily_is_calendar_based() {
        let r = Rotation::Daily;
        assert!(!r.is_due(Some(dt("2026-10-01 00:05")), dt("2026-10-01 23:59")));
        assert!(r.is_due(Some(dt("2026-10-01 23:59")), dt("2026-10-02 00:01")));
    }

    #[test]
    fn weekly_and_monthly() {
        assert!(!Rotation::Weekly.is_due(Some(dt("2026-10-01 09:00")), dt("2026-10-07 23:00")));
        assert!(Rotation::Weekly.is_due(Some(dt("2026-10-01 09:00")), dt("2026-10-08 00:05")));
        assert!(!Rotation::Monthly.is_due(Some(dt("2026-10-01 09:00")), dt("2026-10-31 23:00")));
        assert!(Rotation::Monthly.is_due(Some(dt("2026-10-31 09:00")), dt("2026-11-01 00:05")));
    }

    #[test]
    fn custom_hours_with_slack_and_clamp() {
        let r = Rotation::EveryHours(6);
        assert!(!r.is_due(Some(dt("2026-10-01 00:00")), dt("2026-10-01 05:45")));
        assert!(r.is_due(Some(dt("2026-10-01 00:00")), dt("2026-10-01 05:55")));
        assert_eq!(Rotation::EveryHours(0).sanitized(), Rotation::EveryHours(1));
        assert_eq!(Rotation::EveryHours(10_000).sanitized(), Rotation::EveryHours(744));
    }

    #[test]
    fn clock_going_backwards_rotates() {
        assert!(Rotation::Monthly.is_due(Some(dt("2027-01-01 00:00")), dt("2026-10-01 00:00")));
    }

    #[test]
    fn old_config_without_new_fields_still_loads() {
        let json = r#"{ "target_date": "2026-12-25", "color_mode": "fade_to_red" }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.color_mode, ColorMode::FadeToRed);
        assert_eq!(cfg.rotation, Rotation::Daily);
        let custom: Config = serde_json::from_str(r#"{ "rotation": { "kind": "every_hours", "hours": 6 } }"#).unwrap();
        assert_eq!(custom.rotation, Rotation::EveryHours(6));
    }
}
