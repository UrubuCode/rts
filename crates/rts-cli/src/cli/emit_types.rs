//! `rts emit-types` — generate `rts.d.ts` from what the NEW engine declares.
//!
//! It rendered `rts-codegen-new`'s registry until now, which described a
//! compiler that has not run a program since the cutover: a project
//! type-checking against that file was told about `rts:io`, `rts:net` and a
//! `String` class this engine does not provide, and told nothing about what it
//! does. A `.d.ts` is a promise about the runtime, and that one was measuring
//! the wrong runtime.
//!
//! The source is now `#[rtse::class]` itself — the attribute derives each
//! signature from the Rust one and captures the `///` beside it, so there is no
//! second spelling to keep in agreement. `rts_core::entry::declared` states
//! what the coverage is and what it deliberately leaves out.

use anyhow::{Context, Result};

pub fn command(output: Option<String>) -> Result<()> {
    let dts = rts_core::entry::declared::render();
    match output {
        Some(path) => {
            std::fs::write(&path, &dts).with_context(|| format!("writing {path}"))?;
            println!("wrote {path} ({} bytes)", dts.len());
        }
        None => print!("{dts}"),
    }
    Ok(())
}
