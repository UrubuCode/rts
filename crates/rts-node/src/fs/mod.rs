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

mod codec;
mod dir;
mod dirent;
mod fd;
mod glob;
mod meta;
mod statfs;
mod stats;
mod symbols;
mod words;

use rts_engine::AbiType::{self, Bool, F64, Handle, I64, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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
        .done();
}
