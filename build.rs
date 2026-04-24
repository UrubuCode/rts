use std::path::PathBuf;
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

    let status = Command::new(&rustc)
        .args([
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
        ])
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
    println!("cargo:rerun-if-changed=src/namespaces/rt_all.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
