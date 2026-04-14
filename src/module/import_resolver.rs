use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::compile_options::CompileOptions;
use crate::diagnostics::reporter::{self, RichDiagnostic};
use crate::parser::span::Span;

use super::manifest::{
    DependencySpec, ManifestCache, PackageManifest, RawPackageManifest, load_package_manifest,
    strip_json_comments,
};
use super::module_cache::ModuleCache;
use super::{ImportTarget, ModuleKind, attach_trace};

pub(crate) fn resolve_import_target(
    current_module: &Path,
    specifier: &str,
    import_span: Span,
    owner_manifest: Option<&PackageManifest>,
    workspace_root: &Path,
    module_cache: &ModuleCache,
    manifest_cache: &mut ManifestCache,
    options: CompileOptions,
    trace_route: &[String],
) -> Result<ImportTarget> {
    if specifier.starts_with('.') {
        let base_dir = current_module.parent().ok_or_else(|| {
            anyhow!(
                "module has no parent directory: {}",
                current_module.display()
            )
        })?;
        match resolve_source_module(base_dir, specifier) {
            Ok(path) => {
                return Ok(ImportTarget {
                    path,
                    kind: ModuleKind::Source,
                });
            }
            Err(err) => {
                reporter::emit(
                    RichDiagnostic::error("E001", format!("modulo nao encontrado: '{specifier}'"))
                        .with_span(import_span)
                        .with_note(format!(
                            "caminho base resolvido a partir de {}",
                            current_module.display()
                        ))
                        .with_suggestion(
                            "verifique o caminho relativo e se o arquivo existe em disco",
                        ),
                );
                return Err(err);
            }
        }
    }

    if is_remote_url(specifier) {
        let path = module_cache
            .fetch_remote_import(None, specifier)
            .with_context(|| {
                attach_trace(
                    format!(
                        "failed to fetch remote import '{}' referenced by {}",
                        specifier,
                        current_module.display()
                    ),
                    trace_route,
                    options,
                )
            })
            .map_err(|err| {
                reporter::emit(
                    RichDiagnostic::error(
                        "E002",
                        format!("falha ao baixar modulo remoto '{specifier}'"),
                    )
                    .with_span(import_span)
                    .with_note(err.to_string()),
                );
                err
            })?;
        return Ok(ImportTarget {
            path,
            kind: ModuleKind::CachedDependency,
        });
    }

    if let Some(owner_manifest) = owner_manifest {
        if let Some(dependency) = owner_manifest.dependencies.get(specifier) {
            return resolve_dependency_target(specifier, dependency, owner_manifest, module_cache)
                .with_context(|| {
                    attach_trace(
                        format!(
                            "failed to resolve dependency '{}' declared in {}@{} ({})",
                            specifier,
                            owner_manifest.name,
                            owner_manifest.version,
                            owner_manifest.manifest_path.display()
                        ),
                        trace_route,
                        options,
                    )
                })
                .map_err(|err| {
                    reporter::emit(
                        RichDiagnostic::error(
                            "E003",
                            format!(
                                "falha ao resolver dependencia '{specifier}' declarada em {}",
                                owner_manifest.name
                            ),
                        )
                        .with_span(import_span)
                        .with_note(err.to_string()),
                    );
                    err
                });
        }
    }

    if let Some(path) = resolve_workspace_package_import(workspace_root, specifier, manifest_cache)?
    {
        return Ok(ImportTarget {
            path,
            kind: ModuleKind::WorkspacePackage,
        });
    }

    // Nao encontramos o modulo em nenhum lugar — tentamos sugestao via
    // distancia de Levenshtein contra os modulos builtin e dependencias
    // declaradas no manifest.
    let suggestion = suggest_similar_module(specifier, owner_manifest);

    let mut diag = RichDiagnostic::error("E004", format!("modulo nao encontrado: '{specifier}'"))
        .with_span(import_span)
        .with_note(
            "use imports relativos (.), modulos builtin (rts, fs, path, ...), \
         dependencias do package.json, pacotes do workspace ou URLs http(s)",
        );

    if let Some(suggestion) = suggestion {
        diag = diag.with_suggestion(format!("voce quis dizer '{suggestion}'?"));
    }

    reporter::emit(diag);

    bail!(
        "unsupported import specifier '{}' in {}. use relative imports, package dependencies, workspace packages, builtin modules, or URLs",
        specifier,
        current_module.display()
    )
}

/// Sugere um modulo similar usando distancia de Levenshtein contra builtins
/// + dependencias declaradas no manifest do owner. Retorna `None` se nenhum
/// candidato estiver dentro do limite (distancia <= 2).
fn suggest_similar_module(
    specifier: &str,
    owner_manifest: Option<&PackageManifest>,
) -> Option<String> {
    let mut candidates: Vec<String> = crate::runtime::builtin_module_keys()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    if let Some(manifest) = owner_manifest {
        for dep in manifest.dependencies.keys() {
            candidates.push(dep.clone());
        }
    }

    candidates
        .into_iter()
        .filter_map(|candidate| {
            let dist = levenshtein(specifier, &candidate);
            if dist <= 2 && dist < specifier.len() {
                Some((dist, candidate))
            } else {
                None
            }
        })
        .min_by_key(|(dist, _)| *dist)
        .map(|(_, candidate)| candidate)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

fn resolve_source_module(base_dir: &Path, specifier: &str) -> Result<PathBuf> {
    let candidate = base_dir.join(specifier);
    resolve_source_candidate(&candidate)
}

pub(crate) fn resolve_source_candidate(candidate: &Path) -> Result<PathBuf> {
    if candidate.is_dir() {
        return resolve_directory_entry(candidate);
    }

    let mut attempts = Vec::new();

    if candidate.extension().is_some() {
        attempts.push(candidate.to_path_buf());
    } else {
        attempts.push(candidate.with_extension("ts"));
        attempts.push(candidate.with_extension("rts"));
        attempts.push(candidate.with_extension("js"));
        attempts.push(candidate.join("index.ts"));
        attempts.push(candidate.join("index.rts"));
        attempts.push(candidate.join("index.js"));
    }

    for path in attempts {
        if path.exists() {
            validate_source_extension(&path)?;
            return path.canonicalize().with_context(|| {
                format!("failed to canonicalize import module {}", path.display())
            });
        }
    }

    bail!("unable to resolve module from {}", candidate.display())
}

fn resolve_directory_entry(directory: &Path) -> Result<PathBuf> {
    let manifest_path = directory.join("package.json");
    if manifest_path.exists() {
        let raw = std::fs::read_to_string(&manifest_path).with_context(|| {
            format!(
                "failed to read package manifest {}",
                manifest_path.display()
            )
        })?;
        let clean = strip_json_comments(&raw);
        if let Ok(parsed) = serde_json::from_str::<RawPackageManifest>(&clean) {
            if let Some(main) = parsed.main {
                let main_path = directory.join(main);
                if main_path.exists() {
                    return resolve_source_candidate(&main_path);
                }
            }
        }
    }

    for candidate in [
        directory.join("main.ts"),
        directory.join("main.rts"),
        directory.join("main.js"),
        directory.join("index.ts"),
        directory.join("index.rts"),
        directory.join("index.js"),
    ] {
        if candidate.exists() {
            return resolve_source_candidate(&candidate);
        }
    }

    bail!("unable to resolve module from {}", directory.display())
}

fn resolve_workspace_package_import(
    workspace_root: &Path,
    specifier: &str,
    manifest_cache: &mut ManifestCache,
) -> Result<Option<PathBuf>> {
    let packages_root = workspace_root.join("packages");
    if !packages_root.exists() {
        return Ok(None);
    }

    let mut parts = specifier.splitn(2, '/');
    let Some(package_name) = parts.next() else {
        return Ok(None);
    };
    let subpath = parts.next();
    let package_dir = packages_root.join(package_name);
    if !package_dir.exists() {
        return Ok(None);
    }

    if let Some(subpath) = subpath {
        let candidate = package_dir.join(subpath);
        return resolve_source_candidate(&candidate).map(Some);
    }

    let entry = resolve_package_entry(&package_dir, manifest_cache)?;
    Ok(Some(entry))
}

fn resolve_package_entry(
    package_dir: &Path,
    manifest_cache: &mut ManifestCache,
) -> Result<PathBuf> {
    let manifest_path = package_dir.join("package.json");
    if manifest_path.exists() {
        let manifest = load_package_manifest(&manifest_path, manifest_cache)?;
        let main_candidate = package_dir.join(&manifest.main);
        if main_candidate.exists() {
            return resolve_source_candidate(&main_candidate);
        }
    }

    let fallback_main = package_dir.join("main.ts");
    if fallback_main.exists() {
        return resolve_source_candidate(&fallback_main);
    }

    let fallback_main_js = package_dir.join("main.js");
    if fallback_main_js.exists() {
        return resolve_source_candidate(&fallback_main_js);
    }

    let fallback_index = package_dir.join("index.ts");
    if fallback_index.exists() {
        return resolve_source_candidate(&fallback_index);
    }

    let fallback_index_js = package_dir.join("index.js");
    if fallback_index_js.exists() {
        return resolve_source_candidate(&fallback_index_js);
    }

    bail!(
        "workspace package '{}' has no valid entry file (expected package.json main, main.ts/main.js or index.ts/index.js)",
        package_dir.display()
    )
}

fn resolve_dependency_target(
    module_name: &str,
    dependency: &DependencySpec,
    owner_manifest: &PackageManifest,
    module_cache: &ModuleCache,
) -> Result<ImportTarget> {
    match dependency {
        DependencySpec::Npm { version } => {
            let path = module_cache.resolve_cached_npm_dependency(module_name, version)?;
            Ok(ImportTarget {
                path,
                kind: ModuleKind::CachedDependency,
            })
        }
        DependencySpec::Url { url } => {
            let path = module_cache.fetch_remote_import(Some(module_name), url)?;
            Ok(ImportTarget {
                path,
                kind: ModuleKind::CachedDependency,
            })
        }
        DependencySpec::LocalPath { path } => {
            let candidate = owner_manifest.package_dir.join(path);
            let resolved = resolve_source_candidate(&candidate)?;
            Ok(ImportTarget {
                path: resolved,
                kind: ModuleKind::Source,
            })
        }
    }
}

pub(crate) fn resolve_entry_path(input: &Path) -> Result<PathBuf> {
    if input.exists() {
        validate_source_extension(input)?;
        return input
            .canonicalize()
            .with_context(|| format!("failed to canonicalize entry path {}", input.display()));
    }

    if input.extension().is_some() {
        bail!("entry module not found: {}", input.display());
    }

    for candidate in [
        input.with_extension("ts"),
        input.with_extension("rts"),
        input.with_extension("js"),
    ] {
        if candidate.exists() {
            return candidate.canonicalize().with_context(|| {
                format!("failed to canonicalize entry path {}", candidate.display())
            });
        }
    }

    bail!(
        "entry module not found. tried: {}, {}, {} and {}",
        input.display(),
        input.with_extension("ts").display(),
        input.with_extension("rts").display(),
        input.with_extension("js").display()
    )
}

pub(crate) fn validate_source_extension(path: &Path) -> Result<()> {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        bail!(
            "source file must have .rts, .ts or .js extension: {}",
            path.display()
        );
    };

    if ext != "rts" && ext != "ts" && ext != "js" {
        bail!(
            "unsupported source extension '.{}' in {} (expected .rts, .ts or .js)",
            ext,
            path.display()
        );
    }

    Ok(())
}

pub(crate) fn is_remote_url(specifier: &str) -> bool {
    specifier.starts_with("http://") || specifier.starts_with("https://")
}
