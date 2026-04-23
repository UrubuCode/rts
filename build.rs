//! Build script that prepares an embedded copy of the RTS static library.
//!
//! The crate emits `rts.lib`/`librts.a` via `crate-type = ["staticlib"]`.
//! This script only copies that artifact into `OUT_DIR` for `include_bytes!`.
//! It does not run nested Cargo builds.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    let embedded = out_dir.join("rts_runtime.staticlib");

    match locate_staticlib().as_ref().filter(|path| path.exists()) {
        Some(path) => {
            fs::copy(path, &embedded)
                .unwrap_or_else(|err| panic!("failed to copy {}: {err}", path.display()));
            println!("cargo:rerun-if-changed={}", path.display());
        }
        None => {
            // Fresh trees do not have the staticlib yet when build.rs runs.
            // Keep include_bytes! working and rely on a second build pass.
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

/// Predicts where Cargo writes the current package static library.
fn locate_staticlib() -> Option<PathBuf> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")?;
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(manifest_dir).join("target"));

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    Some(
        target_dir
            .join(profile)
            .join(format!("{}.{}", staticlib_stem(), staticlib_extension())),
    )
}

fn staticlib_stem() -> &'static str {
    if cfg!(target_os = "windows") {
        "rts"
    } else {
        "librts"
    }
}

fn staticlib_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "lib"
    } else {
        "a"
    }
}
