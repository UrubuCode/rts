pub mod cli;
pub mod codegen;
pub mod compile_options;
pub mod diagnostics;
pub mod hir;
pub mod linker;
pub mod mir;
pub mod module_system;
pub mod parser;
pub mod runtime;
pub mod type_system;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use codegen::object::ObjectArtifact;
use compile_options::CompileOptions;
use module_system::{ModuleGraph, ModuleKind, SourceModule};
use parser::ast::{Item, Program};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use type_system::metadata::MetadataTable;

#[derive(Debug, Clone)]
pub struct CompileSummary {
    pub input: PathBuf,
    pub object_file: PathBuf,
    pub binary_file: PathBuf,
    pub app_object_bytes: usize,
    pub runtime_object_bytes: usize,
    pub binary_bytes: u64,
    pub dependency_objects: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub profile: String,
    pub link_backend: String,
    pub link_format: String,
    pub discovered_types: usize,
    pub lowered_functions: usize,
    pub compiled_modules: usize,
}

pub fn compile_file(input: &Path, output: &Path) -> Result<CompileSummary> {
    compile_file_with_options(input, output, CompileOptions::default())
}

pub fn compile_source(source: &str, input: &Path, output: &Path) -> Result<CompileSummary> {
    compile_source_with_options(source, input, output, CompileOptions::default())
}

pub fn compile_file_with_options(
    input: &Path,
    output: &Path,
    options: CompileOptions,
) -> Result<CompileSummary> {
    let graph = ModuleGraph::load(input, options)?;
    compile_graph(&graph, input, output, options)
}

pub fn compile_source_with_options(
    source: &str,
    input: &Path,
    output: &Path,
    options: CompileOptions,
) -> Result<CompileSummary> {
    let optimize_for_production = matches!(
        options.profile,
        crate::compile_options::CompilationProfile::Production
    );
    let program = parser::parse_source(source)?;

    let mut registry = type_system::TypeRegistry::default();
    let empty_imports = type_system::checker::ImportExports::default();
    type_system::checker::check_program(&program, &mut registry, &empty_imports)?;

    let resolver = type_system::resolver::TypeResolver::from_registry(&registry);
    let mut hir = hir::lower::lower(&program, &resolver);
    let _hir_opt = hir::optimize::optimize(&mut hir);

    let mut mir = mir::build::build(&hir);
    let _mono = mir::monomorphize::monomorphize(&mut mir);
    let _opt = mir::optimize::optimize(&mut mir);

    let metadata = MetadataTable::from_registry(&registry);
    let object_path = output.with_extension("o");

    let ObjectArtifact {
        path: object_file,
        bytes_written: app_object_bytes,
    } = codegen::generate_object_with_metadata_options(
        &mir,
        &metadata,
        &object_path,
        true,
        optimize_for_production,
    )?;

    let pure_aot = pure_aot_mode_enabled();
    let runtime_object_bytes;
    let mut dependency_objects = 1usize;
    let mut cache_hits = 0usize;
    let mut cache_misses = 1usize;
    let linked = if pure_aot {
        let deps_dir = resolve_deps_dir(output)?;
        let bootstrap = runtime::bootstrap::compile_source(source, options);
        let runtime_objects = emit_pure_aot_runtime_objects_with_program(
            &deps_dir,
            std::iter::once(source),
            &bootstrap,
            &options,
        )?;
        runtime_object_bytes = runtime_objects.bytes_written;
        dependency_objects += runtime_objects.object_paths.len();
        cache_hits += runtime_objects.cache_hits;
        cache_misses += runtime_objects.cache_misses;

        let mut objects = Vec::with_capacity(1 + runtime_objects.object_paths.len());
        objects.push(object_file.clone());
        objects.extend(runtime_objects.object_paths);
        linker::link_objects_to_binary(&objects, output)?
    } else {
        let linked = linker::link_object_to_binary(&object_file, output)?;
        let bootstrap = runtime::bootstrap::compile_source(source, options);
        let runtime_object_payload =
            runtime::runtime_object::build_for_sources(std::iter::once(source), &bootstrap)?;
        runtime_object_bytes = runtime_object_payload.len();
        runtime::bundle::package_bootstrap_payload(&linked.path, &runtime_object_payload)?;
        linked
    };
    let binary_bytes = std::fs::metadata(&linked.path)
        .with_context(|| format!("failed to stat {}", linked.path.display()))?
        .len();

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file,
        binary_file: linked.path,
        app_object_bytes,
        runtime_object_bytes,
        binary_bytes,
        dependency_objects,
        cache_hits,
        cache_misses,
        profile: options.profile.to_string(),
        link_backend: linked.backend,
        link_format: linked.format,
        discovered_types: registry.len(),
        lowered_functions: mir.functions.len(),
        compiled_modules: 1,
    })
}

fn compile_graph(
    graph: &ModuleGraph,
    input: &Path,
    output: &Path,
    options: CompileOptions,
) -> Result<CompileSummary> {
    let optimize_for_production = matches!(
        options.profile,
        crate::compile_options::CompilationProfile::Production
    );
    let modules = graph.modules().collect::<Vec<_>>();

    if options.emit_module_progress {
        let color_enabled = std::env::var_os("NO_COLOR").is_none();
        for module in &modules {
            if color_enabled {
                println!(
                    "\x1b[2mCompiling module\x1b[0m [\x1b[36m{}\x1b[0m]: \x1b[90m{}\x1b[0m",
                    module.kind.as_str(),
                    module.path.display()
                );
            } else {
                println!(
                    "Compiling module [{}]: {}",
                    module.kind.as_str(),
                    module.path.display()
                );
            }
        }
    }

    let registry = build_registry_for_graph(graph)?;
    let metadata = MetadataTable::from_registry(&registry);
    let resolver = type_system::resolver::TypeResolver::from_registry(&registry);
    let required_exports = collect_required_exports(graph);
    let entry_key = graph.entry().map(|module| module.key.as_str());

    let lowered_modules = modules
        .par_iter()
        .map(|module| {
            let pruned_program = prune_program_for_lowering(
                &module.program,
                &module.key,
                entry_key,
                &required_exports,
            );

            let mut lowered = hir::lower::lower(&pruned_program, &resolver);
            let _hir_opt = hir::optimize::optimize(&mut lowered);
            lowered
        })
        .collect::<Vec<_>>();

    let deps_dir = resolve_deps_dir(output)?;
    let app_name = sanitize_dep_name(
        output
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("app"),
    );

    let mut object_files = Vec::<PathBuf>::new();
    let mut app_object_bytes = 0usize;
    let mut lowered_functions = 0usize;
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;

    for (index, lowered) in lowered_modules.into_iter().enumerate() {
        let module = modules[index];
        if matches!(module.kind, ModuleKind::Builtin) {
            continue;
        }

        let is_entry_module = entry_key.is_some_and(|key| key == module.key);
        let mut mir = mir::build::build(&lowered);
        let _mono = mir::monomorphize::monomorphize(&mut mir);
        let _opt = mir::optimize::optimize(&mut mir);

        if !is_entry_module {
            mir.functions
                .retain(|function| function.name != "main" && function.name != "_start");
        }

        if mir.functions.is_empty() {
            continue;
        }

        lowered_functions += mir.functions.len();
        let stem = module_object_stem(&app_name, module, input);
        let object_path = deps_dir.join(format!("{stem}.o"));
        let meta_path = deps_dir.join(format!("{stem}.m"));
        let source_hash = hash_source(&module.source);

        if is_cached_object_valid(
            &meta_path,
            &object_path,
            &source_hash,
            &options,
            is_entry_module,
        ) {
            cache_hits += 1;
            app_object_bytes += std::fs::metadata(&object_path)
                .map(|metadata| metadata.len() as usize)
                .unwrap_or(0);
            object_files.push(object_path);
            continue;
        }

        let artifact = codegen::generate_object_with_metadata_options(
            &mir,
            &metadata,
            &object_path,
            is_entry_module,
            optimize_for_production,
        )?;

        app_object_bytes += artifact.bytes_written;
        write_object_cache_meta(
            &meta_path,
            &ObjectCacheMeta {
                source_hash,
                profile: options.profile.to_string(),
                debug: options.debug,
                emit_entrypoint: is_entry_module,
                object_bytes: artifact.bytes_written as u64,
                rts_version: env!("CARGO_PKG_VERSION").to_string(),
            },
        )?;

        object_files.push(artifact.path);
        cache_misses += 1;
    }

    if object_files.is_empty() {
        let object_path = deps_dir.join(format!("{app_name}_main.o"));
        let mut fallback_mir = mir::MirModule::default();
        fallback_mir.functions.push(mir::MirFunction {
            name: "main".to_string(),
            blocks: vec![mir::cfg::BasicBlock {
                label: "entry".to_string(),
                statements: vec![mir::MirStatement {
                    text: "ret 0".to_string(),
                }],
                terminator: mir::cfg::Terminator::Return,
            }],
        });
        let artifact = codegen::generate_object_with_metadata_options(
            &fallback_mir,
            &metadata,
            &object_path,
            true,
            optimize_for_production,
        )?;
        app_object_bytes += artifact.bytes_written;
        lowered_functions += 1;
        cache_misses += 1;
        object_files.push(artifact.path);
    }

    let pure_aot = pure_aot_mode_enabled();
    let mut runtime_object_bytes = 0usize;
    if pure_aot {
        let bootstrap = runtime::bootstrap::compile_graph(graph, options)?;
        let runtime_objects = emit_pure_aot_runtime_objects_with_program(
            &deps_dir,
            graph.modules().map(|module| module.source.as_str()),
            &bootstrap,
            &options,
        )?;
        runtime_object_bytes = runtime_objects.bytes_written;
        cache_hits += runtime_objects.cache_hits;
        cache_misses += runtime_objects.cache_misses;
        object_files.extend(runtime_objects.object_paths);
    }

    let linked = linker::link_objects_to_binary(&object_files, output)?;
    if !pure_aot {
        let bootstrap = runtime::bootstrap::compile_graph(graph, options)?;
        let runtime_object_payload = runtime::runtime_object::build_for_sources(
            graph.modules().map(|module| module.source.as_str()),
            &bootstrap,
        )?;
        runtime_object_bytes = runtime_object_payload.len();
        runtime::bundle::package_bootstrap_payload(&linked.path, &runtime_object_payload)?;
    }
    let binary_bytes = std::fs::metadata(&linked.path)
        .with_context(|| format!("failed to stat {}", linked.path.display()))?
        .len();

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file: object_files
            .first()
            .cloned()
            .unwrap_or_else(|| output.with_extension("o")),
        binary_file: linked.path,
        app_object_bytes,
        runtime_object_bytes,
        binary_bytes,
        dependency_objects: object_files.len(),
        cache_hits,
        cache_misses,
        profile: options.profile.to_string(),
        link_backend: linked.backend,
        link_format: linked.format,
        discovered_types: registry.len(),
        lowered_functions,
        compiled_modules: graph.module_count(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObjectCacheMeta {
    source_hash: String,
    profile: String,
    debug: bool,
    emit_entrypoint: bool,
    object_bytes: u64,
    rts_version: String,
}

#[derive(Debug, Default)]
struct RuntimeObjectArtifacts {
    object_paths: Vec<PathBuf>,
    bytes_written: usize,
    cache_hits: usize,
    cache_misses: usize,
}

fn resolve_deps_dir(output: &Path) -> Result<PathBuf> {
    let base = if let Ok(configured) = std::env::var("RTS_DEPS_DIR") {
        let configured = configured.trim();
        if configured.is_empty() {
            PathBuf::from("target").join(".deps")
        } else {
            PathBuf::from(configured)
        }
    } else {
        let _ = output;
        PathBuf::from("target").join(".deps")
    };
    let deps = if base.ends_with(".deps") {
        base
    } else {
        base.join(".deps")
    };
    std::fs::create_dir_all(&deps)
        .with_context(|| format!("failed to create {}", deps.display()))?;
    Ok(deps)
}

fn module_object_stem(app_name: &str, module: &SourceModule, input: &Path) -> String {
    match module.kind {
        ModuleKind::Builtin => "rts".to_string(),
        ModuleKind::CachedDependency => {
            if let Some((name, version)) = cached_dependency_name_version(&module.path) {
                return format!(
                    "{}_{}",
                    sanitize_dep_name(&name),
                    sanitize_dep_name(&version)
                );
            }
            let fallback = short_relative_fallback(&module.path, 4).with_extension("");
            format!(
                "{}_{}",
                app_name,
                sanitize_dep_name(&fallback.to_string_lossy())
            )
        }
        ModuleKind::Entry | ModuleKind::Source | ModuleKind::WorkspacePackage => {
            let relative = relative_module_path(&module.path, input);

            let relative = relative.with_extension("");
            format!(
                "{}_{}",
                app_name,
                sanitize_dep_name(&relative.to_string_lossy())
            )
        }
    }
}

fn relative_module_path(module_path: &Path, input: &Path) -> PathBuf {
    let module_candidates = normalized_path_candidates(module_path);
    let mut root_candidates = Vec::<PathBuf>::new();

    if let Some(input_root) = input.parent() {
        root_candidates.extend(normalized_path_candidates(input_root));
    }
    if let Ok(cwd) = std::env::current_dir() {
        root_candidates.extend(normalized_path_candidates(&cwd));
    }

    for module_candidate in &module_candidates {
        for root in &root_candidates {
            if let Ok(relative) = module_candidate.strip_prefix(root) {
                if !relative.as_os_str().is_empty() {
                    return relative.to_path_buf();
                }
            }
        }
    }

    short_relative_fallback(module_path, 4)
}

fn normalized_path_candidates(path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();

    let direct = strip_windows_verbatim_prefix(path);
    if !candidates.iter().any(|item| item == &direct) {
        candidates.push(direct);
    }

    if let Ok(canonical) = path.canonicalize() {
        let canonical = strip_windows_verbatim_prefix(&canonical);
        if !candidates.iter().any(|item| item == &canonical) {
            candidates.push(canonical);
        }
    }

    candidates
}

fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }

    path.to_path_buf()
}

fn short_relative_fallback(path: &Path, max_segments: usize) -> PathBuf {
    let normalized = strip_windows_verbatim_prefix(path);
    let segments = normalized
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_os_string()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if segments.is_empty() {
        return PathBuf::from("module");
    }

    let start = segments.len().saturating_sub(max_segments);
    let mut fallback = PathBuf::new();
    for segment in segments.into_iter().skip(start) {
        fallback.push(segment);
    }

    fallback
}

fn sanitize_dep_name(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut last_was_sep = false;

    for ch in raw.chars() {
        let mapped = if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '-' | '.') {
            ch
        } else if ch == '@' {
            '$'
        } else {
            '_'
        };

        let is_sep = mapped == '_';
        if is_sep && last_was_sep {
            continue;
        }
        last_was_sep = is_sep;
        output.push(mapped);
    }

    let trimmed = output.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "module".to_string()
    } else {
        trimmed
    }
}

fn cached_dependency_name_version(path: &Path) -> Option<(String, String)> {
    let segments = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if let Some(npm_index) = segments.iter().position(|segment| segment == "npm") {
        let raw_name = segments.get(npm_index + 1)?.clone();
        let version = segments.get(npm_index + 2)?.clone();
        let name = if let Some(stripped) = raw_name.strip_prefix('_') {
            if stripped.is_empty() {
                raw_name
            } else {
                format!("${stripped}")
            }
        } else {
            raw_name
        };
        return Some((name, version));
    }

    if let Some(url_index) = segments.iter().position(|segment| segment == "url") {
        let alias = segments.get(url_index + 1)?.clone();
        let version = segments.get(url_index + 2)?.clone();
        return Some((alias, version));
    }

    None
}

fn hash_source(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    format!("{digest:x}")
}

fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn emit_pure_aot_runtime_objects_with_program<'a>(
    deps_dir: &Path,
    sources: impl IntoIterator<Item = &'a str>,
    program: &runtime::bootstrap::BootstrapProgram,
    options: &CompileOptions,
) -> Result<RuntimeObjectArtifacts> {
    let source_list = sources.into_iter().collect::<Vec<_>>();
    let usage = runtime::namespaces::NamespaceUsage::from_sources(source_list.iter().copied());

    let mut artifacts = RuntimeObjectArtifacts::default();

    let mut core_fingerprint = program.encode();
    for namespace in usage.enabled_namespaces() {
        core_fingerprint.extend_from_slice(namespace.as_bytes());
        core_fingerprint.push(b'\n');
    }
    for function in usage.enabled_functions() {
        core_fingerprint.extend_from_slice(function.as_bytes());
        core_fingerprint.push(b'\n');
    }

    let core = emit_cached_object_bytes(
        deps_dir,
        "builtin_rts_core",
        &hash_bytes(&core_fingerprint),
        options,
        false,
        || runtime::runtime_object::build_for_sources(source_list.iter().copied(), program),
    )?;
    artifacts.bytes_written += core.bytes_written;
    artifacts.cache_hits += usize::from(core.cache_hit);
    artifacts.cache_misses += usize::from(!core.cache_hit);
    artifacts.object_paths.push(core.path);

    let namespaces = usage
        .enabled_namespaces()
        .map(str::to_string)
        .collect::<Vec<_>>();
    for namespace in namespaces {
        let prefix = format!("{namespace}.");
        let functions = usage
            .enabled_functions()
            .filter(|callee| callee.starts_with(&prefix))
            .map(str::to_string)
            .collect::<Vec<_>>();

        let fingerprint = format!("namespace:{namespace}\n{}", functions.join("\n"));
        let stem = format!("builtin_rts_{}", sanitize_dep_name(&namespace));
        let module = emit_cached_object_bytes(
            deps_dir,
            &stem,
            &hash_source(&fingerprint),
            options,
            false,
            || runtime::runtime_object::build_builtin_namespace_object(&namespace, &functions),
        )?;
        artifacts.bytes_written += module.bytes_written;
        artifacts.cache_hits += usize::from(module.cache_hit);
        artifacts.cache_misses += usize::from(!module.cache_hit);
        artifacts.object_paths.push(module.path);
    }

    Ok(artifacts)
}

#[derive(Debug)]
struct CachedObjectEmission {
    path: PathBuf,
    bytes_written: usize,
    cache_hit: bool,
}

fn emit_cached_object_bytes<F>(
    deps_dir: &Path,
    stem: &str,
    source_hash: &str,
    options: &CompileOptions,
    emit_entrypoint: bool,
    build: F,
) -> Result<CachedObjectEmission>
where
    F: FnOnce() -> Result<Vec<u8>>,
{
    let object_path = deps_dir.join(format!("{stem}.o"));
    let meta_path = deps_dir.join(format!("{stem}.m"));

    if is_cached_object_valid(
        &meta_path,
        &object_path,
        source_hash,
        options,
        emit_entrypoint,
    ) {
        let bytes_written = std::fs::metadata(&object_path)
            .map(|metadata| metadata.len() as usize)
            .unwrap_or(0);
        return Ok(CachedObjectEmission {
            path: object_path,
            bytes_written,
            cache_hit: true,
        });
    }

    let bytes = build()?;
    let artifact = codegen::object::write_object_file(&object_path, &bytes)?;
    write_object_cache_meta(
        &meta_path,
        &ObjectCacheMeta {
            source_hash: source_hash.to_string(),
            profile: options.profile.to_string(),
            debug: options.debug,
            emit_entrypoint,
            object_bytes: artifact.bytes_written as u64,
            rts_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )?;

    Ok(CachedObjectEmission {
        path: artifact.path,
        bytes_written: artifact.bytes_written,
        cache_hit: false,
    })
}

fn is_cached_object_valid(
    meta_path: &Path,
    object_path: &Path,
    source_hash: &str,
    options: &CompileOptions,
    emit_entrypoint: bool,
) -> bool {
    if !object_path.is_file() || !meta_path.is_file() {
        return false;
    }

    let meta = std::fs::read_to_string(meta_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ObjectCacheMeta>(&raw).ok());
    let Some(meta) = meta else {
        return false;
    };

    meta.source_hash == source_hash
        && meta.profile == options.profile.to_string()
        && meta.debug == options.debug
        && meta.emit_entrypoint == emit_entrypoint
        && meta.rts_version == env!("CARGO_PKG_VERSION")
}

fn write_object_cache_meta(path: &Path, meta: &ObjectCacheMeta) -> Result<()> {
    let encoded = serde_json::to_string_pretty(meta)
        .map_err(|error| anyhow!("failed to encode object cache metadata: {error}"))?;
    std::fs::write(path, encoded)
        .with_context(|| format!("failed to write object cache metadata {}", path.display()))
}

fn pure_aot_mode_enabled() -> bool {
    match std::env::var("RTS_PURE_AOT") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod incremental_cache_tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use crate::module_system::{ModuleKind, SourceModule};
    use crate::parser::ast::Program;

    use super::{
        cached_dependency_name_version, module_object_stem, relative_module_path, sanitize_dep_name,
    };

    #[test]
    fn sanitize_dep_name_keeps_scope_marker_and_version_dots() {
        assert_eq!(sanitize_dep_name("$kirejs_fs_extra"), "$kirejs_fs_extra");
        assert_eq!(sanitize_dep_name("1.0.0"), "1.0.0");
    }

    #[test]
    fn parses_cached_scoped_dependency_name_and_version() {
        let path = PathBuf::from(r"C:\Users\danie\.rts\modules\npm\_kirejs_fs_extra\1.0.0\main.ts");
        let parsed = cached_dependency_name_version(&path).expect("must parse npm cache layout");
        assert_eq!(parsed.0, "$kirejs_fs_extra");
        assert_eq!(parsed.1, "1.0.0");
    }

    #[test]
    fn module_stem_uses_relative_path_for_source_files() {
        let cwd = std::env::current_dir().expect("cwd");
        let module_path = cwd.join("examples").join("console.ts");
        let input = cwd.join("examples").join("hello_world.ts");

        let source_module = SourceModule {
            key: "entry".to_string(),
            path: module_path,
            source: String::new(),
            program: Program::default(),
            imports: Vec::new(),
            exports: BTreeSet::new(),
            kind: ModuleKind::Entry,
        };

        let stem = module_object_stem("app", &source_module, &input);
        assert_eq!(stem, "app_console");
    }

    #[test]
    fn relative_module_path_keeps_relative_shape() {
        let cwd = std::env::current_dir().expect("cwd");
        let module_path = cwd.join("examples").join("console.ts");
        let input = cwd.join("examples").join("hello_world.ts");
        let relative = relative_module_path(&module_path, &input);

        assert!(relative.is_relative());
        assert_eq!(
            relative.file_name().and_then(|value| value.to_str()),
            Some("console.ts")
        );
    }
}

pub(crate) fn build_registry_for_graph(graph: &ModuleGraph) -> Result<type_system::TypeRegistry> {
    let modules = graph.modules().collect::<Vec<_>>();

    struct ModuleDeclarationBatch {
        module_path: PathBuf,
        declarations: Result<Vec<type_system::checker::TypeDeclaration>>,
    }

    struct ModuleImportCheck {
        module_path: PathBuf,
        result: Result<()>,
    }

    let declaration_batches = modules
        .par_iter()
        .map(|module| ModuleDeclarationBatch {
            module_path: module.path.clone(),
            declarations: type_system::checker::collect_type_declarations(&module.program),
        })
        .collect::<Vec<_>>();

    let import_checks = modules
        .par_iter()
        .map(|module| {
            let import_exports = graph.import_exports_for(module);
            ModuleImportCheck {
                module_path: module.path.clone(),
                result: type_system::checker::check_imports(&module.program, &import_exports),
            }
        })
        .collect::<Vec<_>>();

    let mut registry = type_system::TypeRegistry::default();
    type_system::checker::seed_primitives(&mut registry);

    for batch in declaration_batches {
        let declarations = batch
            .declarations
            .map_err(|error| anyhow!("{} (in module {})", error, batch.module_path.display()))?;
        type_system::checker::register_type_declarations(&mut registry, declarations);
    }

    for check in import_checks {
        check
            .result
            .map_err(|error| anyhow!("{} (in module {})", error, check.module_path.display()))?;
    }

    Ok(registry)
}

fn collect_required_exports(graph: &ModuleGraph) -> BTreeMap<String, BTreeSet<String>> {
    let mut required = BTreeMap::<String, BTreeSet<String>>::new();

    for module in graph.modules() {
        let mut targets_by_specifier = BTreeMap::<String, Vec<String>>::new();
        for import in &module.imports {
            targets_by_specifier
                .entry(import.specifier.clone())
                .or_default()
                .push(import.resolved_key.clone());
        }

        for item in &module.program.items {
            let Item::Import(import_decl) = item else {
                continue;
            };

            let Some(targets) = targets_by_specifier.get(&import_decl.from) else {
                continue;
            };

            for target in targets {
                let symbols = required.entry(target.clone()).or_default();
                symbols.extend(import_decl.names.iter().cloned());
            }
        }
    }

    required
}

fn prune_program_for_lowering(
    program: &Program,
    module_key: &str,
    entry_key: Option<&str>,
    required_exports: &BTreeMap<String, BTreeSet<String>>,
) -> Program {
    if entry_key.is_some_and(|entry| entry == module_key) {
        return program.clone();
    }

    let required_names = required_exports.get(module_key);
    let mut items = Vec::new();

    for item in &program.items {
        match item {
            Item::Class(class_decl) => {
                if should_keep_named_item(required_names, &class_decl.name) {
                    items.push(item.clone());
                }
            }
            Item::Interface(interface_decl) => {
                if should_keep_named_item(required_names, &interface_decl.name) {
                    items.push(item.clone());
                }
            }
            Item::Function(function_decl) => {
                if should_keep_named_item(required_names, &function_decl.name) {
                    items.push(item.clone());
                }
            }
            Item::Import(_) | Item::Statement(_) => items.push(item.clone()),
        }
    }

    Program { items }
}

fn should_keep_named_item(required_names: Option<&BTreeSet<String>>, name: &str) -> bool {
    required_names
        .map(|set| set.contains(name))
        .unwrap_or(false)
}
