use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let entry = manifest
        .join("crates")
        .join("rts-runtime")
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
    // anchor's reference set. Windows has the most acute risk (cache restoration
    // can leave stale rlibs from previous lock states), so every dep we --extern
    // explicitly goes through the anchor-aware lookup.
    let anchor_rlib = find_rlib_named(&deps_dir, "libactix_web-").unwrap_or_else(|| {
        panic!(
            "failed to locate actix_web rlib under {} (required as anchor for dep resolution)\n{}",
            deps_dir.display(),
            dump_deps_listing(&deps_dir)
        )
    });
    let mut anchor_hashes = extract_referenced_hashes(&anchor_rlib);
    // Add actix_web's own hash so anchor matches actix_web rlib too if needed.
    if let Some(h) = rlib_hash(&anchor_rlib, "libactix_web-") {
        anchor_hashes.insert(h.to_string());
    }
    // PRIMARY anchor: the archive now compiles the canonical `rts-runtime`
    // namespace tree, so every dependency MUST resolve to the exact variant
    // (version + feature set) that `rts-runtime` itself was built against —
    // otherwise we get skew like ureq 2 vs 3, `time` without `local-offset`,
    // or a serde_core that json5 wasn't compiled for. The rts_runtime rlib
    // references precisely those variants' hashes.
    //
    // `strict_hashes` keeps ONLY rts_runtime's references. The serde family is
    // resolved against it (not the broad `anchor_hashes` union) because release
    // builds carry two `serde_core` variants — a host one pulled by actix's
    // proc-macro chain and the target one rts_runtime/json5 link. The broad
    // union contains both, so a plain lookup can grab the host variant and break
    // json5's `Deserialize` impls (E0277). The strict set contains only the
    // target variant.
    let mut strict_hashes: HashSet<String> = find_rlib_named(&deps_dir, "librts_runtime-")
        .map(|r| extract_referenced_hashes(&r))
        .unwrap_or_default();
    // serde_core is a TRANSITIVE dep (via serde), so its hash is not in
    // rts_runtime's direct-dep references — the strict set would miss it and the
    // serde_core lookup would fall back to the broad union, which on CI Windows
    // (two serde_core variants: host proc-macro + target) picked the host one and
    // broke json5's Deserialize impls (E0277). json5 and serde_json are
    // target-only single-variant crates that DIRECTLY reference the target
    // serde_core, so folding their references pins the correct variant.
    for prefix in ["libjson5-", "libserde_json-"] {
        if let Some(r) = find_rlib_named(&deps_dir, prefix) {
            for h in extract_referenced_hashes(&r) {
                strict_hashes.insert(h);
            }
        }
    }
    for h in &strict_hashes {
        anchor_hashes.insert(h.clone());
    }
    // Tokio is target-only and pulled by actix-web; use it as a secondary
    // anchor source so deps not directly referenced by actix_web (rayon, regex,
    // rustls, webpki_roots, fltk transitives) still get a robust hash set on
    // platforms where target == host (Windows, Linux, macOS native builds).
    if let Some(tokio_anchor) = find_rlib_with_anchor(&deps_dir, "libtokio-", &anchor_hashes) {
        for h in extract_referenced_hashes(&tokio_anchor) {
            anchor_hashes.insert(h);
        }
    }
    let must_find = |prefix: &str, role: &str| -> PathBuf {
        find_rlib_with_anchor(&deps_dir, prefix, &anchor_hashes).unwrap_or_else(|| {
            panic!(
                "failed to locate {prefix}* rlib under {} (required for {role} runtime symbols)\n{}",
                deps_dir.display(),
                dump_deps_listing(&deps_dir)
            )
        })
    };
    // Strict resolver for the serde family: only rts_runtime's exact variants,
    // never the host proc-macro variant. Falls back to the broad lookup if the
    // strict set is empty (e.g. rts_runtime rlib not found yet).
    let must_find_strict = |prefix: &str, role: &str| -> PathBuf {
        if !strict_hashes.is_empty() {
            if let Some(p) = find_rlib_with_anchor(&deps_dir, prefix, &strict_hashes) {
                return p;
            }
        }
        must_find(prefix, role)
    };
    let rts_abi_rlib = must_find("librts_abi-", "abi");
    let fltk_rlib = must_find("libfltk-", "ui");
    let regex_rlib = must_find("libregex-", "regex");
    let rayon_rlib = must_find("librayon-", "parallel");
    let rayon_core_rlib = must_find("librayon_core-", "parallel");
    let rustls_rlib = must_find("librustls-", "tls");
    let webpki_roots_rlib = must_find("libwebpki_roots-", "tls");
    let serde_json_rlib = must_find_strict("libserde_json-", "json");
    let serde_rlib = must_find_strict("libserde-", "json");
    let serde_core_rlib = must_find_strict("libserde_core-", "json");
    let indexmap_rlib = must_find("libindexmap-", "collections");
    let hashbrown_rlib = must_find("libhashbrown-", "collections");
    let equivalent_rlib = must_find("libequivalent-", "collections");
    let actix_web_rlib = anchor_rlib.clone();
    let tokio_rlib = must_find("libtokio-", "http_server");
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
        // (#617) Marker para ops que dependem do compilador (eval_compile,
        // new Function) serem stubadas no archive AOT. JIT nao seta isto.
        "--cfg",
        "rt_all_archive",
        "-o",
        output.to_str().unwrap(),
        entry.to_str().unwrap(),
    ]);
    cmd.arg("-L")
        .arg(format!("dependency={}", deps_dir.display()));
    cmd.arg("--extern")
        .arg(format!("rts_abi={}", rts_abi_rlib.display()));
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

    // Remaining direct dependencies of the canonical `rts-runtime` namespace
    // source. The archive now compiles the full namespace tree (mirroring
    // rts-runtime's lib.rs), so every crate the namespaces `use` must be
    // resolvable here. Anchor-aware lookup with mtime fallback (single-variant
    // target-only crates take the fallback path).
    // json5 links serde_core directly, so resolve it strictly too (same reason
    // as the serde family above).
    let json5_rlib = must_find_strict("libjson5-", "json");
    cmd.arg("--extern")
        .arg(format!("json5={}", json5_rlib.display()));
    let extra_externs: &[(&str, &str, &str)] = &[
        ("anyhow", "libanyhow-", "errors"),
        ("sha2", "libsha2-", "crypto"),
        ("fancy_regex", "libfancy_regex-", "regex"),
        ("unicode_normalization", "libunicode_normalization-", "string"),
        ("actix_rt", "libactix_rt-", "http_server"),
        ("colored", "libcolored-", "fmt"),
        ("notify", "libnotify-", "runtime"),
        ("flate2", "libflate2-", "compress"),
        ("tar", "libtar-", "archive"),
        ("ureq", "libureq-", "fetch"),
        ("slotmap", "libslotmap-", "gc"),
        ("rustc_hash", "librustc_hash-", "collections"),
        ("time", "libtime-", "date"),
    ];
    // All of these are direct deps of rts-runtime, so their exact (version +
    // feature) variant is pinned in `strict_hashes`. Resolve strictly so e.g.
    // `time` lands on the `local-offset`-enabled build (UtcOffset::
    // current_local_offset, E0599 otherwise) and `ureq` on the 2.x rts-runtime
    // links rather than a stray 3.x in the deps dir.
    for (name, prefix, role) in extra_externs {
        let rlib = must_find_strict(prefix, role);
        cmd.arg("--extern").arg(format!("{name}={}", rlib.display()));
    }

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

    // The archive now compiles the canonical namespace source directly from
    // `rts-runtime` (no more stale `src/namespaces` copy). Watch the whole tree
    // plus the shared tokio runtime module the namespaces depend on.
    println!("cargo:rerun-if-changed=crates/rts-runtime/src/namespaces/");
    println!("cargo:rerun-if-changed=crates/rts-runtime/src/runtime/");
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

fn dump_deps_listing(deps_dir: &Path) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("contents of {}:", deps_dir.display()));
    let entries = match std::fs::read_dir(deps_dir) {
        Ok(e) => e,
        Err(err) => {
            lines.push(format!("  <unable to read deps_dir: {err}>"));
            return lines.join("\n");
        }
    };
    let mut rlibs: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rlib") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        rlibs.push(name.to_string());
    }
    rlibs.sort();
    if rlibs.is_empty() {
        lines.push("  <no rlibs found>".to_string());
    } else {
        for name in rlibs {
            lines.push(format!("  {name}"));
        }
    }
    lines.join("\n")
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
