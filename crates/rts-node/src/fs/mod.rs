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
//! - `statSync(path): object` / `lstatSync(path): object` — a `Stats`-shaped
//!   DATA object (`size`/`mtimeMs`/`atimeMs`/`ctimeMs`/`birthtimeMs`/`mode`/
//!   `isFileValue`/`isDirectoryValue`/`isSymbolicLinkValue`), built via
//!   `alloc_shaped_object` from a real `std::fs::metadata`/`symlink_metadata`.
//!   **Caveat:** Node exposes `isFile()`/`isDirectory()`/`isSymbolicLink()` as
//!   METHODS; this slice has no fn-valued-property machinery, so the boolean
//!   is exposed as plain DATA under the `*Value` suffix instead (see the
//!   doc-comment on `symbols::build_stats_object`). 0/undefined on error.
//! - `mkdirpSync(path): void` — **recursive** (`std::fs::create_dir_all`),
//!   i.e. Node's `mkdirSync(path, { recursive: true })`. The plain
//!   `mkdirSync` stays non-recursive (matches `std::fs::create_dir`).
//!
//! **Deferred** (need machinery this pure string/bool/array/object slice
//! doesn't have): `rmSync`/`rmdirSync` with a real `{recursive: true}` (deep
//! removal) options object; `existsSync`'s cousins that take options; every
//! callback-based async variant (`fs.readFile(path, cb)`, …); `watch`/
//! `watchFile` (needs an event-driven FS watcher); streams
//! (`createReadStream`/`createWriteStream`); `Dirent` objects for
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
//! `rts-std/src/fs/mod.rs::__RTS_FN_NS_FS_READDIR` for the reference pattern);
//! object results (`Stats`) are `alloc_shaped_object` handles. Symbols follow
//! the rts-node convention `__RTS_FN_NODE_FS_*`.
//!
//! All operations here are genuinely side-effecting (real disk I/O), so every
//! `Member` is registered with `pure: false` — unlike `querystring`'s pure
//! string transforms, these must never be constant-folded/deduped by the
//! engine.
//!
//! `node:fs/promises` is registered as a SEPARATE namespace (own canonical
//! key, distinct from `node:fs`) — see `promises.rs` for the Promise-settled
//! surface (`readFile`/`writeFile`/`appendFile`/`mkdir`/`rmdir`/`unlink`/
//! `rename`/`copyFile`/`readdir`/`stat`/`access`) and its module doc for the
//! settled-value convention + the `promise.new_resolved`/`new_rejected`
//! cross-crate wiring.
//!
//! Module layout: `symbols.rs` holds the base sync extern "C"
//! implementations (+ the shared `Stats`-object builder `promises.rs`
//! reuses); `promises.rs` holds the `fs/promises` extern "C" implementations;
//! this file only registers the `Member`s into the engine Registry.

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
             rmSync/mkdirSync/rmdirSync recursive-options objects, callback \
             variants, watch, streams, and Dirent are deferred (need \
             async/event machinery this flat sync slice doesn't have). See \
             node:fs/promises for the Promise-settled surface.",
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
        .member(func(
            "mkdirpSync",
            "__RTS_FN_NODE_FS_MKDIRP_SYNC",
            sig!(StrPtr => Void),
            "mkdirpSync(path: string): void",
            "Recursive mkdir (creates missing parents too) — Node's \
             mkdirSync(path, { recursive: true }).",
            symbols::__RTS_FN_NODE_FS_MKDIRP_SYNC as *const u8,
        ))
        .member(func(
            "statSync",
            "__RTS_FN_NODE_FS_STAT_SYNC",
            sig!(StrPtr => Handle),
            "statSync(path: string): object",
            "Stats-shaped data object (size/mtimeMs/atimeMs/ctimeMs/birthtimeMs/ \
             mode/isFileValue/isDirectoryValue/isSymbolicLinkValue). Follows \
             symlinks. Node exposes isFile()/isDirectory()/isSymbolicLink() as \
             METHODS; here they are DATA under the *Value suffix (no fn-valued \
             property machinery in this slice). 0 on error.",
            symbols::__RTS_FN_NODE_FS_STAT_SYNC as *const u8,
        ))
        .member(func(
            "lstatSync",
            "__RTS_FN_NODE_FS_LSTAT_SYNC",
            sig!(StrPtr => Handle),
            "lstatSync(path: string): object",
            "Same Stats-shaped object as statSync, but does NOT follow symlinks \
             (describes the link itself). 0 on error.",
            symbols::__RTS_FN_NODE_FS_LSTAT_SYNC as *const u8,
        ))
        .done();

    e.ns("node:fs/promises")
        .doc(
            "Promise-settled node:fs surface, real std::fs backed (a namespace \
             DISTINCT from node:fs — reachable as node:fs/promises). Unlike the \
             sync void ops, every member here genuinely resolves/rejects with a \
             real result/std::io::Error — see promises.rs's module doc for the \
             settled-value convention.",
        )
        .member(func(
            "readFile",
            "__RTS_FN_NODE_FS_PROMISES_READ_FILE",
            sig!(StrPtr => Handle),
            "readFile(path: string): Promise<string>",
            "Reads the whole file as UTF-8 text. Rejects with the real I/O error.",
            promises::__RTS_FN_NODE_FS_PROMISES_READ_FILE as *const u8,
        ))
        .member(func(
            "writeFile",
            "__RTS_FN_NODE_FS_PROMISES_WRITE_FILE",
            sig!(StrPtr, StrPtr => Handle),
            "writeFile(path: string, data: string): Promise<void>",
            "Creates or truncates `path` and writes `data`.",
            promises::__RTS_FN_NODE_FS_PROMISES_WRITE_FILE as *const u8,
        ))
        .member(func(
            "appendFile",
            "__RTS_FN_NODE_FS_PROMISES_APPEND_FILE",
            sig!(StrPtr, StrPtr => Handle),
            "appendFile(path: string, data: string): Promise<void>",
            "Creates `path` if missing, appends `data` otherwise.",
            promises::__RTS_FN_NODE_FS_PROMISES_APPEND_FILE as *const u8,
        ))
        .member(func(
            "mkdir",
            "__RTS_FN_NODE_FS_PROMISES_MKDIR",
            sig!(StrPtr => Handle),
            "mkdir(path: string): Promise<void>",
            "Creates the directory at `path` (non-recursive — parent must exist).",
            promises::__RTS_FN_NODE_FS_PROMISES_MKDIR as *const u8,
        ))
        .member(func(
            "rmdir",
            "__RTS_FN_NODE_FS_PROMISES_RMDIR",
            sig!(StrPtr => Handle),
            "rmdir(path: string): Promise<void>",
            "Removes the *empty* directory at `path`.",
            promises::__RTS_FN_NODE_FS_PROMISES_RMDIR as *const u8,
        ))
        .member(func(
            "unlink",
            "__RTS_FN_NODE_FS_PROMISES_UNLINK",
            sig!(StrPtr => Handle),
            "unlink(path: string): Promise<void>",
            "Removes the file at `path`.",
            promises::__RTS_FN_NODE_FS_PROMISES_UNLINK as *const u8,
        ))
        .member(func(
            "rename",
            "__RTS_FN_NODE_FS_PROMISES_RENAME",
            sig!(StrPtr, StrPtr => Handle),
            "rename(oldPath: string, newPath: string): Promise<void>",
            "Renames/moves `oldPath` to `newPath`.",
            promises::__RTS_FN_NODE_FS_PROMISES_RENAME as *const u8,
        ))
        .member(func(
            "copyFile",
            "__RTS_FN_NODE_FS_PROMISES_COPY_FILE",
            sig!(StrPtr, StrPtr => Handle),
            "copyFile(src: string, dest: string): Promise<void>",
            "Copies file contents from `src` to `dest`, overwriting `dest`.",
            promises::__RTS_FN_NODE_FS_PROMISES_COPY_FILE as *const u8,
        ))
        .member(func(
            "readdir",
            "__RTS_FN_NODE_FS_PROMISES_READDIR",
            sig!(StrPtr => Handle),
            "readdir(path: string): Promise<string[]>",
            "Lists directory entry names (no Dirent/withFileTypes).",
            promises::__RTS_FN_NODE_FS_PROMISES_READDIR as *const u8,
        ))
        .member(func(
            "stat",
            "__RTS_FN_NODE_FS_PROMISES_STAT",
            sig!(StrPtr => Handle),
            "stat(path: string): Promise<object>",
            "Stats-shaped data object, same shape as statSync. Follows symlinks.",
            promises::__RTS_FN_NODE_FS_PROMISES_STAT as *const u8,
        ))
        .member(func(
            "access",
            "__RTS_FN_NODE_FS_PROMISES_ACCESS",
            sig!(StrPtr => Handle),
            "access(path: string): Promise<void>",
            "Resolves iff `path` is real/probeable (std::fs::metadata), rejects \
             with the real I/O error otherwise.",
            promises::__RTS_FN_NODE_FS_PROMISES_ACCESS as *const u8,
        ))
        .done();
}
