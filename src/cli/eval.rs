use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(source_arg: Option<String>, options: CompileOptions) -> Result<()> {
    crate::namespaces::rust::eval::set_metrics_enabled(options.debug);

    let source = source_arg
        .ok_or_else(|| anyhow!("missing source for '-e/--eval'"))?
        .trim()
        .to_string();
    if source.is_empty() {
        return Err(anyhow!("inline source for '-e/--eval' cannot be empty"));
    }

    let program = crate::parser::parse_source_with_mode(&source, options.frontend_mode)
        .context("failed to parse inline source")?;

    let mut registry = crate::type_system::TypeRegistry::default();
    let imports = crate::type_system::checker::ImportExports::default();
    crate::type_system::checker::check_program(&program, &mut registry, &imports)
        .context("type check failed for inline source")?;

    let resolver = crate::type_system::resolver::TypeResolver::from_registry(&registry);
    let lowered = crate::hir::lower::lower(&program, &resolver);

    let mir = crate::mir::build::build(&lowered);

    let jit_report = crate::codegen::cranelift::jit::execute(&mir, "main")
        .context("failed to execute inline eval through Cranelift JIT")?;

    if jit_report.executed {
        println!(
            "EVAL executou '{}': {} funcoes lowerizadas, retorno={} (profile={}).",
            jit_report.entry_function,
            jit_report.compiled_functions,
            jit_report.entry_return_value,
            options.profile
        );
    } else {
        println!(
            "EVAL compilou {} funcoes, mas a entry '{}' nao foi encontrada (profile={}).",
            jit_report.compiled_functions, jit_report.entry_function, options.profile
        );
    }

    Ok(())
}
