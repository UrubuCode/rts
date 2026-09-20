//! `rts notice` — the aggregate attribution for what is LINKED into the binary
//! the user compiled.
//!
//! This exists because of whose obligation it is. RTS is MIT and imposes
//! nothing, but `rts compile` statically links `rts-runtime`, and with it ~600
//! third-party crates — several under BSD-2/3-Clause, Apache-2.0 or the Unicode
//! licence, all of which ask that the copyright notice travel "in the
//! documentation and/or other materials provided with the distribution". When
//! someone ships the executable we produced, that distribution is *theirs*, and
//! the obligation lands on a person who never chose any of those dependencies.
//! `THIRD-PARTY-NOTICES.md` recorded this as owed for as long as there was
//! nothing they could be handed.
//!
//! So the notice travels with the toolchain rather than being generated per
//! build: it is embedded here, and a user discharges the whole thing with
//! `rts notice > NOTICE.txt` beside their program. `scripts/notice/build_notice.mjs`
//! is what writes it, and `--check` is what keeps it from going stale.
//!
//! Embedded and not read from disk beside the exe, because a file beside the
//! exe is a file that can be missing — and an attribution notice that is
//! sometimes absent is the failure this was written to remove, not a smaller
//! version of it.

use anyhow::{Context, Result};
use std::path::Path;

/// The generated file, verbatim. `include_str!` rather than a build script: the
/// notice changes when `Cargo.lock` changes, which is a commit, not a build.
const NOTICE: &str = include_str!("../../../../RUNTIME-NOTICE.txt");

pub fn command(output: Option<String>) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(Path::new(&path), NOTICE)
                .with_context(|| format!("writing the notice to {path}"))?;
            println!("{path}: {} bytes", NOTICE.len());
        }
        None => print!("{NOTICE}"),
    }
    Ok(())
}
