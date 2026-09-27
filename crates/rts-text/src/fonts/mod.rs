//! `FontStore`: a CSS family list + weight + style → one [`Face`], or `None`.
//!
//! Sources, in order, for EACH name of the list before moving to the next:
//! 1. faces the host registered (`register` — Ahem today, `@font-face` later);
//! 2. the system directory: the four Blink defaults by file name, any other
//!    family by its `name` table (scanned once, lazily — see `system.rs`);
//! 3. a generic keyword maps to the concrete family Blink picks on Windows
//!    (serif→Times New Roman, sans-serif→Arial, monospace→Consolas,
//!    system-ui→Segoe UI — the same mapping `rts-dom`'s `font_metrics.rs`
//!    tables were measured for) and is looked up through 1 and 2.
//!
//! A name that resolves to nothing is skipped; a list that resolves to
//! nothing answers `None`. There is no last-resort face: guessing one is how a
//! headless layout ends up disagreeing with itself across machines, and the
//! caller (the adapter) owns the fallback.
//!
//! Reuse check: `rts-egui/src/app/fonts.rs` hardcodes `C:/Windows/Fonts/…`
//! paths for egui's own font definitions. It is egui's configuration, not a
//! resolver, and this crate may not depend on rts-egui (F1); the window lot
//! (T3) is where that list can start asking this store instead.

mod system;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::face::Face;

/// `font-style`, reduced to what selects a face. `oblique` asks for the
/// italic face as Blink does when no oblique face exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Normal,
    Italic,
}

/// The generic keyword → the family Blink resolves it to on Windows.
pub fn generic_family(name: &str) -> Option<&'static str> {
    Some(match name {
        "serif" | "ui-serif" => "Times New Roman",
        "sans-serif" => "Arial",
        "monospace" | "ui-monospace" => "Consolas",
        "system-ui" | "ui-sans-serif" | "-apple-system" | "blinkmacsystemfont" => "Segoe UI",
        _ => return None,
    })
}

pub struct FontStore {
    registered: Mutex<Vec<Arc<Face>>>,
    system: system::SystemFonts,
    /// (lowercased family, weight, style) → answer, `None` included, so a
    /// miss is paid once per key and not once per call.
    resolved: Mutex<HashMap<(String, u16, Style), Option<Arc<Face>>>>,
}

impl FontStore {
    /// A store over the platform's font directory (`%WINDIR%\Fonts` on
    /// Windows; none elsewhere yet — see `PLAN.md`).
    pub fn new() -> FontStore {
        Self::with_system_dir(system::default_dir())
    }

    /// A store over `dir` (or no system fonts at all with `None`).
    pub fn with_system_dir(dir: Option<PathBuf>) -> FontStore {
        FontStore { registered: Mutex::new(Vec::new()), system: system::SystemFonts::new(dir), resolved: Mutex::new(HashMap::new()) }
    }

    /// Registers `bytes` under `family`. Every face of a collection is
    /// registered; weight and style come from each face's `OS/2`. Answers how
    /// many faces were added — 0 means the bytes are not a font.
    pub fn register(&self, bytes: Vec<u8>, family: &str) -> usize {
        let data = Arc::new(bytes);
        let n = rustybuzz::ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        let faces: Vec<_> = (0..n).filter_map(|i| Face::parse(data.clone(), i, Some(family))).map(Arc::new).collect();
        let added = faces.len();
        self.registered.lock().unwrap().extend(faces);
        // A registration can change the answer of any family list.
        self.resolved.lock().unwrap().clear();
        added
    }

    /// Resolves a computed `font-family` list. `weight` is 100–900.
    pub fn resolve(&self, families: &str, weight: u16, style: Style) -> Option<Arc<Face>> {
        let key = (families.trim().to_ascii_lowercase(), weight, style);
        if let Some(hit) = self.resolved.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let answer = families.split(',').find_map(|raw| {
            let name = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            if name.is_empty() {
                return None;
            }
            self.resolve_one(name, weight, style)
        });
        self.resolved.lock().unwrap().insert(key, answer.clone());
        answer
    }

    fn resolve_one(&self, name: &str, weight: u16, style: Style) -> Option<Arc<Face>> {
        let lower = name.to_ascii_lowercase();
        // A registered face shadows the system and the generic keyword alike —
        // `@font-face { font-family: serif }` is legal and wins.
        let registered = self.registered.lock().unwrap();
        let candidates: Vec<_> = registered.iter().filter(|f| f.family().eq_ignore_ascii_case(name)).cloned().collect();
        drop(registered);
        if let Some(best) = best_match(&candidates, weight, style) {
            return Some(best);
        }
        if let Some(face) = self.system.find(&lower, weight, style) {
            return Some(face);
        }
        let concrete = generic_family(&lower)?;
        self.resolve_one(concrete, weight, style)
    }

    /// A hash of every face loaded so far — registered and system. It grows
    /// as lazily loaded faces arrive, which is the point: a layout cache
    /// keyed by it cannot survive a change of font.
    pub fn identity(&self) -> u64 {
        let mut ids: Vec<u64> = self.registered.lock().unwrap().iter().map(|f| f.id()).collect();
        ids.extend(self.system.loaded_ids());
        ids.sort_unstable();
        let mut h = DefaultHasher::new();
        ids.hash(&mut h);
        h.finish()
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new()
    }
}

/// CSS Fonts 4 §5.2, reduced: the style first (italic, else what there is),
/// then the weight — for a request ≤ 500 the nearest lighter-or-equal wins
/// before anything heavier, above 500 the nearest heavier-or-equal.
pub(crate) fn best_match(faces: &[Arc<Face>], weight: u16, style: Style) -> Option<Arc<Face>> {
    let want_italic = style == Style::Italic;
    let pool: Vec<_> = if faces.iter().any(|f| f.italic() == want_italic) {
        faces.iter().filter(|f| f.italic() == want_italic).collect()
    } else {
        faces.iter().collect()
    };
    pool.into_iter()
        .min_by_key(|f| weight_distance(weight, f.weight()))
        .cloned()
}

/// Ordering key for §5.2's weight rule: 0 is exact, the preferred direction
/// ranks before the other.
fn weight_distance(want: u16, have: u16) -> u32 {
    let (want, have) = (u32::from(want), u32::from(have));
    let preferred_down = want <= 500;
    let down = have <= want;
    let d = want.abs_diff(have);
    if down == preferred_down || d == 0 { d } else { 10_000 + d }
}
