use std::path::PathBuf;

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::compile_options::CompileOptions;

pub fn command(input_arg: Option<String>, options: CompileOptions) -> Result<()> {
    let input = input_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/console.ts"));

    let graph = crate::module_system::ModuleGraph::load(&input, options)
        .with_context(|| format!("failed to load module graph from {}", input.display()))?;

    let modules = graph.modules().collect::<Vec<_>>();
    let registry = crate::build_registry_for_graph(&graph)
        .with_context(|| format!("type check failed for graph rooted at {}", input.display()))?;

    let resolver = crate::type_system::resolver::TypeResolver::from_registry(&registry);

    let lowered_modules = modules
        .par_iter()
        .map(|module| {
            let mut lowered = crate::hir::lower::lower(&module.program, &resolver);
            let _hir_opt = crate::hir::optimize::optimize(&mut lowered);
            lowered
        })
        .collect::<Vec<_>>();

    let mut merged_hir = crate::hir::nodes::HirModule::default();
    for lowered in lowered_modules {
        merged_hir.items.extend(lowered.items);
        merged_hir.imports.extend(lowered.imports);
        merged_hir.classes.extend(lowered.classes);
        merged_hir.functions.extend(lowered.functions);
        merged_hir.interfaces.extend(lowered.interfaces);
    }

    let mut mir = crate::mir::build::build(&merged_hir);
    let _mono = crate::mir::monomorphize::monomorphize(&mut mir);
    let _opt = crate::mir::optimize::optimize(&mut mir);

    let jit_report = crate::codegen::cranelift::jit::execute(&mir, "main")
        .context("failed to execute MIR through Cranelift JIT")?;

    let run_report = crate::runtime::runner::run_entry(&graph, options)?;

    if jit_report.executed {
        println!(
            "JIT executou '{}': {} funcoes lowerizadas, retorno={} (profile={}, modulos={}).",
            jit_report.entry_function,
            jit_report.compiled_functions,
            jit_report.entry_return_value,
            options.profile,
            graph.module_count()
        );
    } else {
        println!(
            "JIT compilou {} funcoes, mas a entry '{}' nao foi encontrada (profile={}, modulos={}).",
            jit_report.compiled_functions,
            jit_report.entry_function,
            options.profile,
            graph.module_count()
        );
    }

    if run_report.lines_emitted == 0 {
        println!("Nenhuma saida de runtime detectada no fallback atual.");
    }

    Ok(())
}
