use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let entry = manifest
        .join("src")
        .join("namespaces")
        .join("rt_all.rs");

    // Output name: rustc uses the crate name for staticlib output when -o is explicit.
    // We request a plain `.a`; on Windows rust-lld also accepts COFF `.a` archives.
    let output = out.join("runtime_support.a");

    let deps_dir = deps_dir_from_out_dir(&out).unwrap_or_else(|| {
        panic!(
            "failed to discover Cargo deps dir from OUT_DIR: {}",
            out.display()
        )
    });
    let fltk_rlib = find_fltk_rlib(&deps_dir).unwrap_or_else(|| {
        panic!(
            "failed to locate fltk rlib under {} (required for ui runtime symbols)",
            deps_dir.display()
        )
    });

    let mut cmd = Command::new(&rustc);
    cmd.args([
            "--edition",
            "2024",
            "--crate-type",
            "staticlib",
            "--crate-name",
            "rts_rt",
            "-C",
            "opt-level=3",
            "-C",
            "panic=abort",
            "-o",
            output.to_str().unwrap(),
            entry.to_str().unwrap(),
        ]);
    cmd.arg("-L")
        .arg(format!("dependency={}", deps_dir.display()));
    cmd.arg("--extern")
        .arg(format!("fltk={}", fltk_rlib.display()));

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke rustc for runtime_support: {e}"));

    assert!(
        status.success(),
        "rustc failed to compile runtime_support (exit: {status})"
    );

    println!("cargo:rerun-if-changed=src/namespaces/gc/");
    println!("cargo:rerun-if-changed=src/namespaces/io/");
    println!("cargo:rerun-if-changed=src/namespaces/fs/");
    println!("cargo:rerun-if-changed=src/namespaces/math/");
    println!("cargo:rerun-if-changed=src/namespaces/bigfloat/");
    println!("cargo:rerun-if-changed=src/namespaces/time/");
    println!("cargo:rerun-if-changed=src/namespaces/env/");
    println!("cargo:rerun-if-changed=src/namespaces/path/");
    println!("cargo:rerun-if-changed=src/namespaces/buffer/");
    println!("cargo:rerun-if-changed=src/namespaces/string/");
    println!("cargo:rerun-if-changed=src/namespaces/process/");
    println!("cargo:rerun-if-changed=src/namespaces/os/");
    println!("cargo:rerun-if-changed=src/namespaces/collections/");
    println!("cargo:rerun-if-changed=src/namespaces/hash/");
    println!("cargo:rerun-if-changed=src/namespaces/fmt/");
    println!("cargo:rerun-if-changed=src/namespaces/crypto/");
    println!("cargo:rerun-if-changed=src/namespaces/ui/");
    println!("cargo:rerun-if-changed=src/namespaces/rt_all.rs");
    println!("cargo:rerun-if-changed=build.rs");
}

fn deps_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    for ancestor in out_dir.ancestors() {
        let file_name = ancestor.file_name()?.to_string_lossy();
        if file_name.eq_ignore_ascii_case("build") {
            let profile_dir = ancestor.parent()?;
            let deps_dir = profile_dir.join("deps");
            if deps_dir.is_dir() {
                return Some(deps_dir);
            }
        }
    }
    None
}

fn find_fltk_rlib(deps_dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let entries = std::fs::read_dir(deps_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rlib") {
            continue;
        }
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if !file_name.starts_with("libfltk-") {
            continue;
        }

        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &best {
            Some((best_time, _)) if *best_time >= modified => {}
            _ => best = Some((modified, path)),
        }
    }
    best.map(|(_, path)| path)
}
