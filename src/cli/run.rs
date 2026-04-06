use std::path::PathBuf;

use anyhow::{Context, Result};

pub fn command(input_arg: Option<String>) -> Result<()> {
    let input = input_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/console.ts"));

    let graph = crate::module_system::ModuleGraph::load(&input)
        .with_context(|| format!("failed to load module graph from {}", input.display()))?;

    let mut registry = crate::type_system::TypeRegistry::default();

    for module in graph.modules() {
        let import_exports = graph.import_exports_for(module);
        crate::type_system::checker::check_program(&module.program, &mut registry, &import_exports)
            .with_context(|| format!("type check failed for {}", module.path.display()))?;
    }

    let resolver = crate::type_system::resolver::TypeResolver::from_registry(&registry);

    let mut merged_hir = crate::hir::nodes::HirModule::default();
    for module in graph.modules() {
        let lowered = crate::hir::lower::lower(&module.program, &resolver);
        merged_hir.items.extend(lowered.items);
        merged_hir.imports.extend(lowered.imports);
        merged_hir.classes.extend(lowered.classes);
        merged_hir.functions.extend(lowered.functions);
        merged_hir.interfaces.extend(lowered.interfaces);
    }

    let mut mir = crate::mir::build::build(&merged_hir);
    let _mono = crate::mir::monomorphize::monomorphize(&mut mir);
    let _opt = crate::mir::optimize::optimize(&mut mir);

    let jit_report = crate::codegen::cranelift::jit::execute(&mir, "main");

    let run_report = crate::runtime::runner::run_entry(&graph)?;

    println!(
        "JIT preparado para '{}': {} funcoes lowerizadas (modulos={}).",
        jit_report.entry_function,
        jit_report.compiled_functions,
        graph.module_count()
    );

    if run_report.lines_emitted == 0 {
        println!("Nenhuma saida de runtime detectada no fallback atual.");
    }

    Ok(())
}
