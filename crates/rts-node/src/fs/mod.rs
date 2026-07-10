//! `node:fs` — synchronous filesystem operations, Node names/semantics.
//!
//! Native rts-node implementation (no rts-std mirror — this crate never depends
//! on `rts-shared`/`rts-std`; it links only against `rts-engine`). This slice
//! covers the flat, synchronous, string-only surface backed by real
//! `std::fs` calls:
//!
//! - `existsSync(path): boolean`
//! - `readFileSync(path): string` — UTF-8 text variant. Node's `readFileSync`
//!   *without* an encoding returns a `Buffer`; this member is the "as if called
//!   with `{ encoding: 'utf8' }`" form. The raw-`Buffer` overload is deferred
//!   (needs the Buffer/typed-array value model).
//! - `writeFileSync(path, data): void`, `appendFileSync(path, data): void`
//! - `mkdirSync(path): void` — **non-recursive** (`std::fs::create_dir`); the
//!   `{ recursive: true }` options form is deferred (needs an options object).
//! - `rmdirSync(path): void` — removes an *empty* directory
//!   (`std::fs::remove_dir`); recursive `rmdirSync`/`rmSync` with
//!   `{ recursive: true, force: true }` is deferred (options object).
//! - `unlinkSync(path): void`
//! - `renameSync(oldPath, newPath): void`
//! - `copyFileSync(src, dest): void`
//! - `readdirSync(path): string[]` — entry names only (no `withFileTypes`).
//! - `realpathSync(path): string` — `std::fs::canonicalize`.
//! - `accessSync(path): void` — real `std::fs::metadata` existence/permission
//!   probe; the `void` ABI has no error channel, so unlike Node it cannot
//!   `throw` on failure (see the doc-comment on the symbol for the caveat).
//! - `truncateSync(path, len): void` — `File::set_len` (real `ftruncate`
//!   semantics).
//! - `readlinkSync(path): string` — `std::fs::read_link`. 0 on error.
//! - `rmSync(path): void` — removes a file OR an empty directory
//!   (`remove_file`, falling back to `remove_dir`); **non-recursive** — the
//!   `{ recursive: true, force: true }` options form is deferred (options
//!   object).
//! - `mkdtempSync(prefix): string` — `prefix` + 6 OS-entropy-backed
//!   lowercase-alphanumeric chars, via `std::fs::create_dir`. 0 on error.
//!
//! **Deferred** (need machinery this pure string/bool/array slice doesn't
//! have): `statSync`/`lstatSync` (return a `Stats` object — needs the object
//! value model); `rmSync`/`rmdirSync` with a real `{recursive: true}` (deep
//! removal) options object; `existsSync`'s cousins that take options; every
//! callback-based async variant (`fs.readFile(path, cb)`, …) and the
//! `fs/promises` module (need the async/callback + Promise bridge at the
//! `node:fs` surface, not just the sync primitives); `watch`/`watchFile`
//! (needs an event-driven FS watcher); streams (`createReadStream`/
//! `createWriteStream`); `Dirent` objects for
//! `readdirSync({ withFileTypes: true })`; any `encoding`/options object
//! parameter (all calls here use UTF-8 as the sole encoding — matching
//! Node's default of no explicit encoding for buffers, and the only encoding
//! plain strings can represent).
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (`None` on null / invalid
//! UTF-8, in which case we fail soft — return the type's error sentinel or a
//! no-op, never panic); string results are interned to GC string handles;
//! `string[]` results are `Entry::Vec` of string WORDs (see
//! `rts-std/src/fs/mod.rs::__RTS_FN_NS_FS_READDIR` for the reference pattern).
//! Symbols follow the rts-node convention `__RTS_FN_NODE_FS_*`.
//!
//! All operations here are genuinely side-effecting (real disk I/O), so every
//! `Member` is registered with `pure: false` — unlike `querystring`'s pure
//! string transforms, these must never be constant-folded/deduped by the
//! engine.
//!
//! Module layout: `symbols.rs` holds the base sync extern "C" implementations;
//! `promises.rs` is the reserved (currently empty) slot for the deferred
//! `fs/promises` surface; this file only registers the `Member`s into the
//! engine Registry.

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        // Real disk I/O — never pure (must not be constant-folded/deduped).
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:fs` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.ns("node:fs")
        .doc(
            "Synchronous filesystem operations (node:fs), real std::fs backed. \
             statSync/lstatSync, rmSync/mkdirSync/rmdirSync options objects, \
             callback + fs/promises variants, watch, streams, and Dirent are \
             deferred (need object/async machinery this flat sync slice \
             doesn't have).",
        )
        .member(func(
            "existsSync",
            "__RTS_FN_NODE_FS_EXISTS_SYNC",
            sig!(StrPtr => Bool),
            "existsSync(path: string): boolean",
            "True iff `path` exists on disk.",
            symbols::__RTS_FN_NODE_FS_EXISTS_SYNC as *const u8,
        ))
        .member(func(
            "readFileSync",
            "__RTS_FN_NODE_FS_READ_FILE_SYNC",
            sig!(StrPtr => Handle),
            "readFileSync(path: string): string",
            "Reads the whole file as UTF-8 text. 0 on error (missing, denied, or \
             invalid UTF-8). Node without an encoding returns a Buffer; this is \
             the utf8-text variant — the raw-Buffer overload is deferred.",
            symbols::__RTS_FN_NODE_FS_READ_FILE_SYNC as *const u8,
        ))
        .member(func(
            "writeFileSync",
            "__RTS_FN_NODE_FS_WRITE_FILE_SYNC",
            sig!(StrPtr, StrPtr => Void),
            "writeFileSync(path: string, data: string): void",
            "Creates or truncates `path` and writes `data`.",
            symbols::__RTS_FN_NODE_FS_WRITE_FILE_SYNC as *const u8,
        ))
        .member(func(
            "appendFileSync",
            "__RTS_FN_NODE_FS_APPEND_FILE_SYNC",
            sig!(StrPtr, StrPtr => Void),
            "appendFileSync(path: string, data: string): void",
            "Creates `path` if missing, appends `data` otherwise.",
            symbols::__RTS_FN_NODE_FS_APPEND_FILE_SYNC as *const u8,
        ))
        .member(func(
            "mkdirSync",
            "__RTS_FN_NODE_FS_MKDIR_SYNC",
            sig!(StrPtr => Void),
            "mkdirSync(path: string): void",
            "Creates the directory at `path` (non-recursive — parent must exist).",
            symbols::__RTS_FN_NODE_FS_MKDIR_SYNC as *const u8,
        ))
        .member(func(
            "rmdirSync",
            "__RTS_FN_NODE_FS_RMDIR_SYNC",
            sig!(StrPtr => Void),
            "rmdirSync(path: string): void",
            "Removes the *empty* directory at `path`.",
            symbols::__RTS_FN_NODE_FS_RMDIR_SYNC as *const u8,
        ))
        .member(func(
            "unlinkSync",
            "__RTS_FN_NODE_FS_UNLINK_SYNC",
            sig!(StrPtr => Void),
            "unlinkSync(path: string): void",
            "Removes the file at `path`.",
            symbols::__RTS_FN_NODE_FS_UNLINK_SYNC as *const u8,
        ))
        .member(func(
            "renameSync",
            "__RTS_FN_NODE_FS_RENAME_SYNC",
            sig!(StrPtr, StrPtr => Void),
            "renameSync(oldPath: string, newPath: string): void",
            "Renames/moves `oldPath` to `newPath`.",
            symbols::__RTS_FN_NODE_FS_RENAME_SYNC as *const u8,
        ))
        .member(func(
            "copyFileSync",
            "__RTS_FN_NODE_FS_COPY_FILE_SYNC",
            sig!(StrPtr, StrPtr => Void),
            "copyFileSync(src: string, dest: string): void",
            "Copies file contents from `src` to `dest`, overwriting `dest`.",
            symbols::__RTS_FN_NODE_FS_COPY_FILE_SYNC as *const u8,
        ))
        .member(func(
            "readdirSync",
            "__RTS_FN_NODE_FS_READDIR_SYNC",
            sig!(StrPtr => Handle),
            "readdirSync(path: string): string[]",
            "Lists directory entry names (no Dirent/withFileTypes). 0 on error.",
            symbols::__RTS_FN_NODE_FS_READDIR_SYNC as *const u8,
        ))
        .member(func(
            "realpathSync",
            "__RTS_FN_NODE_FS_REALPATH_SYNC",
            sig!(StrPtr => Handle),
            "realpathSync(path: string): string",
            "Resolves `path` to its canonical absolute form. 0 on error.",
            symbols::__RTS_FN_NODE_FS_REALPATH_SYNC as *const u8,
        ))
        .member(func(
            "accessSync",
            "__RTS_FN_NODE_FS_ACCESS_SYNC",
            sig!(StrPtr => Void),
            "accessSync(path: string): void",
            "Real existence/permission probe (std::fs::metadata). Unlike Node, \
             the void ABI has no error channel, so this cannot throw on failure.",
            symbols::__RTS_FN_NODE_FS_ACCESS_SYNC as *const u8,
        ))
        .member(func(
            "truncateSync",
            "__RTS_FN_NODE_FS_TRUNCATE_SYNC",
            sig!(StrPtr, F64 => Void),
            "truncateSync(path: string, len: number): void",
            "Sets the file at `path` to exactly `len` bytes (real `ftruncate` \
             semantics via `File::set_len`).",
            symbols::__RTS_FN_NODE_FS_TRUNCATE_SYNC as *const u8,
        ))
        .member(func(
            "readlinkSync",
            "__RTS_FN_NODE_FS_READLINK_SYNC",
            sig!(StrPtr => Handle),
            "readlinkSync(path: string): string",
            "Resolves the target of the symlink at `path`. 0 on error.",
            symbols::__RTS_FN_NODE_FS_READLINK_SYNC as *const u8,
        ))
        .member(func(
            "rmSync",
            "__RTS_FN_NODE_FS_RM_SYNC",
            sig!(StrPtr => Void),
            "rmSync(path: string): void",
            "Removes a file OR an empty directory at `path`. Non-recursive — \
             { recursive: true, force: true } is deferred.",
            symbols::__RTS_FN_NODE_FS_RM_SYNC as *const u8,
        ))
        .member(func(
            "mkdtempSync",
            "__RTS_FN_NODE_FS_MKDTEMP_SYNC",
            sig!(StrPtr => Handle),
            "mkdtempSync(prefix: string): string",
            "Creates a unique temp directory named `prefix` + 6 OS-entropy \
             lowercase-alphanumeric chars. Returns the created path, 0 on error.",
            symbols::__RTS_FN_NODE_FS_MKDTEMP_SYNC as *const u8,
        ))
        .done();
}
