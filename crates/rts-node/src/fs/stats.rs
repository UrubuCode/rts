//! node:fs — the `Stats` object returned by `statSync`/`lstatSync`. Authored as
//! a `#[rtse::class]`: the instance IS the Rust struct (`Entry::Rtse`), holding
//! the numeric fields + the `ftype` tag directly instead of a flattened
//! `Entry::Map` with a `__rts_class`/`__ftype` side-tag. Every value is read
//! from the real `std::fs::Metadata`.
//!
//! `Stats` is never constructed from JS (`statSync`/`lstatSync`/the callback and
//! promise variants build it internally), so there is no `#[rtse::ctor]` —
//! [`build`] is a plain Rust fn, and `rts_engine::heap::handles::alloc_rtse`
//! (the same allocation path a ctor would generate) turns it into the `u64`
//! handle every caller already expects.

use std::time::UNIX_EPOCH;

use rts_engine::heap::handles::alloc_rtse;

fn time_ms(t: std::io::Result<std::time::SystemTime>) -> f64 {
    t.ok()
        .and_then(|st| st.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

#[cfg(unix)]
fn mode_of(m: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    m.mode() as i64
}

#[cfg(not(unix))]
fn mode_of(m: &std::fs::Metadata) -> i64 {
    // Windows has no POSIX mode: synthesize the read/write bits Node reports.
    if m.permissions().readonly() { 0o444 } else { 0o666 }
}

/// The `ftype` tag: 0 file / 1 dir / 2 symlink / 3 block / 4 char / 5 fifo /
/// 6 socket. Detected from the real file type (the special types only exist on
/// Unix; on other platforms a non-dir/non-symlink is a regular file).
fn ftype_of(m: &std::fs::Metadata) -> i64 {
    if m.is_dir() {
        return 1;
    }
    if m.file_type().is_symlink() {
        return 2;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        let ft = m.file_type();
        if ft.is_block_device() {
            return 3;
        }
        if ft.is_char_device() {
            return 4;
        }
        if ft.is_fifo() {
            return 5;
        }
        if ft.is_socket() {
            return 6;
        }
    }
    0
}

/// The POSIX-only numeric fields (`dev`/`ino`/`nlink`/`uid`/`gid`/`rdev`/
/// `blksize`/`blocks`) from `std::os::unix::fs::MetadataExt`. On non-Unix these
/// are not exposed by `std`; report the values Node itself reports on Windows
/// (`uid`/`gid` = 0, `nlink` = 1, the rest 0) rather than fabricating them.
#[cfg(unix)]
fn posix_fields(m: &std::fs::Metadata) -> [f64; 8] {
    use std::os::unix::fs::MetadataExt;
    [
        m.dev() as f64,
        m.ino() as f64,
        m.nlink() as f64,
        m.uid() as f64,
        m.gid() as f64,
        m.rdev() as f64,
        m.blksize() as f64,
        m.blocks() as f64,
    ]
}

#[cfg(not(unix))]
fn posix_fields(_m: &std::fs::Metadata) -> [f64; 8] {
    [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]
}

/// A `Stats` instance: every field `std::fs::Metadata` gives us, plus the
/// `ftype` tag driving the `is*` predicates.
#[rtse::class("Stats")]
#[derive(Clone)]
pub struct Stats {
    ftype: i64,
    size: f64,
    mode: f64,
    mtime_ms: f64,
    atime_ms: f64,
    birthtime_ms: f64,
    ctime_ms: f64,
    dev: f64,
    ino: f64,
    nlink: f64,
    uid: f64,
    gid: f64,
    rdev: f64,
    blksize: f64,
    blocks: f64,
}

#[rtse::class("Stats")]
impl Stats {
    /// `stats.isFile()`.
    #[rtse::method]
    fn is_file(&self) -> bool {
        self.ftype == 0
    }
    /// `stats.isDirectory()`.
    #[rtse::method]
    fn is_directory(&self) -> bool {
        self.ftype == 1
    }
    /// `stats.isSymbolicLink()`.
    #[rtse::method(name = "isSymbolicLink")]
    fn is_symlink(&self) -> bool {
        self.ftype == 2
    }
    /// `stats.isBlockDevice()`.
    #[rtse::method(name = "isBlockDevice")]
    fn is_block(&self) -> bool {
        self.ftype == 3
    }
    /// `stats.isCharacterDevice()`.
    #[rtse::method(name = "isCharacterDevice")]
    fn is_char(&self) -> bool {
        self.ftype == 4
    }
    /// `stats.isFIFO()`.
    #[rtse::method(name = "isFIFO")]
    fn is_fifo(&self) -> bool {
        self.ftype == 5
    }
    /// `stats.isSocket()`.
    #[rtse::method]
    fn is_socket(&self) -> bool {
        self.ftype == 6
    }

    #[rtse::getter]
    fn size(&self) -> f64 {
        self.size
    }
    #[rtse::getter]
    fn mode(&self) -> f64 {
        self.mode
    }
    #[rtse::getter(name = "mtimeMs")]
    fn mtime_ms(&self) -> f64 {
        self.mtime_ms
    }
    #[rtse::getter(name = "atimeMs")]
    fn atime_ms(&self) -> f64 {
        self.atime_ms
    }
    #[rtse::getter(name = "ctimeMs")]
    fn ctime_ms(&self) -> f64 {
        self.ctime_ms
    }
    #[rtse::getter(name = "birthtimeMs")]
    fn birthtime_ms(&self) -> f64 {
        self.birthtime_ms
    }
    #[rtse::getter]
    fn dev(&self) -> f64 {
        self.dev
    }
    #[rtse::getter]
    fn ino(&self) -> f64 {
        self.ino
    }
    #[rtse::getter]
    fn nlink(&self) -> f64 {
        self.nlink
    }
    #[rtse::getter]
    fn uid(&self) -> f64 {
        self.uid
    }
    #[rtse::getter]
    fn gid(&self) -> f64 {
        self.gid
    }
    #[rtse::getter]
    fn rdev(&self) -> f64 {
        self.rdev
    }
    #[rtse::getter]
    fn blksize(&self) -> f64 {
        self.blksize
    }
    #[rtse::getter]
    fn blocks(&self) -> f64 {
        self.blocks
    }
}

/// Build a `Stats` instance from `std::fs::Metadata`.
pub fn build(m: &std::fs::Metadata) -> u64 {
    let pf = posix_fields(m);
    // ctime is not portable via std; Node reports the inode-change time — use
    // mtime as the closest portable value the metadata exposes.
    alloc_rtse(
        "Stats",
        Stats {
            ftype: ftype_of(m),
            size: m.len() as f64,
            mode: mode_of(m) as f64,
            mtime_ms: time_ms(m.modified()),
            atime_ms: time_ms(m.accessed()),
            birthtime_ms: time_ms(m.created()),
            ctime_ms: time_ms(m.modified()),
            dev: pf[0],
            ino: pf[1],
            nlink: pf[2],
            uid: pf[3],
            gid: pf[4],
            rdev: pf[5],
            blksize: pf[6],
            blocks: pf[7],
        },
    )
}
