use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::codegen::cranelift::jit::JitReport;
use crate::compile_options::CompileOptions;
use crate::namespaces::abi::RuntimeMetricsSnapshot;

#[derive(Debug, Clone, Default)]
struct RunStageTimings {
    graph_load_ms: f64,
    collect_modules_ms: f64,
    build_registry_ms: f64,
    build_resolver_ms: f64,
    lower_modules_ms: f64,
    merge_hir_ms: f64,
    build_mir_ms: f64,
    jit_execute_ms: f64,
    total_ms: f64,
}

#[derive(Debug, Clone)]
struct RunExecutionReport {
    module_count: usize,
    jit_report: JitReport,
    stage_timings: RunStageTimings,
    runtime_metrics: RuntimeMetricsSnapshot,
}

pub fn command(input_arg: Option<String>, options: CompileOptions) -> Result<()> {
    let input = input_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/console.ts"));

    let report = execute_with_report(&input, options)?;
    if options.debug {
        print_debug_timeline(&input, options, &report);
    }

    let jit_report = &report.jit_report;

    if jit_report.executed {
        println!(
            "JIT executou '{}': {} funcoes lowerizadas, retorno={} (profile={}, modulos={}).",
            jit_report.entry_function,
            jit_report.compiled_functions,
            jit_report.entry_return_value,
            options.profile,
            report.module_count
        );
    } else {
        println!(
            "JIT compilou {} funcoes, mas a entry '{}' nao foi encontrada (profile={}, modulos={}).",
            jit_report.compiled_functions,
            jit_report.entry_function,
            options.profile,
            report.module_count
        );
    }

    Ok(())
}

fn execute_with_report(input: &Path, options: CompileOptions) -> Result<RunExecutionReport> {
    crate::namespaces::rust::eval::set_metrics_enabled(options.debug);

    let total_started = Instant::now();
    let mut stage_timings = RunStageTimings::default();

    let started = Instant::now();
    let graph = crate::module::ModuleGraph::load(input, options)
        .with_context(|| format!("failed to load module graph from {}", input.display()))?;
    stage_timings.graph_load_ms = elapsed_ms(started);
    let module_count = graph.module_count();

    let started = Instant::now();
    let modules = graph.modules().collect::<Vec<_>>();
    stage_timings.collect_modules_ms = elapsed_ms(started);

    let started = Instant::now();
    let registry = crate::pipeline::build_registry_for_graph(&graph)
        .with_context(|| format!("type check failed for graph rooted at {}", input.display()))?;
    stage_timings.build_registry_ms = elapsed_ms(started);

    let started = Instant::now();
    let resolver = crate::type_system::resolver::TypeResolver::from_registry(&registry);
    stage_timings.build_resolver_ms = elapsed_ms(started);

    let started = Instant::now();
    let lowered_modules = modules
        .par_iter()
        .map(|module| crate::hir::lower::lower(&module.program, &resolver))
        .collect::<Vec<_>>();
    stage_timings.lower_modules_ms = elapsed_ms(started);

    let started = Instant::now();
    let mut merged_hir = crate::hir::nodes::HirModule::default();
    for lowered in lowered_modules {
        merged_hir.items.extend(lowered.items);
        merged_hir.imports.extend(lowered.imports);
        merged_hir.classes.extend(lowered.classes);
        merged_hir.functions.extend(lowered.functions);
        merged_hir.interfaces.extend(lowered.interfaces);
    }
    stage_timings.merge_hir_ms = elapsed_ms(started);

    let started = Instant::now();
    let typed_mir = crate::mir::typed_build::typed_build(&merged_hir);
    stage_timings.build_mir_ms = elapsed_ms(started);

    let started = Instant::now();
    let jit_report = crate::codegen::cranelift::jit::execute_typed(&typed_mir, "main")
        .context("failed to execute typed MIR through Cranelift JIT")?;
    stage_timings.jit_execute_ms = elapsed_ms(started);
    stage_timings.total_ms = elapsed_ms(total_started);

    let runtime_metrics = crate::namespaces::abi::runtime_metrics_snapshot();

    Ok(RunExecutionReport {
        module_count,
        jit_report,
        stage_timings,
        runtime_metrics,
    })
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn nanos_to_ms(nanos: u128) -> f64 {
    nanos as f64 / 1_000_000.0
}

fn avg_ms(nanos: u128, calls: u64) -> f64 {
    if calls == 0 {
        return 0.0;
    }
    nanos as f64 / calls as f64 / 1_000_000.0
}

fn print_stage_row(label: &str, elapsed_ms: f64) {
    println!("  {:<32} {:>10.3}", label, elapsed_ms);
}

fn print_runtime_row(label: &str, calls: u64, nanos: u128) {
    println!(
        "  {:<32} {:>10.3} total | {:>8} calls | {:>10.6} avg",
        label,
        nanos_to_ms(nanos),
        calls,
        avg_ms(nanos, calls)
    );
}

fn print_runtime_counter_row(label: &str, value: u64) {
    println!("  {:<32} {:>10}", label, value);
}

fn print_debug_timeline(input: &Path, options: CompileOptions, report: &RunExecutionReport) {
    println!("launcher --debug timeline (ms)");
    println!(
        "  input={} | profile={} | frontend={} | modules={}",
        input.display(),
        options.profile,
        options.frontend_mode,
        report.module_count
    );

    let stages = &report.stage_timings;
    print_stage_row("rust.graph.load", stages.graph_load_ms);
    print_stage_row("rust.modules.collect", stages.collect_modules_ms);
    print_stage_row("rust.registry.build", stages.build_registry_ms);
    print_stage_row("rust.resolver.build", stages.build_resolver_ms);
    print_stage_row("rust.hir.lower", stages.lower_modules_ms);
    print_stage_row("rust.hir.merge", stages.merge_hir_ms);
    print_stage_row("rust.mir.build", stages.build_mir_ms);
    print_stage_row("rust.jit.execute", stages.jit_execute_ms);

    let jit = &report.jit_report.timings;
    print_stage_row("jit.initialize", jit.initialize_jit_ms);
    print_stage_row("jit.declare_functions", jit.declare_functions_ms);
    print_stage_row("jit.scan_synthetic", jit.scan_synthetic_calls_ms);
    print_stage_row("jit.declare_helpers", jit.declare_helpers_ms);
    print_stage_row("jit.define_functions", jit.define_functions_ms);
    print_stage_row("jit.define_stubs", jit.define_stubs_ms);
    print_stage_row("jit.finalize", jit.finalize_ms);
    print_stage_row("jit.resolve_entry", jit.resolve_entry_ms);
    print_stage_row("jit.execute_entry", jit.execute_entry_ms);
    print_stage_row("jit.total", jit.total_ms);
    print_stage_row("launcher.total", stages.total_ms);

    let runtime = &report.runtime_metrics;
    print_runtime_row(
        "runtime.__rts_dispatch",
        runtime.dispatch_calls,
        runtime.dispatch_nanos,
    );
    print_runtime_row(
        "runtime.fn_eval_expr",
        runtime.eval_expr_calls,
        runtime.eval_expr_nanos,
    );
    print_runtime_row(
        "runtime.fn_eval_stmt",
        runtime.eval_stmt_calls,
        runtime.eval_stmt_nanos,
    );
    print_runtime_row(
        "runtime.eval.parse",
        runtime.eval_parse_calls,
        runtime.eval_parse_nanos,
    );
    print_runtime_counter_row(
        "runtime.eval.identifier_reads",
        runtime.eval_identifier_reads,
    );
    print_runtime_counter_row(
        "runtime.eval.identifier_writes",
        runtime.eval_identifier_writes,
    );
    print_runtime_counter_row("runtime.eval.call_dispatches", runtime.eval_call_dispatches);
    print_runtime_counter_row(
        "runtime.eval.binding_cache_hits",
        runtime.eval_binding_cache_hits,
    );
    print_runtime_counter_row(
        "runtime.eval.binding_cache_misses",
        runtime.eval_binding_cache_misses,
    );
    print_runtime_row(
        "runtime.__rts_call_dispatch",
        runtime.call_dispatch_calls,
        runtime.call_dispatch_nanos,
    );
}

#[cfg(test)]
mod tests {
    use super::{RunExecutionReport, execute_with_report};
    use std::path::PathBuf;

    use crate::compile_options::CompileOptions;

    fn fixture_rts_simple() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bench")
            .join("rts_simple.ts")
    }

    fn run_fixture_report() -> RunExecutionReport {
        let fixture = fixture_rts_simple();
        assert!(fixture.exists(), "missing fixture: {}", fixture.display());

        execute_with_report(
            &fixture,
            CompileOptions {
                debug: true,
                ..CompileOptions::default()
            },
        )
        .expect("run pipeline should execute rts_simple.ts")
    }

    #[test]
    fn run_collects_stage_and_jit_timings_for_rts_simple() {
        let report = run_fixture_report();
        assert!(report.jit_report.executed);
        assert_eq!(report.jit_report.entry_function, "main");

        let stage_values = [
            report.stage_timings.graph_load_ms,
            report.stage_timings.collect_modules_ms,
            report.stage_timings.build_registry_ms,
            report.stage_timings.build_resolver_ms,
            report.stage_timings.lower_modules_ms,
            report.stage_timings.merge_hir_ms,
            report.stage_timings.build_mir_ms,
            report.stage_timings.jit_execute_ms,
            report.stage_timings.total_ms,
        ];
        for value in stage_values {
            assert!(value.is_finite());
            assert!(value >= 0.0);
        }

        let jit = &report.jit_report.timings;
        let jit_values = [
            jit.initialize_jit_ms,
            jit.declare_functions_ms,
            jit.scan_synthetic_calls_ms,
            jit.declare_helpers_ms,
            jit.define_functions_ms,
            jit.define_stubs_ms,
            jit.finalize_ms,
            jit.resolve_entry_ms,
            jit.execute_entry_ms,
            jit.total_ms,
        ];
        for value in jit_values {
            assert!(value.is_finite());
            assert!(value >= 0.0);
        }
    }

    #[test]
    fn run_collects_runtime_dispatch_metrics_for_rts_simple() {
        let report = run_fixture_report();
        let runtime = report.runtime_metrics;

        // O pipeline tipado compila o bench para código nativo via Cranelift,
        // portanto ainda há chamadas de __rts_dispatch (bind/read de globais,
        // box/unbox de números, etc.) mas eval_stmt/eval_expr/call_dispatch
        // ficam zerados — esses só sobem quando o fallback interpretativo
        // é acionado, o que este benchmark não exercita.
        assert!(runtime.dispatch_calls > 0);
    }
}
