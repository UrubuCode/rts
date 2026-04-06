use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::compile_options::CompileOptions;
use crate::parser;
use crate::parser::ast::{Item, Program};

const MODULES_ENV_VAR: &str = "RTS_MODULES_PATH";

#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub specifier: String,
    pub resolved_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Entry,
    Source,
    WorkspacePackage,
    CachedDependency,
    Builtin,
}

impl ModuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Source => "source",
            Self::WorkspacePackage => "workspace-package",
            Self::CachedDependency => "cached-dependency",
            Self::Builtin => "builtin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceModule {
    pub key: String,
    pub path: PathBuf,
    pub source: String,
    pub program: Program,
    pub imports: Vec<ResolvedImport>,
    pub exports: BTreeSet<String>,
    pub kind: ModuleKind,
}

impl SourceModule {
    fn from_builtin(module: crate::runtime::BuiltinModule) -> Self {
        Self {
            key: module.key.clone(),
            path: PathBuf::from(module.key.clone()),
            source: String::new(),
            program: Program::default(),
            imports: Vec::new(),
            exports: module.exports,
            kind: ModuleKind::Builtin,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleGraph {
    entry_key: String,
    modules: BTreeMap<String, SourceModule>,
}

impl ModuleGraph {
    pub fn load(entry_input: &Path, options: CompileOptions) -> Result<Self> {
        let entry_path = resolve_entry_path(entry_input)?;
        let entry_key = canonical_module_key(&entry_path)?;
        let workspace_root = discover_workspace_root(&entry_path)?;
        let module_cache = ModuleCache::discover()?;
        let mut manifest_cache = ManifestCache::default();

        let mut modules = BTreeMap::new();
        let mut pending = VecDeque::new();
        pending.push_back(PendingModule {
            path: entry_path.clone(),
            kind: ModuleKind::Entry,
            trace_route: vec![entry_path.display().to_string()],
        });

        while let Some(current) = pending.pop_front() {
            let module_key = canonical_module_key(&current.path)?;
            if modules.contains_key(&module_key) {
                continue;
            }

            let source = std::fs::read_to_string(&current.path).with_context(|| {
                attach_trace(
                    format!("failed to read module {}", current.path.display()),
                    &current.trace_route,
                    options,
                )
            })?;

            let program = parser::parse_source(&source).with_context(|| {
                attach_trace(
                    format!("failed to parse module {}", current.path.display()),
                    &current.trace_route,
                    options,
                )
            })?;

            let owner_manifest = find_owner_manifest(
                &current.path,
                &workspace_root,
                &mut manifest_cache,
                options,
                &current.trace_route,
            )?;

            let mut imports = Vec::new();
            for specifier in collect_imports(&program) {
                if let Some(builtin) = crate::runtime::builtin_module(&specifier) {
                    let resolved_key = builtin.key.clone();
                    modules
                        .entry(resolved_key.clone())
                        .or_insert_with(|| SourceModule::from_builtin(builtin));

                    imports.push(ResolvedImport {
                        specifier,
                        resolved_key,
                    });

                    continue;
                }

                let resolved = resolve_import_target(
                    &current.path,
                    &specifier,
                    owner_manifest.as_ref(),
                    &workspace_root,
                    &module_cache,
                    &mut manifest_cache,
                    options,
                    &current.trace_route,
                )?;

                let resolved_key = canonical_module_key(&resolved.path)?;

                let mut trace_route = current.trace_route.clone();
                trace_route.push(format!("{} -> {}", specifier, resolved.path.display()));

                pending.push_back(PendingModule {
                    path: resolved.path,
                    kind: resolved.kind,
                    trace_route,
                });

                imports.push(ResolvedImport {
                    specifier,
                    resolved_key,
                });
            }

            let exports = collect_exports(&source);

            modules.insert(
                module_key.clone(),
                SourceModule {
                    key: module_key,
                    path: current.path,
                    source,
                    program,
                    imports,
                    exports,
                    kind: current.kind,
                },
            );
        }

        Ok(Self { entry_key, modules })
    }

    pub fn entry(&self) -> Option<&SourceModule> {
        self.modules.get(&self.entry_key)
    }

    pub fn modules(&self) -> impl Iterator<Item = &SourceModule> {
        self.modules.values()
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn import_exports_for(&self, module: &SourceModule) -> BTreeMap<String, BTreeSet<String>> {
        let mut map = BTreeMap::new();

        for import in &module.imports {
            if let Some(target) = self.modules.get(&import.resolved_key) {
                map.insert(import.specifier.clone(), target.exports.clone());
            }
        }

        map
    }
}

#[derive(Debug, Clone)]
struct PendingModule {
    path: PathBuf,
    kind: ModuleKind,
    trace_route: Vec<String>,
}

#[derive(Debug, Clone)]
struct ImportTarget {
    path: PathBuf,
    kind: ModuleKind,
}

type ManifestCache = BTreeMap<PathBuf, PackageManifest>;

#[derive(Debug, Clone)]
struct PackageManifest {
    manifest_path: PathBuf,
    package_dir: PathBuf,
    name: String,
    version: String,
    main: String,
    dependencies: BTreeMap<String, DependencySpec>,
}

#[derive(Debug, Clone)]
enum DependencySpec {
    Npm { version: String },
    Url { url: String },
    LocalPath { path: String },
}

#[derive(Debug, Deserialize)]
struct RawPackageManifest {
    name: Option<String>,
    version: Option<String>,
    main: Option<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

fn collect_imports(program: &Program) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Import(import_decl) = item {
                Some(import_decl.from.clone())
            } else {
                None
            }
        })
        .collect()
}

fn collect_exports(source: &str) -> BTreeSet<String> {
    let mut exports = BTreeSet::new();

    for raw_line in source.lines() {
        let line = strip_inline_comment(trim_bom(raw_line).trim());
        if let Some(rest) = line.strip_prefix("export ") {
            parse_export_line(rest.trim(), &mut exports);
        }
    }

    exports
}

fn parse_export_line(rest: &str, exports: &mut BTreeSet<String>) {
    for keyword in ["class", "interface", "function", "const", "let", "var"] {
        let prefix = format!("{keyword} ");
        if let Some(decl) = rest.strip_prefix(&prefix) {
            let name = parse_decl_name(decl);
            if !name.is_empty() {
                exports.insert(name);
            }
            return;
        }
    }

    if let Some(named) = rest.strip_prefix('{') {
        let Some(close) = named.find('}') else {
            return;
        };

        let names_raw = &named[..close];
        for piece in names_raw.split(',') {
            let symbol = piece.trim();
            if symbol.is_empty() {
                continue;
            }

            if let Some((_, alias)) = symbol.split_once(" as ") {
                let alias = alias.trim();
                if !alias.is_empty() {
                    exports.insert(alias.to_string());
                }
            } else {
                exports.insert(symbol.to_string());
            }
        }
    }
}

fn parse_decl_name(text: &str) -> String {
    text.split(|c: char| c == '{' || c == '(' || c == ';' || c == ':' || c.is_whitespace())
        .find(|segment| !segment.is_empty())
        .unwrap_or("")
        .to_string()
}

fn trim_bom(line: &str) -> &str {
    line.trim_start_matches('\u{feff}')
}

fn strip_inline_comment(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
    .trim()
}

fn resolve_entry_path(input: &Path) -> Result<PathBuf> {
    if input.exists() {
        validate_source_extension(input)?;
        return input
            .canonicalize()
            .with_context(|| format!("failed to canonicalize entry path {}", input.display()));
    }

    if input.extension().is_some() {
        bail!("entry module not found: {}", input.display());
    }

    for candidate in [input.with_extension("ts"), input.with_extension("rts")] {
        if candidate.exists() {
            return candidate.canonicalize().with_context(|| {
                format!("failed to canonicalize entry path {}", candidate.display())
            });
        }
    }

    bail!(
        "entry module not found. tried: {}, {} and {}",
        input.display(),
        input.with_extension("ts").display(),
        input.with_extension("rts").display()
    )
}

fn validate_source_extension(path: &Path) -> Result<()> {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        bail!(
            "source file must have .rts or .ts extension: {}",
            path.display()
        );
    };

    if ext != "rts" && ext != "ts" {
        bail!(
            "unsupported source extension '.{}' in {} (expected .rts or .ts)",
            ext,
            path.display()
        );
    }

    Ok(())
}

fn resolve_import_target(
    current_module: &Path,
    specifier: &str,
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
        let path = resolve_source_module(base_dir, specifier)?;
        return Ok(ImportTarget {
            path,
            kind: ModuleKind::Source,
        });
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

    bail!(
        "unsupported import specifier '{}' in {}. use relative imports, package dependencies, workspace packages, builtin modules, or URLs",
        specifier,
        current_module.display()
    )
}

fn resolve_source_module(base_dir: &Path, specifier: &str) -> Result<PathBuf> {
    let candidate = base_dir.join(specifier);
    resolve_source_candidate(&candidate)
}

fn resolve_source_candidate(candidate: &Path) -> Result<PathBuf> {
    if candidate.is_dir() {
        return resolve_directory_entry(candidate);
    }

    let mut attempts = Vec::new();

    if candidate.extension().is_some() {
        attempts.push(candidate.to_path_buf());
    } else {
        attempts.push(candidate.with_extension("ts"));
        attempts.push(candidate.with_extension("rts"));
        attempts.push(candidate.join("index.ts"));
        attempts.push(candidate.join("index.rts"));
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
        directory.join("index.ts"),
        directory.join("index.rts"),
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

    let fallback_index = package_dir.join("index.ts");
    if fallback_index.exists() {
        return resolve_source_candidate(&fallback_index);
    }

    bail!(
        "workspace package '{}' has no valid entry file (expected package.json main, main.ts or index.ts)",
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

fn find_owner_manifest(
    module_path: &Path,
    workspace_root: &Path,
    manifest_cache: &mut ManifestCache,
    options: CompileOptions,
    trace_route: &[String],
) -> Result<Option<PackageManifest>> {
    let workspace_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    let mut current = module_path.parent();
    while let Some(dir) = current {
        let manifest_path = dir.join("package.json");
        if manifest_path.exists() {
            return load_package_manifest(&manifest_path, manifest_cache)
                .map(Some)
                .with_context(|| {
                    attach_trace(
                        format!("invalid package manifest {}", manifest_path.display()),
                        trace_route,
                        options,
                    )
                });
        }

        if dir == workspace_root {
            break;
        }

        current = dir.parent();
    }

    Ok(None)
}

fn load_package_manifest(path: &Path, cache: &mut ManifestCache) -> Result<PackageManifest> {
    let package_dir = path
        .parent()
        .ok_or_else(|| {
            anyhow!(
                "package manifest has no parent directory: {}",
                path.display()
            )
        })?
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize package directory {}",
                path.display()
            )
        })?;

    if let Some(cached) = cache.get(&package_dir) {
        return Ok(cached.clone());
    }

    let raw_content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read package manifest {}", path.display()))?;
    let clean_content = strip_json_comments(&raw_content);
    let raw: RawPackageManifest = serde_json::from_str(&clean_content)
        .with_context(|| format!("failed to parse package manifest {}", path.display()))?;

    let default_name = package_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("package")
        .to_string();

    let mut dependencies = BTreeMap::new();
    for (name, spec) in raw.dependencies {
        dependencies.insert(name, parse_dependency_spec(&spec));
    }

    let manifest = PackageManifest {
        manifest_path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        package_dir: package_dir.clone(),
        name: raw.name.unwrap_or(default_name),
        version: raw.version.unwrap_or_else(|| "0.0.0".to_string()),
        main: raw.main.unwrap_or_else(|| "main.ts".to_string()),
        dependencies,
    };

    cache.insert(package_dir, manifest.clone());
    Ok(manifest)
}

fn parse_dependency_spec(raw: &str) -> DependencySpec {
    let value = raw.trim();

    if let Some(version) = value.strip_prefix("npm:") {
        let version = version.trim();
        return DependencySpec::Npm {
            version: if version.is_empty() {
                "latest".to_string()
            } else {
                version.to_string()
            },
        };
    }

    if is_remote_url(value) {
        return DependencySpec::Url {
            url: value.to_string(),
        };
    }

    if value.starts_with("./") || value.starts_with("../") || value.starts_with('/') {
        return DependencySpec::LocalPath {
            path: value.to_string(),
        };
    }

    DependencySpec::Npm {
        version: if value.is_empty() {
            "latest".to_string()
        } else {
            value.to_string()
        },
    }
}

#[derive(Debug, Clone)]
struct ModuleCache {
    base_dir: PathBuf,
}

impl ModuleCache {
    fn discover() -> Result<Self> {
        let base_dir = resolve_modules_base_dir()?;
        std::fs::create_dir_all(&base_dir).with_context(|| {
            format!(
                "failed to create RTS module cache directory {}",
                base_dir.display()
            )
        })?;
        Ok(Self { base_dir })
    }

    fn resolve_cached_npm_dependency(&self, module_name: &str, version: &str) -> Result<PathBuf> {
        let version = sanitize_segment(version);
        let module_name = sanitize_segment(module_name);
        let root = self.base_dir.join("npm").join(module_name).join(version);

        if !root.exists() {
            bail!(
                "cached npm module not found at {} (expected RTS_MODULES_PATH layout ~/.rts/modules/npm/<name>/<version>/...)",
                root.display()
            );
        }

        resolve_cached_module_entry(&root)
    }

    fn fetch_remote_import(&self, alias: Option<&str>, url: &str) -> Result<PathBuf> {
        let name = alias
            .map(sanitize_segment)
            .unwrap_or_else(|| sanitize_segment(&url_module_name(url)));
        let version = format!("{:016x}", fnv1a64(url.as_bytes()));
        let root = self.base_dir.join("url").join(name).join(version);
        let entry = root.join("main.ts");

        if !entry.exists() {
            std::fs::create_dir_all(&root)
                .with_context(|| format!("failed to create cache directory {}", root.display()))?;
            let body = download_remote_module(url)?;
            std::fs::write(&entry, body).with_context(|| {
                format!("failed to write remote module cache {}", entry.display())
            })?;
        }

        resolve_source_candidate(&entry)
    }
}

fn resolve_cached_module_entry(root: &Path) -> Result<PathBuf> {
    let manifest_path = root.join("package.json");
    if manifest_path.exists() {
        let raw_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let clean_content = strip_json_comments(&raw_content);
        if let Ok(raw) = serde_json::from_str::<RawPackageManifest>(&clean_content) {
            if let Some(main) = raw.main {
                let candidate = root.join(main);
                if candidate.exists() {
                    return resolve_source_candidate(&candidate);
                }
            }
        }
    }

    for candidate in [
        root.join("main.ts"),
        root.join("index.ts"),
        root.join("mod.ts"),
    ] {
        if candidate.exists() {
            return resolve_source_candidate(&candidate);
        }
    }

    bail!("cached module entry not found in {}", root.display())
}

fn resolve_modules_base_dir() -> Result<PathBuf> {
    if let Ok(configured) = std::env::var(MODULES_ENV_VAR) {
        let configured = configured.trim();
        if configured.is_empty() || configured == "~" {
            return default_modules_base_dir();
        }

        let expanded = expand_tilde_path(configured)?;
        return Ok(expanded);
    }

    default_modules_base_dir()
}

fn default_modules_base_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".rts").join("modules"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }

    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Ok(PathBuf::from(profile));
        }
    }

    bail!("unable to resolve user home directory for RTS module cache")
}

fn expand_tilde_path(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return home_dir().map(|home| home.join(".rts").join("modules"));
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().map(|home| home.join(rest));
    }

    Ok(PathBuf::from(raw))
}

fn sanitize_segment(raw: &str) -> String {
    let sanitized = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "module".to_string()
    } else {
        sanitized
    }
}

fn url_module_name(url: &str) -> String {
    let without_query = url
        .split_once('?')
        .map(|(head, _)| head)
        .unwrap_or(url)
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(url);

    let raw_name = without_query.rsplit('/').next().unwrap_or("remote").trim();

    let name = raw_name
        .strip_suffix(".ts")
        .or_else(|| raw_name.strip_suffix(".rts"))
        .unwrap_or(raw_name);

    if name.is_empty() {
        "remote".to_string()
    } else {
        name.to_string()
    }
}

fn download_remote_module(url: &str) -> Result<String> {
    match ureq::get(url).call() {
        Ok(response) => response
            .into_string()
            .with_context(|| format!("failed to decode remote module body from {}", url)),
        Err(ureq::Error::Status(code, response)) => bail!(
            "failed to fetch remote module {} (HTTP {} {})",
            url,
            code,
            response.status_text()
        ),
        Err(ureq::Error::Transport(error)) => {
            bail!("failed to fetch remote module {} ({})", url, error)
        }
    }
}

fn discover_workspace_root(entry_path: &Path) -> Result<PathBuf> {
    for ancestor in entry_path.ancestors() {
        if ancestor.join("packages").exists() {
            return ancestor.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize workspace root {}",
                    ancestor.display()
                )
            });
        }
    }

    std::env::current_dir().context("failed to resolve current directory for workspace root")
}

fn attach_trace(prefix: String, trace_route: &[String], options: CompileOptions) -> String {
    if !options.include_trace_data() || trace_route.is_empty() {
        return prefix;
    }

    let mut out = prefix;
    out.push_str("\nImport trace route:");
    for segment in trace_route {
        out.push_str("\n  - ");
        out.push_str(segment);
    }
    out
}

fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' && matches!(chars.peek(), Some('/')) {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    output.push('\n');
                    break;
                }
            }
            continue;
        }

        output.push(ch);
    }

    output
}

fn is_remote_url(specifier: &str) -> bool {
    specifier.starts_with("http://") || specifier.starts_with("https://")
}

fn canonical_module_key(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize module {}", path.display()))?;

    Ok(canonical.to_string_lossy().to_string())
}

fn fnv1a64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}
