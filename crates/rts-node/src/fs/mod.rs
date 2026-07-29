//! `node:fs` — the synchronous filesystem surface over `std::fs`, plus the
//! `Stats` object. Real operations, Node-style errors on failure, no fabricated
//! results.
//!
//! Functions: readFileSync (Buffer / encoding→string), writeFileSync,
//! appendFileSync, existsSync/accessSync, mkdirSync (+recursive), rmdirSync,
//! rmSync (recursive), unlinkSync, renameSync/copyFileSync/truncateSync,
//! readdirSync/realpathSync, statSync/lstatSync. Stats: size/mode/mtimeMs/
//! atimeMs/ctimeMs/birthtimeMs + isFile/isDirectory/isSymbolicLink.
//!
//! Options objects (`mkdirSync(path, { recursive: true })`, `rmSync`) are read
//! via `opt_bool`, which handles both a shaped object literal (the engine's
//! default object representation: slot 0 = shape id, values keyed by
//! `global_shape_keys`) and an `Entry::Map`.
//!
//! `Stats` is an object-backed Registry class (`__rts_class = "Stats"`, the
//! StringDecoder/Hash model); statSync/lstatSync build it and its ts return type
//! drives getter/method dispatch.
//!
//! The fd family (openSync/readSync/writeSync/closeSync/fstatSync/ftruncateSync/
//! fsyncSync) is backed by a side File table in `fd.rs`.
//!
//! Deferred (need the async event loop / stream / promise subsystems): the
//! callback + `fs/promises` variants, FileHandle, Dir/Dirent +
//! opendirSync, ReadStream/WriteStream, watch/watchFile, cpSync, the full
//! options objects (mode/encoding-object/withFileTypes), chmod/chown/symlink/
//! utimes, statfs.
//!
//! Layout: `words` (helpers), `stats` (Stats object), `symbols` (extern points),
//! `mod` (registration).
//!
//! # Authoring: `#[rtse::function]`, not a hand-built `Member`
//!
//! Every free function (module-level `fs.*`/`fs.promises.*`/the private
//! `__streambridge` bridge) is authored with `#[rtse::function]` — the linker
//! symbol, `AbiType`s, ts signature and doc all DERIVE from the Rust
//! declaration instead of being spelled a second time in a hand-built `Member`
//! row. The macro always fixes `MemberFlags::NONE`, so `throws(...)` below
//! patches `THROWS` on at registration for the ops that report a real failure
//! that way (same pattern as `rts-shared/src/serde_ns`). The `Stats`/`FSWatcher`/
//! `FileHandle`/`Dir`/`Dirent` classes stay hand-built `e.class(...)` rows —
//! `#[rtse::function]` is free-functions-only (no receiver, no class); see the
//! per-file "NOT converted" notes for the remaining exceptions (a `Constant`
//! member, the 4 `node:fs/__streambridge` bridges pinned to a hardcoded literal
//! symbol via the explicit-string form, and `watchfile::__RTS_FN_NODE_FS_WATCHFILE_FIRE`
//! — same hardcoded-symbol constraint, but not a registered Member at all).
//! Two-member arity overloads (`readFileSync`/`writeFileSync`/…) use the
//! macro's `overload = "…"` key to keep a shared JS `value` while giving the
//! linker a distinct symbol per Rust fn — see `rts-macro/src/abi/scope.rs`.

mod callbacks;
mod codec;
mod dir;
mod dirent;
mod fd;
mod filehandle;
mod glob;
mod meta;
mod promises;
mod statfs;
mod stats;
mod streambridge;
mod symbols;
mod watch;
mod watchfile;
mod words;

use rts_engine::AbiType::{self, Bool, F64, Handle, I64, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind};

/// The ambient `.ts` prelude implementing `fs.ReadStream`/`fs.WriteStream` (and
/// the `createReadStream`/`createWriteStream` factories) over the ambient stream
/// `Readable`/`Writable` + the private `engine.fs_*` file-IO bridge. Included
/// AFTER `node:stream` in `PRELUDE_TS`; bound to `node:fs` by the module loader
/// (`node_reexported_globals`).
pub const FS_STREAM_TS: &str = include_str!("stream.ts");

/// The ambient `.ts` prelude implementing `fs.Utf8Stream` (the append-only
/// fixed-encoding logger sink) over the `engine.fs_*` bridges. Included AFTER
/// `FS_STREAM_TS` (shares the stream `Stream` base); bound to `node:fs`.
pub const FS_UTF8STREAM_TS: &str = include_str!("utf8stream.ts");

/// The ambient `.ts` `promises.open` wrapper — augments the native FileHandle
/// with its stream methods (createReadStream/createWriteStream/readableWebStream).
/// Bound to `node:fs/promises`'s `open`. Included AFTER the fs streams.
pub const FS_FHSTREAM_TS: &str = include_str!("fhstream.ts");

#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: rts_engine::Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// The macro fixes `MemberFlags::NONE`; patch `THROWS` on at registration for a
/// member whose body reports a real failure by setting the engine's pending-
/// error slot (`throw_io`/`throw_js_error`), same pattern as `rts-shared/src/
/// serde_ns::throws`.
fn throws(mut m: Member) -> Member {
    m.flags = MemberFlags::THROWS;
    m
}

/// Registers the `Stats` class + the `node:fs` module.
pub fn register(e: &mut Engine) {
    use symbols as s;
    use MemberKind::{InstanceGetter, InstanceMethod};

    e.class("Stats")
        .doc("Stats — filesystem metadata (node:fs statSync/lstatSync).")
        .member(m("isFile", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_FILE", "isFile(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_FILE as *const u8))
        .member(m("isDirectory", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_DIRECTORY", "isDirectory(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_DIRECTORY as *const u8))
        .member(m("isSymbolicLink", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_SYMLINK", "isSymbolicLink(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_SYMLINK as *const u8))
        .member(m("isBlockDevice", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_BLOCK", "isBlockDevice(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_BLOCK as *const u8))
        .member(m("isCharacterDevice", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_CHAR", "isCharacterDevice(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_CHAR as *const u8))
        .member(m("isFIFO", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_FIFO", "isFIFO(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_FIFO as *const u8))
        .member(m("isSocket", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_STATS_IS_SOCKET", "isSocket(): boolean", s::__RTS_FN_NODE_FS_STATS_IS_SOCKET as *const u8))
        .member(m("size", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_SIZE", "size: number", s::__RTS_FN_NODE_FS_STATS_SIZE as *const u8))
        .member(m("mode", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_MODE", "mode: number", s::__RTS_FN_NODE_FS_STATS_MODE as *const u8))
        .member(m("mtimeMs", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_MTIME_MS", "mtimeMs: number", s::__RTS_FN_NODE_FS_STATS_MTIME_MS as *const u8))
        .member(m("atimeMs", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_ATIME_MS", "atimeMs: number", s::__RTS_FN_NODE_FS_STATS_ATIME_MS as *const u8))
        .member(m("ctimeMs", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_CTIME_MS", "ctimeMs: number", s::__RTS_FN_NODE_FS_STATS_CTIME_MS as *const u8))
        .member(m("birthtimeMs", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_BIRTHTIME_MS", "birthtimeMs: number", s::__RTS_FN_NODE_FS_STATS_BIRTHTIME_MS as *const u8))
        .member(m("dev", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_DEV", "dev: number", s::__RTS_FN_NODE_FS_STATS_DEV as *const u8))
        .member(m("ino", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_INO", "ino: number", s::__RTS_FN_NODE_FS_STATS_INO as *const u8))
        .member(m("nlink", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_NLINK", "nlink: number", s::__RTS_FN_NODE_FS_STATS_NLINK as *const u8))
        .member(m("uid", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_UID", "uid: number", s::__RTS_FN_NODE_FS_STATS_UID as *const u8))
        .member(m("gid", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_GID", "gid: number", s::__RTS_FN_NODE_FS_STATS_GID as *const u8))
        .member(m("rdev", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_RDEV", "rdev: number", s::__RTS_FN_NODE_FS_STATS_RDEV as *const u8))
        .member(m("blksize", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_BLKSIZE", "blksize: number", s::__RTS_FN_NODE_FS_STATS_BLKSIZE as *const u8))
        .member(m("blocks", InstanceGetter, vec![Handle], F64, "__RTS_FN_NODE_FS_STATS_BLOCKS", "blocks: number", s::__RTS_FN_NODE_FS_STATS_BLOCKS as *const u8))
        .done();

    e.class("FSWatcher")
        .doc("FSWatcher — a filesystem watcher from fs.watch(path, listener).")
        .member(m("close", InstanceMethod, vec![Handle], Void, "__RTS_FN_NODE_FS_WATCHER_CLOSE", "close(): void", watch::__RTS_FN_NODE_FS_WATCHER_CLOSE as *const u8))
        .done();

    e.class("FileHandle")
        .doc("FileHandle — an open file handle from fs.promises.open(path, flags).")
        .member(m("close", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_FS_FH_CLOSE", "close(): object", filehandle::__RTS_FN_NODE_FS_FH_CLOSE as *const u8))
        .member(m("readFile", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_FS_FH_READ_FILE", "readFile(): object", filehandle::__RTS_FN_NODE_FS_FH_READ_FILE as *const u8))
        .member(m("readFile", InstanceMethod, vec![Handle, StrPtr], Handle, "__RTS_FN_NODE_FS_FH_READ_FILE_ENC", "readFile(encoding: string): object", filehandle::__RTS_FN_NODE_FS_FH_READ_FILE_ENC as *const u8))
        .member(m("writeFile", InstanceMethod, vec![Handle, Handle], Handle, "__RTS_FN_NODE_FS_FH_WRITE_FILE", "writeFile(data: object): object", filehandle::__RTS_FN_NODE_FS_FH_WRITE_FILE as *const u8))
        .member(m("stat", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_FS_FH_STAT", "stat(): object", filehandle::__RTS_FN_NODE_FS_FH_STAT as *const u8))
        .member(m("truncate", InstanceMethod, vec![Handle, I64], Handle, "__RTS_FN_NODE_FS_FH_TRUNCATE", "truncate(len: number): object", filehandle::__RTS_FN_NODE_FS_FH_TRUNCATE as *const u8))
        .member(m("sync", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_FS_FH_SYNC", "sync(): object", filehandle::__RTS_FN_NODE_FS_FH_SYNC as *const u8))
        .member(m("datasync", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_FS_FH_DATASYNC", "datasync(): object", filehandle::__RTS_FN_NODE_FS_FH_DATASYNC as *const u8))
        .member(m("chmod", InstanceMethod, vec![Handle, I64], Handle, "__RTS_FN_NODE_FS_FH_CHMOD", "chmod(mode: number): object", filehandle::__RTS_FN_NODE_FS_FH_CHMOD as *const u8))
        .done();

    e.class("Dir")
        .doc("Dir — an open directory handle from opendirSync(path).")
        .member(m("readSync", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_FS_DIR_READ", "readSync(): object", dir::__RTS_FN_NODE_FS_DIR_READ as *const u8))
        .member(m("closeSync", InstanceMethod, vec![Handle], Void, "__RTS_FN_NODE_FS_DIR_CLOSE", "closeSync(): void", dir::__RTS_FN_NODE_FS_DIR_CLOSE as *const u8))
        .member(m("path", InstanceGetter, vec![Handle], Handle, "__RTS_FN_NODE_FS_DIR_PATH", "path: string", dir::__RTS_FN_NODE_FS_DIR_PATH as *const u8))
        .done();

    e.class("Dirent")
        .doc("Dirent — a directory entry from readdirSync(path, { withFileTypes: true }).")
        .member(m("isFile", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_FILE", "isFile(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_FILE as *const u8))
        .member(m("isDirectory", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_DIRECTORY", "isDirectory(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_DIRECTORY as *const u8))
        .member(m("isSymbolicLink", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_SYMLINK", "isSymbolicLink(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_SYMLINK as *const u8))
        .member(m("isBlockDevice", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_BLOCK", "isBlockDevice(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_BLOCK as *const u8))
        .member(m("isCharacterDevice", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_CHAR", "isCharacterDevice(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_CHAR as *const u8))
        .member(m("isFIFO", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_FIFO", "isFIFO(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_FIFO as *const u8))
        .member(m("isSocket", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NODE_FS_DIRENT_IS_SOCKET", "isSocket(): boolean", dirent::__RTS_FN_NODE_FS_DIRENT_IS_SOCKET as *const u8))
        .member(m("name", InstanceGetter, vec![Handle], Handle, "__RTS_FN_NODE_FS_DIRENT_NAME", "name: string", dirent::__RTS_FN_NODE_FS_DIRENT_NAME as *const u8))
        .member(m("parentPath", InstanceGetter, vec![Handle], Handle, "__RTS_FN_NODE_FS_DIRENT_PARENT_PATH", "parentPath: string", dirent::__RTS_FN_NODE_FS_DIRENT_PARENT_PATH as *const u8))
        .member(m("path", InstanceGetter, vec![Handle], Handle, "__RTS_FN_NODE_FS_DIRENT_PARENT_PATH", "path: string", dirent::__RTS_FN_NODE_FS_DIRENT_PARENT_PATH as *const u8))
        .done();

    e.module("node:fs", |mm| {
        mm.doc("Filesystem (node:fs): readFileSync/writeFileSync/appendFileSync, existsSync/accessSync, mkdirSync/rmdirSync/rmSync/unlinkSync, renameSync/copyFileSync/truncateSync, readdirSync/realpathSync, statSync/lstatSync.");
        mm.registry(throws(symbols::read_file_sync_entry()));
        mm.registry(throws(symbols::read_file_sync_enc_entry()));
        mm.registry(throws(symbols::write_file_sync_entry()));
        mm.registry(throws(symbols::write_file_sync_enc_entry()));
        mm.registry(throws(symbols::append_file_sync_entry()));
        mm.registry(throws(symbols::append_file_sync_enc_entry()));
        mm.registry(throws(symbols::exists_sync_entry()));
        mm.registry(throws(symbols::access_sync_entry()));
        mm.registry(throws(symbols::mkdir_sync_entry()));
        mm.registry(throws(symbols::mkdir_sync_opts_entry()));
        mm.registry(throws(symbols::rmdir_sync_entry()));
        mm.registry(throws(symbols::rm_sync_entry()));
        mm.registry(throws(symbols::rm_sync_opts_entry()));
        mm.registry(throws(symbols::unlink_sync_entry()));
        mm.registry(throws(symbols::rename_sync_entry()));
        mm.registry(throws(symbols::copy_file_sync_entry()));
        mm.registry(throws(symbols::cp_sync_entry()));
        mm.registry(throws(symbols::cp_sync_opts_entry()));
        mm.registry(throws(symbols::truncate_sync_entry()));
        mm.registry(throws(symbols::chmod_sync_entry()));
        mm.registry(throws(symbols::link_sync_entry()));
        mm.registry(throws(symbols::utimes_sync_entry()));
        mm.registry(throws(symbols::readdir_sync_entry()));
        mm.registry(throws(dirent::readdir_sync_opts_entry()));
        mm.registry(throws(symbols::realpath_sync_entry()));
        mm.registry(throws(symbols::mkdtemp_sync_entry()));
        mm.registry(throws(symbols::readlink_sync_entry()));
        mm.registry(throws(symbols::stat_sync_entry()));
        mm.registry(throws(symbols::lstat_sync_entry()));
        mm.registry(throws(statfs::statfs_sync_entry()));
        // NOT converted: `MemberKind::Constant` — `#[rtse::function]` always
        // emits `MemberKind::Function`.
        mm.member(m("constants", MemberKind::Constant, vec![], Handle, "__RTS_FN_NODE_FS_CONSTANTS", "constants: object", symbols::__RTS_FN_NODE_FS_CONSTANTS as *const u8));
        // The file-descriptor family (open File table in fd.rs).
        mm.registry(throws(fd::open_sync_entry()));
        mm.registry(throws(fd::close_sync_entry()));
        mm.registry(throws(fd::read_sync_entry()));
        mm.registry(throws(fd::write_sync_entry()));
        mm.registry(throws(fd::fstat_sync_entry()));
        mm.registry(throws(fd::ftruncate_sync_entry()));
        mm.registry(throws(fd::fsync_sync_entry()));
        mm.registry(throws(fd::fdatasync_sync_entry()));
        mm.registry(throws(fd::fchmod_sync_entry()));
        mm.registry(throws(fd::futimes_sync_entry()));
        mm.registry(throws(fd::fchown_sync_entry()));
        mm.registry(throws(meta::symlink_sync_entry()));
        mm.registry(throws(meta::chown_sync_entry()));
        mm.registry(throws(meta::lchown_sync_entry()));
        mm.registry(throws(meta::lchmod_sync_entry()));
        mm.registry(throws(dir::opendir_sync_entry()));
        mm.registry(throws(glob::glob_sync_entry()));
        mm.registry(throws(fd::writev_sync2_entry()));
        mm.registry(throws(fd::writev_sync_entry()));
        mm.registry(throws(fd::readv_sync2_entry()));
        mm.registry(throws(fd::readv_sync_entry()));
        // Callback (err-first) forms — the work is synchronous (#207), the callback
        // is invoked once via the codegen bridge.
        mm.registry(throws(callbacks::read_file_entry()));
        mm.registry(throws(callbacks::read_file_enc_entry()));
        mm.registry(throws(callbacks::write_file_entry()));
        mm.registry(throws(callbacks::append_file_entry()));
        mm.registry(throws(callbacks::mkdir_entry()));
        mm.registry(throws(callbacks::unlink_entry()));
        mm.registry(throws(callbacks::rmdir_entry()));
        mm.registry(throws(callbacks::rm_entry()));
        mm.registry(throws(callbacks::rename_entry()));
        mm.registry(throws(callbacks::copy_file_entry()));
        mm.registry(throws(callbacks::access_entry()));
        mm.registry(throws(callbacks::chmod_entry()));
        mm.registry(throws(callbacks::stat_entry()));
        mm.registry(throws(callbacks::lstat_entry()));
        mm.registry(throws(callbacks::readdir_entry()));
        mm.registry(throws(callbacks::realpath_entry()));
        mm.registry(throws(callbacks::exists_entry()));
        mm.registry(throws(watch::watch_entry()));
        mm.registry(throws(watchfile::watch_file0_entry()));
        mm.registry(throws(watchfile::watch_file_opts_entry()));
        mm.registry(throws(watchfile::unwatch_file_entry()));
    });

    // node:fs/promises — the Promise-returning surface. Each returns an already-
    // settled Promise (the work is synchronous, #207); `await` resolves at once.
    e.module("node:fs/promises", |mm| {
        mm.doc("Filesystem promises (node:fs/promises): readFile/writeFile/appendFile, mkdir/rmdir/rm/unlink, rename/copyFile/truncate/access, stat/lstat/readdir/realpath/readlink.");
        mm.registry(throws(promises::read_file_entry()));
        mm.registry(throws(promises::read_file_enc_entry()));
        mm.registry(throws(promises::write_file_entry()));
        mm.registry(throws(promises::append_file_entry()));
        mm.registry(throws(promises::mkdir_entry()));
        mm.registry(throws(promises::unlink_entry()));
        mm.registry(throws(promises::rmdir_entry()));
        mm.registry(throws(promises::rm_entry()));
        mm.registry(throws(promises::rename_entry()));
        mm.registry(throws(promises::copy_file_entry()));
        mm.registry(throws(promises::access_entry()));
        mm.registry(throws(promises::truncate_entry()));
        mm.registry(throws(promises::stat_entry()));
        mm.registry(throws(promises::lstat_entry()));
        mm.registry(throws(promises::readdir_entry()));
        mm.registry(throws(promises::realpath_entry()));
        mm.registry(throws(promises::readlink_entry()));
        mm.registry(throws(filehandle::open_entry()));
    });

    // PRIVATE file-IO bridge for the `.ts` ReadStream/WriteStream prelude. Not
    // user-importable (`.private()`); the `engine.*` privacy gate further limits
    // callers to prelude-origin code. Each takes/returns raw PolyValue words (see
    // `streambridge.rs`); the symbols carry the `__RTS_FN_NS_ENGINE_FS_*` names the
    // engine's `engineobj` lowering + `abi_sig` reference, and harvest into the JIT
    // table like any registered member. Each is pinned to its EXACT legacy symbol
    // via `#[rtse::function("...")]` (the explicit-string form) rather than a
    // `module = "…"`-derived name, because `engineobj.rs` calls each by that
    // literal string from outside this crate — see the doc comments on
    // `streambridge::{read,write,append}_bytes` / `filehandle::open_handle`.
    e.module("node:fs/__streambridge", |mm| {
        mm.private();
        mm.doc("PRIVATE fs file-IO bridge for the ReadStream/WriteStream prelude.");
        mm.registry(streambridge::read_bytes_entry());
        mm.registry(streambridge::write_bytes_entry());
        mm.registry(streambridge::append_bytes_entry());
        mm.registry(filehandle::open_handle_entry());
    });
}
