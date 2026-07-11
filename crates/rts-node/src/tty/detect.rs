//! node:tty — terminal detection: `isatty` (real fd query) and the
//! color-capability algorithm behind `getColorDepth`/`hasColors`, computed from
//! the live environment exactly as Node keys off `NO_COLOR`/`FORCE_COLOR`/
//! `COLORTERM`/`TERM`. No fabricated capabilities.

/// `tty.isatty(fd)` — whether `fd` refers to a terminal. Handles the standard
/// descriptors 0/1/2 on every platform (other fds → `false`, matching how a
/// non-std fd is rarely a console handle we can name portably).
pub fn isatty(fd: i32) -> bool {
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetStdHandle(n: u32) -> *mut core::ffi::c_void;
            fn GetConsoleMode(h: *mut core::ffi::c_void, mode: *mut u32) -> i32;
        }
        // STD_INPUT/OUTPUT/ERROR = -10/-11/-12 as DWORD.
        let std_id = match fd {
            0 => 0xFFFF_FFF6u32, // -10
            1 => 0xFFFF_FFF5,    // -11
            2 => 0xFFFF_FFF4,    // -12
            _ => return false,
        };
        let mut mode = 0u32;
        unsafe {
            let h = GetStdHandle(std_id);
            GetConsoleMode(h, &mut mode) != 0
        }
    }
    #[cfg(unix)]
    {
        unsafe { libc::isatty(fd) == 1 }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = fd;
        false
    }
}

/// Color-support depth in BITS (Node's convention): 1 = monochrome (2 colors),
/// 4 = 16 colors, 8 = 256 colors, 24 = 16M truecolor. Derived from the same env
/// variables Node consults, with the fd's TTY status as the baseline.
pub fn color_depth(fd: i32) -> u32 {
    let env = |k: &str| std::env::var(k).ok();

    // NO_COLOR (any non-empty value) forces monochrome.
    if env("NO_COLOR").map(|v| !v.is_empty()).unwrap_or(false) {
        return 1;
    }
    // FORCE_COLOR overrides detection: 0=off, 1=16, 2=256, 3=16M (also "true"/"").
    if let Some(fc) = env("FORCE_COLOR") {
        return match fc.as_str() {
            "0" | "false" | "none" => 1,
            "1" | "true" | "" => 4,
            "2" => 8,
            _ => 24, // "3" and anything higher
        };
    }

    let term = env("TERM").unwrap_or_default().to_lowercase();
    let colorterm = env("COLORTERM").unwrap_or_default().to_lowercase();

    if term == "dumb" {
        return 1;
    }
    if colorterm == "truecolor" || colorterm == "24bit" {
        return 24;
    }
    if term.contains("256") || term.contains("256color") {
        return 8;
    }
    if !colorterm.is_empty()
        || term.contains("color")
        || term.contains("ansi")
        || term.contains("xterm")
        || term.contains("screen")
        || term.contains("vt100")
    {
        return 4;
    }
    // No explicit signal: a real terminal still supports basic 16-color; a
    // non-TTY (pipe/file) supports none.
    if isatty(fd) { 4 } else { 1 }
}

/// `tty.WriteStream.hasColors([count])` — whether at least `count` colors are
/// available (`count` defaults to 16). `2**depth >= count`.
pub fn has_colors(count: u32, fd: i32) -> bool {
    let count = if count == 0 { 16 } else { count };
    (1u64 << color_depth(fd)) >= count as u64
}
