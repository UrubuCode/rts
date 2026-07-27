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
use rts_symbol_baker::{DEFAULT_OUT, SCANNED_CRATES, render, scan_workspace};

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

    let decls = scan_workspace(&root, SCANNED_CRATES)?;
    let rendered = render(decls)?;

    if check {
        let current = std::fs::read_to_string(&out)
            .with_context(|| format!("read {} (never baked?)", out.display()))?;
        if current != rendered {
            bail!(
                "{} is STALE — a scanned source changed without re-baking.\n\
                 Run `cargo run -p rts-symbol-baker` and commit the result.",
                out.display()
            );
        }
        eprintln!("symbol table up to date: {}", out.display());
        return Ok(());
    }

    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("create out dir {}", dir.display()))?;
    }
    std::fs::write(&out, &rendered).with_context(|| format!("write {}", out.display()))?;
    eprintln!(
        "baked symbol table: {} bytes → {}",
        rendered.len(),
        out.display()
    );
    Ok(())
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next().with_context(|| format!("{flag} needs a value"))
}
