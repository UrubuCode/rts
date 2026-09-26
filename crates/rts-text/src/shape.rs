//! Shaping through `rustybuzz`, with kerning and standard ligatures on — what
//! CSS `font-kerning: auto` and `font-variant-ligatures: normal` mean — and a
//! cache of shaped runs.
//!
//! ## The cache
//!
//! Keyed by `(face id, size bits, kerning, text)`; the value is the shaped
//! glyphs and their summed advance. A layout asks the width of the same words
//! many times per pass (min-content, max-content, the line breaker), and
//! shaping is the one step here that costs more than a table lookup — it
//! also re-parses the face's layout tables, because a `rustybuzz::Face`
//! borrows the bytes and the alternative, leaking them to `'static`, would
//! make every registered font immortal.
//!
//! Two generations of at most `GENERATION` entries each: lookups read the
//! young one, then the old one (promoting a hit); when the young one fills,
//! the old one is dropped and the young one becomes old. That bounds memory
//! at `2 × GENERATION` runs and keeps a working set without per-entry
//! bookkeeping — a real LRU would cost a list update per hit for nothing a
//! layout pass can tell apart. `cache_stats` answers hits and misses since the
//! process started, for the host to print (plan, ruler 6).
//!
//! Reuse check: `rts-egui`'s `TEXT_WIDTH_CACHE` (medida.rs) caches epaint's
//! glyph-width SUMS per egui context. It answers a different question with a
//! different font system, and it goes away when T3 replaces that measurer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rustybuzz::{Feature, UnicodeBuffer, ttf_parser::Tag};

use crate::face::Face;

/// One positioned glyph, in pixels at the size it was shaped at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u16,
    pub x_advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    /// Byte index into the run's text of the first character this glyph
    /// covers.
    pub cluster: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
}

const GENERATION: usize = 8192;

type Key = (u64, u32, bool, Box<str>);
type Run = (Arc<[Glyph]>, f32);

#[derive(Default)]
struct Cache {
    young: HashMap<Key, Run>,
    old: HashMap<Key, Run>,
    hits: u64,
    misses: u64,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Shapes `text` in `face` at `size` px. `kerning: false` turns the `kern`
/// feature off (CSS `font-kerning: none`); ligatures stay on either way.
pub fn shape(face: &Face, text: &str, size: f32, kerning: bool) -> Vec<Glyph> {
    run(face, text, size, kerning).0.to_vec()
}

/// The summed advance of the shaped run — the width the layout asks for —
/// without copying the glyphs out of the cache.
pub fn shaped_width(face: &Face, text: &str, size: f32, kerning: bool) -> f32 {
    run(face, text, size, kerning).1
}

pub fn cache_stats() -> CacheStats {
    let guard = CACHE.lock().unwrap();
    guard.as_ref().map_or_else(CacheStats::default, |c| CacheStats { hits: c.hits, misses: c.misses, entries: c.young.len() + c.old.len() })
}

fn run(face: &Face, text: &str, size: f32, kerning: bool) -> Run {
    if text.is_empty() {
        return (Arc::from([]), 0.0);
    }
    let key: Key = (face.id(), size.to_bits(), kerning, Box::from(text));
    {
        let mut guard = CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(Cache::default);
        if let Some(hit) = cache.young.get(&key) {
            let hit = hit.clone();
            cache.hits += 1;
            return hit;
        }
        if let Some(hit) = cache.old.remove(&key) {
            cache.hits += 1;
            insert(cache, key, hit.clone());
            return hit;
        }
        cache.misses += 1;
    }
    let shaped = shape_uncached(face, text, size, kerning);
    let mut guard = CACHE.lock().unwrap();
    insert(guard.get_or_insert_with(Cache::default), key, shaped.clone());
    shaped
}

fn insert(cache: &mut Cache, key: Key, value: Run) {
    if cache.young.len() >= GENERATION {
        cache.old = std::mem::take(&mut cache.young);
    }
    cache.young.insert(key, value);
}

fn shape_uncached(face: &Face, text: &str, size: f32, kerning: bool) -> Run {
    let (data, index) = face.data();
    let Some(rb) = rustybuzz::Face::from_slice(data, index) else {
        // `Face::parse` already accepted these bytes, so this is unreachable
        // short of a table ttf-parser reads and rustybuzz refuses.
        return (Arc::from([]), 0.0);
    };
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let features: &[Feature] = if kerning { &[] } else { &[Feature::new(Tag::from_bytes(b"kern"), 0, ..)] };
    let out = rustybuzz::shape(&rb, features, buffer);
    let scale = face.units_to_px(size);
    let glyphs: Arc<[Glyph]> = out
        .glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(info, pos)| Glyph {
            id: info.glyph_id as u16,
            x_advance: pos.x_advance as f32 * scale,
            x_offset: pos.x_offset as f32 * scale,
            y_offset: pos.y_offset as f32 * scale,
            cluster: info.cluster,
        })
        .collect();
    // Summed in font units and scaled once, as the generated advance table
    // is: summing rounded-per-glyph pixels would drift by glyph count.
    let units: i64 = out.glyph_positions().iter().map(|p| i64::from(p.x_advance)).sum();
    (glyphs, units as f32 * scale)
}
