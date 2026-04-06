pub mod cli;
pub mod codegen;
pub mod diagnostics;
pub mod hir;
pub mod linker;
pub mod mir;
pub mod module_system;
pub mod parser;
pub mod runtime;
pub mod type_system;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use codegen::object::ObjectArtifact;
use hir::nodes::HirModule;
use module_system::ModuleGraph;
use type_system::metadata::MetadataTable;

#[derive(Debug, Clone)]
pub struct CompileSummary {
    pub input: PathBuf,
    pub object_file: PathBuf,
    pub binary_file: PathBuf,
    pub link_backend: String,
    pub link_format: String,
    pub discovered_types: usize,
    pub lowered_functions: usize,
    pub compiled_modules: usize,
}

pub fn compile_file(input: &Path, output: &Path) -> Result<CompileSummary> {
    let graph = ModuleGraph::load(input)?;
    compile_graph(&graph, input, output)
}

pub fn compile_source(source: &str, input: &Path, output: &Path) -> Result<CompileSummary> {
    let program = parser::parse_source(source)?;

    let mut registry = type_system::TypeRegistry::default();
    let empty_imports = type_system::checker::ImportExports::default();
    type_system::checker::check_program(&program, &mut registry, &empty_imports)?;

    let resolver = type_system::resolver::TypeResolver::from_registry(&registry);
    let hir = hir::lower::lower(&program, &resolver);

    let mut mir = mir::build::build(&hir);
    let _mono = mir::monomorphize::monomorphize(&mut mir);
    let _opt = mir::optimize::optimize(&mut mir);

    let metadata = MetadataTable::from_registry(&registry);
    let object_path = output.with_extension("o");

    let ObjectArtifact {
        path: object_file, ..
    } = codegen::generate_object_with_metadata(&mir, &metadata, &object_path)?;

    let linked = linker::link_object_to_binary(&object_file, output)?;
    let bootstrap = runtime::bootstrap::compile_source(source);
    runtime::bundle::package_bootstrap_payload(&linked.path, &bootstrap.encode())?;

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file,
        binary_file: linked.path,
        link_backend: linked.backend,
        link_format: linked.format,
        discovered_types: registry.len(),
        lowered_functions: mir.functions.len(),
        compiled_modules: 1,
    })
}

fn compile_graph(graph: &ModuleGraph, input: &Path, output: &Path) -> Result<CompileSummary> {
    let mut registry = type_system::TypeRegistry::default();

    for module in graph.modules() {
        let import_exports = graph.import_exports_for(module);
        type_system::checker::check_program(&module.program, &mut registry, &import_exports)
            .map_err(|error| {
                anyhow!(
                    "{} (in module {})",
                    error,
                    module.path.display()
                )
            })?;
    }

    let resolver = type_system::resolver::TypeResolver::from_registry(&registry);
    let mut merged_hir = HirModule::default();

    for module in graph.modules() {
        let lowered = hir::lower::lower(&module.program, &resolver);
        merged_hir.items.extend(lowered.items);
        merged_hir.imports.extend(lowered.imports);
        merged_hir.classes.extend(lowered.classes);
        merged_hir.functions.extend(lowered.functions);
        merged_hir.interfaces.extend(lowered.interfaces);
    }

    let mut mir = mir::build::build(&merged_hir);
    let _mono = mir::monomorphize::monomorphize(&mut mir);
    let _opt = mir::optimize::optimize(&mut mir);

    let metadata = MetadataTable::from_registry(&registry);
    let object_path = output.with_extension("o");

    let ObjectArtifact {
        path: object_file, ..
    } = codegen::generate_object_with_metadata(&mir, &metadata, &object_path)?;

    let linked = linker::link_object_to_binary(&object_file, output)?;
    let bootstrap = runtime::bootstrap::compile_graph(graph)?;
    runtime::bundle::package_bootstrap_payload(&linked.path, &bootstrap.encode())?;

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file,
        binary_file: linked.path,
        link_backend: linked.backend,
        link_format: linked.format,
        discovered_types: registry.len(),
        lowered_functions: mir.functions.len(),
        compiled_modules: graph.module_count(),
    })
}
