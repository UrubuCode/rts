//! Step 10, slice 2 — the RUN-PATH side of the resident prelude.
//!
//! The `rts-prelude-baker` bin (build-time) produces `prelude.o` + a manifest +
//! a generated address table. `prelude.o` is linked into `rts.exe` and the
//! generated `prelude_symbols()` is compiled into the ROOT bin (the crate that
//! owns the final link + the ONE runtime instance). At startup the root bin hands
//! both to this module via [`install`]; the run path then, INSTEAD of lowering +
//! machine-compiling the prelude every run, seeds the shape/gcell state from the
//! manifest and registers the resident symbol addresses on the `JITBuilder` so a
//! user module's `Import` of a prelude fn resolves to the linked-in code.
//!
//! Everything here is INERT until the consumer (commit 3) reads it: [`install`]
//! only fills the globals; nothing in the ordinary build path consults them yet.
//! When no baked artifact is present the root bin installs nothing and the run
//! path keeps today's fallback (lower + merge + compile the prelude).

use std::sync::{Mutex, OnceLock};

/// The installed resident prelude, if the root bin was linked with a baked
/// `prelude.o`. `None` (never installed) → the fallback path.
struct Resident {
    /// `(symbol name, resident address)` for every baked prelude fn — the raw
    /// bincode manifest bytes come alongside. Addresses stored as `usize` (a raw
    /// pointer is not `Sync`); re-cast to `*const u8` at registration time.
    symbols: Vec<(&'static str, usize)>,
    /// The bincode-serialized `PreludeManifest` (deserialized lazily by the
    /// consumer). Kept as bytes so this module carries no heavy type.
    manifest_bytes: Vec<u8>,
}

fn slot() -> &'static Mutex<Option<Resident>> {
    static R: OnceLock<Mutex<Option<Resident>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

/// Install the resident prelude produced by the baker + linked into this binary.
/// Called ONCE by the root bin at startup with the generated `prelude_symbols()`
/// table and the embedded manifest bytes. Idempotent (last install wins). A
/// binary built WITHOUT a baked prelude simply never calls this.
///
/// `symbols` is the generated `Vec<(&'static str, *const u8)>`; the pointers are
/// resident code addresses (valid for the process lifetime).
pub fn install(symbols: Vec<(&'static str, *const u8)>, manifest_bytes: Vec<u8>) {
    let symbols = symbols
        .into_iter()
        .map(|(name, ptr)| (name, ptr as usize))
        .collect();
    *slot().lock().expect("resident prelude slot poisoned") =
        Some(Resident {
            symbols,
            manifest_bytes,
        });
}

/// Whether a resident (baked, linked-in) prelude is installed — the gate the run
/// path checks to choose the resident path over the fallback.
#[allow(dead_code)]
pub(crate) fn is_installed() -> bool {
    slot()
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// The resident symbol table as `(name, address)` for `JITBuilder::symbol`, or
/// empty when none is installed.
#[allow(dead_code)]
pub(crate) fn symbols() -> Vec<(&'static str, *const u8)> {
    slot()
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref()
                .map(|r| r.symbols.iter().map(|(n, a)| (*n, *a as *const u8)).collect())
        })
        .unwrap_or_default()
}

/// The raw manifest bytes, or `None` when no resident prelude is installed.
#[allow(dead_code)]
pub(crate) fn manifest_bytes() -> Option<Vec<u8>> {
    slot()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|r| r.manifest_bytes.clone()))
}

/// Deserialize the installed manifest, or `None` when none is installed / it fails
/// to decode (a corrupt artifact falls back to the non-resident path).
#[allow(dead_code)]
pub(crate) fn manifest() -> Option<super::bake::PreludeManifest> {
    let bytes = manifest_bytes()?;
    bincode::deserialize(&bytes).ok()
}
