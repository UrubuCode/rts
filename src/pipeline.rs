use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::CompileSummary;
use crate::cache::{
    OBJECT_CACHE_SCHEMA, ObjectCacheMeta, RuntimeObjectArtifacts, hash_source,
    is_cached_object_valid, write_object_cache_meta,
};
use crate::codegen;
use crate::codegen::object::ObjectArtifact;
use crate::compile_options::CompileOptions;
use crate::hir;
use crate::linker;
use crate::mir;
use crate::module::{ModuleGraph, ModuleKind, SourceModule};
use crate::namespaces;
use crate::parser::ast::{Item, Program};
use crate::runtime_lib;
use crate::type_system;
use crate::type_system::metadata::MetadataTable;

pub(crate) const LAUNCHER_NAMESPACE_CATALOG_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LauncherNamespaceCatalog {
    pub(crate) schema: u32,
    pub(crate) namespaces: Vec<LauncherNamespaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LauncherNamespaceEntry {
    pub(crate) namespace: String,
    pub(crate) callees: Vec<String>,
}

pub(crate) fn compile_graph(
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

            hir::lower::lower(&pruned_program, &resolver)
        })
        .collect::<Vec<_>>();

    let deps_dir = resolve_deps_dir(output)?;
    let launcher_dir = resolve_launcher_cache_dir(output)?;
    sync_namespace_artifacts(&launcher_dir)?;
    let app_name = sanitize_dep_name(
        output
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("app"),
    );
    let usage = namespaces::NamespaceUsage::from_sources(
        graph.modules().map(|module| module.source.as_str()),
    );
    let runtime_namespaces = usage
        .enabled_namespaces()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let runtime_functions = usage.enabled_functions().count();

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
        let mut typed_mir = mir::typed_build::typed_build(&lowered);

        if !is_entry_module {
            typed_mir
                .functions
                .retain(|function| function.name != "main" && function.name != "_start");
        }

        if typed_mir.functions.is_empty() {
            continue;
        }

        lowered_functions += typed_mir.functions.len();
        let stem = module_object_stem(&app_name, module, input);
        let object_path = deps_dir.join(format!("{stem}.o"));
        let meta_path = deps_dir.join(format!("{stem}.m"));
        let source_hash = hash_source(&module.source);
        let deps_hash = graph.transitive_deps_hash(&module.key);

        if is_cached_object_valid(
            &meta_path,
            &object_path,
            &source_hash,
            &deps_hash,
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

        let artifact =
            codegen::generate_typed_object(&typed_mir, &object_path, is_entry_module, &options)?;

        app_object_bytes += artifact.bytes_written;
        write_object_cache_meta(
            &meta_path,
            &ObjectCacheMeta {
                cache_schema: OBJECT_CACHE_SCHEMA,
                source_hash,
                deps_hash,
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
        let artifact =
            emit_fallback_main_object(&deps_dir, &app_name, &metadata, optimize_for_production)?;
        app_object_bytes += artifact.bytes_written;
        lowered_functions += 1;
        cache_misses += 1;
        object_files.push(artifact.path);
    }

    let runtime_objects = emit_selected_namespace_objects(&deps_dir, &usage, &options)?;
    let runtime_support_library = runtime_lib::resolve_runtime_support_library(&deps_dir)?;
    let runtime_object_bytes = runtime_objects.bytes_written;
    cache_hits += runtime_objects.cache_hits;
    cache_misses += runtime_objects.cache_misses;
    object_files.extend(runtime_objects.object_paths);
    object_files.push(runtime_support_library);

    let linked = linker::link_objects_to_binary(&object_files, output)?;
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
        runtime_cache_dir: deps_dir,
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
        runtime_namespaces,
        runtime_functions,
    })
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

pub(crate) fn collect_required_exports(graph: &ModuleGraph) -> BTreeMap<String, BTreeSet<String>> {
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

pub(crate) fn prune_program_for_lowering(
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

pub(crate) fn resolve_deps_dir(output: &Path) -> Result<PathBuf> {
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

pub(crate) fn resolve_launcher_cache_dir(output: &Path) -> Result<PathBuf> {
    let base = if let Ok(configured) = std::env::var("RTS_LAUNCHER_CACHE_DIR") {
        let configured = configured.trim();
        if configured.is_empty() {
            PathBuf::from("target").join(".launcher")
        } else {
            PathBuf::from(configured)
        }
    } else {
        let _ = output;
        PathBuf::from("target").join(".launcher")
    };

    let launcher = if base.ends_with(".launcher") {
        base
    } else {
        base.join(".launcher")
    };

    std::fs::create_dir_all(&launcher)
        .with_context(|| format!("failed to create {}", launcher.display()))?;
    Ok(launcher)
}

fn emit_fallback_main_object(
    deps_dir: &Path,
    app_name: &str,
    metadata: &MetadataTable,
    optimize_for_production: bool,
) -> Result<ObjectArtifact> {
    let object_path = deps_dir.join(format!("{app_name}_bootstrap_main.o"));
    let mir_module = mir::MirModule {
        functions: vec![mir::MirFunction {
            name: "main".to_string(),
            blocks: vec![mir::cfg::BasicBlock {
                label: "entry".to_string(),
                statements: vec![mir::MirStatement {
                    text: "ret".to_string(),
                }],
                terminator: mir::cfg::Terminator::Return,
            }],
        }],
    };
    codegen::generate_object_with_metadata_options(
        &mir_module,
        metadata,
        &object_path,
        true,
        optimize_for_production,
    )
}

pub(crate) fn write_launcher_namespace_catalog(launcher_dir: &Path) -> Result<()> {
    let catalog = namespaces::catalog();
    let payload = LauncherNamespaceCatalog {
        schema: LAUNCHER_NAMESPACE_CATALOG_SCHEMA,
        namespaces: catalog
            .into_iter()
            .map(|entry| LauncherNamespaceEntry {
                namespace: entry.namespace,
                callees: entry.callees,
            })
            .collect(),
    };

    let output_path = launcher_dir.join("rts_namespace_catalog.json");
    let encoded = serde_json::to_string_pretty(&payload)
        .map_err(|error| anyhow!("failed to encode launcher namespace catalog: {error}"))?;
    std::fs::write(&output_path, encoded).with_context(|| {
        format!(
            "failed to write launcher namespace catalog {}",
            output_path.display()
        )
    })
}

pub(crate) fn sync_namespace_artifacts(launcher_dir: &Path) -> Result<()> {
    write_launcher_namespace_catalog(launcher_dir)?;
    let obsolete_runtime_cache = launcher_dir.join("runtime");
    if obsolete_runtime_cache.is_dir() {
        std::fs::remove_dir_all(&obsolete_runtime_cache).with_context(|| {
            format!(
                "failed to remove obsolete launcher runtime cache {}",
                obsolete_runtime_cache.display()
            )
        })?;
    }
    let dts_path = namespaces::default_typescript_output_path();
    namespaces::emit_typescript_declarations(&dts_path)
        .with_context(|| format!("failed to emit {}", dts_path.display()))
}

pub(crate) fn emit_selected_namespace_objects(
    deps_dir: &Path,
    usage: &namespaces::NamespaceUsage,
    options: &CompileOptions,
) -> Result<RuntimeObjectArtifacts> {
    let callees = usage.enabled_functions().collect::<Vec<_>>();
    if callees.is_empty() {
        return Ok(RuntimeObjectArtifacts::default());
    }

    let mut ordered = callees
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<String>>();
    ordered.sort();
    ordered.dedup();

    let bytes = crate::codegen::cranelift::object_builder::build_namespace_dispatch_object(
        &ordered,
        options.profile.as_str() == "production",
    )?;
    let output_path = deps_dir.join("rts_namespace_dispatch.o");
    let artifact = crate::codegen::object::write_object_file(&output_path, &bytes)?;

    Ok(RuntimeObjectArtifacts {
        object_paths: vec![artifact.path],
        bytes_written: artifact.bytes_written,
        cache_hits: 0,
        cache_misses: 1,
    })
}

pub(crate) fn module_object_stem(app_name: &str, module: &SourceModule, input: &Path) -> String {
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

pub(crate) fn relative_module_path(module_path: &Path, input: &Path) -> PathBuf {
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

pub(crate) fn sanitize_dep_name(raw: &str) -> String {
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

pub(crate) fn cached_dependency_name_version(path: &Path) -> Option<(String, String)> {
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
