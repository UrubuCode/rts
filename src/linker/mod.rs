pub mod object_linker;
pub mod system_linker;
pub mod toolchain;

use std::path::{Path, PathBuf};

use anyhow::Result;

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
        }
    }

    fn backend_or_default(&self) -> LinkBackendPreference {
        self.backend.unwrap_or_else(LinkBackendPreference::from_env)
    }
}

pub fn link_object_to_binary(object_path: &Path, output_path: &Path) -> Result<LinkedBinary> {
    link_object_to_binary_with_request(object_path, output_path, &LinkRequest::from_env())
}

pub fn link_object_to_binary_with_request(
    object_path: &Path,
    output_path: &Path,
    request: &LinkRequest,
) -> Result<LinkedBinary> {
    match request.backend_or_default() {
        LinkBackendPreference::Object => link_with_object_backend(object_path, output_path),
        LinkBackendPreference::System => {
            link_with_system_backend(object_path, output_path, request)
        }
        LinkBackendPreference::Auto => {
            match link_with_system_backend(object_path, output_path, request) {
                Ok(linked) => Ok(linked),
                Err(system_error) => {
                    eprintln!(
                        "RTS linker: system backend unavailable ({}). Falling back to object backend.",
                        system_error
                    );
                    link_with_object_backend(object_path, output_path)
                }
            }
        }
    }
}

fn link_with_object_backend(object_path: &Path, output_path: &Path) -> Result<LinkedBinary> {
    let artifact = object_linker::link(object_path, output_path)?;
    Ok(LinkedBinary {
        path: artifact.path,
        backend: "object".to_string(),
        format: artifact.format,
    })
}

fn link_with_system_backend(
    object_path: &Path,
    output_path: &Path,
    request: &LinkRequest,
) -> Result<LinkedBinary> {
    let artifact = system_linker::link(object_path, output_path, request.target_triple.as_deref())?;
    Ok(LinkedBinary {
        path: artifact.path,
        backend: format!("system:{}", artifact.linker),
        format: artifact.format,
    })
}

#[cfg(test)]
mod tests {
    use super::LinkBackendPreference;

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
}
