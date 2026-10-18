//! The filesystem questions resolution asks, counted.
//!
//! `RTS_RESOLVE_STATS=1` prints, to stderr at the end of a graph compile, how
//! many times resolution QUERIED the disk and how many of those reached it.
//! Counting lives with the questions so that no call site can ask one uncounted.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) static RESOLVE_WRITTEN: AtomicUsize = AtomicUsize::new(0);
pub(super) static FILE_QUERIES: AtomicUsize = AtomicUsize::new(0);
pub(super) static FILE_STATS: AtomicUsize = AtomicUsize::new(0);
pub(super) static DIR_QUERIES: AtomicUsize = AtomicUsize::new(0);
pub(super) static DIR_STATS: AtomicUsize = AtomicUsize::new(0);
pub(super) static MANIFEST_QUERIES: AtomicUsize = AtomicUsize::new(0);
pub(super) static MANIFEST_READS: AtomicUsize = AtomicUsize::new(0);
pub(super) static MANIFEST_PARSES: AtomicUsize = AtomicUsize::new(0);
pub(super) static WALK_PARSES: AtomicUsize = AtomicUsize::new(0);
pub(super) static FRONT_PARSES: AtomicUsize = AtomicUsize::new(0);

pub(super) fn bump(counter: &AtomicUsize) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Zeroes every counter; called when a load begins.
pub(super) fn reset() {
    for counter in [
        &RESOLVE_WRITTEN,
        &FILE_QUERIES,
        &FILE_STATS,
        &DIR_QUERIES,
        &DIR_STATS,
        &MANIFEST_QUERIES,
        &MANIFEST_READS,
        &MANIFEST_PARSES,
        &WALK_PARSES,
        &FRONT_PARSES,
    ] {
        counter.store(0, Ordering::Relaxed);
    }
}

/// Prints the counters when `RTS_RESOLVE_STATS` is set.
pub(super) fn report() {
    if std::env::var_os("RTS_RESOLVE_STATS").is_none() {
        return;
    }
    let get = |counter: &AtomicUsize| counter.load(Ordering::Relaxed);
    eprintln!(
        "resolve-stats: resolve_written={} is_file(queries={} stats={}) is_dir(queries={} stats={}) package.json(queries={} reads={} parses={}) parses(walk={} front_end={})",
        get(&RESOLVE_WRITTEN),
        get(&FILE_QUERIES),
        get(&FILE_STATS),
        get(&DIR_QUERIES),
        get(&DIR_STATS),
        get(&MANIFEST_QUERIES),
        get(&MANIFEST_READS),
        get(&MANIFEST_PARSES),
        get(&WALK_PARSES),
        get(&FRONT_PARSES),
    );
}

/// What one `load` has already learned from the disk.
///
/// Present only while a load runs ([`scope`]): the tree does not change under a
/// walk, so a question answered once is answered for the whole of it. Outside a
/// load (the runtime resolver, `names_any_file`) there is no cache and every
/// question reaches the disk, as before.
#[derive(Default)]
struct Cache {
    files: HashMap<PathBuf, bool>,
    dirs: HashMap<PathBuf, bool>,
    manifests: HashMap<PathBuf, Option<Rc<serde_json::Value>>>,
}

thread_local! {
    static CACHE: RefCell<Option<Cache>> = const { RefCell::new(None) };
}

/// Holds the cache for as long as it lives; dropping it (also on an early
/// `?` return) removes the cache so nothing stale outlives its load.
pub(super) struct Scope;

impl Drop for Scope {
    fn drop(&mut self) {
        CACHE.with(|cache| *cache.borrow_mut() = None);
    }
}

/// Starts a fresh cache for one load.
pub(super) fn scope() -> Scope {
    CACHE.with(|cache| *cache.borrow_mut() = Some(Cache::default()));
    Scope
}

/// `path.is_file()`, remembered for the duration of a load.
pub(super) fn is_file(path: &Path) -> bool {
    bump(&FILE_QUERIES);
    let known = CACHE.with(|cache| {
        cache.borrow().as_ref().and_then(|held| held.files.get(path).copied())
    });
    if let Some(answer) = known {
        return answer;
    }
    bump(&FILE_STATS);
    let answer = path.is_file();
    CACHE.with(|cache| {
        if let Some(held) = cache.borrow_mut().as_mut() {
            held.files.insert(path.to_owned(), answer);
        }
    });
    answer
}

/// `path.is_dir()`, remembered for the duration of a load.
pub(super) fn is_dir(path: &Path) -> bool {
    bump(&DIR_QUERIES);
    let known = CACHE.with(|cache| {
        cache.borrow().as_ref().and_then(|held| held.dirs.get(path).copied())
    });
    if let Some(answer) = known {
        return answer;
    }
    bump(&DIR_STATS);
    let answer = path.is_dir();
    CACHE.with(|cache| {
        if let Some(held) = cache.borrow_mut().as_mut() {
            held.dirs.insert(path.to_owned(), answer);
        }
    });
    answer
}

/// The parsed `package.json` of a package root — `None` for absent, unreadable
/// or malformed — read and parsed once per load.
pub(super) fn manifest(root: &Path) -> Option<Rc<serde_json::Value>> {
    bump(&MANIFEST_QUERIES);
    let known = CACHE.with(|cache| {
        cache.borrow().as_ref().and_then(|held| held.manifests.get(root).cloned())
    });
    if let Some(answer) = known {
        return answer;
    }
    bump(&MANIFEST_READS);
    let answer = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|text| {
            bump(&MANIFEST_PARSES);
            serde_json::from_str(&text).ok()
        })
        .map(Rc::new);
    CACHE.with(|cache| {
        if let Some(held) = cache.borrow_mut().as_mut() {
            held.manifests.insert(root.to_owned(), answer.clone());
        }
    });
    answer
}
