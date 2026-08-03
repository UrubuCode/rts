//! `rts-symbol-baker` CLI.
//!
//! ```text
//! rts-symbol-baker [--check] [--root <workspace-root>] [--out <path>]
//! ```
//!
//! Default: scan the workspace and WRITE the generated table to
//! [`DEFAULT_OUT`]. With `--check`: render in memory and compare byte-for-byte
//! against the checked-in file, exiting non-zero on any difference. `--check` is
//! the drift guard — a checked-in generated artefact is only trustworthy if
//! something fails when a source changes and nobody re-baked.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use rts_symbol_baker::{DEFAULT_OUT, SCANNED_CRATES, scan_workspace};

fn main() -> Result<()> {
    let mut check = false;
    let mut root: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--check" => check = true,
            "--root" => root = Some(PathBuf::from(next(&mut args, "--root")?)),
            "--out" => out = Some(PathBuf::from(next(&mut args, "--out")?)),
            other => bail!("unknown argument `{other}` (expected --check/--root/--out)"),
        }
    }

    // Default root: the workspace this crate lives in (`crates/rts-symbol-baker`
    // → two levels up). Keeps `cargo run -p rts-symbol-baker` working from
    // anywhere without an argument.
    let root = root.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let out = out.unwrap_or_else(|| root.join(DEFAULT_OUT));

    // One scan, one order, two renderings. Both files describe the same
    // declarations — by NAME in symbol_table.rs, by INDEX in entries.rs — and
    // they are prepared once so they cannot describe different sets.
    let decls = rts_symbol_baker::emit::prepare(scan_workspace(&root, SCANNED_CRATES)?)?;
    let rendered = rts_symbol_baker::emit::render_prepared(&decls);
    let entries = rts_symbol_baker::entries::render(&decls)?;

    let entries_out = out
        .parent()
        .map(|dir| dir.join("entries.rs"))
        .unwrap_or_else(|| PathBuf::from("entries.rs"));

    if check {
        // Both, because a stale entries.rs is worse than a stale symbol_table.rs:
        // a name that moved fails to link, an index that moved calls the wrong
        // function.
        verify(&out, &rendered)?;
        verify(&entries_out, &entries)?;
        eprintln!("generated tables up to date");
        return Ok(());
    }

    write(&out, &rendered)?;
    write(&entries_out, &entries)?;
    Ok(())
}

fn verify(path: &std::path::Path, rendered: &str) -> Result<()> {
    let current = std::fs::read_to_string(path)
        .with_context(|| format!("read {} (never baked?)", path.display()))?;
    if current != rendered {
        bail!(
            "{} is STALE — a scanned source changed without re-baking.\n\
             Run `cargo run -p rts-symbol-baker` and commit the result.",
            path.display()
        );
    }
    Ok(())
}

fn write(path: &std::path::Path, rendered: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("create out dir {}", dir.display()))?;
    }
    std::fs::write(path, rendered).with_context(|| format!("write {}", path.display()))?;
    eprintln!("baked {} bytes → {}", rendered.len(), path.display());
    Ok(())
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next().with_context(|| format!("{flag} needs a value"))
}
