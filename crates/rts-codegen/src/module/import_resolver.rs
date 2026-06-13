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
                return classify_resolved(path, ModuleKind::Source, specifier, import_span, options);
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
            let target =
                resolve_dependency_target(specifier, dependency, owner_manifest, module_cache)
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
                    })?;
            // Reclassifica caso a dependência resolva para um `.node` (aplica o
            // gate --allow-native-addons de forma uniforme).
            return classify_resolved(target.path, target.kind, specifier, import_span, options);
        }
    }

    if let Some(path) = resolve_workspace_package_import(workspace_root, specifier, manifest_cache)?
    {
        return Ok(ImportTarget {
            path,
            kind: ModuleKind::WorkspacePackage,
        });
    }

    // node_modules/<specifier> installed by rts i / npm / bun / yarn
    if let Some(path) = resolve_node_modules_import(workspace_root, specifier, manifest_cache)? {
        return classify_resolved(
            path,
            ModuleKind::CachedDependency,
            specifier,
            import_span,
            options,
        );
    }

    // Embedded TypeScript builtins served under the "rts:<name>" scheme but
    // NOT backed by a SPECS entry (they are full TS source modules).
    if specifier == "rts:test" {
        let path = module_cache
            .write_builtin_ts("test", crate::namespaces::test::BUNDLE_TS)
            .with_context(|| "failed to cache rts:test bundle")?;
        return Ok(ImportTarget {
            path,
            kind: ModuleKind::Source,
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
            // `.node` é deixado passar como path resolvido; o gate
            // `--allow-native-addons` é aplicado uma única vez no caller
            // (`resolve_import_target`/`resolve_entry_path`), que conhece as
            // `CompileOptions`. Aqui só validamos extensões de source.
            validate_source_extension(&path, true)?;
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
        // O entry point é sempre TS/JS — um `.node` como entry não faz sentido
        // (não tem `main`). `allow_native = false` rejeita com erro claro.
        validate_source_extension(input, false)?;
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

/// Valida a extensão de um módulo. `.node` (addon nativo N-API) é aceito apenas
/// quando `allow_native` — ver `is_native_addon` e a flag `allow_native_addons`.
pub(crate) fn validate_source_extension(path: &Path, allow_native: bool) -> Result<()> {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        bail!(
            "source file must have .rts, .ts or .js extension: {}",
            path.display()
        );
    };

    if ext == "node" {
        if allow_native {
            return Ok(());
        }
        bail!(
            "native addon '{}' requires --allow-native-addons (N-API addons run \
             native code outside the sandbox; only pure N-API is supported, not \
             V8-direct/NAN)",
            path.display()
        );
    }

    if ext != "rts" && ext != "ts" && ext != "js" {
        bail!(
            "unsupported source extension '.{}' in {} (expected .rts, .ts or .js)",
            ext,
            path.display()
        );
    }

    Ok(())
}

/// `true` se o path tem extensão `.node` (addon nativo).
pub(crate) fn is_native_addon(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("node")
}

/// Decide o `ModuleKind` de um path já resolvido e aplica o gate
/// `--allow-native-addons`. Um `.node` vira `ModuleKind::NativeAddon` (e exige a
/// flag); qualquer outra extensão mantém o `default_kind` que o resolvedor
/// determinou. Ponto único onde a política de addon nativo é aplicada nos
/// caminhos de import (relativo, node_modules, dependência de manifest).
fn classify_resolved(
    path: PathBuf,
    default_kind: ModuleKind,
    specifier: &str,
    import_span: Span,
    options: CompileOptions,
) -> Result<ImportTarget> {
    if is_native_addon(&path) {
        if !options.allow_native_addons {
            reporter::emit(
                RichDiagnostic::error(
                    "E005",
                    format!("addon nativo '.node' requer --allow-native-addons: '{specifier}'"),
                )
                .with_span(import_span)
                .with_note(
                    "addons N-API rodam código nativo fora do sandbox; só N-API \
                     puro é suportado (não V8-direto/NAN)",
                )
                .with_suggestion("rode com: rts run --allow-native-addons <entry>"),
            );
            bail!(
                "native addon '{}' requires --allow-native-addons",
                path.display()
            );
        }
        return Ok(ImportTarget {
            path,
            kind: ModuleKind::NativeAddon,
        });
    }
    Ok(ImportTarget {
        path,
        kind: default_kind,
    })
}

pub(crate) fn is_remote_url(specifier: &str) -> bool {
    specifier.starts_with("http://") || specifier.starts_with("https://")
}

/// Resolve `specifier` against `<workspace_root>/node_modules/`.
/// Handles plain packages (`axios`), scoped packages (`@org/pkg`), and
/// sub-path imports (`pkg/subpath`).
fn resolve_node_modules_import(
    workspace_root: &Path,
    specifier: &str,
    manifest_cache: &mut ManifestCache,
) -> Result<Option<PathBuf>> {
    let node_modules = workspace_root.join("node_modules");
    if !node_modules.exists() {
        return Ok(None);
    }

    // Split specifier into package root + optional subpath
    // e.g. "axios/lib/core" → ("axios", Some("lib/core"))
    //      "@org/pkg/sub"   → ("@org/pkg", Some("sub"))
    let (pkg_name, subpath) = split_pkg_specifier(specifier);
    let pkg_dir = node_modules.join(pkg_name);

    if !pkg_dir.exists() {
        return Ok(None);
    }

    if let Some(sub) = subpath {
        let candidate = pkg_dir.join(sub);
        match resolve_source_candidate(&candidate) {
            Ok(path) => return Ok(Some(path)),
            Err(_) => return Ok(None),
        }
    }

    match resolve_package_entry(&pkg_dir, manifest_cache) {
        Ok(path) => Ok(Some(path)),
        Err(_) => Ok(None),
    }
}

fn split_pkg_specifier(specifier: &str) -> (&str, Option<&str>) {
    if specifier.starts_with('@') {
        // Scoped: @org/pkg or @org/pkg/subpath
        if let Some(rest) = specifier.strip_prefix('@') {
            if let Some(slash) = rest.find('/') {
                let after_scope_pkg = &rest[slash + 1..];
                if let Some(sub_slash) = after_scope_pkg.find('/') {
                    let pkg_end = 1 + slash + 1 + sub_slash; // offset in original
                    return (&specifier[..pkg_end], Some(&specifier[pkg_end + 1..]));
                }
            }
        }
        return (specifier, None);
    }

    if let Some(slash) = specifier.find('/') {
        (&specifier[..slash], Some(&specifier[slash + 1..]))
    } else {
        (specifier, None)
    }
}
