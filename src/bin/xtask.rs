//! `cargo run --bin xtask -- build-release`
//!
//! Executa o build em dois passos para resolver o problema de chicken-and-egg
//! do `build.rs`: o primeiro `cargo build --release --lib` produz a staticlib;
//! o segundo `cargo build --release` a embute no binário final.
//!
//! Uso:
//!   cargo run --bin xtask -- build-release          # release
//!   cargo run --bin xtask -- build-release --dev    # debug (para testes)

use std::env;
use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match cmd {
        "build-release" => {
            let dev = args.iter().any(|a| a == "--dev");
            build_release(dev);
        }
        "help" | "--help" | "-h" => {
            eprintln!("Usage: cargo run --bin xtask -- <command>");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  build-release          Build release binary (two-pass)");
            eprintln!("  build-release --dev    Build debug binary (two-pass)");
        }
        other => {
            eprintln!("unknown command: {other}");
            exit(1);
        }
    }
}

fn build_release(dev: bool) {
    let profile_flag: &[&str] = if dev { &[] } else { &["--release"] };

    // Pass 1: build only the lib so the staticlib artifact exists.
    eprintln!("==> pass 1: building lib...");
    run("cargo", &[&["build", "--lib"], profile_flag].concat());

    // Pass 2: build everything; build.rs now finds the staticlib and embeds it.
    eprintln!("==> pass 2: building bin (embeds staticlib)...");
    run("cargo", &[&["build"], profile_flag].concat());

    let binary = if dev {
        "target/debug/rts"
    } else {
        "target/release/rts"
    };
    eprintln!("==> done: {binary}");
    #[cfg(windows)]
    eprintln!("==> done: {binary}.exe");
}

fn run(program: &str, args: &[&str]) {
    let status = Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("failed to run `{program}`: {e}");
            exit(1);
        });
    if !status.success() {
        eprintln!("`{program} {}` failed with {status}", args.join(" "));
        exit(status.code().unwrap_or(1));
    }
}
