pub mod cli;
pub mod codegen;
pub mod compile_options;
pub mod diagnostics;
pub mod hir;
pub mod linker;
pub mod mir;
pub mod module;
pub mod namespaces;
pub mod parser;
pub mod runtime;
pub mod type_system;

mod cache;
mod pipeline;
mod runtime_lib;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use codegen::object::ObjectArtifact;
use compile_options::CompileOptions;
use module::ModuleGraph;
use type_system::metadata::MetadataTable;

#[derive(Debug, Clone)]
pub struct CompileSummary {
    pub input: PathBuf,
    pub object_file: PathBuf,
    pub binary_file: PathBuf,
    pub runtime_cache_dir: PathBuf,
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
    pub runtime_namespaces: Vec<String>,
    pub runtime_functions: usize,
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
    pipeline::compile_graph(&graph, input, output, options)
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
    let program = parser::parse_source_with_mode(source, options.frontend_mode)?;

    let mut registry = type_system::TypeRegistry::default();
    let empty_imports = type_system::checker::ImportExports::default();
    type_system::checker::check_program(&program, &mut registry, &empty_imports)?;

    let resolver = type_system::resolver::TypeResolver::from_registry(&registry);
    let mut hir = hir::lower::lower(&program, &resolver);
    let _hir_opt = hir::optimize::optimize_with_mode(&mut hir, options.frontend_mode);

    let typed_mir = mir::typed_build::typed_build(&hir);

    let _metadata = MetadataTable::from_registry(&registry);
    let deps_dir = pipeline::resolve_deps_dir(output)?;
    let launcher_dir = pipeline::resolve_launcher_cache_dir(output)?;
    pipeline::sync_namespace_artifacts(&launcher_dir)?;
    let usage = namespaces::NamespaceUsage::from_sources(std::iter::once(source));
    let runtime_namespaces = usage
        .enabled_namespaces()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let runtime_functions = usage.enabled_functions().count();
    let app_name = pipeline::sanitize_dep_name(
        output
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("app"),
    );
    let object_path = deps_dir.join(format!("{app_name}_entry.o"));

    let ObjectArtifact {
        path: object_file,
        bytes_written: app_object_bytes,
    } = codegen::generate_typed_object(
        &typed_mir,
        &object_path,
        true,
        optimize_for_production,
    )?;
    let runtime_objects =
        pipeline::emit_selected_namespace_objects(&deps_dir, &usage, &options)?;
    let runtime_support_library = runtime_lib::resolve_runtime_support_library(&deps_dir)?;
    let runtime_object_bytes = runtime_objects.bytes_written;
    let dependency_objects = 2usize + runtime_objects.object_paths.len();
    let cache_hits = runtime_objects.cache_hits;
    let cache_misses = 1usize + runtime_objects.cache_misses;

    let mut objects = Vec::with_capacity(2 + runtime_objects.object_paths.len());
    objects.push(object_file.clone());
    objects.extend(runtime_objects.object_paths);
    objects.push(runtime_support_library);
    let linked = linker::link_objects_to_binary(&objects, output)?;

    let binary_bytes = std::fs::metadata(&linked.path)
        .with_context(|| format!("failed to stat {}", linked.path.display()))?
        .len();

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file,
        binary_file: linked.path,
        runtime_cache_dir: deps_dir,
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
        lowered_functions: typed_mir.functions.len(),
        compiled_modules: 1,
        runtime_namespaces,
        runtime_functions,
    })
}

#[cfg(test)]
mod incremental_cache_tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use crate::module::{ModuleKind, SourceModule};
    use crate::parser::ast::Program;
    use crate::pipeline::{
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
