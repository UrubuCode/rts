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
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use codegen::object::ObjectArtifact;
use compile_options::CompileOptions;
use hir::nodes::HirModule;
use module_system::ModuleGraph;
use parser::ast::{Item, Program};
use rayon::prelude::*;
use type_system::metadata::MetadataTable;

#[derive(Debug, Clone)]
pub struct CompileSummary {
    pub input: PathBuf,
    pub object_file: PathBuf,
    pub binary_file: PathBuf,
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
        path: object_file, ..
    } = codegen::generate_object_with_metadata(&mir, &metadata, &object_path)?;

    let linked = linker::link_object_to_binary(&object_file, output)?;
    let bootstrap = runtime::bootstrap::compile_source(source, options);
    runtime::bundle::package_bootstrap_payload(&linked.path, &bootstrap.encode())?;

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file,
        binary_file: linked.path,
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
    let modules = graph.modules().collect::<Vec<_>>();

    if options.emit_module_progress {
        for module in &modules {
            println!(
                "Compiling module [{}]: {}",
                module.kind.as_str(),
                module.path.display()
            );
        }
    }

    let registry = build_registry_for_graph(graph)?;

    let resolver = type_system::resolver::TypeResolver::from_registry(&registry);
    let mut merged_hir = HirModule::default();
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

    for lowered in lowered_modules {
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
    let bootstrap = runtime::bootstrap::compile_graph(graph, options)?;
    runtime::bundle::package_bootstrap_payload(&linked.path, &bootstrap.encode())?;

    Ok(CompileSummary {
        input: input.to_path_buf(),
        object_file,
        binary_file: linked.path,
        profile: options.profile.to_string(),
        link_backend: linked.backend,
        link_format: linked.format,
        discovered_types: registry.len(),
        lowered_functions: mir.functions.len(),
        compiled_modules: graph.module_count(),
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
