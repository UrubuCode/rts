//! `node:fs` — the synchronous filesystem surface over `std::fs`, plus the
//! `Stats` object. Real operations, Node-style errors on failure, no fabricated
//! results.
//!
//! Functions: readFileSync (Buffer / encoding→string), writeFileSync,
//! appendFileSync, existsSync, accessSync, mkdirSync (+recursive), rmdirSync,
//! rmSync (recursive), unlinkSync, renameSync, copyFileSync, truncateSync,
//! readdirSync, realpathSync, statSync, lstatSync. Stats: size/mode/mtimeMs/
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

use rts_engine::AbiType::{self, Bool, F64, Handle, I64, PolyValue, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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

#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// A module function that can throw a Node-style fs error → flagged
/// `MemberFlags::THROWS` so the engine routes the post-call pending-error slot to
/// an enclosing `try/catch` (registry_call.rs). Without the flag a builtin's
/// throw propagates uncaught.
fn func(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    let mut member = m(name, MemberKind::Function, args, ret, symbol, ts, fp);
    member.flags = MemberFlags::THROWS;
    member
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

    e.ns("node:fs")
        .doc("Filesystem (node:fs): readFileSync/writeFileSync/appendFileSync, existsSync/accessSync, mkdirSync/rmdirSync/rmSync/unlinkSync, renameSync/copyFileSync/truncateSync, readdirSync/realpathSync, statSync/lstatSync.")
        .member(func("readFileSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_READ_FILE", "readFileSync(path: string): number[]", s::__RTS_FN_NODE_FS_READ_FILE as *const u8))
        .member(func("readFileSync", vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_FS_READ_FILE_ENC", "readFileSync(path: string, encoding: string): string", s::__RTS_FN_NODE_FS_READ_FILE_ENC as *const u8))
        .member(func("writeFileSync", vec![StrPtr, Handle], Void, "__RTS_FN_NODE_FS_WRITE_FILE", "writeFileSync(path: string, data: object): void", s::__RTS_FN_NODE_FS_WRITE_FILE as *const u8))
        .member(func("writeFileSync", vec![StrPtr, StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_WRITE_FILE_ENC", "writeFileSync(path: string, data: string, encoding: string): void", s::__RTS_FN_NODE_FS_WRITE_FILE_ENC as *const u8))
        .member(func("appendFileSync", vec![StrPtr, Handle], Void, "__RTS_FN_NODE_FS_APPEND_FILE", "appendFileSync(path: string, data: object): void", s::__RTS_FN_NODE_FS_APPEND_FILE as *const u8))
        .member(func("appendFileSync", vec![StrPtr, StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_APPEND_FILE_ENC", "appendFileSync(path: string, data: string, encoding: string): void", s::__RTS_FN_NODE_FS_APPEND_FILE_ENC as *const u8))
        .member(func("existsSync", vec![StrPtr], Bool, "__RTS_FN_NODE_FS_EXISTS", "existsSync(path: string): boolean", s::__RTS_FN_NODE_FS_EXISTS as *const u8))
        .member(func("accessSync", vec![StrPtr], Void, "__RTS_FN_NODE_FS_ACCESS", "accessSync(path: string): void", s::__RTS_FN_NODE_FS_ACCESS as *const u8))
        .member(func("mkdirSync", vec![StrPtr], Void, "__RTS_FN_NODE_FS_MKDIR", "mkdirSync(path: string): void", s::__RTS_FN_NODE_FS_MKDIR as *const u8))
        .member(func("mkdirSync", vec![StrPtr, Handle], Void, "__RTS_FN_NODE_FS_MKDIR_OPTS", "mkdirSync(path: string, options: object): void", s::__RTS_FN_NODE_FS_MKDIR_OPTS as *const u8))
        .member(func("rmdirSync", vec![StrPtr], Void, "__RTS_FN_NODE_FS_RMDIR", "rmdirSync(path: string): void", s::__RTS_FN_NODE_FS_RMDIR as *const u8))
        .member(func("rmSync", vec![StrPtr], Void, "__RTS_FN_NODE_FS_RM", "rmSync(path: string): void", s::__RTS_FN_NODE_FS_RM as *const u8))
        .member(func("rmSync", vec![StrPtr, Handle], Void, "__RTS_FN_NODE_FS_RM_OPTS", "rmSync(path: string, options: object): void", s::__RTS_FN_NODE_FS_RM_OPTS as *const u8))
        .member(func("unlinkSync", vec![StrPtr], Void, "__RTS_FN_NODE_FS_UNLINK", "unlinkSync(path: string): void", s::__RTS_FN_NODE_FS_UNLINK as *const u8))
        .member(func("renameSync", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_RENAME", "renameSync(oldPath: string, newPath: string): void", s::__RTS_FN_NODE_FS_RENAME as *const u8))
        .member(func("copyFileSync", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_COPY_FILE", "copyFileSync(src: string, dest: string): void", s::__RTS_FN_NODE_FS_COPY_FILE as *const u8))
        .member(func("cpSync", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_CP", "cpSync(src: string, dest: string): void", s::__RTS_FN_NODE_FS_CP as *const u8))
        .member(func("cpSync", vec![StrPtr, StrPtr, Handle], Void, "__RTS_FN_NODE_FS_CP_OPTS", "cpSync(src: string, dest: string, options: object): void", s::__RTS_FN_NODE_FS_CP_OPTS as *const u8))
        .member(func("truncateSync", vec![StrPtr, I64], Void, "__RTS_FN_NODE_FS_TRUNCATE", "truncateSync(path: string, len: number): void", s::__RTS_FN_NODE_FS_TRUNCATE as *const u8))
        .member(func("chmodSync", vec![StrPtr, I64], Void, "__RTS_FN_NODE_FS_CHMOD", "chmodSync(path: string, mode: number): void", s::__RTS_FN_NODE_FS_CHMOD as *const u8))
        .member(func("linkSync", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_LINK", "linkSync(existingPath: string, newPath: string): void", s::__RTS_FN_NODE_FS_LINK as *const u8))
        .member(func("utimesSync", vec![StrPtr, F64, F64], Void, "__RTS_FN_NODE_FS_UTIMES", "utimesSync(path: string, atime: number, mtime: number): void", s::__RTS_FN_NODE_FS_UTIMES as *const u8))
        .member(func("readdirSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_READDIR", "readdirSync(path: string): string[]", s::__RTS_FN_NODE_FS_READDIR as *const u8))
        .member(func("readdirSync", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_FS_READDIR_OPTS", "readdirSync(path: string, options: object): object[]", dirent::__RTS_FN_NODE_FS_READDIR_OPTS as *const u8))
        .member(func("realpathSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_REALPATH", "realpathSync(path: string): string", s::__RTS_FN_NODE_FS_REALPATH as *const u8))
        .member(func("mkdtempSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_MKDTEMP", "mkdtempSync(prefix: string): string", s::__RTS_FN_NODE_FS_MKDTEMP as *const u8))
        .member(func("readlinkSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_READLINK", "readlinkSync(path: string): string", s::__RTS_FN_NODE_FS_READLINK as *const u8))
        .member(func("statSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_STAT", "statSync(path: string): Stats", s::__RTS_FN_NODE_FS_STAT as *const u8))
        .member(func("lstatSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_LSTAT", "lstatSync(path: string): Stats", s::__RTS_FN_NODE_FS_LSTAT as *const u8))
        .member(func("statfsSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_STATFS", "statfsSync(path: string): object", statfs::__RTS_FN_NODE_FS_STATFS as *const u8))
        .member(m("constants", MemberKind::Constant, vec![], Handle, "__RTS_FN_NODE_FS_CONSTANTS", "constants: object", s::__RTS_FN_NODE_FS_CONSTANTS as *const u8))
        // The file-descriptor family (open File table in fd.rs).
        .member(func("openSync", vec![StrPtr, StrPtr], I64, "__RTS_FN_NODE_FS_OPEN", "openSync(path: string, flags: string): number", fd::__RTS_FN_NODE_FS_OPEN as *const u8))
        .member(func("closeSync", vec![I64], Void, "__RTS_FN_NODE_FS_CLOSE", "closeSync(fd: number): void", fd::__RTS_FN_NODE_FS_CLOSE as *const u8))
        .member(func("readSync", vec![I64, Handle, I64, I64, I64], I64, "__RTS_FN_NODE_FS_READ", "readSync(fd: number, buffer: number[], offset: number, length: number, position: number): number", fd::__RTS_FN_NODE_FS_READ as *const u8))
        .member(func("writeSync", vec![I64, Handle, I64, I64, I64], I64, "__RTS_FN_NODE_FS_WRITE", "writeSync(fd: number, buffer: number[], offset: number, length: number, position: number): number", fd::__RTS_FN_NODE_FS_WRITE as *const u8))
        .member(func("fstatSync", vec![I64], Handle, "__RTS_FN_NODE_FS_FSTAT", "fstatSync(fd: number): Stats", fd::__RTS_FN_NODE_FS_FSTAT as *const u8))
        .member(func("ftruncateSync", vec![I64, I64], Void, "__RTS_FN_NODE_FS_FTRUNCATE", "ftruncateSync(fd: number, len: number): void", fd::__RTS_FN_NODE_FS_FTRUNCATE as *const u8))
        .member(func("fsyncSync", vec![I64], Void, "__RTS_FN_NODE_FS_FSYNC", "fsyncSync(fd: number): void", fd::__RTS_FN_NODE_FS_FSYNC as *const u8))
        .member(func("fdatasyncSync", vec![I64], Void, "__RTS_FN_NODE_FS_FDATASYNC", "fdatasyncSync(fd: number): void", fd::__RTS_FN_NODE_FS_FDATASYNC as *const u8))
        .member(func("fchmodSync", vec![I64, I64], Void, "__RTS_FN_NODE_FS_FCHMOD", "fchmodSync(fd: number, mode: number): void", fd::__RTS_FN_NODE_FS_FCHMOD as *const u8))
        .member(func("futimesSync", vec![I64, F64, F64], Void, "__RTS_FN_NODE_FS_FUTIMES", "futimesSync(fd: number, atime: number, mtime: number): void", fd::__RTS_FN_NODE_FS_FUTIMES as *const u8))
        .member(func("fchownSync", vec![I64, I64, I64], Void, "__RTS_FN_NODE_FS_FCHOWN", "fchownSync(fd: number, uid: number, gid: number): void", fd::__RTS_FN_NODE_FS_FCHOWN as *const u8))
        .member(func("symlinkSync", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_FS_SYMLINK", "symlinkSync(target: string, path: string): void", meta::__RTS_FN_NODE_FS_SYMLINK as *const u8))
        .member(func("chownSync", vec![StrPtr, I64, I64], Void, "__RTS_FN_NODE_FS_CHOWN", "chownSync(path: string, uid: number, gid: number): void", meta::__RTS_FN_NODE_FS_CHOWN as *const u8))
        .member(func("lchownSync", vec![StrPtr, I64, I64], Void, "__RTS_FN_NODE_FS_LCHOWN", "lchownSync(path: string, uid: number, gid: number): void", meta::__RTS_FN_NODE_FS_LCHOWN as *const u8))
        .member(func("lchmodSync", vec![StrPtr, I64], Void, "__RTS_FN_NODE_FS_LCHMOD", "lchmodSync(path: string, mode: number): void", meta::__RTS_FN_NODE_FS_LCHMOD as *const u8))
        .member(func("opendirSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_OPENDIR", "opendirSync(path: string): Dir", dir::__RTS_FN_NODE_FS_OPENDIR as *const u8))
        .member(func("globSync", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_GLOB", "globSync(pattern: string): string[]", glob::__RTS_FN_NODE_FS_GLOB as *const u8))
        .member(func("writevSync", vec![I64, Handle], I64, "__RTS_FN_NODE_FS_WRITEV2", "writevSync(fd: number, buffers: object[]): number", fd::__RTS_FN_NODE_FS_WRITEV2 as *const u8))
        .member(func("writevSync", vec![I64, Handle, I64], I64, "__RTS_FN_NODE_FS_WRITEV", "writevSync(fd: number, buffers: object[], position: number): number", fd::__RTS_FN_NODE_FS_WRITEV as *const u8))
        .member(func("readvSync", vec![I64, Handle], I64, "__RTS_FN_NODE_FS_READV2", "readvSync(fd: number, buffers: object[]): number", fd::__RTS_FN_NODE_FS_READV2 as *const u8))
        .member(func("readvSync", vec![I64, Handle, I64], I64, "__RTS_FN_NODE_FS_READV", "readvSync(fd: number, buffers: object[], position: number): number", fd::__RTS_FN_NODE_FS_READV as *const u8))
        // Callback (err-first) forms — the work is synchronous (#207), the callback
        // is invoked once via the codegen bridge.
        .member(func("readFile", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_READ_FILE", "readFile(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_READ_FILE as *const u8))
        .member(func("readFile", vec![StrPtr, StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_READ_FILE_ENC", "readFile(path: string, encoding: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_READ_FILE_ENC as *const u8))
        .member(func("writeFile", vec![StrPtr, Handle, PolyValue], Void, "__RTS_FN_NODE_FS_CB_WRITE_FILE", "writeFile(path: string, data: object, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_WRITE_FILE as *const u8))
        .member(func("appendFile", vec![StrPtr, Handle, PolyValue], Void, "__RTS_FN_NODE_FS_CB_APPEND_FILE", "appendFile(path: string, data: object, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_APPEND_FILE as *const u8))
        .member(func("mkdir", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_MKDIR", "mkdir(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_MKDIR as *const u8))
        .member(func("unlink", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_UNLINK", "unlink(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_UNLINK as *const u8))
        .member(func("rmdir", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_RMDIR", "rmdir(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_RMDIR as *const u8))
        .member(func("rm", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_RM", "rm(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_RM as *const u8))
        .member(func("rename", vec![StrPtr, StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_RENAME", "rename(oldPath: string, newPath: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_RENAME as *const u8))
        .member(func("copyFile", vec![StrPtr, StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_COPY_FILE", "copyFile(src: string, dest: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_COPY_FILE as *const u8))
        .member(func("access", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_ACCESS", "access(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_ACCESS as *const u8))
        .member(func("chmod", vec![StrPtr, I64, PolyValue], Void, "__RTS_FN_NODE_FS_CB_CHMOD", "chmod(path: string, mode: number, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_CHMOD as *const u8))
        .member(func("stat", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_STAT", "stat(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_STAT as *const u8))
        .member(func("lstat", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_LSTAT", "lstat(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_LSTAT as *const u8))
        .member(func("readdir", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_READDIR", "readdir(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_READDIR as *const u8))
        .member(func("realpath", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_REALPATH", "realpath(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_REALPATH as *const u8))
        .member(func("exists", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_CB_EXISTS", "exists(path: string, callback: object): void", callbacks::__RTS_FN_NODE_FS_CB_EXISTS as *const u8))
        .member(func("watch", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_FS_WATCH", "watch(path: string, listener: object): FSWatcher", watch::__RTS_FN_NODE_FS_WATCH as *const u8))
        .member(func("watchFile", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_FS_WATCHFILE0", "watchFile(path: string, listener: object): void", watchfile::__RTS_FN_NODE_FS_WATCHFILE0 as *const u8))
        .member(func("watchFile", vec![StrPtr, I64, PolyValue], Void, "__RTS_FN_NODE_FS_WATCHFILE_OPTS", "watchFile(path: string, intervalMs: number, listener: object): void", watchfile::__RTS_FN_NODE_FS_WATCHFILE_OPTS as *const u8))
        .member(func("unwatchFile", vec![StrPtr], Void, "__RTS_FN_NODE_FS_UNWATCHFILE", "unwatchFile(path: string): void", watchfile::__RTS_FN_NODE_FS_UNWATCHFILE as *const u8))
        .done();

    // node:fs/promises — the Promise-returning surface. Each returns an already-
    // settled Promise (the work is synchronous, #207); `await` resolves at once.
    use promises as pr;
    e.ns("node:fs/promises")
        .doc("Filesystem promises (node:fs/promises): readFile/writeFile/appendFile, mkdir/rmdir/rm/unlink, rename/copyFile/truncate/access, stat/lstat/readdir/realpath/readlink.")
        .member(func("readFile", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_READ_FILE", "readFile(path: string): object", pr::__RTS_FN_NODE_FS_P_READ_FILE as *const u8))
        .member(func("readFile", vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_FS_P_READ_FILE_ENC", "readFile(path: string, encoding: string): object", pr::__RTS_FN_NODE_FS_P_READ_FILE_ENC as *const u8))
        .member(func("writeFile", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_FS_P_WRITE_FILE", "writeFile(path: string, data: object): object", pr::__RTS_FN_NODE_FS_P_WRITE_FILE as *const u8))
        .member(func("appendFile", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_FS_P_APPEND_FILE", "appendFile(path: string, data: object): object", pr::__RTS_FN_NODE_FS_P_APPEND_FILE as *const u8))
        .member(func("mkdir", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_MKDIR", "mkdir(path: string): object", pr::__RTS_FN_NODE_FS_P_MKDIR as *const u8))
        .member(func("unlink", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_UNLINK", "unlink(path: string): object", pr::__RTS_FN_NODE_FS_P_UNLINK as *const u8))
        .member(func("rmdir", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_RMDIR", "rmdir(path: string): object", pr::__RTS_FN_NODE_FS_P_RMDIR as *const u8))
        .member(func("rm", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_RM", "rm(path: string): object", pr::__RTS_FN_NODE_FS_P_RM as *const u8))
        .member(func("rename", vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_FS_P_RENAME", "rename(oldPath: string, newPath: string): object", pr::__RTS_FN_NODE_FS_P_RENAME as *const u8))
        .member(func("copyFile", vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_FS_P_COPY_FILE", "copyFile(src: string, dest: string): object", pr::__RTS_FN_NODE_FS_P_COPY_FILE as *const u8))
        .member(func("access", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_ACCESS", "access(path: string): object", pr::__RTS_FN_NODE_FS_P_ACCESS as *const u8))
        .member(func("truncate", vec![StrPtr, I64], Handle, "__RTS_FN_NODE_FS_P_TRUNCATE", "truncate(path: string, len: number): object", pr::__RTS_FN_NODE_FS_P_TRUNCATE as *const u8))
        .member(func("stat", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_STAT", "stat(path: string): object", pr::__RTS_FN_NODE_FS_P_STAT as *const u8))
        .member(func("lstat", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_LSTAT", "lstat(path: string): object", pr::__RTS_FN_NODE_FS_P_LSTAT as *const u8))
        .member(func("readdir", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_READDIR", "readdir(path: string): object", pr::__RTS_FN_NODE_FS_P_READDIR as *const u8))
        .member(func("realpath", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_REALPATH", "realpath(path: string): object", pr::__RTS_FN_NODE_FS_P_REALPATH as *const u8))
        .member(func("readlink", vec![StrPtr], Handle, "__RTS_FN_NODE_FS_P_READLINK", "readlink(path: string): object", pr::__RTS_FN_NODE_FS_P_READLINK as *const u8))
        .member(func("open", vec![StrPtr, StrPtr], Handle, "__RTS_FN_NODE_FS_P_OPEN", "open(path: string, flags: string): FileHandle", filehandle::__RTS_FN_NODE_FS_P_OPEN as *const u8))
        .done();

    // PRIVATE file-IO bridge for the `.ts` ReadStream/WriteStream prelude. Not
    // user-importable (`.private()`); the `engine.*` privacy gate further limits
    // callers to prelude-origin code. Each takes/returns raw PolyValue words (see
    // `streambridge.rs`); the symbols carry the `__RTS_FN_NS_ENGINE_FS_*` names the
    // engine's `engineobj` lowering + `abi_sig` reference, and harvest into the JIT
    // table like any registered member.
    e.ns("node:fs/__streambridge")
        .private()
        .doc("PRIVATE fs file-IO bridge for the ReadStream/WriteStream prelude.")
        .member(m("readBytes", MemberKind::Function, vec![PolyValue], PolyValue, "__RTS_FN_NS_ENGINE_FS_READ_BYTES", "readBytes(path: object): object", streambridge::__RTS_FN_NS_ENGINE_FS_READ_BYTES as *const u8))
        .member(m("writeBytes", MemberKind::Function, vec![PolyValue, PolyValue], PolyValue, "__RTS_FN_NS_ENGINE_FS_WRITE_BYTES", "writeBytes(path: object, data: object): object", streambridge::__RTS_FN_NS_ENGINE_FS_WRITE_BYTES as *const u8))
        .member(m("appendBytes", MemberKind::Function, vec![PolyValue, PolyValue], PolyValue, "__RTS_FN_NS_ENGINE_FS_APPEND_BYTES", "appendBytes(path: object, data: object): object", streambridge::__RTS_FN_NS_ENGINE_FS_APPEND_BYTES as *const u8))
        .done();
}
