//! The four things a terminal can be asked or told, per platform.
//!
//! # Why this is a seam
//!
//! Everything in [`super`] is JS-shaped — reading `this.fd`, answering a tuple,
//! calling a callback. Nothing here knows a value: it takes an `i64` fd and
//! answers Rust. The split is by concern rather than by size, and it is what
//! keeps the two `#[cfg]` ladders out of the class definitions, where they would
//! have been interleaved with the argument reading and impossible to read.
//!
//! # Reuse-check
//!
//! Searched for `GetConsoleMode`, `GetConsoleScreenBufferInfo`, `TIOCGWINSZ`,
//! `tcsetattr` and `windows-sys` across `crates/`. The only prior answer is the
//! OLD engine's `rts-node/src/tty/detect.rs`, which this crate must not depend
//! on (it is the engine being replaced, and it reaches `rts_engine::Engine`).
//! What is reused from it is its DECISION rather than its code: it declares the
//! handful of Win32 entry points it needs with a hand-written `extern "system"`
//! block instead of adding `windows-sys`, and `Cargo.toml` is `lib.rs`'s to own
//! — the same reason `os.rs` gives for not reaching for one. `rts-cranelift`
//! owns nothing about file descriptors, and `entry/modules.rs` exposes nothing
//! terminal-shaped, so no host surface answers any of this.

#[cfg(windows)]
use std::os::windows::io::{BorrowedHandle, RawHandle};
use std::sync::{LazyLock, Mutex};

/// Whether an fd is a terminal.
///
/// `std::io::IsTerminal` rather than `libc::isatty`/`GetConsoleMode` by hand,
/// per the spec's §5.1 preference — it is the one member of this file `std`
/// already answers on both platforms.
pub(super) fn is_terminal(fd: i64) -> bool {
    use std::io::IsTerminal;
    #[cfg(unix)]
    {
        use std::os::fd::{BorrowedFd, RawFd};
        let Ok(raw) = RawFd::try_from(fd) else {
            return false;
        };
        // SAFETY: the fd is not proven open here — an invalid one makes
        // `is_terminal()` answer `false` rather than any call actually
        // dereferencing memory, which is the same "no throw" contract
        // `isatty(3)` itself has. No ownership is taken: the borrow is dropped
        // before returning, and the underlying fd is never closed by it.
        let borrowed = unsafe { BorrowedFd::borrow_raw(raw) };
        borrowed.is_terminal()
    }
    #[cfg(windows)]
    {
        let Some(handle) = os_handle(fd) else {
            return false;
        };
        // SAFETY: `_get_osfhandle` answers a borrowed view of the CRT's own
        // handle table; it is not owned or closed here, matching the promise
        // `BorrowedHandle` documents. An invalid value was rejected by
        // `os_handle`.
        let borrowed = unsafe { BorrowedHandle::borrow_raw(handle as RawHandle) };
        borrowed.is_terminal()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = fd;
        false
    }
}

/// `(columns, rows)`, or `None` when the fd has no window to measure.
///
/// `None` — not a plausible `(80, 24)`. A guessed geometry is the wrong answer
/// that runs: a progress bar drawn to a width nobody has wraps every line.
pub(super) fn window_size(fd: i64) -> Option<(u32, u32)> {
    #[cfg(unix)]
    {
        let raw = libc::c_int::try_from(fd).ok()?;
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: `TIOCGWINSZ` writes exactly a `winsize` through the pointer,
        // which is a live local of that type. A non-terminal fd fails the call
        // rather than writing anything.
        let ok = unsafe { libc::ioctl(raw, libc::TIOCGWINSZ as libc::c_ulong, &mut size) } == 0;
        match ok && size.ws_col > 0 {
            true => Some((u32::from(size.ws_col), u32::from(size.ws_row))),
            false => None,
        }
    }
    #[cfg(windows)]
    {
        let handle = os_handle(fd)?;
        let mut info = win::ScreenBufferInfo::default();
        // SAFETY: the call writes exactly a `CONSOLE_SCREEN_BUFFER_INFO`
        // through the pointer, and `ScreenBufferInfo` is that record declared
        // `#[repr(C)]` field for field. A non-console handle fails instead.
        if unsafe { win::GetConsoleScreenBufferInfo(handle, &mut info) } == 0 {
            return None;
        }
        // The WINDOW, not the buffer: a console's scrollback buffer is taller
        // than the visible window, and `rows` is what the user can see.
        let columns = i32::from(info.window.right) - i32::from(info.window.left) + 1;
        let rows = i32::from(info.window.bottom) - i32::from(info.window.top) + 1;
        match columns > 0 && rows > 0 {
            true => Some((columns as u32, rows as u32)),
            false => None,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = fd;
        None
    }
}

/// What a terminal was set to before this process made it raw.
///
/// Held per fd so `setRawMode(false)` RESTORES rather than applies some
/// canonical cooked mode: the terminal a program inherited is not necessarily
/// the default one, and writing a guessed cooked mode back would leave the shell
/// subtly different from how it was found.
#[cfg(unix)]
type Saved = libc::termios;
#[cfg(windows)]
type Saved = u32;

#[cfg(any(unix, windows))]
static COOKED: LazyLock<Mutex<Vec<(i64, Saved)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Puts an fd into raw mode, or back into whatever it was before. Answers
/// whether the OS accepted it.
///
/// Process-wide state, not per-thread — a terminal's mode belongs to the device
/// — which is why the saved-mode table is one shared `Mutex` rather than a
/// thread-local (§5.4 of the spec names the same hazard).
pub(super) fn set_raw_mode(fd: i64, raw: bool) -> bool {
    #[cfg(unix)]
    {
        let Ok(target) = libc::c_int::try_from(fd) else {
            return false;
        };
        let mut state: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: writes exactly a `termios` through a live local of that type.
        if unsafe { libc::tcgetattr(target, &mut state) } != 0 {
            return false;
        }
        let Ok(mut saved) = COOKED.lock() else {
            return false;
        };
        let wanted = match raw {
            true => {
                if !saved.iter().any(|(held, _)| *held == fd) {
                    saved.push((fd, state));
                }
                let mut made = state;
                // SAFETY: mutates the local in place; `cfmakeraw` is the libc
                // spelling of the flag set §5.1 lists, reached rather than
                // re-derived so the two cannot disagree.
                unsafe { libc::cfmakeraw(&mut made) };
                made
            }
            false => match saved.iter().find(|(held, _)| *held == fd) {
                Some((_, original)) => *original,
                // Never made raw by this process: leave the device alone rather
                // than imposing a cooked mode it may never have had.
                None => return true,
            },
        };
        // SAFETY: `wanted` is a live, fully initialised `termios`.
        unsafe { libc::tcsetattr(target, libc::TCSANOW, &wanted) == 0 }
    }
    #[cfg(windows)]
    {
        let Some(handle) = os_handle(fd) else {
            return false;
        };
        let mut mode = 0u32;
        // SAFETY: writes one `DWORD` through a live local. A non-console handle
        // fails the call instead.
        if unsafe { win::GetConsoleMode(handle, &mut mode) } == 0 {
            return false;
        }
        let Ok(mut saved) = COOKED.lock() else {
            return false;
        };
        let wanted = match raw {
            true => {
                if !saved.iter().any(|(held, _)| *held == fd) {
                    saved.push((fd, mode));
                }
                mode & !(win::ENABLE_ECHO_INPUT | win::ENABLE_LINE_INPUT | win::ENABLE_PROCESSED_INPUT)
            }
            false => match saved.iter().find(|(held, _)| *held == fd) {
                Some((_, original)) => *original,
                None => return true,
            },
        };
        // SAFETY: a plain integer call on a handle the CRT owns.
        unsafe { win::SetConsoleMode(handle, wanted) != 0 }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (fd, raw);
        false
    }
}

/// Writes bytes straight to an fd, answering whether all of them landed.
///
/// Direct rather than through a buffered writer: §5.7 flags that the ambient
/// `console` already owns fd 1/2 through another path, and adding a SECOND
/// buffer over the same descriptor is what turns an ordering hazard into
/// reordered output. Unbuffered means an escape sequence is at least ordered
/// against whatever `console` has already flushed.
pub(super) fn write_all(fd: i64, bytes: &[u8]) -> bool {
    #[cfg(windows)]
    if !enable_virtual_terminal(fd) {
        return false;
    }
    #[cfg(any(unix, windows))]
    {
        let Ok(target) = libc::c_int::try_from(fd) else {
            return false;
        };
        let mut written = 0usize;
        while written < bytes.len() {
            let count = write_one(target, &bytes[written..]);
            if count <= 0 {
                return false;
            }
            written += count as usize;
        }
        true
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (fd, bytes);
        false
    }
}

/// One `write(2)` (or `_write`) call, answering its raw return.
///
/// Split by platform for the same reason [`os_handle`] is guarded: the UCRT's
/// `_write` validates `target` against its own descriptor table the identical
/// way `_get_osfhandle` does, so a bogus fd here hits the same "invalid
/// parameter" abort without the guard. This is not exercised by anything in
/// this crate's suite today — every caller of [`write_all`] goes through a
/// stream whose `fd` came from [`super::isatty`]'s own validated path — but
/// the hazard is identical, so the remedy is applied here too rather than left
/// for the next caller to rediscover.
#[cfg(windows)]
fn write_one(target: libc::c_int, bytes: &[u8]) -> isize {
    // SAFETY: pointer and length describe a live slice; a closed or read-only
    // fd answers a negative count rather than writing.
    unsafe {
        win::with_quiet_invalid_parameter(|| {
            libc::write(target, bytes.as_ptr().cast::<libc::c_void>(), bytes.len() as _) as isize
        })
    }
}

#[cfg(unix)]
fn write_one(target: libc::c_int, bytes: &[u8]) -> isize {
    // SAFETY: pointer and length describe a live slice; a closed or read-only
    // fd answers a negative count rather than writing.
    unsafe { libc::write(target, bytes.as_ptr().cast::<libc::c_void>(), bytes.len() as _) as isize }
}

/// Whether ANSI escapes written to this fd will be interpreted.
///
/// A console that refuses `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is a legacy
/// `conhost` needing the `SetConsoleCursorPosition`/`FillConsoleOutputCharacterW`
/// path, which this module refuses by name — so the honest answer is to write
/// NOTHING rather than spray `\x1b[2K` into the user's screen as literal text.
/// A handle that is not a console at all (redirected to a file or a pipe) is not
/// that case: Node writes the bytes there too, and so does this.
#[cfg(windows)]
fn enable_virtual_terminal(fd: i64) -> bool {
    let Some(handle) = os_handle(fd) else {
        return false;
    };
    let mut mode = 0u32;
    // SAFETY: writes one `DWORD` through a live local.
    if unsafe { win::GetConsoleMode(handle, &mut mode) } == 0 {
        return true;
    }
    if mode & win::ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
        return true;
    }
    // SAFETY: a plain integer call on a handle the CRT owns.
    unsafe { win::SetConsoleMode(handle, mode | win::ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0 }
}

/// The `HANDLE` a CRT fd number stands for.
///
/// # Why the call is bracketed
///
/// `_get_osfhandle` does not just answer `-1` for an fd outside the CRT's own
/// descriptor table — it invokes the UCRT's "invalid parameter" handler first,
/// and the process default there is `abort()`. That is fatal for a fd nothing
/// here chose: `isatty(999)` on a bogus number is ordinary JS, and this
/// module's own doc (`tty/mod.rs`, "never throws") promises `false`, not a
/// crashed process. Verified by reproduction: `isatty(999)` killed the process
/// with 0xC0000409 before this guard existed, both under `rts run` and
/// `rts test`.
#[cfg(windows)]
fn os_handle(fd: i64) -> Option<win::Handle> {
    let target = libc::c_int::try_from(fd).ok()?;
    let handle = unsafe { win::with_quiet_invalid_parameter(|| libc::get_osfhandle(target)) };
    match handle == -1isize as libc::intptr_t {
        true => None,
        false => Some(handle as win::Handle),
    }
}

/// The four Win32 entry points this module needs, declared rather than
/// depended on — see the reuse-check in the module doc.
#[cfg(windows)]
#[allow(
    dead_code,
    reason = "the records below are what the OS WRITES, so every field must be \
              present and in order even though only `window` is read back — \
              dropping the unread ones would change the struct's size and the \
              call would scribble past it"
)]
mod win {
    pub(super) type Handle = *mut core::ffi::c_void;

    pub(super) const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
    pub(super) const ENABLE_LINE_INPUT: u32 = 0x0002;
    pub(super) const ENABLE_ECHO_INPUT: u32 = 0x0004;
    pub(super) const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

    #[repr(C)]
    #[derive(Default)]
    pub(super) struct Coord {
        pub(super) x: i16,
        pub(super) y: i16,
    }

    #[repr(C)]
    #[derive(Default)]
    pub(super) struct SmallRect {
        pub(super) left: i16,
        pub(super) top: i16,
        pub(super) right: i16,
        pub(super) bottom: i16,
    }

    /// `CONSOLE_SCREEN_BUFFER_INFO`, field for field and in order.
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct ScreenBufferInfo {
        pub(super) size: Coord,
        pub(super) cursor: Coord,
        pub(super) attributes: u16,
        pub(super) window: SmallRect,
        pub(super) maximum: Coord,
    }

    unsafe extern "system" {
        pub(super) fn GetConsoleScreenBufferInfo(handle: Handle, info: *mut ScreenBufferInfo) -> i32;
        pub(super) fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
        pub(super) fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
    }

    /// The UCRT's own "something was invalid" callback shape
    /// (`_invalid_parameter_handler`, `<stdlib.h>`) — never called BY this
    /// module, only INSTALLED, so [`with_quiet_invalid_parameter`] can make it
    /// do nothing instead of the default `abort()`.
    type InvalidParameterHandler =
        Option<unsafe extern "C" fn(*const u16, *const u16, *const u16, u32, usize)>;

    // `extern "C"`, not `"system"`: this is a UCRT runtime function
    // (`<stdlib.h>`), not a Win32 API call, and the two calling conventions
    // agree on x64 but not on x86 — matching the C declaration is what keeps
    // this correct on either. No `#[link(name = ...)]` needed: `libc`'s own
    // `get_osfhandle`/`write` above already pull in the CRT import library
    // (`msvcrt`/`libcmt`, `mod.rs`'s reuse-check line 252) that exports this
    // symbol too, at the same final link.
    unsafe extern "C" {
        fn _set_thread_local_invalid_parameter_handler(handler: InvalidParameterHandler) -> InvalidParameterHandler;
    }

    /// Does nothing — installed only for the duration of one CRT call, so an
    /// fd that call rejects answers through its normal "invalid" return value
    /// instead of taking the process down.
    unsafe extern "C" fn ignore_invalid_parameter(
        _expression: *const u16,
        _function: *const u16,
        _file: *const u16,
        _line: u32,
        _reserved: usize,
    ) {
    }

    /// Runs `f` with the UCRT's invalid-parameter path silenced ON THIS
    /// THREAD, then restores whatever was installed before — the handler is
    /// documented thread-local by its own name, so this cannot race a call on
    /// another thread, and nothing here nests (no caller of `f` in this module
    /// calls back into this function).
    ///
    /// # Safety
    /// `f` is a CRT call whose default failure path is `abort()` for exactly
    /// the input this exists to make survivable (an fd outside the CRT's own
    /// table); the caller is responsible for `f` performing no other unsafety.
    pub(super) unsafe fn with_quiet_invalid_parameter<T>(f: impl FnOnce() -> T) -> T {
        // SAFETY: installs a handler that reads nothing and writes nothing —
        // seen above, `ignore_invalid_parameter` — then restores the previous
        // one immediately after `f` returns.
        let previous = unsafe { _set_thread_local_invalid_parameter_handler(Some(ignore_invalid_parameter)) };
        let result = f();
        unsafe {
            _set_thread_local_invalid_parameter_handler(previous);
        }
        result
    }
}
