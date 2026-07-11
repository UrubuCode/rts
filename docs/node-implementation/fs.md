# node:fs

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:fs` (+ `node:fs/promises`) |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import fs from "node:fs"` · `import * as fs from "node:fs"` · `import { readFileSync, statSync, ... } from "node:fs"` · `import fsPromises from "node:fs/promises"` · `import { open, readFile, FileHandle } from "node:fs/promises"` · CJS `require("fs")` / `require("fs/promises")` (bare-specifier `"fs"` alias, not just `"node:fs"`) |
| Globals exposed | none (fs exposes nothing on `globalThis`; it must be imported). Consumers commonly pair it with the ambient `Buffer`/`URL`/`AbortController` globals, which are primordial/other-module concerns, not part of this spec |

## 1. Purpose

`node:fs` is Node's filesystem API: POSIX-style file and directory
manipulation (open/read/write/stat/rename/link/symlink/watch), exposed in
three parallel calling conventions — synchronous (blocking, throws), callback
(Node-style `(err, ...)` last-arg), and Promise-based (`fs/promises`, plus a
`FileHandle` object-oriented wrapper around an open descriptor). It also
provides streaming (`ReadStream`/`WriteStream`), directory iteration
(`Dir`/`Dirent`), filesystem-change notification (`watch`/`watchFile`), and a
`constants` table of POSIX flag/mode bit values. It is the single largest and
most load-bearing Node compat surface after `node:path`/`node:buffer`.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `fs.Stats`
*Not constructed directly by user code — returned by `stat`/`lstat`/`fstat`/`statSync`/... .*

Methods:
- `isBlockDevice(): boolean`
- `isCharacterDevice(): boolean`
- `isDirectory(): boolean`
- `isFIFO(): boolean`
- `isFile(): boolean`
- `isSocket(): boolean`
- `isSymbolicLink(): boolean`

Properties (all `number | bigint`, bigint form only populated when the
originating call passed `{ bigint: true }`):
`dev`, `ino`, `mode`, `nlink`, `uid`, `gid`, `rdev`, `size`, `blksize`,
`blocks`. Time fields: `atimeMs`, `mtimeMs`, `ctimeMs`, `birthtimeMs` (always
`number`, millisecond float); `atimeNs`, `mtimeNs`, `ctimeNs`, `birthtimeNs`
(`bigint`, nanosecond precision, **bigint-mode only**); `atime`, `mtime`,
`ctime`, `birthtime` (always `Date`, derived from the `*Ms` fields).

#### `fs.StatFs`
*Returned by `statfs`/`statfsSync`/`fsPromises.statfs`.*

Properties only (`number | bigint` per the same `bigint` option rule):
`type`, `bsize`, `blocks`, `bfree`, `bavail`, `files`, `ffree`.

#### `fs.Dirent`
*Returned by `readdir(..., { withFileTypes: true })` and `Dir.read()`.*

Methods: `isBlockDevice()`, `isCharacterDevice()`, `isDirectory()`,
`isFIFO()`, `isFile()`, `isSocket()`, `isSymbolicLink()` — all `boolean`,
same semantics as `Stats` but derived from the directory-entry `d_type`
(cheaper than a full `stat`, and — like POSIX `d_type` — may be unreliable /
require a fallback `lstat` on some filesystems).

Properties: `name: string | Buffer` (entry name, encoding depends on the
`readdir` call's `encoding` option), `parentPath: string` (absolute path of
the containing directory; **replaces the older, still-aliased `path`
property**).

#### `fs.Dir`
*Returned by `opendir`/`opendirSync`/`fsPromises.opendir`. Async-iterable.*

Methods:
- `close(): Promise<void>`
- `close(callback: (err?: NodeJS.ErrnoException) => void): void`
- `closeSync(): void`
- `read(): Promise<Dirent | null>`
- `read(callback: (err: NodeJS.ErrnoException | null, dirent: Dirent | null) => void): void`
- `readSync(): Dirent | null`
- `[Symbol.asyncIterator](): AsyncIterableIterator<Dirent>`
- `[Symbol.asyncDispose](): Promise<void>`

Properties: `path: string` (read-only, the path passed to `opendir`).

#### `fs.FSWatcher` — extends `EventEmitter`
*Returned by `fs.watch(...)`.*

Methods: `close(): void`, `ref(): this`, `unref(): this`.
Events: `'change'` `(eventType: 'rename'|'change', filename: string | Buffer | null)`,
`'close'` `()`, `'error'` `(error: Error)`.

#### `fs.StatWatcher` — extends `EventEmitter`
*Returned by `fs.watchFile(...)`.*

Methods: `ref(): this`, `unref(): this`.
Events: `'change'` `(curr: Stats, prev: Stats)`. (No dedicated `'close'`; stop
via `fs.unwatchFile()`.)

#### `fs.ReadStream` — extends `stream.Readable`
*Returned by `fs.createReadStream()` / `filehandle.createReadStream()`.*

Properties: `bytesRead: number`, `path: string | Buffer`, `pending: boolean`
(true until the underlying `'open'` event fires).
Events (own, in addition to inherited `Readable` events `'data'`/`'end'`/
`'error'`/`'close'`/`'pause'`/`'resume'`/`'readable'`): `'open'`
`(fd: number)`, `'ready'` `()`.

#### `fs.WriteStream` — extends `stream.Writable`
*Returned by `fs.createWriteStream()` / `filehandle.createWriteStream()`.*

Methods: `close(callback?: (err?: NodeJS.ErrnoException) => void): void`.
Properties: `bytesWritten: number`, `path: string | Buffer`,
`pending: boolean`.
Events (own, plus inherited `Writable` events `'drain'`/`'finish'`/`'error'`/
`'close'`/`'pipe'`/`'unpipe'`): `'open'` `(fd: number)`, `'ready'` `()`.

#### `fs.Utf8Stream` *(verify — recent addition, confirm exact Node version before implementing; a high-throughput fixed-encoding append-only file-writer stream, distinct from `WriteStream`)*

Constructor: `new fs.Utf8Stream(options?: Utf8StreamOptions)`.
Methods: `write(data: string): boolean`, `end(): void`, `flush(callback?: (err?: Error) => void): void`, `flushSync(): void`, `reopen(file?: string): void`, `destroy(): void`, `[Symbol.dispose](): void`.
Properties: `fd`, `file`, `append`, `mode`, `sync`, `fsync`, `mkdir`,
`minLength`, `maxLength`, `periodicFlush`, `writing`, `contentMode`.
Events: `'ready'`, `'write'` `(bytesWritten: number)`, `'drain'`, `'drop'`
`(data)`, `'finish'`, `'close'`, `'error'` `(err: Error)`.

#### `fsPromises.FileHandle`
*Returned by `fsPromises.open(...)`. Wraps one open OS file descriptor.*

Methods (all Promise-returning unless noted):
- `appendFile(data: string | Uint8Array, options?: WriteFileOptions): Promise<void>`
- `chmod(mode: number): Promise<void>`
- `chown(uid: number, gid: number): Promise<void>`
- `close(): Promise<void>`
- `createReadStream(options?: CreateReadStreamOptions): fs.ReadStream`
- `createWriteStream(options?: CreateWriteStreamOptions): fs.WriteStream`
- `datasync(): Promise<void>`
- `read(buffer: Uint8Array, offset: number, length: number, position: number | bigint | null): Promise<{ bytesRead: number, buffer: Uint8Array }>`
- `read(options?: FileHandleReadOptions): Promise<{ bytesRead: number, buffer: Uint8Array }>`
- `read(buffer: Uint8Array, options?: FileHandleReadOptions): Promise<{ bytesRead: number, buffer: Uint8Array }>`
- `readableWebStream(options?: { type?: 'bytes' }): ReadableStream`
- `readFile(options?: ReadFileOptions): Promise<Buffer | string>`
- `readLines(options?: ReadLineOptions): readline.InterfaceConstructor`
- `readv(buffers: readonly Uint8Array[], position?: number | null): Promise<{ bytesRead: number, buffers: Uint8Array[] }>`
- `stat(options?: StatOptions): Promise<fs.Stats>`
- `sync(): Promise<void>`
- `truncate(len?: number): Promise<void>`
- `utimes(atime: TimeLike, mtime: TimeLike): Promise<void>`
- `write(buffer: Uint8Array, offset?: number, length?: number, position?: number | null): Promise<{ bytesWritten: number, buffer: Uint8Array }>`
- `write(buffer: Uint8Array, options?: FileHandleWriteOptions): Promise<{ bytesWritten: number, buffer: Uint8Array }>`
- `write(data: string, position?: number | null, encoding?: BufferEncoding): Promise<{ bytesWritten: number, buffer: string }>`
- `writeFile(data: string | Uint8Array, options?: WriteFileOptions): Promise<void>`
- `writev(buffers: readonly Uint8Array[], position?: number | null): Promise<{ bytesWritten: number, buffers: Uint8Array[] }>`
- `[Symbol.asyncDispose](): Promise<void>`

Properties: `fd: number`.
Events: `'close'` `()`.

### 2.2 Top-level functions

Every entry below documents three variants where they exist:
**sync** (`fs.xSync`, throws `Error` with `.code`), **callback**
(`fs.x(..., callback)`, last-arg `(err, ...)`), **promise**
(`fsPromises.x(...)`, from `node:fs/promises`). fd-based operations additionally
have a `FileHandle` instance-method form (documented under §2.1).

---

#### `access`
- sync: `fs.accessSync(path: PathLike, mode?: number): void`
- callback: `fs.access(path: PathLike, mode: number | undefined, callback: (err: NodeJS.ErrnoException | null) => void): void`
- promise: `fsPromises.access(path: PathLike, mode?: number): Promise<void>`

| param | type | optional | default |
|---|---|---|---|
| path | `PathLike` | no | — |
| mode | `number` (OR of `F_OK`\|`R_OK`\|`W_OK`\|`X_OK`) | yes | `fs.constants.F_OK` |

Throws: `ENOENT`, `EACCES`. No return value (resolves/returns `undefined` on success).

#### `exists` *(deprecated — do not use `existsSync` result to guard a later op; TOCTOU race)*
- callback only: `fs.exists(path: PathLike, callback: (exists: boolean) => void): void` — **non-standard callback shape, no `err` first arg**
- sync: `fs.existsSync(path: PathLike): boolean` (not deprecated)
- no promise variant (use `fsPromises.access` / `stat` + catch `ENOENT` instead)

#### `read`
- sync: `fs.readSync(fd: number, buffer: Uint8Array, offset: number, length: number, position: number | bigint | null): number`
- sync (options form): `fs.readSync(fd: number, buffer: Uint8Array, options?: { offset?: number, length?: number, position?: number | bigint | null }): number`
- callback: `fs.read(fd: number, buffer: Uint8Array, offset: number, length: number, position: number | bigint | null, callback: (err: NodeJS.ErrnoException | null, bytesRead: number, buffer: Uint8Array) => void): void`
- callback (options form): `fs.read(fd: number, options: FileReadOptions, callback: (...) => void): void`
- promise: only via `FileHandle.read(...)` (no bare `fsPromises.read(fd, ...)`)

| param | type | optional | default |
|---|---|---|---|
| fd | `number` | no | — |
| buffer | `Buffer \| TypedArray \| DataView` | no | — |
| offset | `number` (byte offset into `buffer`) | yes | `0` |
| length | `number` (bytes to read) | yes | `buffer.byteLength - offset` |
| position | `number \| bigint \| null` | yes | `null` (reads from current fd position and advances it) |

Throws: `EBADF`, `EIO`, `EAGAIN`.

#### `readFile`
- sync: `fs.readFileSync(path: PathOrFileDescriptor, options?: ReadFileOptions | BufferEncoding): string | Buffer`
- callback: `fs.readFile(path: PathOrFileDescriptor, options: ReadFileOptions | BufferEncoding | undefined, callback: (err: NodeJS.ErrnoException | null, data: string | Buffer) => void): void`
- promise: `fsPromises.readFile(path: PathLike | FileHandle, options?: ReadFileOptions | BufferEncoding): Promise<string | Buffer>`

| param | type | optional | default |
|---|---|---|---|
| path | `string \| Buffer \| URL \| number` (fd) | no | — |
| options.encoding | `BufferEncoding \| null` | yes | `null` (→ `Buffer` return) |
| options.flag | `string` | yes | `'r'` |
| options.signal | `AbortSignal` | yes | — (promise/FileHandle only) |

Throws: `ENOENT`, `EISDIR`, `EACCES`. Whole file is read into memory — not for huge files.

#### `readv`
- sync: `fs.readvSync(fd: number, buffers: readonly Uint8Array[], position?: number | null): number`
- callback: `fs.readv(fd: number, buffers: readonly Uint8Array[], position: number | null | undefined, callback: (err, bytesRead: number, buffers: Uint8Array[]) => void): void`
- promise: only via `FileHandle.readv(...)`.

Scatter/gather read (POSIX `readv`/Windows equivalent) into multiple buffers in order.

#### `readdir`
- sync: `fs.readdirSync(path: PathLike, options?: ReaddirOptions): string[] | Buffer[] | Dirent[]`
- callback: `fs.readdir(path: PathLike, options: ReaddirOptions | undefined, callback: (err, files: string[] | Buffer[] | Dirent[]) => void): void`
- promise: `fsPromises.readdir(path: PathLike, options?: ReaddirOptions): Promise<string[] | Buffer[] | Dirent[]>`

| param | type | optional | default |
|---|---|---|---|
| path | `PathLike` | no | — |
| options.encoding | `BufferEncoding \| 'buffer'` | yes | `'utf8'` |
| options.withFileTypes | `boolean` | yes | `false` |
| options.recursive | `boolean` | yes | `false` |

Throws: `ENOENT`, `ENOTDIR`.

#### `readlink`
- sync: `fs.readlinkSync(path: PathLike, options?: EncodingOption): string | Buffer`
- callback: `fs.readlink(path: PathLike, options: EncodingOption | undefined, callback: (err, linkString: string | Buffer) => void): void`
- promise: `fsPromises.readlink(path: PathLike, options?: EncodingOption): Promise<string | Buffer>`

Throws: `EINVAL` (not a symlink), `ENOENT`.

#### `realpath`
- sync: `fs.realpathSync(path: PathLike, options?: EncodingOption): string | Buffer`
- callback: `fs.realpath(path: PathLike, options: EncodingOption | undefined, callback: (err, resolvedPath: string | Buffer) => void): void`
- promise: `fsPromises.realpath(path: PathLike, options?: EncodingOption): Promise<string | Buffer>`
- also: `fs.realpathSync.native(path, options?)` / `fs.realpath.native(path, options?, callback)` — OS-syscall-backed variant (no `fsPromises` equivalent); resolves symlinks + `.`/`..` canonically.

Throws: `ENOENT`, `ELOOP` (symlink cycle).

#### `write`
- sync (buffer form): `fs.writeSync(fd: number, buffer: Uint8Array, offset?: number, length?: number, position?: number | null): number`
- sync (string form): `fs.writeSync(fd: number, string: string, position?: number | null, encoding?: BufferEncoding): number`
- callback (buffer form): `fs.write(fd, buffer, offset, length, position, callback: (err, bytesWritten: number, buffer: Uint8Array) => void): void`
- callback (string form): `fs.write(fd, string, position, encoding, callback: (err, bytesWritten: number, string: string) => void): void`
- promise: only via `FileHandle.write(...)` (both overloads).

Throws: `EBADF`, `ENOSPC`, `EAGAIN`.

#### `writev`
- sync: `fs.writevSync(fd: number, buffers: readonly Uint8Array[], position?: number | null): number`
- callback: `fs.writev(fd, buffers, position, callback: (err, bytesWritten: number, buffers: Uint8Array[]) => void): void`
- promise: only via `FileHandle.writev(...)`.

#### `writeFile`
- sync: `fs.writeFileSync(file: PathOrFileDescriptor, data: string | NodeJS.ArrayBufferView | Iterable<string|Uint8Array> | AsyncIterable<string|Uint8Array> | Stream, options?: WriteFileOptions): void`
- callback: `fs.writeFile(file, data, options: WriteFileOptions | undefined, callback: (err) => void): void`
- promise: `fsPromises.writeFile(file, data, options?: WriteFileOptions): Promise<void>`

| param | type | optional | default |
|---|---|---|---|
| file | `PathLike \| number \| FileHandle` | no | — |
| data | `string \| Uint8Array \| Iterable \| AsyncIterable \| Stream` | no | — |
| options.encoding | `BufferEncoding \| null` | yes | `'utf8'` |
| options.mode | `number` | yes | `0o666` |
| options.flag | `string` | yes | `'w'` |
| options.flush | `boolean` (fsync before close) | yes | `false` |
| options.signal | `AbortSignal` | yes | — |

Throws: `EACCES`, `EISDIR`, `ENOSPC`. Fully replaces file contents (unlike `appendFile`).

#### `appendFile`
Same signatures/shapes as `writeFile` (sync/callback/promise), default
`options.flag = 'a'` (create-if-missing, append at end). Throws same codes.

#### `close`
- sync: `fs.closeSync(fd: number): void`
- callback: `fs.close(fd: number, callback?: (err?: NodeJS.ErrnoException) => void): void`
- promise: via `FileHandle.close(): Promise<void>` (no bare `fsPromises.close(fd)` — fd ownership is always through a `FileHandle`).

Throws: `EBADF`.

#### `fstat`
- sync: `fs.fstatSync(fd: number, options?: StatOptions): fs.Stats`
- callback: `fs.fstat(fd, options: StatOptions | undefined, callback: (err, stats: fs.Stats) => void): void`
- promise: via `FileHandle.stat(options?: StatOptions): Promise<fs.Stats>`

#### `fsync`
- sync: `fs.fsyncSync(fd: number): void`
- callback: `fs.fsync(fd, callback: (err?) => void): void`
- promise: via `FileHandle.sync(): Promise<void>`

#### `fdatasync`
- sync: `fs.fdatasyncSync(fd: number): void`
- callback: `fs.fdatasync(fd, callback: (err?) => void): void`
- promise: via `FileHandle.datasync(): Promise<void>`

#### `ftruncate`
- sync: `fs.ftruncateSync(fd: number, len?: number): void`
- callback: `fs.ftruncate(fd, len: number | undefined, callback: (err?) => void): void`
- promise: via `FileHandle.truncate(len?: number): Promise<void>`

Default `len = 0`.

#### `futimes`
- sync: `fs.futimesSync(fd: number, atime: TimeLike, mtime: TimeLike): void`
- callback: `fs.futimes(fd, atime, mtime, callback: (err?) => void): void`
- promise: via `FileHandle.utimes(atime, mtime): Promise<void>`

#### `fchmod`
- sync: `fs.fchmodSync(fd: number, mode: number): void`
- callback: `fs.fchmod(fd, mode, callback: (err?) => void): void`
- promise: via `FileHandle.chmod(mode): Promise<void>`

#### `fchown`
- sync: `fs.fchownSync(fd: number, uid: number, gid: number): void`
- callback: `fs.fchown(fd, uid, gid, callback: (err?) => void): void`
- promise: via `FileHandle.chown(uid, gid): Promise<void>`

#### `stat`
- sync: `fs.statSync(path: PathLike, options?: StatSyncOptions): fs.Stats | undefined`
- callback: `fs.stat(path, options: StatOptions | undefined, callback: (err, stats: fs.Stats) => void): void`
- promise: `fsPromises.stat(path, options?: StatOptions): Promise<fs.Stats>`

`statSync` uniquely supports `throwIfNoEntry: false` → returns `undefined`
instead of throwing `ENOENT`. Follows symlinks.

#### `lstat`
Same shapes as `stat` (sync/callback/promise) — identical except it does
**not** follow a symlink at the final path component (reports the link
itself).

#### `statfs`
- sync: `fs.statfsSync(path: PathLike, options?: { bigint?: boolean }): fs.StatFs`
- callback: `fs.statfs(path, options: { bigint?: boolean } | undefined, callback: (err, stats: fs.StatFs) => void): void`
- promise: `fsPromises.statfs(path, options?): Promise<fs.StatFs>`

Filesystem-level statistics (like POSIX `statvfs`), not per-file.

#### `mkdir`
- sync: `fs.mkdirSync(path: PathLike, options?: number | string | MkdirOptions): string | undefined`
- callback: `fs.mkdir(path, options: number | string | MkdirOptions | undefined, callback: (err, path?: string) => void): void`
- promise: `fsPromises.mkdir(path, options?): Promise<string | undefined>`

| param | type | optional | default |
|---|---|---|---|
| options.recursive | `boolean` | yes | `false` |
| options.mode | `number` | yes | `0o777` (ignored on Windows) |

Returns the first created directory path when `recursive: true`, else `undefined`.
Throws: `EEXIST` (non-recursive, path exists), `ENOENT` (missing parent, non-recursive).

#### `mkdtemp`
- sync: `fs.mkdtempSync(prefix: string, options?: EncodingOption): string`
- callback: `fs.mkdtemp(prefix, options: EncodingOption | undefined, callback: (err, folder: string) => void): void`
- promise: `fsPromises.mkdtemp(prefix, options?): Promise<string>`

Appends 6 random characters to `prefix` to produce a unique directory name;
caller is responsible for later removal (except the disposable variant below).

#### `mkdtempDisposable`
- sync: `fs.mkdtempDisposableSync(prefix: string, options?: { encoding?: BufferEncoding }): { path: string, remove(): void, [Symbol.dispose](): void }`
- promise: `fsPromises.mkdtempDisposable(prefix, options?): Promise<{ path, remove(): Promise<void>, [Symbol.asyncDispose](): Promise<void> }>`
- no callback form.

#### `rmdir` *(deprecated — recursive option; use `rm` instead for recursive removal)*
- sync: `fs.rmdirSync(path: PathLike, options?: RmDirOptions): void`
- callback: `fs.rmdir(path, options: RmDirOptions | undefined, callback: (err?) => void): void`
- promise: `fsPromises.rmdir(path, options?): Promise<void>`

| param | type | optional | default |
|---|---|---|---|
| options.recursive | `boolean` (deprecated) | yes | `false` |
| options.maxRetries | `number` | yes | `0` |
| options.retryDelay | `number` (ms) | yes | `100` |

Throws: `ENOTEMPTY` (non-recursive, non-empty dir), `ENOENT`, `ENOTDIR`.

#### `opendir`
- sync: `fs.opendirSync(path: PathLike, options?: OpenDirOptions): fs.Dir`
- callback: `fs.opendir(path, options: OpenDirOptions | undefined, callback: (err, dir: fs.Dir) => void): void`
- promise: `fsPromises.opendir(path, options?): Promise<fs.Dir>`

| param | type | optional | default |
|---|---|---|---|
| options.encoding | `BufferEncoding` | yes | `'utf8'` |
| options.bufferSize | `number` | yes | `32` |
| options.recursive | `boolean` | yes | `false` |

#### `open`
- sync: `fs.openSync(path: PathLike, flags?: string | number, mode?: number): number`
- callback: `fs.open(path, flags: string | number | undefined, mode: number | undefined, callback: (err, fd: number) => void): void`
- promise: `fsPromises.open(path, flags?: string | number, mode?: number): Promise<FileHandle>`

| param | type | optional | default |
|---|---|---|---|
| flags | `string` (`'r'`,`'r+'`,`'w'`,`'wx'`,`'w+'`,`'wx+'`,`'a'`,`'ax'`,`'a+'`,`'ax+'`,`'as'`,`'as+'`,`'rs+'`) or numeric O_* bitmask | yes | `'r'` |
| mode | `number` | yes | `0o666` |

Throws: `ENOENT` (no `O_CREAT`), `EEXIST` (`x` flag + exists).

#### `openAsBlob`
- promise only, no sync/callback: `fs.openAsBlob(path: PathLike, options?: { type?: string }): Promise<Blob>`

Returns a global `Blob` view of the file contents; does not keep the fd open
across the process lifetime the way a `FileHandle` does.

#### `copyFile`
- sync: `fs.copyFileSync(src: PathLike, dest: PathLike, mode?: number): void`
- callback: `fs.copyFile(src, dest, mode: number | undefined, callback: (err?) => void): void`
- promise: `fsPromises.copyFile(src, dest, mode?): Promise<void>`

`mode` is an OR of `fs.constants.COPYFILE_EXCL` / `COPYFILE_FICLONE` /
`COPYFILE_FICLONE_FORCE`. Default `0` (overwrite allowed, no COW hint).
Throws: `EEXIST` (with `COPYFILE_EXCL`), `ENOENT`.

#### `cp`
- sync: `fs.cpSync(src: PathLike, dest: PathLike, options?: CopyOptions): void`
- callback: `fs.cp(src, dest, options: CopyOptions | undefined, callback: (err?) => void): void`
- promise: `fsPromises.cp(src, dest, options?): Promise<void>`

| param | type | optional | default |
|---|---|---|---|
| options.recursive | `boolean` | yes | `false` |
| options.dereference | `boolean` | yes | `false` |
| options.errorOnExist | `boolean` | yes | `false` |
| options.filter | `(src: string, dest: string) => boolean \| Promise<boolean>` | yes | — |
| options.force | `boolean` | yes | `true` |
| options.preserveTimestamps | `boolean` | yes | `false` |
| options.verbatimSymlinks | `boolean` | yes | `false` |
| options.mode | `number` | yes | `0` |

Throws: `ERR_FS_CP_EEXIST`, `ERR_FS_CP_DIR_TO_NON_DIR`, `ERR_FS_CP_NON_DIR_TO_DIR`, `ERR_FS_EISDIR`.

#### `unlink`
- sync: `fs.unlinkSync(path: PathLike): void`
- callback: `fs.unlink(path, callback: (err?) => void): void`
- promise: `fsPromises.unlink(path): Promise<void>`

Removes a file/symlink (not a directory). Throws: `ENOENT`, `EISDIR` (POSIX)/`EPERM` (Windows) on a directory.

#### `rm`
- sync: `fs.rmSync(path: PathLike, options?: RmOptions): void`
- callback: `fs.rm(path, options: RmOptions | undefined, callback: (err?) => void): void`
- promise: `fsPromises.rm(path, options?): Promise<void>`

| param | type | optional | default |
|---|---|---|---|
| options.force | `boolean` (ignore `ENOENT`) | yes | `false` |
| options.maxRetries | `number` | yes | `0` |
| options.recursive | `boolean` | yes | `false` |
| options.retryDelay | `number` (ms) | yes | `100` |

The modern, non-deprecated recursive-remove entry point. Throws: `ENOTEMPTY` (non-recursive), `ENOENT` (unless `force`).

#### `rename`
- sync: `fs.renameSync(oldPath: PathLike, newPath: PathLike): void`
- callback: `fs.rename(oldPath, newPath, callback: (err?) => void): void`
- promise: `fsPromises.rename(oldPath, newPath): Promise<void>`

Throws: `ENOENT`, `EXDEV` (cross-device rename, POSIX), `EPERM` (cross-device, Windows), `ENOTEMPTY`.

#### `truncate`
- sync: `fs.truncateSync(path: PathLike, len?: number): void`
- callback: `fs.truncate(path, len: number | undefined, callback: (err?) => void): void`
- promise: `fsPromises.truncate(path, len?): Promise<void>`

Default `len = 0`. Opens the path internally (not fd-based — see `ftruncate` for the fd form).

#### `link`
- sync: `fs.linkSync(existingPath: PathLike, newPath: PathLike): void`
- callback: `fs.link(existingPath, newPath, callback: (err?) => void): void`
- promise: `fsPromises.link(existingPath, newPath): Promise<void>`

Creates a hard link. Throws: `EEXIST`, `EPERM` (cross-filesystem/dir hardlink), `ENOENT`.

#### `symlink`
- sync: `fs.symlinkSync(target: PathLike, path: PathLike, type?: SymlinkType): void`
- callback: `fs.symlink(target, path, type: SymlinkType | undefined, callback: (err?) => void): void`
- promise: `fsPromises.symlink(target, path, type?): Promise<void>`

`type: 'dir' | 'file' | 'junction'`, Windows-only distinction (ignored on
POSIX). Default `'file'`; auto-detected as `'dir'` when target exists and is
a directory, if `type` omitted. Throws: `EEXIST`, `EPERM` (Windows, no
privilege/dev-mode for non-junction symlinks).

#### `lchmod` *(macOS-only syscall; not available on Linux/Windows — throws `ENOSYS` there)*
- sync: `fs.lchmodSync(path: PathLike, mode: number): void`
- callback: `fs.lchmod(path, mode, callback: (err?) => void): void`
- promise: `fsPromises.lchmod(path, mode): Promise<void>`

#### `lchown`
- sync: `fs.lchownSync(path: PathLike, uid: number, gid: number): void`
- callback: `fs.lchown(path, uid, gid, callback: (err?) => void): void`
- promise: `fsPromises.lchown(path, uid, gid): Promise<void>`

Changes ownership of the symlink itself, not its target. No-op / `ENOSYS` on Windows (no POSIX uid/gid concept).

#### `chmod`
- sync: `fs.chmodSync(path: PathLike, mode: number): void`
- callback: `fs.chmod(path, mode, callback: (err?) => void): void`
- promise: `fsPromises.chmod(path, mode): Promise<void>`

On Windows only the read-only bit is honored (`mode & 0o200` clear → read-only).

#### `chown`
- sync: `fs.chownSync(path: PathLike, uid: number, gid: number): void`
- callback: `fs.chown(path, uid, gid, callback: (err?) => void): void`
- promise: `fsPromises.chown(path, uid, gid): Promise<void>`

Throws: `EPERM` (unprivileged process changing owner, POSIX). No-op semantics on Windows (verify — commonly a silent no-op / `ENOSYS`).

#### `utimes`
- sync: `fs.utimesSync(path: PathLike, atime: TimeLike, mtime: TimeLike): void`
- callback: `fs.utimes(path, atime, mtime, callback: (err?) => void): void`
- promise: `fsPromises.utimes(path, atime, mtime): Promise<void>`

`TimeLike = number | string | Date`. Numbers are interpreted as Unix seconds (not ms).

#### `lutimes`
Same shapes as `utimes` (sync/callback/promise) but operates on the symlink
itself without dereferencing.

#### `glob`
- sync: `fs.globSync(pattern: string | string[], options?: GlobOptions): string[] | Dirent[]`
- callback: `fs.glob(pattern, options: GlobOptions | undefined, callback: (err, matches: string[] | Dirent[]) => void): void`
- promise: `fsPromises.glob(pattern, options?): AsyncIterable<string | Dirent>` **(async-iterable, not a resolved array — differs from the sync/callback forms which return/pass a full array)**

| param | type | optional | default |
|---|---|---|---|
| pattern | `string \| string[]` | no | — |
| options.cwd | `string` | yes | `process.cwd()` |
| options.exclude | `string \| string[] \| (path: string) => boolean` | yes | — |
| options.withFileTypes | `boolean` | yes | `false` |
| options.maxDepth | `number` | yes | — |

#### `watch`
- callback/EventEmitter form: `fs.watch(filename: PathLike, options?: WatchOptions | BufferEncoding, listener?: (eventType: 'rename'|'change', filename: string | Buffer | null) => void): fs.FSWatcher`
- promise/async-iterator form: `fsPromises.watch(filename: PathLike, options?: WatchOptions): AsyncIterable<{ eventType: 'rename'|'change', filename: string | Buffer | null }>` — **no `fs.watchSync`; the promise form has no listener callback, it is consumed with `for await`.**

| param | type | optional | default |
|---|---|---|---|
| options.persistent | `boolean` | yes | `true` |
| options.recursive | `boolean` | yes | `false` (platform-dependent support, see §4) |
| options.encoding | `BufferEncoding` | yes | `'utf8'` |
| options.signal | `AbortSignal` | yes | — |

#### `watchFile` *(polling-based; prefer `watch` where available)*
- callback only, no sync/promise: `fs.watchFile(filename: PathLike, options: WatchFileOptions | undefined, listener: (curr: fs.Stats, prev: fs.Stats) => void): fs.StatWatcher`

| param | type | optional | default |
|---|---|---|---|
| options.persistent | `boolean` | yes | `true` |
| options.interval | `number` (ms) | yes | `5007` |

#### `unwatchFile`
- callback/sync-effect, no return of interest: `fs.unwatchFile(filename: PathLike, listener?: (curr: fs.Stats, prev: fs.Stats) => void): void`

Stops one listener, or all listeners for `filename` if `listener` omitted.

#### `createReadStream`
- `fs.createReadStream(path: PathLike, options?: ReadStreamOptions | BufferEncoding): fs.ReadStream` (no sync/promise "function" form — the class itself streams asynchronously; see `FileHandle.createReadStream` for the fd-owning variant)

| param | type | optional | default |
|---|---|---|---|
| options.flags | `string` | yes | `'r'` |
| options.encoding | `BufferEncoding` | yes | `null` |
| options.fd | `number \| FileHandle` | yes | — |
| options.mode | `number` | yes | `0o666` |
| options.autoClose | `boolean` | yes | `true` |
| options.emitClose | `boolean` | yes | `true` |
| options.start | `number` | yes | `0` |
| options.end | `number` | yes | `Infinity` |
| options.highWaterMark | `number` | yes | `65536` (64 KiB) |
| options.signal | `AbortSignal` | yes | — |

#### `createWriteStream`
- `fs.createWriteStream(path: PathLike, options?: WriteStreamOptions | BufferEncoding): fs.WriteStream`

| param | type | optional | default |
|---|---|---|---|
| options.flags | `string` | yes | `'w'` |
| options.encoding | `BufferEncoding` | yes | `'utf8'` |
| options.fd | `number \| FileHandle` | yes | — |
| options.mode | `number` | yes | `0o666` |
| options.autoClose | `boolean` | yes | `true` |
| options.emitClose | `boolean` | yes | `true` |
| options.start | `number` | yes | — |
| options.highWaterMark | `number` | yes | `16384` (16 KiB) |
| options.flush | `boolean` | yes | `false` |
| options.signal | `AbortSignal` | yes | — |

### 2.3 Properties & constants

- `fs.constants` (and identically `fsPromises.constants`) — plain object, all
  values `number`:
  - **Access mode** (for `access`/`accessSync`): `F_OK=0`, `R_OK=4`, `W_OK=2`, `X_OK=1`
  - **Open flags** (for `open`, numeric `flags`): `O_RDONLY=0`, `O_WRONLY=1`,
    `O_RDWR=2`, `O_CREAT=64`, `O_EXCL=128`, `O_NOCTTY=256`, `O_TRUNC=512`,
    `O_APPEND=1024`, `O_DIRECTORY=65536`, `O_NOATIME=262144`,
    `O_NOFOLLOW=131072`, `O_SYNC=1052672`, `O_DSYNC=4096`, `O_SYMLINK`
    (platform-dependent), `O_NONBLOCK` (platform-dependent)
  - **Copy-file flags**: `COPYFILE_EXCL=1`, `COPYFILE_FICLONE=2`, `COPYFILE_FICLONE_FORCE=4`
  - **File type bitmask** (`stats.mode & S_IFMT`): `S_IFMT=61440`,
    `S_IFREG=32768`, `S_IFDIR=16384`, `S_IFCHR=8192`, `S_IFBLK=24576`,
    `S_IFIFO=4096`, `S_IFLNK=40960`, `S_IFSOCK=49152`
  - **Permission bits**: `S_IRWXU=448`, `S_IRUSR=256`, `S_IWUSR=128`,
    `S_IXUSR=64`, `S_IRWXG=56`, `S_IRGRP=32`, `S_IWGRP=16`, `S_IXGRP=8`,
    `S_IRWXO=7`, `S_IROTH=4`, `S_IWOTH=2`, `S_IXOTH=1`, `S_ISUID=2048`,
    `S_ISGID=1024`, `S_ISVTX=512`
- `fs.F_OK` / `fs.R_OK` / `fs.W_OK` / `fs.X_OK` — deprecated top-level
  aliases of the same `fs.constants.*` values.
- `fs.promises` — namespace object identical to importing `node:fs/promises` (deprecated as a property access path in favor of the dedicated module; still live).

### 2.4 Events (indexed by owning class — see §2.1 for full context)

| Event | Emitter | Payload |
|---|---|---|
| `'change'` | `FSWatcher` | `(eventType: 'rename'\|'change', filename: string\|Buffer\|null)` |
| `'change'` | `StatWatcher` | `(curr: Stats, prev: Stats)` |
| `'close'` | `FSWatcher`, `ReadStream`, `WriteStream`, `FileHandle`, `Utf8Stream` | `()` |
| `'error'` | `FSWatcher`, `ReadStream`(inherited), `WriteStream`(inherited), `Utf8Stream` | `(error: Error)` |
| `'open'` | `ReadStream`, `WriteStream` | `(fd: number)` |
| `'ready'` | `ReadStream`, `WriteStream`, `Utf8Stream` | `()` |
| `'drain'`, `'drop'`, `'finish'`, `'write'` | `Utf8Stream` | see §2.1 |

## 3. Types & option objects

```ts
type PathLike = string | Buffer | URL;
type PathOrFileDescriptor = PathLike | number;
type BufferEncoding =
  | "ascii" | "utf8" | "utf-8" | "utf16le" | "utf-16le" | "ucs2" | "ucs-2"
  | "base64" | "base64url" | "latin1" | "binary" | "hex";
type TimeLike = number | string | Date;
type SymlinkType = "dir" | "file" | "junction";
type NoParamCallback = (err: NodeJS.ErrnoException | null) => void;

interface ObjectEncodingOptions {
  encoding?: BufferEncoding | null;
}
interface EncodingOption {
  encoding?: BufferEncoding | "buffer" | null;
}

interface StatOptions {
  bigint?: boolean; // default false
}
interface StatSyncOptions extends StatOptions {
  throwIfNoEntry?: boolean; // default true
}

interface ReadFileOptions extends ObjectEncodingOptions {
  flag?: string;          // default 'r'
  signal?: AbortSignal;   // promise/FileHandle only
}

interface WriteFileOptions extends ObjectEncodingOptions {
  mode?: number;           // default 0o666
  flag?: string;           // default 'w' (writeFile) / 'a' (appendFile)
  flush?: boolean;         // default false
  signal?: AbortSignal;
}

interface ReaddirOptions extends ObjectEncodingOptions {
  withFileTypes?: boolean; // default false
  recursive?: boolean;     // default false
}

interface MkdirOptions {
  recursive?: boolean; // default false
  mode?: number;        // default 0o777, POSIX only
}

interface RmOptions {
  force?: boolean;       // default false
  maxRetries?: number;   // default 0
  recursive?: boolean;   // default false
  retryDelay?: number;   // default 100 (ms)
}

interface RmDirOptions {
  recursive?: boolean;  // deprecated, default false
  maxRetries?: number;  // default 0
  retryDelay?: number;  // default 100 (ms)
}

interface CopyOptions {
  dereference?: boolean;
  errorOnExist?: boolean;
  filter?: (src: string, dest: string) => boolean | Promise<boolean>;
  force?: boolean;              // default true
  mode?: number;
  preserveTimestamps?: boolean;
  recursive?: boolean;
  verbatimSymlinks?: boolean;
}

interface OpenDirOptions extends ObjectEncodingOptions {
  bufferSize?: number;  // default 32
  recursive?: boolean;  // default false
}

interface GlobOptions {
  cwd?: string;
  exclude?: string | readonly string[] | ((fileName: string) => boolean);
  withFileTypes?: boolean;
  maxDepth?: number;
}

interface WatchOptions {
  persistent?: boolean;  // default true
  recursive?: boolean;   // default false, platform-dependent (see §4)
  encoding?: BufferEncoding | "buffer"; // default 'utf8'
  signal?: AbortSignal;
}

interface WatchFileOptions {
  persistent?: boolean; // default true
  interval?: number;    // default 5007 (ms)
}

interface ReadStreamOptions {
  flags?: string;          // default 'r'
  encoding?: BufferEncoding;
  fd?: number | FileHandle;
  mode?: number;            // default 0o666
  autoClose?: boolean;      // default true
  emitClose?: boolean;      // default true
  start?: number;
  end?: number;             // default Infinity
  highWaterMark?: number;   // default 65536
  signal?: AbortSignal;
}

interface WriteStreamOptions {
  flags?: string;          // default 'w'
  encoding?: BufferEncoding; // default 'utf8'
  fd?: number | FileHandle;
  mode?: number;            // default 0o666
  autoClose?: boolean;      // default true
  emitClose?: boolean;      // default true
  start?: number;
  highWaterMark?: number;   // default 16384
  flush?: boolean;          // default false
  signal?: AbortSignal;
}

interface FileReadResult<T extends NodeJS.ArrayBufferView = Buffer> {
  bytesRead: number;
  buffer: T;
}
interface FileWriteResult<T extends NodeJS.ArrayBufferView = Buffer> {
  bytesWritten: number;
  buffer: T;
}
interface FileHandleReadOptions {
  buffer?: NodeJS.ArrayBufferView;
  offset?: number;
  length?: number;
  position?: number | bigint | null;
}
interface FileHandleWriteOptions {
  offset?: number;
  length?: number;
  position?: number | null;
}

interface StatsShape {
  dev: number | bigint; ino: number | bigint; mode: number | bigint;
  nlink: number | bigint; uid: number | bigint; gid: number | bigint;
  rdev: number | bigint; size: number | bigint; blksize: number | bigint;
  blocks: number | bigint;
  atimeMs: number; mtimeMs: number; ctimeMs: number; birthtimeMs: number;
  atimeNs?: bigint; mtimeNs?: bigint; ctimeNs?: bigint; birthtimeNs?: bigint;
  atime: Date; mtime: Date; ctime: Date; birthtime: Date;
}
interface StatFsShape {
  type: number | bigint; bsize: number | bigint; blocks: number | bigint;
  bfree: number | bigint; bavail: number | bigint; files: number | bigint;
  ffree: number | bigint;
}
interface DirentShape {
  name: string | Buffer;
  parentPath: string;
}
```

## 4. Node semantics & edge cases

**Error codes** (POSIX `errno` names, surfaced on `err.code`; `err.errno`,
`err.syscall`, `err.path` — and `err.dest` for two-path ops like
`rename`/`copyFile` — are also populated):

| Code | Meaning | Typical source ops |
|---|---|---|
| `ENOENT` | no such file/directory | `open`, `stat`, `unlink`, `rmdir`, `readlink`, `rename` |
| `EEXIST` | target already exists | `mkdir` (non-recursive), `open` with `x` flag, `link`, `symlink` |
| `EPERM` | operation not permitted | `unlink` on a dir (Windows), `chown` unprivileged, cross-device `rename` (Windows) |
| `EACCES` | permission denied | `open`, `read`, `write`, `chmod`, `mkdir` |
| `EISDIR` | is a directory | `open`+write, `unlink` on a dir (POSIX), `readFile` on a dir |
| `ENOTDIR` | not a directory | path component traversal, `opendir`/`readdir` on non-dir |
| `ENOTEMPTY` | directory not empty | `rmdir`/`rm` non-recursive on non-empty dir, `rename` onto non-empty dir |
| `EMFILE` | too many open files (process) | `open` at per-process fd limit |
| `ENFILE` | too many open files (system) | `open` at system-wide fd limit |
| `ELOOP` | too many symlink levels | `open`/`stat`/`readlink` on a symlink cycle |
| `EINVAL` | invalid argument | bad flags/encoding/position, `readlink` on a non-symlink |
| `EIO` | low-level I/O error | disk/device failure during read/write |
| `EXDEV` | cross-device link/rename (POSIX) | `rename`/`link` across filesystems |
| `EBADF` | bad file descriptor | fd-based op on a closed/invalid fd |
| `ENOSPC` | no space left on device | `write`, `writeFile`, `ftruncate` growing a file |
| `ENOSYS` | function not implemented | `lchmod` (Linux), some ops without OS support |

**Windows vs POSIX**:
- Symlinks: `type: 'dir' | 'file' | 'junction'` matters only on Windows;
  creating non-junction symlinks needs Administrator privilege or Developer
  Mode; junctions need neither and don't require the target to exist yet.
- Path separators: POSIX accepts only `/`; Windows accepts both `/` and `\`
  and normalizes internally. Windows additionally has per-drive cwd
  (`process.chdir('D:\\')` only changes `D:`'s cwd, not the process cwd if
  another drive is current).
- Permission bits: POSIX honors the full `mode` bit pattern; Windows
  `chmod`/`open(..., mode)` only reliably toggles the read-only attribute
  (clearing/setting `0o200`), all execute/group/other bits are ignored.
- `unlink` on a directory: `EISDIR` on POSIX, `EPERM` on Windows (must use
  `rmdir`/`rm`).
- `chown`/`lchown`: no POSIX uid/gid concept on Windows — effectively a
  no-op or `ENOSYS` (verify exact Node behavior per platform before wiring
  the native call as a hard error vs a silent success).
- `lchmod`: only implemented on macOS (BSD `lchmod` syscall); `ENOSYS` on
  Linux and Windows.

**Encodings**: `utf8`/`utf-8` (default for string ops), `ascii` (7-bit,
lossy on high bytes), `latin1`/`binary` (byte-transparent ISO-8859-1),
`base64`, `base64url` (RFC4648 `-`/`_` alphabet), `hex`, `ucs2`/`ucs-2`/
`utf16le`/`utf-16le`. When no `encoding` (or `encoding: null`) is given,
byte-returning ops (`readFile`, `FileHandle.readFile`) yield a `Buffer`;
otherwise a `string`. Path-returning ops (`readdir`, `readlink`, `realpath`,
`mkdtemp`) yield `Buffer` when `encoding: 'buffer'` is passed.

**`fs.watch` caveats** (platform support matrix):

| Platform | `recursive: true` | `filename` argument reliability |
|---|---|---|
| Linux (inotify) | not supported (inotify watches one directory level; RTS must recurse by hand or reject/warn) | usually present, not 100% guaranteed |
| macOS (FSEvents) | supported | often `null` — must independently `stat`/`realpath` to confirm existence |
| Windows (`ReadDirectoryChangesW`) | supported | generally reliable |

Renames are detected via inode reuse/rename tracking depending on platform;
a rapid create-delete-recreate of the same name can coalesce or drop events.
Network filesystem mounts are generally unreliable for `watch`. `watchFile`
(stat-polling, default interval 5007 ms) is the portable-but-slower
fallback and is the only mechanism that works uniformly cross-platform
including network shares.

**Threadpool / blocking model**: every non-`Sync`, non-fully-async op in
Node's `fs` runs on the libuv threadpool (default size 4,
`UV_THREADPOOL_SIZE` env var tunable up to 1024), *not* on the main event
loop thread — including `readFile`, `stat`, `readdir`, `open`, `mkdir`,
`rm`, `copyFile`, `watch`'s callback delivery is main-thread but its OS-level
subscription setup crosses the threadpool too on some platforms. `*Sync`
calls always run on the calling (usually main) thread and block it
completely — never in a hot path shared with I/O callbacks. Ordering
across different call styles (`fs.readFile` vs `fs.promises.readFile`) is
**not** synchronized/serialized by Node; concurrent calls on the same path
may complete out of issue-order.

**Deprecations**: `fs.exists(path, callback)` (non-standard callback shape,
no `err`; use `access`/`stat`); `fs.rmdir(path, { recursive: true })`
(use `fs.rm(path, { recursive: true })`); `fs.SyncWriteStream` (removed,
never public API); `fs.F_OK`/`R_OK`/`W_OK`/`X_OK` top-level aliases
(use `fs.constants.*`); string-form numeric flags stay supported but new
code should prefer `fs.constants.O_*` bitmasks for `open`.

**Permission model**: Node's experimental `--permission` /
`--experimental-permission` flag (with `--allow-fs-read=<path>`,
`--allow-fs-write=<path>`, `*` wildcard) gates every fs read/write behind
an allowlist and throws an `ERR_ACCESS_DENIED`-style error when a path
isn't covered — this is a Node-process-level security feature, not part of
the fs *language* surface. RTS has no equivalent process permission model
today; see §7 (deferred, not required for functional parity).

**Disposal / `AbortSignal`**: `FileHandle` supports `Symbol.asyncDispose`
for `await using` cleanup; relying on GC-triggered auto-close (a process
warning is emitted, no guaranteed close) is explicitly discouraged by the
Node docs — always `.close()` explicitly. `AbortSignal` (`signal` option)
is honored by `readFile`/`writeFile`/`appendFile` (promise + `FileHandle`
forms) and by `createReadStream`/`createWriteStream`; it is **not**
honored by `open`, bare `read`/`write`, or any of the callback-form
top-level functions (abort mid-syscall isn't meaningfully supported there).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` owns its own Rust implementation — no dependency on `rts-std`.

| Surface area | Rust backing |
|---|---|
| open/read/write/close/seek | `std::fs::File` + `std::os::unix::fs::FileExt` (`read_at`/`write_at` for positional I/O without disturbing the fd cursor) / `std::os::windows::fs::FileExt` (`seek_read`/`seek_write`) |
| stat/lstat/fstat/metadata | `std::fs::metadata`/`symlink_metadata`/`File::metadata`; POSIX fields (`uid`/`gid`/`mode`/`ino`/`dev`/`nlink`/`blksize`/`blocks`) via `std::os::unix::fs::MetadataExt`; Windows via `std::os::windows::fs::MetadataExt` (`file_index`, `file_attributes`) with synthesized `mode`/`uid`/`gid` (0/read-only-bit-derived) to keep the `Stats` shape uniform |
| statfs | POSIX `libc::statvfs`/`statfs` (via a direct `libc` binding owned by `rts-node`, not `rts-std`'s `os` module); Windows `GetDiskFreeSpaceExW`/`GetVolumeInformationW` |
| mkdir/rmdir/rm/rename/copy/link/symlink/readlink/realpath | `std::fs::{create_dir, create_dir_all, remove_dir, remove_dir_all, remove_file, rename, copy, hard_link, soft_link/symlink_file/symlink_dir, read_link, canonicalize}` |
| chmod/chown | `std::fs::Permissions` + `set_permissions` (POSIX mode bits via `std::os::unix::fs::PermissionsExt`); Windows: toggle `FILE_ATTRIBUTE_READONLY` only. `chown`/`lchown` via raw `libc::chown`/`lchown` (POSIX only; no-op/`ENOSYS` stub on Windows) |
| utimes/lutimes/futimes | POSIX `libc::utimensat`/`futimens` (nanosecond precision) directly, since `std::fs` has no utimes API; Windows `SetFileTime` |
| cp (recursive copy) | hand-rolled walk over `std::fs::read_dir` + the single-file `copy` primitive above, owned entirely by `rts-node` (no shared "walker" crate needed) |
| glob | `rts-node`'s own glob-pattern matcher (either the `glob` crate as a direct `rts-node` dependency, or a small hand-rolled matcher — both legitimate per the "own its own crates" decision; does **not** reuse `rts-runtime`'s `regex` namespace, which is a different crate/namespace) |
| mkdtemp | `std::env::temp_dir()` + a random-suffix loop using the process's own CSPRNG (small inline generator in `rts-node`, or `std::time`-seeded fallback) retried on `EEXIST` |
| open flags → OS flags | `std::fs::OpenOptions` for the common flag strings (`r`/`w`/`a`/`r+`/`w+`/`a+`/`x` variants); raw `O_*` bit passthrough via `OpenOptionsExt::custom_flags` (Unix) / `OpenOptionsExt::attributes`+`share_mode` (Windows) for the numeric-flags overload |
| watch (FSWatcher) | Linux: raw `inotify` syscalls (own small wrapper, or the `inotify`/`notify` crate as a direct `rts-node` dep); macOS: FSEvents via `notify`/raw `CoreServices` bindings; Windows: `ReadDirectoryChangesW`. A single cross-platform crate (`notify`) is the pragmatic single dependency covering all three, owned by `rts-node` |
| watchFile (StatWatcher) | portable polling loop: periodic `stat` + field-diff, driven by the RTS timer/interval primitive, no native OS watch API needed |
| Dir/Dirent (opendir/readdir) | `std::fs::read_dir` (`ReadDir`/`DirEntry`); `d_type`-equivalent via `DirEntry::file_type()` (POSIX gets it from the dirent cheaply, Windows synthesizes from `metadata()`) |
| Stats bigint variant | same call, results represented in the wider handle-backed struct and read out either as `f64`/`i64` (default) or exposed as JS `BigInt` at the `.ts` layer when `{ bigint: true }` |
| streams (ReadStream/WriteStream) | thin `.ts` `Readable`/`Writable` subclasses driving repeated fd-based `read`/`write` extern calls in chunks of `highWaterMark`; no separate native stream object |

### 5.2 ABI surface

Convention: `__RTS_FN_NODE_FS_<NAME>`, replacing the current interim table
(`crates/rts-node/src/fs/mod.rs`) whose members still literally borrow
`__RTS_FN_NS_FS_*` symbols owned by `rts-std` — those borrowed symbols are
deleted along with the `rts-std` `fs` module and every one is re-implemented
natively under the new prefix.

Representative primitives (not exhaustive — one row per ABI-distinct
primitive; the `.ts` layer composes these into the full multi-overload
Node surface):

| Symbol | Args (`AbiType`) | Return | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_FS_OPEN` | `StrPtr` path, `I64` flags, `I32` mode | `I64` (fd, or `-errno` on failure — sign-encoded status, `.ts` shim raises) | fd is a raw OS integer, not a `Handle` — Node fd semantics require a small integer the JS side can pass to any fd-based call |
| `__RTS_FN_NODE_FS_CLOSE` | `I64` fd | `I32` (0 / `-errno`) | |
| `__RTS_FN_NODE_FS_READ` | `I64` fd, `Handle` bufHandle (ArrayBuffer), `I64` offset, `I64` length, `I64` position (`-1` = current) | `I64` (bytesRead, or `-errno`) | writes into the ArrayBuffer's backing bytes in place |
| `__RTS_FN_NODE_FS_WRITE` | `I64` fd, `Handle` bufHandle, `I64` offset, `I64` length, `I64` position | `I64` (bytesWritten, or `-errno`) | |
| `__RTS_FN_NODE_FS_STAT` / `_LSTAT` / `_FSTAT` | `StrPtr` path (or `I64` fd for fstat) | `Handle` (opaque `NodeStats` handle) | numeric fields read back via a handful of `__RTS_FN_NODE_FS_STATS_GET_<FIELD>(Handle) -> I64/F64` accessors, or one struct-returning call the `.ts` layer destructures — implementer's choice, documented in §5.8(a) |
| `__RTS_FN_NODE_FS_STATFS` | `StrPtr` path | `Handle` (opaque `NodeStatFs`) | |
| `__RTS_FN_NODE_FS_MKDIR` | `StrPtr` path, `Bool` recursive, `I32` mode | `I32` (0 / `-errno`) | |
| `__RTS_FN_NODE_FS_RM` | `StrPtr` path, `Bool` recursive, `Bool` force | `I32` | |
| `__RTS_FN_NODE_FS_RENAME` / `_COPY_FILE` / `_LINK` / `_SYMLINK` | `StrPtr` a, `StrPtr` b, (+ `I32` flags for copyFile / `StrPtr` type for symlink) | `I32` | two-path ops |
| `__RTS_FN_NODE_FS_READLINK` / `_REALPATH` | `StrPtr` path | `Handle` (GC string) | |
| `__RTS_FN_NODE_FS_READDIR_OPEN` | `StrPtr` path | `Handle` (opaque `NodeDir` iterator) | backs both `Dir` and `readdir`'s array form (`.ts` drains it) |
| `__RTS_FN_NODE_FS_READDIR_NEXT` | `Handle` dirHandle | `Handle` (opaque `NodeDirent`, or a sentinel `0` handle for EOF) | |
| `__RTS_FN_NODE_FS_DIRENT_NAME` / `_TYPE` | `Handle` direntHandle | `StrPtr` / `I32` (type tag) | `.ts` `Dirent.isFile()` etc. compare the tag |
| `__RTS_FN_NODE_FS_CHMOD` / `_CHOWN` / `_UTIMES` (+ `L`/`F` prefixed fd/symlink variants) | path or fd + numeric params | `I32` | |
| `__RTS_FN_NODE_FS_WATCH_OPEN` | `StrPtr` path, `Bool` recursive, `PolyValue` listener fn | `Handle` (opaque `NodeWatcher`) | listener is invoked from the watcher's background thread via the callback-bridge (see §5.3/§5.4) — carried as `PolyValue` (not `Handle`) precisely because it must cross the ABI as an arbitrary callable value, per `AbiType::PolyValue`'s documented purpose |
| `__RTS_FN_NODE_FS_WATCH_CLOSE` | `Handle` watcherHandle | `Void` | |
| `__RTS_FN_NODE_FS_GLOB` | `StrPtr` pattern, `StrPtr` cwd, `Bool` withFileTypes | `Handle` (GC array of strings, or dirent handles) | |

Rich objects — `Stats`, `StatFs`, `Dir`/directory-iterator cursor, `Dirent`,
`FSWatcher`/`StatWatcher` (native OS-watch subscription), `Utf8Stream`'s
internal buffer state — are **opaque `Handle`s** into `rts-node`'s own
handle table (built the same shard-aware way as `rts-engine::heap`'s
`HandleTable`, but owned by `rts-node` so it stays fully independent). The
JS-shaped classes (`fs.Stats`, `fs.Dirent`, `fs.Dir`, `fs.FSWatcher`,
`fs.ReadStream`/`WriteStream`, `FileHandle`) are **`.ts` shims** wrapping
these handles + the raw `open`/`read`/`write`/`close`/`stat` externs; the
multi-overload argument parsing (string vs Buffer vs URL path, options
object vs bare encoding string, sync/callback/promise dispatch) is entirely
`.ts`-side, never encoded in the native surface.

### 5.3 Async model

- **Sync (`*Sync`)**: `.ts` shim calls the native extern directly on the
  calling thread; the extern itself is a plain blocking `std::fs`/`libc`
  call. No event-loop/tokio involvement.
- **Callback**: `.ts` shim schedules the same blocking primitive onto a
  background-thread pool (see §5.4/§5.7 — needs the shared tokio runtime's
  `spawn_blocking`, or an equivalent RTS-owned worker pool) and invokes the
  user's `(err, ...)` callback via the `PolyValue` function-invocation bridge
  once the result is ready, marshalled back onto the event loop so ordering
  relative to other microtasks/timers matches Node's "callback runs on a
  future turn" contract.
- **Promise (`fs/promises`, `FileHandle`)**: same background-thread
  execution as callback, but settles an RTS `Promise` handle instead of
  invoking a callback — reuses the same `promise.create`-style
  spawn-and-settle pattern used elsewhere in the runtime (§5.7 flags the
  exact subsystem).
- **`watch`/`watchFile`**: long-lived background OS-thread (native
  inotify/FSEvents/ReadDirectoryChangesW blocking-read loop, or a tokio
  interval task for the polling `watchFile` case) that calls back into JS
  *repeatedly* over the object's lifetime — this is structurally an
  EventEmitter source, not a one-shot promise/callback, and must reuse
  whatever generic "native thread → JS callback" bridge backs
  `events`/timers today.
- **Streams**: `ReadStream`/`WriteStream` are `.ts`-level `Readable`/
  `Writable` subclasses that internally loop callback-style `read`/`write`
  calls chunk-by-chunk, honoring backpressure (`highWaterMark`) at the
  `.ts` stream layer — no separate native streaming primitive is needed
  beyond the same `read`/`write` externs used everywhere else.

### 5.4 Multithread / worker interaction

- fd numbers are OS-global (process-wide) resources — the native fd table
  itself needs no RTS-thread-local partitioning; `rts-node`'s own opaque
  `Handle` table for `Stats`/`Dir`/`Watcher`/etc. should be shard-aware the
  same way `rts-engine::heap::HandleTable` already is, so concurrent
  filesystem access from multiple RTS worker threads doesn't serialize on
  one lock.
- `FSWatcher`/`StatWatcher` each own a background OS thread (or tokio task)
  that is logically independent of any single JS worker thread; per
  `docs/specs/rts-threading-model.md`, the watcher's event delivery back
  into a specific JS thread's env-records is a **publish across regions** —
  the watcher thread's fired event must be treated like a `channel` send
  (or a promotion-on-publication of the small event payload — `eventType` +
  `filename` string — into the shared heap) rather than a raw cross-thread
  pointer write, so the GC/region model stays sound.
  - If `fs.watch`'s listener closes over a `threadLocal` variable, that
    capture must be rejected/boxed the same way any other
    cross-thread-invoked closure is under the threading model — the
    watcher callback executes conceptually "from another thread's
    perspective" even if implemented as a tokio task on a shared runtime
    worker.
- `worker_threads` interaction (out of scope for this module's core, but
  relevant since `fs` handles/fds are commonly shared across workers): a
  raw fd (`number`) is safe to pass across RTS worker boundaries as a plain
  integer (it's already an OS-global handle, no GC/region concerns); a
  `FileHandle`/`Stats`/`Dir` **opaque `Handle`**, however, is only valid
  within the region/heap that owns its slot unless `rts-node`'s handle
  table is itself promoted to the shared-heap tier — flagged here as a
  design point for whichever PR wires `fs` + `worker_threads` together
  (not required for the P0 single-thread-correct implementation).
- No RTS-engine primordial state is touched by this module; all mutable
  state (open-fd bookkeeping, watcher subscriptions) is `rts-node`-local,
  matching the "each namespace owns its own `Arc<Mutex<T>>`/shard table"
  convention used throughout the runtime layer.

### 5.5 Buffer / TypedArray interop

- Every byte-carrying parameter (`read`/`write`/`readv`/`writev` buffers,
  `readFile`/`writeFile` data when no encoding forces a string) crosses the
  ABI as a `Handle` to the primordial `ArrayBuffer`/`Uint8Array` (`Buffer`
  extends `Uint8Array`) that already backs TypedArrays in the engine —
  `rts-node` never allocates its own byte-buffer representation; it reads/
  writes directly into the engine-owned backing store via the same
  raw-memory access path TypedArrays use, passed as `(Handle, offset,
  length)` triples so partial reads/writes (`fs.read(fd, buf, offset,
  length, pos)`) don't require slicing a fresh buffer per call.
- String-mode reads (`options.encoding` set) decode the raw bytes to a JS
  string at the `.ts`/engine string-primitive boundary *after* the native
  read completes — the native extern always fills raw bytes; encoding
  interpretation is never done in Rust for anything but the small set of
  Node-native transforms (`base64`/`hex`/`base64url` — which do belong in
  `rts-node`-owned Rust helpers, since they are pure byte transforms with
  no JS-object concerns, mirroring how `crypto`'s base64/hex live directly
  in that namespace today rather than in a shared crate).
- `Buffer`'s allocation pool (`Buffer.allocUnsafe`'s internal pooling
  behavior) is a `.ts`-shim-level concern layered on top of the primordial
  `ArrayBuffer`, not something the native `fs` externs need to know about.

### 5.6 Doctrine placement

`fs` is unambiguously **non-primordial** — no native literal/syntactic form
(`new fs.Stats()` isn't even user-constructible; everything is reached via
`import ... from "node:fs"`). The engine must not name `fs`, `Stats`,
`Dirent`, `FileHandle`, etc. anywhere in `crates/rts-codegen-new/`.
Resolution is entirely through the existing node-module **data table**
already present in `crates/rts-node/src/lib.rs`:
`NodespaceSpec { node_module: "fs", ns_prefix: "node_fs", members }`,
collected into `NODE_SPECS`, resolved by `node_lookup("node_fs.readFileSync")`
and `ns_prefix_for("node:fs")` — an `import ... from "node:fs"` maps to the
`node_fs` codegen namespace purely by data lookup, with zero `if
module_name == "fs"` anywhere in the front-end. This is the direct fs-module
instance of the general "Registry for node modules" pattern the doctrine
requires.

Native-extern vs `.ts`-shim split, restated concretely for this module:
- **Native extern** (raw, `NodespaceMember`-declared, one job each): the
  primitives table in §5.2 — open/read/write/close/stat-family/mkdir/rm/
  rename/link/symlink/readlink/realpath/chmod/chown/utimes/readdir-cursor/
  watch-subscribe.
- **`.ts` shim** (shipped by `rts-node`, JS-shaped ergonomics): the
  `fs`/`fs/promises` default-export objects, every class in §2.1
  (`Stats`/`Dirent`/`Dir`/`FSWatcher`/`StatWatcher`/`ReadStream`/
  `WriteStream`/`FileHandle`/`Utf8Stream`), all multi-overload argument
  normalization (string|Buffer|URL path, options-object-or-bare-string
  encoding, numeric-or-string `open` flags), the `constants` object, the
  sync/callback/promise fan-out from one shared internal helper, `cp`'s
  recursive-walk *user-facing* filter/error semantics (the walk itself may
  be native, but Node's exact error-shape/`filter`-callback contract is
  `.ts`), and `glob`'s pattern-to-matches JS-facing shape.

### 5.7 Shared-infra dependencies (FLAG)

`rts-node` cannot depend on `rts-std`, but full `fs` parity (specifically:
callback variants, all of `fs/promises`, `watch`/`watchFile`, and streams)
needs infrastructure that **today only exists inside `rts-std`**. This must
be hoisted to a crate both `rts-engine`-tier and `rts-node` can reach
(either directly into `rts-engine`, or a new shared low crate below both)
before those areas can be implemented for real:

- **Promise settle/resolve-reject subsystem** — currently
  `rts-std/src/promise/` + `rts-std/src/promise_slot.rs`. Every
  `fs/promises` function and `FileHandle` method needs to create-and-settle
  a Promise the same way the existing `promise.create` pattern does.
- **Shared multi-thread tokio runtime** — currently
  `rts-std/src/runtime/async_rt.rs` (`rt()` global `OnceLock<Runtime>` +
  `on_thread_start`/`stop` GC-registration hooks). Needed to run blocking
  syscalls off whatever thread issued a callback/promise `fs` call without
  stalling the event loop, and as the natural host for `watchFile`'s
  polling interval task.
  - **Note this is doubly-flagged**: the GC's `thread_registry` hook that
    makes tokio-spawned tasks stack-scannable also currently lives under
    `rts-std`'s runtime wiring even though the *scanner* itself is
    `rts-engine`-owned — the registration call needs to be reachable from
    wherever the hoisted runtime ends up.
- **Event loop / microtask + macrotask draining** — currently
  `rts-std/src/event_loop.rs`. Needed so that scheduled callbacks
  (`fs.readFile(cb)`), settled promises, and repeating watcher events
  actually get drained and executed by `rts run`/AOT's main loop.
  A native-thread `fs.watch` event that fires without a live pending
  event-loop tick would otherwise never be delivered.
- **Generic native-thread → JS-callback invocation bridge** (the mechanism
  already backing `EventEmitter`/timers — invoking a `PolyValue` function
  handle from a background thread). `fs.watch`'s repeated listener
  invocation and every callback-style `fs` function need this; if it is
  `rts-std`-namespace-coupled today it needs the same hoist.
- **Not flagged (already reachable, no hoist needed)**: `rts-engine::heap`'s
  `HandleTable`/GC (used only as an architectural model to imitate, or
  directly if `rts-node` is given its own instance); the primordial
  `ArrayBuffer`/`TypedArray`/`Buffer` memory model (§5.5); `Promise` *as a
  primordial JS value* (the engine's own `Promise` object machinery,
  distinct from the `rts-std` create/settle glue above, which is
  runtime-implementation, not language-primordial).

If no hoist happens first, `fs`'s **sync** surface (§5.8 phase a) is fully
implementable standalone; everything callback/promise/watch/stream-shaped
is blocked on this flag being resolved.

### 5.8 Implementation phases

a. **Sync core primitives**: native `__RTS_FN_NODE_FS_*` externs for
   open/close/read/write/stat/lstat/fstat/mkdir/rmdir/rm/unlink/rename/
   copyFile/link/symlink/readlink/realpath/chmod/chown/utimes + the
   `Stats`/`StatFs` opaque-handle accessors and `fs.constants`. Wire the
   `*Sync` `.ts` functions directly on top — this alone is useful (many
   scripts/tools only need sync fs) and needs none of §5.7's flagged infra.
b. **`Dir`/`Dirent`/`readdir`/`opendir`/`glob`**: readdir-cursor extern +
   `Dirent` handle accessors; `.ts` `Dir` class + async-iterator protocol
   (iterator protocol itself is primordial/engine-owned, per the doctrine —
   `Dir`'s `[Symbol.asyncIterator]` is a thin `.ts` wrapper over the
   `_NEXT` extern).
c. **Hoist the blocker** (§5.7): move (or make reachable from) `rts-node`
   the promise-settle subsystem, the shared tokio runtime, the event loop,
   and the native-thread callback-invocation bridge. This is cross-cutting
   and benefits every other node module, not just `fs` — treat it as its
   own PR/milestone per the "resolve blocking limitations first" rule.
d. **Callback variants**: every sync primitive from (a)/(b) gets a
   `spawn_blocking`-backed callback form once (c) lands.
e. **Promise variants + `FileHandle`**: `fs/promises` functions and the
   `FileHandle` class (open once, `fd` reused across method calls,
   `Symbol.asyncDispose`) built on the same background execution as (d)
   but settling a Promise instead of invoking a callback.
f. **`cp`/`rm` recursive + `mkdtemp`/`mkdtempDisposable`**: compound
   operations layered on (a)-(e)'s primitives; `filter` callback support
   for `cp` needs the callback-invocation bridge from (c).
g. **`watch`/`watchFile`**: platform-native watcher (`notify` crate or
   hand-rolled per-platform) for `fs.watch`'s `FSWatcher`; polling loop for
   `watchFile`'s `StatWatcher`; `fs.promises.watch`'s async-iterator form
   layered on the same subscription, yielding via the iterator protocol
   instead of an EventEmitter.
h. **Streams**: `.ts` `ReadStream`/`WriteStream` (and `FileHandle`'s
   stream-returning methods) as `Readable`/`Writable` subclasses driving
   chunked read/write calls; `Utf8Stream` last (flagged §7 for version
   verification) since it is the least certain part of the surface.
i. **`openAsBlob`**, `readableWebStream`, `readLines` (FileHandle
   convenience methods) — thin layers once streams (h) and the global
   `Blob`/`ReadableStream`/`readline` pieces they depend on exist elsewhere
   in the runtime; lowest priority, most cross-module dependency.

## 6. Test plan

```
tests/node/fs/sync-basic.test.ts
  - writeFileSync + readFileSync round-trip (utf8 string, then Buffer/no-encoding)
  - existsSync true/false, statSync isFile/isDirectory, mkdirSync + rmSync recursive
  - appendFileSync appends rather than truncates
  - renameSync, copyFileSync (with COPYFILE_EXCL throwing EEXIST), unlinkSync
  - symlinkSync + readlinkSync + lstatSync (isSymbolicLink true, statSync follows through)
  - error paths: readFileSync on missing path throws with .code === 'ENOENT';
    mkdirSync non-recursive on existing dir throws EEXIST;
    rmSync non-recursive on non-empty dir throws ENOTEMPTY

tests/node/fs/callback-basic.test.ts
  - readFile/writeFile/appendFile/stat/mkdir/rm callback forms, err-first contract
  - nested callback chain (open -> read -> close) using raw fd
  - verify callback fires on a later turn, not synchronously (ordering assertion
    via a marker array pushed before/after the call)

tests/node/fs/promises-basic.test.ts
  - fs/promises readFile/writeFile/mkdir/rm happy path with await
  - fsPromises.open returns FileHandle; fh.read/fh.write/fh.stat/fh.close round-trip
  - Promise.all([...]) over several concurrent promise fs ops on independent paths
  - rejected promise .code assertion (ENOENT) via try/catch
  - `await using` FileHandle disposal (Symbol.asyncDispose) if `using` syntax supported,
    else explicit fh.close() equivalence test

tests/node/fs/filehandle.test.ts
  - open with 'w', 'r+', 'a' flags and verify resulting file content/position semantics
  - fh.read with explicit position vs null (current-position advance) both exercised
  - fh.readFile vs manual fh.read chunk loop produce identical bytes
  - fh.truncate/fh.utimes/fh.chmod each verified via a following stat

tests/node/fs/dir-dirent.test.ts
  - opendir + for-await-of iteration over Dirent entries, isFile/isDirectory checks
  - readdirSync withFileTypes true vs false (string[] vs Dirent[])
  - readdir recursive option (nested dirs) if implemented in this phase
  - empty directory iterates zero entries without error

tests/node/fs/streams.test.ts
  - createReadStream + 'data'/'end' events reconstruct full file content
  - createWriteStream + drain/finish events, backpressure with a large payload
    and a small highWaterMark
  - createReadStream({ start, end }) partial-range read
  - stream 'error' event on a missing path (no unhandled rejection/crash)

tests/node/fs/watch.test.ts
  - fs.watch on a file: modify it, assert a 'change' event fires (best-effort;
    mark platform-dependent expectations explicitly, esp. recursive on Linux)
  - fs.watchFile/unwatchFile polling round trip with a short custom interval
  - fs.promises.watch consumed via for-await, break out of the loop to stop it
  - watcher.close() (FSWatcher) stops further events

tests/node/fs/glob-cp-rm.test.ts
  - fs.cp recursive directory copy, verify nested file contents + a filter callback
    that excludes one subpath
  - fs.rm recursive removes a populated tree; force:true swallows ENOENT
  - fs.globSync a simple `*.txt` pattern against a fixture directory

tests/node/fs/constants.test.ts
  - fs.constants.F_OK/R_OK/W_OK/X_OK values match documented numeric constants
  - fs.access with W_OK on a read-only file rejects/throws EACCES

tests/node/fs/multithread.test.ts   (once worker_threads exists)
  - two RTS worker threads each writeFile to distinct paths concurrently, assert
    no cross-contamination and both complete
  - a raw fd opened on one thread and passed via a channel/message to another
    thread is usable there (validates the "fd is a plain OS-global integer,
    safe across threads" claim in §5.4)
```

## 7. Open questions / deferrals

- **`fs.Utf8Stream`** — surfaced by the doc fetch as a distinct
  high-throughput fixed-encoding writer class; its exact introducing
  version and full option/behavior contract must be verified directly
  against Node 25's changelog/source before implementing (marked
  `(verify)` throughout §2.1). Lowest priority in the implementation order
  (§5.8i).
- **Linux recursive `fs.watch`** — not natively supported by inotify;
  decide whether RTS reproduces Node's own userspace directory-tree-walk
  fallback (Node itself does not provide one — it simply doesn't support
  `recursive: true` on Linux and the app must roll its own multi-watch) or
  documents the same limitation and leaves it to userland `.ts` code.
- **Permission model (`--permission`)** — no RTS process-level permission
  system exists; deferred entirely until/unless RTS adopts an equivalent
  security model. Not required for functional fs parity.
- **`chown`/`lchown` exact Windows behavior** — Node's own docs are not
  fully explicit on no-op vs `ENOSYS` vs silent-success; verify against
  real Node 25 on Windows before locking in the native stub's behavior.
- **Per-drive working directory on Windows** (`process.chdir('D:\\')`
  semantics) — affects relative-path resolution for every `fs` call; likely
  belongs to `node:process`/`node:path`'s implementation spec rather than
  duplicated here, but flagged since `fs` is the primary consumer.
- **`UV_THREADPOOL_SIZE`-equivalent knob** — decide whether RTS exposes an
  analogous env var / config to size whatever worker pool backs
  callback/promise `fs` ops, or simply rides the shared tokio runtime's own
  worker-count configuration (simpler, but loses Node's per-subsystem
  threadpool-size tuning knob — acceptable divergence, should be stated
  explicitly in the eventual PR per the "regress explicitly" rule).
- **`readableWebStream`/`FileHandle.readLines`/`fs.openAsBlob`** — depend on
  global `ReadableStream`/`readline`/`Blob` machinery that may not exist
  yet elsewhere in RTS; deferred to last (§5.8i) and blocked on those
  globals' own implementation status, not re-litigated here.
- **`fs.constants.O_SYMLINK`/`O_NONBLOCK`** and a few other
  platform-conditionally-defined constants were not exhaustively
  enumerable from the fetched docs (Node only defines them on platforms
  that support the underlying flag) — enumerate the exact conditional set
  from Node's `lib/internal/fs/utils.js`/`deps/uv` constant tables at
  implementation time rather than guessing values here.
