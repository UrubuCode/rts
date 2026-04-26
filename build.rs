use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let entry = manifest.join("src").join("namespaces").join("rt_all.rs");

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
    let regex_rlib = find_rlib_named(&deps_dir, "libregex-").unwrap_or_else(|| {
        panic!(
            "failed to locate regex rlib under {} (required for regex runtime symbols)",
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
        "-C",
        "embed-bitcode=no",
        "-o",
        output.to_str().unwrap(),
        entry.to_str().unwrap(),
    ]);
    cmd.arg("-L")
        .arg(format!("dependency={}", deps_dir.display()));
    cmd.arg("--extern")
        .arg(format!("fltk={}", fltk_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("regex={}", regex_rlib.display()));

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke rustc for runtime_support: {e}"));

    assert!(
        status.success(),
        "rustc failed to compile runtime_support (exit: {status})"
    );

    // Strip LLVM bitcode sections from the archive so platform linkers (Apple ld,
    // lld-link) don't try LTO with bitcode produced by a newer LLVM than they have.
    // embed-bitcode=no above removes bitcode from our own crate objects; this strips
    // it from pre-compiled dependency rlibs (regex, memchr, fltk, …) that were built
    // with bitcode already embedded.
    strip_bitcode_from_archive(&output, &rustc);

    println!("cargo:rerun-if-changed=src/namespaces/gc/");
    println!("cargo:rerun-if-changed=src/namespaces/io/");
    println!("cargo:rerun-if-changed=src/namespaces/fs/");
    println!("cargo:rerun-if-changed=src/namespaces/math/");
    println!("cargo:rerun-if-changed=src/namespaces/num/");
    println!("cargo:rerun-if-changed=src/namespaces/mem/");
    println!("cargo:rerun-if-changed=src/namespaces/backtrace/");
    println!("cargo:rerun-if-changed=src/namespaces/alloc/");
    println!("cargo:rerun-if-changed=src/namespaces/bigfloat/");
    println!("cargo:rerun-if-changed=src/namespaces/time/");
    println!("cargo:rerun-if-changed=src/namespaces/env/");
    println!("cargo:rerun-if-changed=src/namespaces/path/");
    println!("cargo:rerun-if-changed=src/namespaces/buffer/");
    println!("cargo:rerun-if-changed=src/namespaces/string/");
    println!("cargo:rerun-if-changed=src/namespaces/process/");
    println!("cargo:rerun-if-changed=src/namespaces/ptr/");
    println!("cargo:rerun-if-changed=src/namespaces/os/");
    println!("cargo:rerun-if-changed=src/namespaces/collections/");
    println!("cargo:rerun-if-changed=src/namespaces/hash/");
    println!("cargo:rerun-if-changed=src/namespaces/hint/");
    println!("cargo:rerun-if-changed=src/namespaces/fmt/");
    println!("cargo:rerun-if-changed=src/namespaces/crypto/");
    println!("cargo:rerun-if-changed=src/namespaces/regex/");
    println!("cargo:rerun-if-changed=src/namespaces/ui/");
    println!("cargo:rerun-if-changed=src/namespaces/runtime/");
    println!("cargo:rerun-if-changed=src/namespaces/rt_all.rs");
    println!("cargo:rerun-if-changed=build.rs");
}

fn strip_bitcode_from_archive(archive: &Path, rustc: &str) {
    // macOS: xcrun bitcode_strip is always available and handles Mach-O archives natively.
    #[cfg(target_os = "macos")]
    {
        let tmp = archive.with_extension("tmp");
        let ok = Command::new("xcrun")
            .args(["bitcode_strip", "-r"])
            .arg(archive)
            .arg("-o")
            .arg(&tmp)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            let _ = std::fs::rename(&tmp, archive);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
        return;
    }

    // Other platforms: use llvm-objcopy from Rust's llvm-tools-preview component
    // (same LLVM version as the compiler, guaranteed to understand the bitcode format).
    // Falls back to llvm-objcopy found in PATH (e.g. LLVM installation on Windows CI).
    #[allow(unreachable_code)]
    if let Some(objcopy) = find_llvm_objcopy(rustc) {
        let _ = Command::new(objcopy)
            .arg("--strip-section=.llvmbc")
            .arg("--strip-section=.llvmcmd")
            .arg(archive)
            .status();
    }
}

fn find_llvm_objcopy(rustc: &str) -> Option<PathBuf> {
    let binary = if cfg!(windows) {
        "llvm-objcopy.exe"
    } else {
        "llvm-objcopy"
    };

    // Prefer the copy bundled with the Rust toolchain (llvm-tools-preview component).
    if let Some(path) = rustc_sysroot_tool(rustc, binary) {
        return Some(path);
    }

    // Fallback: any llvm-objcopy visible in PATH.
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join(binary))
        .find(|p| p.is_file())
}

fn rustc_sysroot_tool(rustc: &str, binary: &str) -> Option<PathBuf> {
    let sysroot_out = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !sysroot_out.status.success() {
        return None;
    }
    let sysroot = String::from_utf8_lossy(&sysroot_out.stdout)
        .trim()
        .to_string();

    let host_out = Command::new(rustc).arg("-vV").output().ok()?;
    if !host_out.status.success() {
        return None;
    }
    let host = String::from_utf8_lossy(&host_out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(|s| s.trim().to_string()))?;

    let path = PathBuf::from(sysroot)
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join(binary);
    path.is_file().then_some(path)
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
    find_rlib_named(deps_dir, "libfltk-")
}

fn find_rlib_named(deps_dir: &Path, prefix: &str) -> Option<PathBuf> {
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
        if !file_name.starts_with(prefix) {
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
