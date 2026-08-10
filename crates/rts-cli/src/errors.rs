//! How a failed command is printed.
//!
//! # What this replaced
//!
//! `rts-diagnostics`, 733 lines: a `RichDiagnostic` with codes, spans, notes and
//! suggestions, a source store keyed by `FileId`, a snippet renderer with a
//! caret line, and a process-global engine collecting them.
//!
//! Every one of those had exactly zero producers. `emit()` was never called from
//! outside that crate — the things that used to call it were the old engine's
//! parser and type checker, which reported spanned diagnostics and were deleted
//! with it. So `global_engine().has_errors()` was a constant `false`, the branch
//! reading it was unreachable, and `render_all()` rendered an empty list.
//!
//! What actually printed every error this CLI has produced since the cutover is
//! the `anyhow` chain formatter below.
//!
//! # When a span comes back
//!
//! It will, and from a different direction: the engine records a
//! `rts_cranelift::fault::Position` per instruction and nothing yet maps an
//! address back to a line. When that mapping exists, the renderer belongs beside
//! it — with a live producer — rather than restored here from a crate that
//! outlived its callers.

use std::fmt::Write;

/// The `anyhow` chain, deepest cause first, context frames under it.
///
/// The filter drops wrapper frames that say only which stage failed: a reader
/// who asked to run a program already knows the run is what failed, and the
/// chain is worth printing precisely for the part they do not know.
pub fn format_anyhow_error(error: &anyhow::Error, use_color: bool) -> String {
    let red = match use_color {
        true => "\x1b[1;31m",
        false => "",
    };
    let reset = match use_color {
        true => "\x1b[0m",
        false => "",
    };
    let bold = match use_color {
        true => "\x1b[1m",
        false => "",
    };
    let dim = match use_color {
        true => "\x1b[2m",
        false => "",
    };

    let chain: Vec<String> = error.chain().map(|cause| cause.to_string()).collect();
    let meaningful: Vec<&str> = chain
        .iter()
        .map(String::as_str)
        .filter(|line| {
            !line.starts_with("JIT run of")
                && !line.starts_with("failed to parse")
                && !line.starts_with("failed to read")
                && *line != "JIT compile failed"
                && *line != "compile failed"
        })
        .collect();

    let Some(primary) = meaningful.last() else {
        return format!("{red}error{reset}{bold}: {error}{reset}\n");
    };

    let mut out = format!("{red}error{reset}{bold}: {primary}{reset}\n");
    for frame in meaningful.iter().rev().skip(1) {
        let _ = writeln!(out, "{dim}      at {frame}{reset}");
    }
    out
}

/// Whether stderr is a terminal, which is the only reason to emit colour.
pub fn stderr_supports_color() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}
