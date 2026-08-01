//! WHERE the on-disk caches live, and how their files are named.
//!
//! One decision, in one place, so the JIT program cache ([`super::progcache`]),
//! the AOT object cache and — when the module system lands — a per-module cache
//! all agree on the layout instead of each inventing a path.
//!
//! ## The root
//!
//! - A project with a `node_modules/` directory at or above the entry file caches
//!   **inside the project**: `<project>/node_modules/.rts/`. That is where
//!   `rts clean` already looks (`rts-cli/src/cli/clean.rs`), and it keeps a
//!   project's compiled artefacts with the project — deleting the project deletes
//!   its cache, and two checkouts of the same repo do not share one.
//! - Anything else (a loose script, `rts eval`, the unit tests) caches in
//!   `%TEMP%/.rts/`. There is no project to put it in.
//!
//! ## The name
//!
//! A file-backed program is named by the hash of its **canonical path**, not of
//! its contents. That is deliberate: content-naming gives every edit of a file a
//! NEW cache file and never removes the old one, so the directory grows without
//! bound over a work session (the previous layout,
//! `%TEMP%/rts-jit-cache/prog_<contenthash>.bin`, did exactly this). Path-naming
//! gives one slot per source file, overwritten in place — which is also the only
//! shape a per-module cache can have, since a module's slot must be findable
//! from its path before anything has read it.
//!
//! Validity therefore cannot live in the filename. It lives in the `.meta`
//! sidecar, which every reader checks first (see [`super::progcache`]).
//!
//! A string program (`rts eval`, `render_source`) has no path, so it falls back
//! to being named by its content key — it has no other identity.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Directory name used both inside a project and under the temp dir.
const CACHE_DIR: &str = ".rts";

/// One program's cache slot: the directory plus the shared stem its artefacts
/// are named with (`<stem>.bin`, `<stem>.obj`, `<stem>.meta`).
pub(super) struct Slot {
    dir: PathBuf,
    stem: String,
}

impl Slot {
    /// The compiled-program manifest (bincode [`super::bake::PreludeManifest`]).
    pub(super) fn bin(&self) -> PathBuf {
        self.dir.join(format!("{}.bin", self.stem))
    }

    /// The native object bytes emitted by an AOT compile of this program.
    pub(super) fn obj(&self) -> PathBuf {
        self.dir.join(format!("{}.obj", self.stem))
    }

    /// The validity header — read before either artefact above.
    pub(super) fn meta(&self) -> PathBuf {
        self.dir.join(format!("{}.meta", self.stem))
    }

    /// Create the cache directory, best-effort. Called before a write; a failure
    /// simply makes the write fail and the cache miss next time.
    pub(super) fn ensure_dir(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
    }
}

/// The cache slot for the program whose entry file is `entry` (`None` for a
/// string program), with `content_key` used as the name only when there is no
/// path to name it by.
pub(super) fn slot(entry: Option<&Path>, content_key: u64) -> Slot {
    let dir = root(entry);
    let stem = match entry {
        Some(p) => format!("{:016x}", hash_path(p)),
        None => format!("s{content_key:016x}"),
    };
    Slot { dir, stem }
}

/// `<project>/node_modules/.rts` when a `node_modules` directory exists at or
/// above `entry`; `%TEMP%/.rts` otherwise (and always, for a string program).
pub(super) fn root(entry: Option<&Path>) -> PathBuf {
    entry
        .and_then(find_node_modules)
        .map(|nm| nm.join(CACHE_DIR))
        .unwrap_or_else(|| std::env::temp_dir().join(CACHE_DIR))
}

/// Walk up from `entry`'s directory looking for a `node_modules` directory,
/// returning the first one found.
///
/// Starting at the entry's PARENT (not the entry itself) and walking to the
/// filesystem root is the same search Node's resolver does, so a program deep in
/// `src/` finds the project root's `node_modules` rather than missing it.
fn find_node_modules(entry: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());
    let mut dir = start.parent()?;
    loop {
        let candidate = dir.join("node_modules");
        if candidate.is_dir() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// Hash of a source file's CANONICAL path.
///
/// Canonicalizing first means `./src/main.ts` and an absolute path to the same
/// file share one slot instead of compiling twice; when the file does not exist
/// yet (it always does on this path, but the call is fallible) the given path is
/// hashed as written.
fn hash_path(p: &Path) -> u64 {
    let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let mut h = DefaultHasher::new();
    canon.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program with no `node_modules` anywhere above it caches under the temp
    /// dir, and a string program always does.
    #[test]
    fn temp_root_without_node_modules() {
        let tmp = std::env::temp_dir().join(CACHE_DIR);
        assert_eq!(root(None), tmp);
        // The temp dir itself has no `node_modules` ancestor in any sane setup.
        let loose = std::env::temp_dir().join("rts-cachedir-probe.ts");
        assert_eq!(root(Some(&loose)), tmp);
    }

    /// With a `node_modules` beside the entry, the slot moves into the project.
    #[test]
    fn project_root_with_node_modules() {
        let base = std::env::temp_dir().join("rts-cachedir-test-proj");
        let nm = base.join("node_modules");
        std::fs::create_dir_all(&nm).unwrap();
        let entry = base.join("main.ts");
        std::fs::write(&entry, "").unwrap();

        let got = root(Some(&entry));
        assert!(
            got.ends_with(Path::new("node_modules").join(CACHE_DIR)),
            "expected the project cache dir, got {}",
            got.display()
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The three artefacts of one program share a stem and differ only by
    /// extension — a per-module cache depends on this being derivable from the
    /// path alone.
    #[test]
    fn artefacts_share_a_stem() {
        let s = slot(None, 0xdead_beef);
        let bin = s.bin();
        assert_eq!(bin.extension().unwrap(), "bin");
        assert_eq!(s.obj().extension().unwrap(), "obj");
        assert_eq!(s.meta().extension().unwrap(), "meta");
        assert_eq!(bin.file_stem(), s.obj().file_stem());
        assert_eq!(bin.file_stem(), s.meta().file_stem());
    }
}
