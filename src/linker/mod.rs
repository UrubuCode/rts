pub mod object_linker;
pub mod system_linker;
pub mod toolchain;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct LinkedBinary {
    pub path: PathBuf,
    pub backend: String,
    pub format: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkBackendPreference {
    Auto,
    Object,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSubsystem {
    Console,
    Windows,
}

impl WindowsSubsystem {
    fn from_env() -> Option<Self> {
        let raw = std::env::var("RTS_WINDOWS_SUBSYSTEM")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())?;
        Self::from_raw(&raw)
    }

    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw {
            "console" => Some(Self::Console),
            "windows" | "gui" => Some(Self::Windows),
            _ => None,
        }
    }
}

impl LinkBackendPreference {
    fn from_env() -> Self {
        let raw = std::env::var("RTS_LINKER_BACKEND")
            .unwrap_or_else(|_| "auto".to_string())
            .trim()
            .to_ascii_lowercase();

        Self::from_raw(&raw)
    }

    fn from_raw(raw: &str) -> Self {
        match raw {
            "object" | "manual" => Self::Object,
            "system" | "native" => Self::System,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinkRequest {
    pub backend: Option<LinkBackendPreference>,
    pub target_triple: Option<String>,
    pub windows_subsystem: Option<WindowsSubsystem>,
}

impl LinkRequest {
    pub fn from_env() -> Self {
        let target_triple = std::env::var("RTS_TARGET")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Self {
            backend: Some(LinkBackendPreference::from_env()),
            target_triple,
            windows_subsystem: WindowsSubsystem::from_env(),
        }
    }

    fn backend_or_default(&self) -> LinkBackendPreference {
        self.backend.unwrap_or_else(LinkBackendPreference::from_env)
    }
}

pub fn link_object_to_binary(object_path: &Path, output_path: &Path) -> Result<LinkedBinary> {
    link_objects_to_binary_with_request(
        &[object_path.to_path_buf()],
        output_path,
        &LinkRequest::from_env(),
    )
}

pub fn link_objects_to_binary(
    object_paths: &[PathBuf],
    output_path: &Path,
) -> Result<LinkedBinary> {
    link_objects_to_binary_with_request(object_paths, output_path, &LinkRequest::from_env())
}

pub fn link_object_to_binary_with_request(
    object_path: &Path,
    output_path: &Path,
    request: &LinkRequest,
) -> Result<LinkedBinary> {
    link_objects_to_binary_with_request(&[object_path.to_path_buf()], output_path, request)
}

pub fn link_objects_to_binary_with_request(
    object_paths: &[PathBuf],
    output_path: &Path,
    request: &LinkRequest,
) -> Result<LinkedBinary> {
    if object_paths.is_empty() {
        bail!("linker received no object files");
    }

    match request.backend_or_default() {
        LinkBackendPreference::Object => link_with_object_backend(object_paths, output_path),
        LinkBackendPreference::System => {
            link_with_system_backend(object_paths, output_path, request)
        }
        LinkBackendPreference::Auto => {
            match link_with_system_backend(object_paths, output_path, request) {
                Ok(linked) => Ok(linked),
                Err(system_error) => {
                    if object_paths.len() != 1 {
                        bail!(
                            "system backend unavailable ({}); object fallback requires exactly 1 object file (got {}). install/configure platform runtime libs for the target linker",
                            system_error,
                            object_paths.len()
                        );
                    }
                    eprintln!(
                        "RTS linker: system backend unavailable ({}). Falling back to object backend.",
                        system_error
                    );
                    link_with_object_backend(object_paths, output_path)
                }
            }
        }
    }
}

fn link_with_object_backend(object_paths: &[PathBuf], output_path: &Path) -> Result<LinkedBinary> {
    if object_paths.len() != 1 {
        bail!(
            "object linker backend only supports one object file (received {})",
            object_paths.len()
        );
    }

    let artifact = object_linker::link(&object_paths[0], output_path)?;
    Ok(LinkedBinary {
        path: artifact.path,
        backend: "object".to_string(),
        format: artifact.format,
    })
}

fn link_with_system_backend(
    object_paths: &[PathBuf],
    output_path: &Path,
    request: &LinkRequest,
) -> Result<LinkedBinary> {
    let artifact = system_linker::link(
        object_paths,
        output_path,
        request.target_triple.as_deref(),
        request.windows_subsystem,
    )?;
    Ok(LinkedBinary {
        path: artifact.path,
        backend: format!("system:{}", artifact.linker),
        format: artifact.format,
    })
}

#[cfg(test)]
mod tests {
    use super::{LinkBackendPreference, WindowsSubsystem};

    #[test]
    fn linker_backend_manual_alias_maps_to_object() {
        assert_eq!(
            LinkBackendPreference::from_raw("manual"),
            LinkBackendPreference::Object
        );
    }

    #[test]
    fn linker_backend_native_alias_maps_to_system() {
        assert_eq!(
            LinkBackendPreference::from_raw("native"),
            LinkBackendPreference::System
        );
    }

    #[test]
    fn linker_backend_unknown_maps_to_auto() {
        assert_eq!(
            LinkBackendPreference::from_raw("unknown-value"),
            LinkBackendPreference::Auto
        );
    }

    #[test]
    fn windows_subsystem_parses_aliases() {
        assert_eq!(
            WindowsSubsystem::from_raw("console"),
            Some(WindowsSubsystem::Console)
        );
        assert_eq!(
            WindowsSubsystem::from_raw("windows"),
            Some(WindowsSubsystem::Windows)
        );
        assert_eq!(
            WindowsSubsystem::from_raw("gui"),
            Some(WindowsSubsystem::Windows)
        );
        assert_eq!(WindowsSubsystem::from_raw("invalid"), None);
    }
}
