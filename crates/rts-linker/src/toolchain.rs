//! Toolchain resolution for the system linker backend.
//!
//! Split into cohesive submodules:
//! - [`target`] — target triple / flavor + linker-handle types
//! - [`paths`] — toolchain cache directories + binary path helpers
//! - [`discover`] — system-linker discovery (cache, PATH, rustup/sysroot)
//! - [`download`] — optional linker download (templated URL / Rust dist)
//! - [`windows_sdk`] — Windows SDK / MSVC CRT discovery + xwin provisioning

mod discover;
mod download;
mod paths;
mod target;
mod windows_sdk;

pub use discover::resolve_linker;
pub use paths::toolchains_base_dir;
pub use target::{ResolvedLinker, TargetFlavor, TargetTriple, ToolchainLayout};
pub use windows_sdk::ensure_windows_msvc_runtime_lib_paths;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::TargetFlavor;
    use super::target::flavor_from_triple;
    use super::windows_sdk::{discover_complete_windows_msvc_lib_paths, xwin_arch_for_target};

    #[test]
    fn flavor_detection_works_for_common_triples() {
        assert_eq!(
            flavor_from_triple("x86_64-pc-windows-msvc"),
            TargetFlavor::Coff
        );
        assert_eq!(
            flavor_from_triple("x86_64-unknown-linux-gnu"),
            TargetFlavor::Elf
        );
        assert_eq!(
            flavor_from_triple("aarch64-apple-darwin"),
            TargetFlavor::MachO
        );
    }

    #[test]
    fn xwin_arch_mapping_matches_common_targets() {
        assert_eq!(
            xwin_arch_for_target("x86_64-pc-windows-msvc"),
            xwin::Arch::X86_64
        );
        assert_eq!(
            xwin_arch_for_target("i686-pc-windows-msvc"),
            xwin::Arch::X86
        );
        assert_eq!(
            xwin_arch_for_target("aarch64-pc-windows-msvc"),
            xwin::Arch::Aarch64
        );
    }

    #[test]
    fn discover_windows_lib_paths_from_splat_layout() {
        let root = temp_test_dir("windows_msvc_splat");
        std::fs::create_dir_all(root.join("sdk/lib/um/x64")).expect("create um");
        std::fs::create_dir_all(root.join("sdk/lib/ucrt/x64")).expect("create ucrt");
        std::fs::create_dir_all(root.join("crt/lib/x64")).expect("create crt");

        std::fs::write(root.join("sdk/lib/um/x64/kernel32.lib"), b"").expect("write kernel32");
        std::fs::write(root.join("sdk/lib/ucrt/x64/ucrt.lib"), b"").expect("write ucrt");
        std::fs::write(root.join("crt/lib/x64/vcruntime.lib"), b"").expect("write vcruntime");

        let paths = discover_complete_windows_msvc_lib_paths(&root, "x86_64-pc-windows-msvc")
            .expect("paths should be discovered");
        assert_eq!(paths.len(), 3);

        let _ = std::fs::remove_dir_all(&root);
    }

    fn temp_test_dir(tag: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift");
        std::env::temp_dir().join(format!(
            "rts_toolchain_{tag}_{}_{}",
            std::process::id(),
            now.as_nanos()
        ))
    }
}
