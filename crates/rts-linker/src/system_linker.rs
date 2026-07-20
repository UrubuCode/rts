use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::WindowsSubsystem;
use super::toolchain::{
    ResolvedLinker, TargetFlavor, TargetTriple, ToolchainLayout,
    ensure_windows_msvc_runtime_lib_paths, resolve_linker,
};

#[derive(Debug, Clone)]
pub struct LinkedArtifact {
    pub path: PathBuf,
    pub format: String,
    pub linker: String,
}

pub fn link(
    object_paths: &[PathBuf],
    output_path: &Path,
    explicit_target: Option<&str>,
    windows_subsystem: Option<WindowsSubsystem>,
    keep_all_runtime_symbols: bool,
) -> Result<LinkedArtifact> {
    if object_paths.is_empty() {
        bail!("system linker received no object files to link");
    }

    let layout = ToolchainLayout::resolve(explicit_target)?;
    let linker = resolve_linker(&layout)?;
    let final_path = normalize_output_path(output_path, layout.target.flavor);

    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    let args = build_linker_args(
        &layout.target,
        object_paths,
        &final_path,
        &linker,
        windows_subsystem,
        keep_all_runtime_symbols,
    )?;
    let (invocation_args, rsp_file) = prepare_invocation_args(&linker, &args)?;
    let output = Command::new(&linker.path)
        .args(&invocation_args)
        .output()
        .with_context(|| {
            format!(
                "failed to invoke system linker '{}' for target {}",
                linker.path.display(),
                layout.target.triple
            )
        })?;
    if let Some(path) = rsp_file {
        let _ = std::fs::remove_file(path);
    }

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "system linker '{}' failed for target {} (status={:?}, stdout='{}', stderr='{}')",
            linker.path.display(),
            layout.target.triple,
            output.status.code(),
            stdout,
            stderr
        );
    }

    Ok(LinkedArtifact {
        path: final_path,
        format: layout.target.flavor.format_name().to_string(),
        linker: linker.name(),
    })
}

fn prepare_invocation_args(
    linker: &ResolvedLinker,
    args: &[String],
) -> Result<(Vec<String>, Option<PathBuf>)> {
    let total_len = args.iter().map(|arg| arg.len() + 1).sum::<usize>();
    if total_len < 24_000 || args.len() < 200 {
        return Ok((args.to_vec(), None));
    }

    let mut prelude = Vec::<String>::new();
    let mut response_args = args.to_vec();

    if linker.is_rust_lld()
        && response_args.len() >= 2
        && response_args[0] == "-flavor"
        && response_args[1] == "link"
    {
        prelude.push(response_args.remove(0));
        prelude.push(response_args.remove(0));
    }

    let rsp_path = std::env::temp_dir().join(format!("rts_linker_{}.rsp", std::process::id()));
    let body = response_args
        .iter()
        .map(|arg| quote_rsp_arg(arg))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&rsp_path, body).with_context(|| {
        format!(
            "failed to write linker response file {}",
            rsp_path.display()
        )
    })?;

    let mut invocation = prelude;
    invocation.push(format!("@{}", rsp_path.display()));
    Ok((invocation, Some(rsp_path)))
}

fn quote_rsp_arg(arg: &str) -> String {
    if arg.chars().all(|ch| !ch.is_whitespace() && ch != '"') {
        return arg.to_string();
    }

    let escaped = arg.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn build_linker_args(
    target: &TargetTriple,
    object_paths: &[PathBuf],
    output_path: &Path,
    linker: &ResolvedLinker,
    windows_subsystem: Option<WindowsSubsystem>,
    keep_all_runtime_symbols: bool,
) -> Result<Vec<String>> {
    match target.flavor {
        TargetFlavor::Coff => {
            let mut args = Vec::new();
            let requires_runtime = true;

            if linker.is_rust_lld() {
                args.push("-flavor".to_string());
                args.push("link".to_string());
            } else if !linker.is_link_style() {
                bail!(
                    "COFF target requires link-compatible linker, found '{}'",
                    linker.path.display()
                );
            }

            args.push("/nologo".to_string());
            // Bootstrap codegen emits a `main` symbol; keep mainCRTStartup for
            // both subsystem modes so generated programs don't need WinMain.
            let subsystem = windows_subsystem.unwrap_or(WindowsSubsystem::Console);
            args.push("/entry:mainCRTStartup".to_string());
            match subsystem {
                WindowsSubsystem::Console => args.push("/subsystem:console".to_string()),
                WindowsSubsystem::Windows => args.push("/subsystem:windows".to_string()),
            }
            // DELAY-LOAD the GPU stack. The runtime links egui/wgpu, so every
            // AOT binary statically imports opengl32.dll — and Windows loads
            // every static import BEFORE `main`. opengl32 alone is ~64 ms of
            // process startup (measured), paid by a compiled program that never
            // opens a window (math/CLI), which is most of them. Delay-loading
            // defers it to the first real GL call — exactly when a GUI program
            // engages the glow fallback — so a non-GUI binary never pays it.
            // Scoped to opengl32 only: a wider list destabilized a binary in
            // earlier testing, and opengl32 carries essentially all of the cost.
            // `delayimp.lib` (the delay-load thunk helper) resolves via the MSVC
            // lib paths added below.
            args.push("/DELAYLOAD:opengl32.dll".to_string());
            args.push("delayimp.lib".to_string());
            if !keep_all_runtime_symbols {
                // Dead code / COMDAT elimination — strips unused namespace functions.
                args.push("/OPT:REF".to_string());
                args.push("/OPT:ICF".to_string());
            } else {
                // `--all-namespaces` força `/WHOLEARCHIVE` no runtime, que arrasta os
                // import-stubs do kernel32 já contidos no archive — colidindo com o
                // `/defaultlib:kernel32.lib` do SDK (duplicate symbol DuplicateHandle/
                // FreeLibrary/…). `/FORCE:MULTIPLE` deixa o linker usar a 1ª definição
                // (idêntica) em vez de abortar. Só vale neste modo (sem ele, OPT:REF
                // já evita o problema removendo o não-usado).
                args.push("/FORCE:MULTIPLE".to_string());
            }
            args.push(format!("/out:{}", output_path.display()));
            for object_path in object_paths {
                if keep_all_runtime_symbols
                    && object_path.extension().and_then(|e| e.to_str()) == Some("a")
                {
                    args.push(format!("/WHOLEARCHIVE:{}", object_path.display()));
                } else {
                    args.push(object_path.display().to_string());
                }
            }

            if requires_runtime {
                for path in windows_runtime_lib_paths(&target.triple) {
                    args.push(format!("/libpath:{}", path.display()));
                }
                for lib in windows_runtime_default_libs() {
                    args.push(format!("/defaultlib:{lib}"));
                }
            }

            Ok(args)
        }
        TargetFlavor::Elf => {
            let mut args = Vec::new();
            if linker.is_rust_lld() {
                args.push("-flavor".to_string());
                args.push("gnu".to_string());
            }
            args.push("-o".to_string());
            args.push(output_path.display().to_string());

            // Raw linkers (ld.lld, rust-lld) don't add CRT startup code automatically.
            // Probe system lib paths and prepend crt1.o + crti.o before user objects.
            let syslib_paths = if linker.is_raw_linker() {
                elf_sysroot_lib_paths(&target.triple)
            } else {
                Vec::new()
            };
            // Raw linkers also do NOT set a default PT_INTERP (GNU ld does; lld
            // does not): a dynamically-linked exe without it never runs the
            // dynamic loader — libc symbols stay unrelocated and the binary
            // segfaults on the first call. Probe the platform loader path.
            if linker.is_raw_linker() {
                if let Some(interp) = elf_dynamic_linker(&target.triple) {
                    args.push("-dynamic-linker".to_string());
                    args.push(interp.display().to_string());
                }
            }
            for crt in ["crt1.o", "crti.o"] {
                if let Some(p) = find_crt_object(&syslib_paths, crt) {
                    args.push(p.display().to_string());
                }
            }

            // Abre group para resolver dependencias circulares entre o
            // runtime_support.a e as libs do sistema passadas depois. Sem
            // --start-group, refs em archive sao resolvidos so' no primeiro
            // pass — libs depois ficam unresolved.
            if linker.is_compiler_driver() {
                args.push("-Wl,--start-group".to_string());
            } else {
                args.push("--start-group".to_string());
            }

            for object_path in object_paths {
                let is_archive =
                    object_path.extension().and_then(|e| e.to_str()) == Some("a");
                if keep_all_runtime_symbols && is_archive {
                    if linker.is_compiler_driver() {
                        args.push("-Wl,--whole-archive".to_string());
                        args.push(object_path.display().to_string());
                        args.push("-Wl,--no-whole-archive".to_string());
                    } else {
                        args.push("--whole-archive".to_string());
                        args.push(object_path.display().to_string());
                        args.push("--no-whole-archive".to_string());
                    }
                } else {
                    args.push(object_path.display().to_string());
                }
            }

            // Library search paths and system libs for raw linkers.
            for path in &syslib_paths {
                args.push(format!("-L{}", path.display()));
            }
            // Libs basicas: -lm necessaria SEMPRE (runtime_support.a
            // chama exp/log/sqrt/etc); -lc/-lpthread/-ldl so' raw linker
            // (compiler driver adiciona automaticamente).
            args.push("-lm".to_string());
            args.push("-lpthread".to_string());
            args.push("-ldl".to_string());
            if !syslib_paths.is_empty() {
                args.push("-lc".to_string());
                // libgcc_s provides stack unwinding; only link if present on this
                // system. `-lgcc_s` only resolves `libgcc_s.so`/`.a` — the bare
                // runtime `libgcc_s.so.1` (present WITHOUT the gcc dev package,
                // which owns the `.so` symlink) does NOT satisfy it and ld.lld
                // errors "unable to find library -lgcc_s". Pass the versioned
                // file by full path in that case.
                if syslib_paths.iter().any(|p| p.join("libgcc_s.so").is_file()) {
                    args.push("-lgcc_s".to_string());
                } else if let Some(so1) = syslib_paths
                    .iter()
                    .map(|p| p.join("libgcc_s.so.1"))
                    .find(|p| p.is_file())
                {
                    args.push(so1.display().to_string());
                }
            }
            // Fecha --start-group apos todas as libs.
            if linker.is_compiler_driver() {
                args.push("-Wl,--end-group".to_string());
            } else {
                args.push("--end-group".to_string());
            }

            if !keep_all_runtime_symbols {
                // Compiler drivers (cc/clang) require -Wl, prefix for raw linker flags.
                if linker.is_compiler_driver() {
                    args.push("-Wl,--gc-sections".to_string());
                } else {
                    args.push("--gc-sections".to_string());
                }
            }

            // crtn.o must follow all objects and libraries.
            if let Some(p) = find_crt_object(&syslib_paths, "crtn.o") {
                args.push(p.display().to_string());
            }

            Ok(args)
        }
        TargetFlavor::MachO => {
            let mut args = Vec::new();
            if linker.is_rust_lld() {
                args.push("-flavor".to_string());
                args.push("darwin".to_string());
                args.push("-arch".to_string());
                args.push(macho_arch_for_target(&target.triple).to_string());
            }
            args.push("-o".to_string());
            args.push(output_path.display().to_string());
            for object_path in object_paths {
                let is_archive =
                    object_path.extension().and_then(|e| e.to_str()) == Some("a");
                if keep_all_runtime_symbols && is_archive {
                    args.push("-force_load".to_string());
                    args.push(object_path.display().to_string());
                } else {
                    args.push(object_path.display().to_string());
                }
            }
            if !keep_all_runtime_symbols {
                args.push("-dead_strip".to_string());
            }
            // Compiler drivers deduce platform version from MACOSX_DEPLOYMENT_TARGET.
            if !linker.is_compiler_driver() {
                let (min_ver, sdk_ver) = macos_platform_versions(&target.triple);
                args.push("-platform_version".to_string());
                args.push("macos".to_string());
                args.push(min_ver);
                args.push(sdk_ver);
            }
            // Raw linkers need explicit SDK lib path, framework search paths, and -lSystem;
            // compiler drivers add these automatically via their built-in sysroot knowledge.
            if linker.is_raw_linker() {
                if let Some(sdk_lib) = macos_sdk_lib_path() {
                    args.push(format!("-L{}", sdk_lib.display()));
                } else {
                    args.push("-L/usr/lib".to_string());
                }
                args.push("-lSystem".to_string());
                // Framework search paths: SDK first (authoritative), then system fallback.
                if let Some(sdk_fw) = macos_sdk_frameworks_path() {
                    args.push(format!("-F{}", sdk_fw.display()));
                }
                args.push("-F/System/Library/Frameworks".to_string());
            }
            // runtime_support.a references these macOS frameworks (cpal ->
            // coreaudio for audio; winit/wgpu/egui for windowing + Metal). Raw
            // linkers and compiler drivers both need them explicit, plus libobjc
            // for the objc2 runtime calls winit makes.
            args.push("-lobjc".to_string());
            for framework in macos_frameworks() {
                args.push("-framework".to_string());
                args.push((*framework).to_string());
            }
            Ok(args)
        }
    }
}

/// The platform's dynamic-loader path (`PT_INTERP`) for a raw ELF link, probed
/// on disk: glibc first (the overwhelmingly common case), then musl. `None`
/// (no loader found) leaves the args unchanged — a fully-static system would
/// not have one, and adding a bogus path is worse than the linker's default.
fn elf_dynamic_linker(triple: &str) -> Option<PathBuf> {
    let lower = triple.to_ascii_lowercase();
    let candidates: &[&str] = if lower.starts_with("x86_64-") {
        &[
            "/lib64/ld-linux-x86-64.so.2",
            "/lib/ld-linux-x86-64.so.2",
            "/lib/ld-musl-x86_64.so.1",
        ]
    } else if lower.starts_with("aarch64-") {
        &["/lib/ld-linux-aarch64.so.1", "/lib/ld-musl-aarch64.so.1"]
    } else {
        &[]
    };
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

/// Probe standard library search paths on the current Linux system.
/// Covers Debian/Ubuntu (multiarch), Fedora/RHEL (/usr/lib64), and generic layouts.
fn elf_sysroot_lib_paths(triple: &str) -> Vec<PathBuf> {
    let mut paths = Vec::<PathBuf>::new();
    let multiarch = elf_multiarch_triplet(triple);

    if !multiarch.is_empty() {
        for base in ["/usr/lib", "/lib"] {
            let p = PathBuf::from(format!("{base}/{multiarch}"));
            if p.is_dir() {
                paths.push(p);
            }
        }
    }

    for candidate in ["/usr/lib64", "/lib64", "/usr/lib", "/lib"] {
        let p = PathBuf::from(candidate);
        if p.is_dir() && !paths.contains(&p) {
            paths.push(p);
        }
    }

    // GCC lib dir provides libgcc_s.so and extra CRT objects on some distros.
    if !multiarch.is_empty() {
        if let Ok(entries) = std::fs::read_dir(format!("/usr/lib/gcc/{multiarch}")) {
            let mut versions: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            if let Some(latest) = versions.last() {
                if !paths.contains(latest) {
                    paths.push(latest.clone());
                }
            }
        }
    }

    paths
}

fn elf_multiarch_triplet(triple: &str) -> &'static str {
    let lower = triple.to_ascii_lowercase();
    if lower.starts_with("x86_64-") {
        "x86_64-linux-gnu"
    } else if lower.starts_with("aarch64-") {
        "aarch64-linux-gnu"
    } else if lower.starts_with("i686-") || lower.starts_with("i386-") {
        "i386-linux-gnu"
    } else if lower.starts_with("armv7") {
        "arm-linux-gnueabihf"
    } else {
        ""
    }
}

fn find_crt_object(lib_paths: &[PathBuf], name: &str) -> Option<PathBuf> {
    lib_paths.iter().find_map(|dir| {
        let p = dir.join(name);
        p.is_file().then_some(p)
    })
}

fn macos_sdk_path() -> Option<PathBuf> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sdk = String::from_utf8(output.stdout).ok()?;
    let p = PathBuf::from(sdk.trim().to_string());
    p.is_dir().then_some(p)
}

/// Returns `<SDK>/usr/lib` for raw linker `-L` search.
fn macos_sdk_lib_path() -> Option<PathBuf> {
    let lib = macos_sdk_path()?.join("usr").join("lib");
    lib.is_dir().then_some(lib)
}

/// Returns `<SDK>/System/Library/Frameworks` for raw linker `-F` search.
fn macos_sdk_frameworks_path() -> Option<PathBuf> {
    let fw = macos_sdk_path()?
        .join("System")
        .join("Library")
        .join("Frameworks");
    fw.is_dir().then_some(fw)
}

fn macos_frameworks() -> &'static [&'static str] {
    &[
        // cpal -> coreaudio-rs (audio backend).
        "CoreFoundation",
        "CoreAudio",
        "AudioUnit",
        "AudioToolbox",
        // winit (windowing): AppKit windows/events, CoreGraphics display APIs
        // (CGMainDisplayID/CGDisplayBounds/…), Carbon TIS keyboard-layout calls,
        // CoreVideo CVDisplayLink.
        "AppKit",
        "Foundation",
        "CoreGraphics",
        "Carbon",
        "CoreVideo",
        // wgpu (render backend): Metal + CAMetalLayer (QuartzCore) + IOSurface.
        "Metal",
        "QuartzCore",
        "IOSurface",
        "IOKit",
        // glutin/glow GL fallback (egui glow backend): CGL context creation
        // (_CGLErrorString/_CGLSetParameter/_CGLChoosePixelFormat/…).
        "OpenGL",
    ]
}

fn macho_arch_for_target(triple: &str) -> &'static str {
    if triple.starts_with("aarch64-") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn macos_platform_versions(triple: &str) -> (String, String) {
    let min = std::env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| {
        if triple.starts_with("aarch64-") {
            "11.0".to_string()
        } else {
            "10.13".to_string()
        }
    });
    let sdk = Command::new("xcrun")
        .args(["--show-sdk-version"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| min.clone());
    (min, sdk)
}

fn windows_runtime_default_libs() -> &'static [&'static str] {
    &[
        // Import libs exigidas pelo Rust std + runtime (net=ws2_32,
        // crypto/getrandom=bcrypt, dirs=shell32/ole32,
        // node:os networkInterfaces=iphlpapi/GetAdaptersAddresses).
        "kernel32.lib",
        "userenv.lib",
        "advapi32.lib",
        "bcrypt.lib",
        "ws2_32.lib",
        "iphlpapi.lib",
        "ntdll.lib",
        "shell32.lib",
        "ole32.lib",
        "synchronization.lib",
        // GUI/GPU stack (rts-egui → winit + wgpu + glutin/glow). winit needs
        // window/input/DPI (user32/gdi32/dwmapi/imm32/shcore/uxtheme); wgpu's dx12
        // backend needs dxgi/d3d12; the glow-backend's WGL context (glutin) needs
        // opengl32 (wglCreateContext/wglMakeCurrent/wglGetProcAddress/…); oleaut
        // for COM. The AOT link is manual (Cranelift .o + staticlib), so the
        // crates' `#[link]` directives don't propagate — these must be passed
        // explicitly.
        "user32.lib",
        "gdi32.lib",
        "opengl32.lib",
        "dwmapi.lib",
        "imm32.lib",
        "shcore.lib",
        "uxtheme.lib",
        "oleaut32.lib",
        "dxgi.lib",
        "d3d12.lib",
        "propsys.lib",
        "winmm.lib",
        "version.lib",
        // Rust staticlib on MSVC uses the dynamic CRT by default; keep only
        // the matching dynamic import libraries to avoid duplicate symbols
        // like `__report_gsfailure` that appear in both static and dynamic
        // variants.
        "ucrt.lib",
        "vcruntime.lib",
        "msvcrt.lib",
        "legacy_stdio_definitions.lib",
    ]
}

fn windows_runtime_lib_paths(target_triple: &str) -> Vec<PathBuf> {
    let mut paths = Vec::<PathBuf>::new();
    if let Ok(raw) = std::env::var("LIB") {
        for part in raw
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let candidate = PathBuf::from(part);
            if candidate.is_dir() {
                paths.push(candidate);
            }
        }
    }

    let arch = windows_arch_for_target(target_triple);

    let sdk_root = std::env::var("WindowsSdkDir")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10"));
    let lib_root = sdk_root.join("Lib");
    if lib_root.is_dir() {
        let mut versions = std::fs::read_dir(&lib_root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        versions.sort();
        if let Some(version) = versions.pop() {
            let um = version.join("um").join(arch);
            if um.is_dir() {
                paths.push(um);
            }

            let ucrt = version.join("ucrt").join(arch);
            if ucrt.is_dir() {
                paths.push(ucrt);
            }
        }
    }

    // MSVC toolchain libs (libcmt.lib, msvcrt.lib, vcruntime.lib) live under
    // the MSVC install, separate from the Windows SDK. Auto-discover the
    // latest installed VC Tools directory so `rts compile` works without
    // requiring the Developer Command Prompt environment.
    for msvc_path in msvc_tool_lib_paths(arch) {
        paths.push(msvc_path);
    }

    if !windows_runtime_libs_available(&paths) {
        match ensure_windows_msvc_runtime_lib_paths(target_triple) {
            Ok(downloaded) => paths.extend(downloaded),
            Err(error) => eprintln!(
                "RTS toolchain: automatic Windows SDK/CRT provisioning failed ({:#}). Continuing with local linker search paths.",
                error
            ),
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

/// Walks common Visual Studio install roots looking for the MSVC `lib`
/// directory corresponding to the current architecture.
fn msvc_tool_lib_paths(arch: &str) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(env) {
            roots.push(PathBuf::from(base).join("Microsoft Visual Studio"));
        }
    }
    roots.push(PathBuf::from(r"C:\Program Files\Microsoft Visual Studio"));
    roots.push(PathBuf::from(
        r"C:\Program Files (x86)\Microsoft Visual Studio",
    ));

    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        // Layout: <root>/<year>/<edition>/VC/Tools/MSVC/<version>/lib/<arch>
        let Ok(years) = std::fs::read_dir(&root) else {
            continue;
        };
        for year in years.flatten() {
            let Ok(editions) = std::fs::read_dir(year.path()) else {
                continue;
            };
            for edition in editions.flatten() {
                let msvc = edition.path().join("VC").join("Tools").join("MSVC");
                if !msvc.is_dir() {
                    continue;
                }
                let Ok(versions) = std::fs::read_dir(&msvc) else {
                    continue;
                };
                let mut version_dirs: Vec<PathBuf> = versions
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|p| p.is_dir())
                    .collect();
                version_dirs.sort();
                if let Some(latest) = version_dirs.last() {
                    let lib = latest.join("lib").join(arch);
                    if lib.is_dir() {
                        out.push(lib);
                    }
                }
            }
        }
    }
    out
}

fn windows_runtime_libs_available(paths: &[PathBuf]) -> bool {
    let has_um = paths.iter().any(|path| path.join("kernel32.lib").is_file());
    let has_ucrt = paths.iter().any(|path| path.join("ucrt.lib").is_file());
    let has_crt = paths
        .iter()
        .any(|path| path.join("vcruntime.lib").is_file() || path.join("msvcrt.lib").is_file());
    has_um && has_ucrt && has_crt
}

fn windows_arch_for_target(target_triple: &str) -> &'static str {
    let lower = target_triple.to_ascii_lowercase();
    if lower.starts_with("x86_64-") {
        "x64"
    } else if lower.starts_with("i686-") || lower.starts_with("x86-") {
        "x86"
    } else if lower.starts_with("aarch64-") {
        "arm64"
    } else {
        "x64"
    }
}

fn normalize_output_path(path: &Path, flavor: TargetFlavor) -> PathBuf {
    if matches!(flavor, TargetFlavor::Coff) && path.extension().is_none() {
        return path.with_extension("exe");
    }
    path.to_path_buf()
}
