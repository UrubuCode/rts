use std::collections::HashSet;
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

    // actix-web pulls in a proc-macro chain (actix-web-codegen) that causes some
    // shared crates (serde, serde_core, hashbrown, ...) to be compiled twice:
    // once for the target and once for the host (proc-macro deps). Picking the
    // newest rlib by mtime can land on the host variant, which conflicts with
    // serde_json (target-only) and produces "multiple different versions of
    // crate `serde_core` in the dependency graph".
    //
    // Anchor on a unique target-only crate (actix_web is unique because it's a
    // direct dep used at runtime, not via proc-macro) and read its rmeta hash
    // set. For each duplicated dep, prefer the variant whose hash is in the
    // anchor's reference set.
    let anchor_rlib = find_rlib_named(&deps_dir, "libactix_web-").unwrap_or_else(|| {
        panic!(
            "failed to locate actix_web rlib under {} (required as anchor for dep resolution)",
            deps_dir.display()
        )
    });
    let anchor_hashes = extract_referenced_hashes(&anchor_rlib);
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
    let rayon_rlib = find_rlib_named(&deps_dir, "librayon-").unwrap_or_else(|| {
        panic!(
            "failed to locate rayon rlib under {} (required for parallel runtime symbols)",
            deps_dir.display()
        )
    });
    let rayon_core_rlib = find_rlib_named(&deps_dir, "librayon_core-").unwrap_or_else(|| {
        panic!(
            "failed to locate rayon_core rlib under {} (required for parallel runtime symbols)",
            deps_dir.display()
        )
    });
    let rustls_rlib = find_rlib_named(&deps_dir, "librustls-").unwrap_or_else(|| {
        panic!(
            "failed to locate rustls rlib under {} (required for tls runtime symbols)",
            deps_dir.display()
        )
    });
    let webpki_roots_rlib = find_rlib_with_anchor(&deps_dir, "libwebpki_roots-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate webpki_roots rlib under {} (required for tls runtime symbols)",
                deps_dir.display()
            )
        });
    let serde_json_rlib = find_rlib_with_anchor(&deps_dir, "libserde_json-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate serde_json rlib under {} (required for json runtime symbols)",
                deps_dir.display()
            )
        });
    let serde_rlib = find_rlib_with_anchor(&deps_dir, "libserde-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate serde rlib under {} (required for json runtime symbols)",
                deps_dir.display()
            )
        });
    let serde_core_rlib = find_rlib_with_anchor(&deps_dir, "libserde_core-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate serde_core rlib under {} (required for json runtime symbols)",
                deps_dir.display()
            )
        });
    let indexmap_rlib = find_rlib_with_anchor(&deps_dir, "libindexmap-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate indexmap rlib under {} (required for collections runtime symbols)",
                deps_dir.display()
            )
        });
    let hashbrown_rlib = find_rlib_with_anchor(&deps_dir, "libhashbrown-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate hashbrown rlib under {} (required for collections runtime symbols)",
                deps_dir.display()
            )
        });
    let equivalent_rlib = find_rlib_with_anchor(&deps_dir, "libequivalent-", &anchor_hashes)
        .unwrap_or_else(|| {
            panic!(
                "failed to locate equivalent rlib under {} (required for collections runtime symbols)",
                deps_dir.display()
            )
        });
    let actix_web_rlib = anchor_rlib.clone();
    let tokio_rlib = find_rlib_named(&deps_dir, "libtokio-").unwrap_or_else(|| {
        panic!(
            "failed to locate tokio rlib under {} (required for http_server runtime symbols)",
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
    cmd.arg("--extern")
        .arg(format!("rayon={}", rayon_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("rayon_core={}", rayon_core_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("rustls={}", rustls_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("webpki_roots={}", webpki_roots_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("serde_json={}", serde_json_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("serde={}", serde_rlib.display()));
    // serde_core isn't used directly in our source, but we anchor its --extern
    // so rustc resolves the trait impls (Serialize/Serializer) to the same
    // serde_core variant that serde and serde_json were compiled against.
    cmd.arg("--extern")
        .arg(format!("serde_core={}", serde_core_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("indexmap={}", indexmap_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("hashbrown={}", hashbrown_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("equivalent={}", equivalent_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("actix_web={}", actix_web_rlib.display()));
    cmd.arg("--extern")
        .arg(format!("tokio={}", tokio_rlib.display()));

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke rustc for runtime_support: {e}"));

    assert!(
        status.success(),
        "rustc failed to compile runtime_support (exit: {status})"
    );

    // Strip LLVM bitcode sections from the archive so platform linkers (Apple ld)
    // don't trip on bitcode embedded in pre-compiled dependency rlibs (fltk, regex, …).
    // embed-bitcode=no above removes bitcode from our own objects; this handles the rest.
    strip_bitcode_from_archive(&output);

    println!("cargo:rerun-if-changed=src/namespaces/gc/");
    println!("cargo:rerun-if-changed=src/namespaces/io/");
    println!("cargo:rerun-if-changed=src/namespaces/json/");
    println!("cargo:rerun-if-changed=src/namespaces/date/");
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
    println!("cargo:rerun-if-changed=src/namespaces/atomic/");
    println!("cargo:rerun-if-changed=src/namespaces/sync/");
    println!("cargo:rerun-if-changed=src/namespaces/thread/");
    println!("cargo:rerun-if-changed=src/namespaces/parallel/");
    println!("cargo:rerun-if-changed=src/namespaces/net/");
    println!("cargo:rerun-if-changed=src/namespaces/tls/");
    println!("cargo:rerun-if-changed=src/namespaces/http_server/");
    println!("cargo:rerun-if-changed=src/namespaces/rt_all.rs");
    println!("cargo:rerun-if-changed=src/nodespace/");
    println!("cargo:rerun-if-changed=build.rs");
}

fn strip_bitcode_from_archive(archive: &Path) {
    // macOS: xcrun bitcode_strip handles Mach-O archives natively, no LLVM tools needed.
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

    // Linux: GNU objcopy (binutils) strips ELF sections without any LLVM dependency.
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("objcopy")
            .args(["--remove-section=.llvmbc", "--remove-section=.llvmcmd"])
            .arg(archive)
            .status();
        return;
    }

    // Windows (COFF): lld-link ignores .llvmbc/.llvmcmd in COFF archives — no stripping needed.
    #[allow(unreachable_code)]
    let _ = archive;
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

/// Find an rlib with a hash that appears in `allowed`, falling back to mtime
/// when no candidate matches (single-variant deps still hit this path).
fn find_rlib_with_anchor(
    deps_dir: &Path,
    prefix: &str,
    allowed: &HashSet<String>,
) -> Option<PathBuf> {
    let entries = std::fs::read_dir(deps_dir).ok()?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rlib") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !file_name.starts_with(prefix) {
            continue;
        }
        candidates.push(path);
    }

    // Prefer a candidate whose 16-hex hash is referenced by the anchor crate.
    for path in &candidates {
        if let Some(hash) = rlib_hash(path, prefix) {
            if allowed.contains(hash) {
                return Some(path.clone());
            }
        }
    }

    // Fall back to mtime when there's no anchor match (e.g. single-variant deps).
    find_rlib_named(deps_dir, prefix)
}

fn rlib_hash<'a>(path: &'a Path, prefix: &str) -> Option<&'a str> {
    let file_name = path.file_name()?.to_str()?;
    let stem = file_name.strip_prefix(prefix)?.strip_suffix(".rlib")?;
    if stem.len() == 16 && stem.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(stem)
    } else {
        None
    }
}

/// Read an rlib's bytes and collect every 16-hex-char crate hash it references
/// (its own metadata hash plus its dependencies'). Used to anchor disambiguation
/// when several variants of the same crate exist in the deps dir (target vs
/// proc-macro/host build).
fn extract_referenced_hashes(rlib_path: &Path) -> HashSet<String> {
    let bytes = std::fs::read(rlib_path).unwrap_or_default();
    let mut out = HashSet::new();
    if bytes.len() < 18 {
        return out;
    }
    for window in bytes.windows(17) {
        if window[0] != b'-' {
            continue;
        }
        let tail = &window[1..];
        if !tail.iter().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        if let Ok(s) = std::str::from_utf8(tail) {
            out.insert(s.to_string());
        }
    }
    out
}
