//! Build script that prepares an embedded copy of the RTS static library.
//!
//! The library crate is compiled with `crate-type = ["rlib", "staticlib"]`,
//! meaning `target/<profile>/{librts.a|rts.lib}` appears during the same
//! cargo invocation that links the bin. This script copies that artifact
//! into `OUT_DIR` so the bin can embed it via `include_bytes!`.
//!
//! Chicken-and-egg caveat: on a fresh tree the static library does not yet
//! exist when this script first runs. To keep `cargo check` and the very
//! first `cargo build` compiling, we fall back to writing an empty
//! placeholder. The binary detects the empty payload at runtime and asks
//! the user to rebuild — the second build captures the real artifact.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    let embedded = out_dir.join("rts_runtime.staticlib");

    let source = locate_staticlib();
    match source.as_ref().filter(|p| p.exists()) {
        Some(path) => {
            fs::copy(path, &embedded)
                .unwrap_or_else(|e| panic!("failed to copy {}: {e}", path.display()));
            println!("cargo:rerun-if-changed={}", path.display());
        }
        None => {
            // Placeholder empty file keeps `include_bytes!` happy while the
            // staticlib has not been produced yet. The runtime refuses to
            // extract an empty payload and prints a rebuild hint instead.
            fs::write(&embedded, b"").expect("failed to write placeholder staticlib");
        }
    }

    println!(
        "cargo:rustc-env=RTS_RUNTIME_STATICLIB={}",
        embedded.display()
    );
    println!(
        "cargo:rustc-env=RTS_RUNTIME_STATICLIB_EXT={}",
        staticlib_extension()
    );
    println!("cargo:rerun-if-changed=build.rs");
}

/// Predicts where Cargo writes the current package's static library.
///
/// Honours `CARGO_TARGET_DIR` and falls back to `<manifest>/target`. The
/// profile is inferred from the `PROFILE` env var Cargo exports to build
/// scripts.
fn locate_staticlib() -> Option<PathBuf> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")?;
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(manifest_dir).join("target"));

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let stem = staticlib_stem();
    let ext = staticlib_extension();
    let candidate = target_dir
        .join(&profile)
        .join(format!("{stem}.{ext}"));
    Some(candidate)
}

fn staticlib_stem() -> String {
    #[cfg(target_os = "windows")]
    {
        "rts".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "librts".to_string()
    }
}

fn staticlib_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "lib"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "a"
    }
}

// Reference to avoid "unused" warnings on unreachable branches.
#[allow(dead_code)]
fn _path_ref(_: &Path) {}
