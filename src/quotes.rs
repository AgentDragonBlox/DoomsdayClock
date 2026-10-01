//! The quote library and the rotation queue.
//!
//! Two files, deliberately separate:
//! * `quotes.json` - the *library*. Plain, hand-editable, shareable. Seeded
//!   from the copy embedded in the exe on first run.
//! * `state.json`  - the *rotation state* (what's showing, what's next).
//!   Machine bookkeeping; deleting it just starts a fresh shuffle.
//!
//! ## Rotation algorithm ("newest first, then shuffle")
//! * `priority` holds quotes added since the last time they could be
//!   shown, **newest first**. Anything in it always goes up next.
//! * `deck` is a shuffled pass over the whole library, popped from the end.
//! * When both are empty, every quote is reshuffled into a new deck (with
//!   the quote currently on screen kept away from the top, so the same
//!   quote never appears twice in a row).
//!
//! So a quote you just added shows on the very next rotation; once all new
//! quotes have had their turn, the full library cycles in random order with
//! no repeats until every quote has been shown once.
//!
//! Quotes added by editing `quotes.json` by hand (with or without an `id`)
//! are detected on load and treated exactly like quotes added in the app.

use std::collections::HashSet;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::paths::{Paths, atomic_write};

/// The starter library, compiled into the binary.
pub const BUNDLED_QUOTES: &str = include_str!("../assets/quotes.json");

/// Longer than this won't fit legibly on a wallpaper.
pub const MAX_QUOTE_CHARS: usize = 320;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Quote {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u32,
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<NaiveDateTime>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct Library {
    #[serde(default)]
    quotes: Vec<Quote>,
}

/// Import accepts either the full library object or a bare array.
#[derive(Deserialize)]
#[serde(untagged)]
enum ImportFile {
    Library(Library),
    Bare(Vec<Quote>),
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct RotationState {
    current: Option<u32>,
    last_rotated: Option<NaiveDateTime>,
    priority: Vec<u32>,
    deck: Vec<u32>,
    rng: u64,
}

/// Everything the scheduled run needs to remember between runs.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct AppState {
    pub rotation: RotationState,
    /// Hash of everything that went into the last wallpaper. If nothing
    /// changed, the hourly/daily run exits without rendering or writing.
    pub last_render_key: u64,
    pub wallpaper_generation: u32,
}

#[derive(Debug, Default, PartialEq)]
pub struct ImportReport {
    pub added: usize,
    pub duplicates: usize,
    pub invalid: usize,
}

#[derive(Debug, PartialEq)]
pub enum AddError {
    Empty,
    TooLong,
    Duplicate,
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AddError::Empty => "The quote is empty.",
            AddError::TooLong => "That quote is too long to fit on a wallpaper (320 characters max).",
            AddError::Duplicate => "That quote is already in your library.",
        })
    }
}

pub struct QuoteStore {
    quotes: Vec<Quote>,
    pub state: AppState,
    next_id: u32,
}

impl QuoteStore {
    /// Loads library + state, seeding the library from the bundled copy on
    /// first run, and reconciling any hand edits.
    pub fn load(paths: &Paths, now: NaiveDateTime) -> Self {
        let lib_text = std::fs::read_to_string(paths.quotes_file()).ok();
        let first_run = lib_text.is_none();
        let library: Library = lib_text
            .as_deref()
            .and_then(|t| parse_any(t).ok())
            .unwrap_or_else(|| parse_any(BUNDLED_QUOTES).unwrap_or_default());

        let state: Option<AppState> =
            std::fs::read_to_string(paths.state_file()).ok().and_then(|t| serde_json::from_str(&t).ok());

        let store = Self::from_parts(library.quotes, state, now);
        if first_run {
            // Persist the seeded library so the user has a file to edit.
            let _ = store.save(paths);
        }
        store
    }

    fn from_parts(quotes: Vec<Quote>, state: Option<AppState>, now: NaiveDateTime) -> Self {
        let had_state = state.is_some();
        let mut store = Self { quotes: Vec::new(), state: state.unwrap_or_default(), next_id: 1 };
        if store.state.rotation.rng == 0 {
            store.state.rotation.rng = seed();
        }

        // De-duplicate and assign ids to entries that don't have one.
        let mut seen_keys = HashSet::new();
        let mut seen_ids = HashSet::new();
        let mut fresh = Vec::new();
        for mut q in quotes {
            q.text = q.text.trim().to_string();
            q.author = q.author.trim().to_string();
            if q.text.is_empty() || !seen_keys.insert(dedupe_key(&q.text)) {
                continue;
            }
            if q.id == 0 || !seen_ids.insert(q.id) {
                q.id = 0;
                fresh.push(q);
            } else {
                store.quotes.push(q);
            }
        }
        store.next_id = store.quotes.iter().map(|q| q.id).max().unwrap_or(0) + 1;
        for mut q in fresh {
            q.id = store.next_id;
            store.next_id += 1;
            if q.added.is_none() && had_state {
                q.added = Some(now);
            }
            store.quotes.push(q);
        }

        store.reconcile(had_state);
        store
    }

    /// Drops dangling ids from the queues and queues any quote the state
    /// doesn't know about (= added by hand) as new.
    fn reconcile(&mut self, had_state: bool) {
        let ids: HashSet<u32> = self.quotes.iter().map(|q| q.id).collect();
        let r = &mut self.state.rotation;
        r.priority.retain(|id| ids.contains(id));
        r.deck.retain(|id| ids.contains(id));
        if r.current.is_some_and(|id| !ids.contains(&id)) {
            r.current = None;
        }

        if !had_state {
            // Fresh install: nothing is "new", just shuffle everything.
            r.priority.clear();
            r.deck.clear();
            return;
        }

        let tracked: HashSet<u32> = r.priority.iter().chain(r.deck.iter()).chain(r.current.iter()).copied().collect();
        let mut untracked: Vec<&Quote> = self.quotes.iter().filter(|q| !tracked.contains(&q.id)).collect();
        // A quote is untracked if it is new, or if it was already shown in
        // the current pass. Only the former should jump the queue: anything
        // with an `added` time older than the last rotation has had its turn.
        let last = r.last_rotated;
        untracked.retain(|q| match (q.added, last) {
            (Some(a), Some(l)) => a > l,
            (None, _) => false,
            (Some(_), None) => true,
        });
        untracked.sort_by(|a, b| b.added.cmp(&a.added).then(b.id.cmp(&a.id)));
        let mut new_front: Vec<u32> = untracked.iter().map(|q| q.id).collect();
        new_front.retain(|id| !r.priority.contains(id));
        new_front.append(&mut r.priority);
        r.priority = new_front;
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        let lib = Library { quotes: self.quotes.clone() };
        atomic_write(&paths.quotes_file(), &serde_json::to_vec_pretty(&lib).map_err(std::io::Error::other)?)?;
        atomic_write(&paths.state_file(), &serde_json::to_vec_pretty(&self.state).map_err(std::io::Error::other)?)
    }

    pub fn quotes(&self) -> &[Quote] {
        &self.quotes
    }

    pub fn len(&self) -> usize {
        self.quotes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quotes.is_empty()
    }

    pub fn get(&self, id: u32) -> Option<&Quote> {
        self.quotes.iter().find(|q| q.id == id)
    }

    /// The quote on screen right now, picking one if nothing is current.
    pub fn current(&mut self, now: NaiveDateTime) -> Option<&Quote> {
        if self.state.rotation.current.is_none() {
            self.advance(now);
        }
        let id = self.state.rotation.current?;
        self.get(id)
    }

    pub fn current_id(&self) -> Option<u32> {
        self.state.rotation.current
    }

    pub fn last_rotated(&self) -> Option<NaiveDateTime> {
        self.state.rotation.last_rotated
    }

    /// Ids waiting in the "newest first" queue.
    pub fn queued_new(&self) -> &[u32] {
        &self.state.rotation.priority
    }

    pub fn is_queued_new(&self, id: u32) -> bool {
        self.state.rotation.priority.contains(&id)
    }

    /// Moves to the next quote. Returns the new current id.
    pub fn advance(&mut self, now: NaiveDateTime) -> Option<u32> {
        if self.quotes.is_empty() {
            self.state.rotation.current = None;
            return None;
        }
        let next = if !self.state.rotation.priority.is_empty() {
            Some(self.state.rotation.priority.remove(0))
        } else {
            if self.state.rotation.deck.is_empty() {
                self.reshuffle();
            }
            self.state.rotation.deck.pop()
        };
        self.state.rotation.current = next;
        self.state.rotation.last_rotated = Some(now);
        next
    }

    fn reshuffle(&mut self) {
        let r = &mut self.state.rotation;
        r.deck = self.quotes.iter().map(|q| q.id).collect();
        let mut rng = SplitMix64(r.rng);
        // Fisher-Yates.
        for i in (1..r.deck.len()).rev() {
            let j = (rng.next() % (i as u64 + 1)) as usize;
            r.deck.swap(i, j);
        }
        r.rng = rng.0;
        // The deck is popped from the end: keep the on-screen quote off it.
        if r.deck.len() > 1 && r.deck.last() == r.current.as_ref() {
            let last = r.deck.len() - 1;
            r.deck.swap(0, last);
        }
    }

    /// Adds a quote typed into the app. It jumps to the front of the queue.
    pub fn add(&mut self, text: &str, author: &str, now: NaiveDateTime) -> Result<u32, AddError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(AddError::Empty);
        }
        if text.chars().count() > MAX_QUOTE_CHARS {
            return Err(AddError::TooLong);
        }
        let key = dedupe_key(text);
        if self.quotes.iter().any(|q| dedupe_key(&q.text) == key) {
            return Err(AddError::Duplicate);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.quotes.push(Quote { id, text: text.to_string(), author: author.trim().to_string(), added: Some(now) });
        self.state.rotation.priority.insert(0, id);
        Ok(id)
    }

    /// Merges another quotes file. New quotes keep their order from the file
    /// and go ahead of everything else in the queue.
    pub fn import_json(&mut self, json: &str, now: NaiveDateTime) -> Result<ImportReport, String> {
        let lib = parse_any(json).map_err(|e| format!("Not a valid quotes file: {e}"))?;
        let mut existing: HashSet<String> = self.quotes.iter().map(|q| dedupe_key(&q.text)).collect();
        let mut report = ImportReport::default();
        let mut batch = Vec::new();
        for q in lib.quotes {
            let text = q.text.trim();
            if text.is_empty() || text.chars().count() > MAX_QUOTE_CHARS {
                report.invalid += 1;
                continue;
            }
            if !existing.insert(dedupe_key(text)) {
                report.duplicates += 1;
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.quotes.push(Quote {
                id,
                text: text.to_string(),
                author: q.author.trim().to_string(),
                added: Some(now),
            });
            batch.push(id);
            report.added += 1;
        }
        let r = &mut self.state.rotation;
        batch.append(&mut r.priority);
        r.priority = batch;
        Ok(report)
    }

    pub fn remove(&mut self, id: u32) {
        self.quotes.retain(|q| q.id != id);
        let r = &mut self.state.rotation;
        r.priority.retain(|&x| x != id);
        r.deck.retain(|&x| x != id);
        if r.current == Some(id) {
            r.current = None;
        }
    }
}

fn parse_any(text: &str) -> Result<Library, serde_json::Error> {
    Ok(match serde_json::from_str::<ImportFile>(text)? {
        ImportFile::Library(l) => l,
        ImportFile::Bare(quotes) => Library { quotes },
    })
}

/// Case-, punctuation- and whitespace-insensitive identity, so "Done is
/// better than perfect." and "done is better than perfect" are one quote.
fn dedupe_key(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn seed() -> u64 {
    let t =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    (t ^ (std::process::id() as u64).rotate_left(32)) | 1
}

/// SplitMix64: a 5-line, statistically solid PRNG. Shuffling a quote deck
/// does not justify pulling in the `rand` crate and its dependency tree.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    fn store(n: u32) -> QuoteStore {
        let quotes = (1..=n)
            .map(|i| Quote { id: i, text: format!("Quote number {i}"), author: String::new(), added: None })
            .collect();
        QuoteStore::from_parts(quotes, None, t(0))
    }

    #[test]
    fn full_pass_shows_every_quote_once() {
        let mut s = store(25);
        let mut seen = HashSet::new();
        for _ in 0..25 {
            assert!(seen.insert(s.advance(t(1)).unwrap()));
        }
        assert_eq!(seen.len(), 25);
    }

    #[test]
    fn never_repeats_across_reshuffle_boundary() {
        let mut s = store(3);
        let mut last = s.advance(t(0));
        for _ in 0..300 {
            let next = s.advance(t(0));
            assert_ne!(next, last);
            last = next;
        }
    }

    #[test]
    fn newest_added_goes_first() {
        let mut s = store(10);
        s.advance(t(0));
        let a = s.add("First new quote", "", t(1)).unwrap();
        let b = s.add("Second new quote", "", t(2)).unwrap();
        assert_eq!(s.advance(t(3)), Some(b));
        assert_eq!(s.advance(t(4)), Some(a));
        assert!(s.queued_new().is_empty());
    }

    #[test]
    fn import_dedupes_and_queues_in_file_order() {
        let mut s = store(3);
        let json =
            r#"[{"text":"quote NUMBER 1!"},{"text":"Brand new one","author":"Me"},{"text":"Another"},{"text":"  "}]"#;
        let report = s.import_json(json, t(1)).unwrap();
        assert_eq!(report, ImportReport { added: 2, duplicates: 1, invalid: 1 });
        let first = s.advance(t(2)).unwrap();
        assert_eq!(s.get(first).unwrap().text, "Brand new one");
    }

    #[test]
    fn hand_added_quotes_are_detected_as_new() {
        let mut s = store(5);
        s.advance(t(1));
        let mut quotes = s.quotes().to_vec();
        quotes.push(Quote {
            id: 0,
            text: "Typed straight into quotes.json".into(),
            author: String::new(),
            added: None,
        });
        let mut reloaded = QuoteStore::from_parts(quotes, Some(s.state.clone()), t(2));
        let next = reloaded.advance(t(3)).unwrap();
        assert_eq!(reloaded.get(next).unwrap().text, "Typed straight into quotes.json");
    }

    #[test]
    fn removing_current_quote_is_safe() {
        let mut s = store(2);
        let id = s.advance(t(0)).unwrap();
        s.remove(id);
        assert_ne!(s.current(t(1)).map(|q| q.id), Some(id));
    }

    #[test]
    fn bundled_library_parses_and_is_clean() {
        let lib = parse_any(BUNDLED_QUOTES).expect("assets/quotes.json must parse");
        assert!(lib.quotes.len() >= 200, "starter library should be big");
        let mut keys = HashSet::new();
        for q in &lib.quotes {
            assert!(!q.text.trim().is_empty());
            assert!(q.text.chars().count() <= MAX_QUOTE_CHARS, "too long: {}", q.text);
            assert!(keys.insert(dedupe_key(&q.text)), "duplicate: {}", q.text);
        }
    }
}
