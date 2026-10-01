//! Finds font files for family names, with a fallback chain and an on-disk
//! cache.
//!
//! Why a cache? Enumerating every installed font means opening and parsing
//! hundreds of files (100-300 ms on a typical Windows install). The
//! scheduled `--update` run only ever needs the same two fonts, so the
//! resolved file path is remembered and later runs open just that one file.
//!
//! "Gill Sans Nova" is an Office *cloud font*: Office downloads it into
//! `%LOCALAPPDATA%\Microsoft\FontCache\4\CloudFonts` and only Office apps
//! see it. We scan that folder too (see `platform::extra_font_dirs`), so if
//! you have Office 365 the real font is used even though Windows itself
//! doesn't list it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::paths::atomic_write;
use crate::platform;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Role {
    /// The big countdown headline: as heavy as the family goes.
    Countdown,
    /// The quote: a regular, readable weight.
    Quote,
}

impl Role {
    fn weight(self) -> u16 {
        match self {
            Role::Countdown => 900,
            Role::Quote => 400,
        }
    }

    /// Tried in order when the requested font isn't installed.
    fn fallbacks(self) -> &'static [(&'static str, u16)] {
        match self {
            Role::Countdown => &[
                ("Gill Sans Nova", 900),
                ("Gill Sans Ultra Bold", 900),
                ("Gill Sans MT", 700),
                ("Arial Black", 900),
                ("Segoe UI Black", 900),
                ("Segoe UI", 700),
                ("Liberation Sans", 700),
                ("DejaVu Sans", 700),
            ],
            Role::Quote => &[
                ("Gill Sans Nova", 400),
                ("Gill Sans MT", 400),
                ("Segoe UI", 400),
                ("Liberation Sans", 400),
                ("DejaVu Sans", 400),
            ],
        }
    }
}

/// A font loaded into memory, ready to rasterise.
#[derive(Clone)]
pub struct LoadedFont {
    data: Arc<Vec<u8>>,
    index: u32,
    /// The family actually used (may be a fallback).
    pub family: String,
    /// True if this is the font the user asked for, false for a fallback.
    pub exact: bool,
}

impl LoadedFont {
    fn from_file(path: &Path, index: u32, family: String, exact: bool) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        ttf_parser::Face::parse(&data, index).ok()?;
        Some(Self { data: Arc::new(data), index, family, exact })
    }

    pub fn face(&self) -> ttf_parser::Face<'_> {
        // Validated in every constructor, so this cannot fail.
        ttf_parser::Face::parse(&self.data, self.index).expect("font validated at load time")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct CacheEntry {
    request: String,
    role: Role,
    path: PathBuf,
    index: u32,
    family: String,
    exact: bool,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct FontCache {
    entries: Vec<CacheEntry>,
}

pub struct FontLibrary {
    db: Option<fontdb::Database>,
    cache: FontCache,
    cache_path: PathBuf,
    cache_dirty: bool,
    trust_cached_fallbacks: bool,
}

impl FontLibrary {
    /// `trust_cached_fallbacks`: the silent scheduled run passes `true` (fast:
    /// never rescans). The settings window passes `false`, so a font you
    /// installed since last time is picked up the moment you open the app.
    pub fn new(cache_path: PathBuf, trust_cached_fallbacks: bool) -> Self {
        let cache =
            std::fs::read_to_string(&cache_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { db: None, cache, cache_path, cache_dirty: false, trust_cached_fallbacks }
    }

    /// Resolves `request` (a family name or a font file path) to a font.
    /// Returns `None` only if the machine has no usable fonts at all.
    pub fn resolve(&mut self, request: &str, role: Role) -> Option<LoadedFont> {
        let request = request.trim();

        let trust_fallbacks = self.trust_cached_fallbacks;
        if let Some(hit) =
            self.cache.entries.iter().find(|e| e.request == request && e.role == role && (e.exact || trust_fallbacks))
            && let Some(font) = LoadedFont::from_file(&hit.path, hit.index, hit.family.clone(), hit.exact)
        {
            return Some(font);
        }

        let as_path = Path::new(request);
        let is_font_file = as_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc"));
        if is_font_file && as_path.is_file() {
            let name = as_path.file_stem().and_then(|s| s.to_str()).unwrap_or(request).to_string();
            return LoadedFont::from_file(as_path, 0, name, true);
        }

        let db = self.db.get_or_insert_with(load_database);
        let (id, exact) = find_requested(db, request, role.weight())
            .map(|id| (id, true))
            .or_else(|| role.fallbacks().iter().find_map(|(fam, w)| find_family(db, fam, *w)).map(|id| (id, false)))
            .or_else(|| {
                let q = fontdb::Query { families: &[fontdb::Family::SansSerif], ..Default::default() };
                db.query(&q).or_else(|| db.faces().next().map(|f| f.id)).map(|id| (id, false))
            })?;

        let info = db.face(id)?;
        let family = info.families.first().map(|(n, _)| n.clone()).unwrap_or_default();
        let path = match &info.source {
            fontdb::Source::File(p) | fontdb::Source::SharedFile(p, _) => p.clone(),
            fontdb::Source::Binary(_) => return None,
        };
        let font = LoadedFont::from_file(&path, info.index, family.clone(), exact)?;

        self.cache.entries.retain(|e| !(e.request == request && e.role == role));
        self.cache.entries.push(CacheEntry {
            request: request.to_string(),
            role,
            path,
            index: info.index,
            family,
            exact,
        });
        self.cache_dirty = true;
        Some(font)
    }

    /// Every installed family name, sorted - for the font picker.
    pub fn family_names(&mut self) -> Vec<String> {
        let db = self.db.get_or_insert_with(load_database);
        let mut names: Vec<String> = db.faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect();
        names.sort_unstable_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    pub fn save_cache(&mut self) {
        if self.cache_dirty {
            if let Ok(json) = serde_json::to_vec_pretty(&self.cache) {
                let _ = atomic_write(&self.cache_path, &json);
            }
            self.cache_dirty = false;
        }
    }
}

fn load_database() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for dir in platform::extra_font_dirs() {
        if dir.is_dir() {
            db.load_fonts_dir(dir);
        }
    }
    db
}

fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Matches "Gill Sans Nova Ultra Bold" whether the font calls itself that
/// (legacy family name), "GillSansNova-UltraBold" (PostScript name), or
/// family "Gill Sans Nova" + weight Ultra Bold (typographic name + weight).
fn find_requested(db: &fontdb::Database, request: &str, default_weight: u16) -> Option<fontdb::ID> {
    if request.is_empty() {
        return None;
    }
    let want = normalize(request);
    if let Some(f) = db.faces().find(|f| normalize(&f.post_script_name) == want) {
        return Some(f.id);
    }
    if let Some(id) = find_family(db, request, default_weight) {
        return Some(id);
    }
    let (family, weight) = split_weight_suffix(request)?;
    find_family(db, family, weight)
}

/// Best face in `family` (case-insensitive) for `weight`, upright preferred.
fn find_family(db: &fontdb::Database, family: &str, weight: u16) -> Option<fontdb::ID> {
    let want = family.to_lowercase();
    db.faces()
        .filter(|f| f.families.iter().any(|(n, _)| n.to_lowercase() == want))
        .min_by_key(|f| {
            let italic_penalty = if f.style == fontdb::Style::Normal { 0 } else { 10_000 };
            italic_penalty + (f.weight.0 as i32 - weight as i32).unsigned_abs()
        })
        .map(|f| f.id)
}

/// "Gill Sans Nova Ultra Bold" -> ("Gill Sans Nova", 800).
fn split_weight_suffix(request: &str) -> Option<(&str, u16)> {
    const SUFFIXES: &[(&str, u16)] = &[
        (" ultra black", 950),
        (" extra black", 950),
        (" ultra bold", 800),
        (" ultrabold", 800),
        (" extra bold", 800),
        (" extrabold", 800),
        (" semi bold", 600),
        (" semibold", 600),
        (" demi bold", 600),
        (" ultra light", 200),
        (" extra light", 200),
        (" black", 900),
        (" heavy", 900),
        (" bold", 700),
        (" medium", 500),
        (" regular", 400),
        (" book", 400),
        (" light", 300),
        (" thin", 100),
    ];
    let lower = request.to_lowercase();
    SUFFIXES.iter().find_map(|(suffix, w)| {
        lower.ends_with(suffix).then(|| (request[..request.len() - suffix.len()].trim_end(), *w))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_suffixes() {
        assert_eq!(split_weight_suffix("Gill Sans Nova Ultra Bold"), Some(("Gill Sans Nova", 800)));
        assert_eq!(split_weight_suffix("Segoe UI Black"), Some(("Segoe UI", 900)));
        assert_eq!(split_weight_suffix("Georgia"), None);
    }

    #[test]
    fn resolves_something_on_any_machine() {
        let dir = std::env::temp_dir().join(format!("ddc-font-test-{}", std::process::id()));
        let mut lib = FontLibrary::new(dir.join("cache.json"), false);
        let f = lib.resolve("Definitely Not An Installed Font 12345", Role::Countdown).expect("a fallback font");
        assert!(!f.exact);
        assert!(f.face().units_per_em() > 0);
    }
}
