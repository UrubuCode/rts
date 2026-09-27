//! The table of a compiled page's resources: `path → bytes`, consulted by the
//! two natives that read a local file (`readTextFile`, `setImageFile`) BEFORE
//! the disk — and the recorder that fills it at build time (lot AOT-1).
//!
//! # One table, two modes
//!
//! `rts compile` runs the page's own loader (`loadResources` in `dom.ts`) once,
//! in a throwaway JIT, with this module RECORDING: every file the two natives
//! read from disk is kept, under the exact string the loader handed them. The
//! build places that list in the manifest; the compiled binary's startup hands
//! it back through [`declare`], and from then on the same natives answer from
//! it first. The alternative that lost was a second resolver in Rust walking
//! `<link>`/`@import`/`<script src>`/`<img>` itself: two answers to "which
//! files does this page reference", which drift the day the loader learns a
//! new case. Running the loader cannot disagree with the loader.
//!
//! # The key
//!
//! The string the loader passed, after the `file://` strip both natives already
//! apply — nothing more. The compiled binary embeds the same HTML and the same
//! resource base, so its loader produces the same strings; normalising here
//! would be a second spelling of a path rule that lives in `__resolveUrl`.
//!
//! # Why a miss falls through to the disk
//!
//! A JIT run declares nothing, and an AOT binary built before this lot carries
//! no table: both must read exactly as they did. A binary WITH a table also
//! still reads a path it did not record (a page that computes an image path at
//! run time), which is the same answer it gave before this module existed.
//!
//! Thread-local, not process-global: the natives run on the thread that owns
//! the engine `Context`, which is the thread `rts-runtime-boot::run` declares
//! from and the thread the build's throwaway JIT records on. Bytes, not cells —
//! nothing here is a reference the collector must be told about.

use std::cell::RefCell;
use std::collections::HashMap;

/// One resource: the path the loader asked for, and the bytes it got.
pub type Resource = (String, Vec<u8>);

thread_local! {
    static TABLE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    static RECORDER: RefCell<Option<Vec<Resource>>> = const { RefCell::new(None) };
}

/// Seeds the table the natives consult before the disk. Replaces whatever was
/// declared before: a process declares once, at startup, and a test that
/// declares twice means the second table.
pub fn declare(resources: Vec<Resource>) {
    TABLE.with(|table| *table.borrow_mut() = resources.into_iter().collect());
}

/// Turns recording ON for this thread, discarding anything a previous
/// recording left behind.
pub fn record_into() {
    RECORDER.with(|recorder| *recorder.borrow_mut() = Some(Vec::new()));
}

/// Turns recording OFF and answers what was recorded, in the order the natives
/// first read each path. Empty when recording was never on.
pub fn take_recorded() -> Vec<Resource> {
    RECORDER.with(|recorder| recorder.borrow_mut().take()).unwrap_or_default()
}

/// The bytes a local path answers: the declared table first, the disk second.
/// A disk hit is recorded when recording is on — once per path, since a page
/// that links one sheet twice needs it in the binary once.
pub fn read(path: &str) -> Option<Vec<u8>> {
    let path = path.strip_prefix("file://").unwrap_or(path);
    if let Some(bytes) = TABLE.with(|table| table.borrow().get(path).cloned()) {
        return Some(bytes);
    }
    let bytes = std::fs::read(path).ok()?;
    RECORDER.with(|recorder| {
        if let Some(recorded) = recorder.borrow_mut().as_mut()
            && !recorded.iter().any(|(seen, _)| seen == path)
        {
            recorded.push((path.to_owned(), bytes.clone()));
        }
    });
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::{declare, read, record_into, take_recorded};

    fn scratch(name: &str, contents: &[u8]) -> String {
        let dir = std::env::temp_dir().join("rts-dom-bridge-tabela-tests");
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("a scratch file");
        path.to_string_lossy().into_owned()
    }

    /// What a compiled binary relies on: a declared path answers from the
    /// table even when the disk says something else (or nothing at all).
    #[test]
    fn a_declared_path_answers_from_the_table_before_the_disk() {
        let on_disk = scratch("declared.css", b"from disk");
        declare(vec![(on_disk.clone(), b"from table".to_vec())]);
        assert_eq!(read(&on_disk).as_deref(), Some(&b"from table"[..]));
        assert_eq!(
            read(&format!("file://{on_disk}")).as_deref(),
            Some(&b"from table"[..]),
            "the same `file://` strip the natives applied before the table existed"
        );
        declare(Vec::new());
    }

    /// What a JIT run and an old binary rely on: a path the table does not
    /// hold is read from the disk exactly as before, and a missing one is None.
    #[test]
    fn a_path_the_table_lacks_falls_back_to_the_disk() {
        declare(vec![("elsewhere".to_owned(), b"x".to_vec())]);
        let on_disk = scratch("miss.css", b"disk bytes");
        assert_eq!(read(&on_disk).as_deref(), Some(&b"disk bytes"[..]));
        assert_eq!(read(&format!("{on_disk}.absent")), None);
        declare(Vec::new());
    }

    /// What the build relies on: recording keeps exactly the paths read from
    /// disk, in first-read order, once each — and a failed read is not kept.
    #[test]
    fn recording_keeps_each_disk_read_once_in_order() {
        let first = scratch("first.css", b"1");
        let second = scratch("second.png", b"2");
        record_into();
        read(&second);
        read(&format!("{first}.absent"));
        read(&first);
        read(&second);
        let recorded = take_recorded();
        assert_eq!(
            recorded,
            vec![(second.clone(), b"2".to_vec()), (first.clone(), b"1".to_vec())]
        );
        read(&first);
        assert!(take_recorded().is_empty(), "take_recorded turns recording off");
    }
}
