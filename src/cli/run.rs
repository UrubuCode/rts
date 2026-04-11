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
    source_stats: SourceStats,
    namespace_usage: Vec<NamespaceUsageEntry>,
    gc_stats: crate::namespaces::gc::arena::GcStats,
    value_store_stats: crate::namespaces::abi::ValueStoreStats,
    runtime_eval_warnings: usize,
}

/// Analise agregada do fonte apos lowering para HIR.
/// Usado pelo `--dump-statistics` para mostrar a "forma" do programa.
#[derive(Debug, Clone, Default)]
struct SourceStats {
    functions: usize,
    classes: usize,
    interfaces: usize,
    imports: usize,
}

/// Contabiliza quantos call sites usam cada namespace.
/// Derivado do merged HIR apos lowering.
#[derive(Debug, Clone)]
struct NamespaceUsageEntry {
    namespace: String,
    callee_count: usize,
}

pub fn command(input_arg: Option<String>, options: CompileOptions) -> Result<()> {
    command_with_watch(input_arg, options, false)
}

/// Entry-point de `rts run` com suporte opcional a `--watch`.
/// Quando `watch = true`, executa uma vez e registra watchers em todos os
/// arquivos do grafo. Re-executa ao detectar mudanca. Sai com Ctrl+C.
pub fn command_with_watch(
    input_arg: Option<String>,
    options: CompileOptions,
    watch: bool,
) -> Result<()> {
    let input = input_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/console.ts"));

    if watch {
        run_with_watcher(&input, options)
    } else {
        execute_file(&input, options)
    }
}

/// Loop de watch: executa o arquivo, registra watchers em todos os paths
/// do grafo de modulos, bloqueia ate receber evento de mudanca, re-executa.
/// Erros de compilacao nao interrompem o loop — o usuario pode corrigir
/// e salvar novamente.
fn run_with_watcher(input: &Path, options: CompileOptions) -> Result<()> {
    use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

    eprintln!("rts watch: iniciando execucao inicial de {}", input.display());

    // Primeira execucao — coletamos os paths do grafo mesmo se falhar.
    let _ = execute_file(input, options);

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .with_context(|| "failed to create file watcher")?;

    // Determina quais paths observar. Se a compilacao falhou, cai no entry file.
    let paths_to_watch = match crate::module::ModuleGraph::load(input, options) {
        Ok(graph) => graph.disk_paths(),
        Err(_) => vec![input.to_path_buf()],
    };

    for path in &paths_to_watch {
        if let Err(err) = watcher.watch(path, RecursiveMode::NonRecursive) {
            eprintln!(
                "rts watch: aviso — nao foi possivel observar {}: {err}",
                path.display()
            );
        }
    }
    eprintln!(
        "rts watch: observando {} arquivo(s). Ctrl+C para sair.",
        paths_to_watch.len()
    );

    // Loop principal. Agrupamos eventos com um pequeno debounce (300ms) porque
    // editores como VS Code disparam multiplos eventos (rename + create + write)
    // ao salvar um arquivo.
    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                if !is_meaningful_event(&event) {
                    continue;
                }
                // Drena eventos subsequentes durante a janela de debounce.
                loop {
                    match rx.recv_timeout(Duration::from_millis(300)) {
                        Ok(_) => continue,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }

                eprintln!("\nrts watch: mudanca detectada, re-executando...");
                let _ = execute_file(input, options);

                // Re-registra watchers caso o grafo tenha mudado (novo import).
                if let Ok(graph) = crate::module::ModuleGraph::load(input, options) {
                    let new_paths = graph.disk_paths();
                    // Remove watchers antigos que nao estao mais no grafo.
                    for path in &paths_to_watch {
                        if !new_paths.contains(path) {
                            let _ = watcher.unwatch(path);
                        }
                    }
                    // Adiciona novos.
                    for path in &new_paths {
                        if !paths_to_watch.contains(path) {
                            let _ = watcher.watch(path, RecursiveMode::NonRecursive);
                        }
                    }
                }
            }
            Ok(Err(err)) => {
                eprintln!("rts watch: erro do watcher: {err}");
            }
            Err(_) => return Ok(()),
        }
    }
}

fn is_meaningful_event(event: &notify::Event) -> bool {
    use notify::EventKind;
    matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    )
}

/// Executes a single file and returns Ok(()) or the first error.
/// Used by `rts test` to run individual test files.
pub fn execute_file(input: &Path, options: CompileOptions) -> Result<()> {
    let report = execute_with_report(input, options)?;
    if options.debug {
        print_debug_timeline(input, options, &report);
    }

    // Relatório do JIT só aparece em modo --debug (ver print_debug_timeline).
    // Sem --debug, não poluímos stdout: o programa do usuário é o único output.
    let _ = &report.jit_report;
    let _ = options;

    Ok(())
}

fn execute_with_report(input: &Path, options: CompileOptions) -> Result<RunExecutionReport> {
    crate::namespaces::rust::eval::set_metrics_enabled(options.debug);
    crate::namespaces::abi::set_dispatch_metrics_enabled(options.debug);

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

    // Coleta metricas do source antes do build (para nao contar functions
    // sinteticas adicionadas pelo build_typed).
    let source_stats = collect_source_stats(&merged_hir);
    let namespace_usage = collect_namespace_usage(&merged_hir);

    let started = Instant::now();
    let typed_mir = crate::mir::typed_build::typed_build(&merged_hir);
    stage_timings.build_mir_ms = elapsed_ms(started);

    // Apos o build, coleta contagem de warnings RuntimeEval emitidos.
    // O build ja emitiu warnings W001-W011 ao encontrar instrucoes
    // RuntimeEval — basta contar quantos warnings ha no engine global.
    let runtime_eval_warnings = crate::diagnostics::reporter::global_engine()
        .warnings_count();

    let started = Instant::now();
    let jit_report = crate::codegen::cranelift::jit::execute_typed(&typed_mir, "main")
        .context("failed to execute typed MIR through Cranelift JIT")?;
    stage_timings.jit_execute_ms = elapsed_ms(started);
    stage_timings.total_ms = elapsed_ms(total_started);

    let runtime_metrics = crate::namespaces::abi::runtime_metrics_snapshot();
    let gc_stats = crate::namespaces::gc::arena::stats();
    let value_store_stats = crate::namespaces::abi::value_store_stats();

    Ok(RunExecutionReport {
        module_count,
        jit_report,
        stage_timings,
        runtime_metrics,
        source_stats,
        namespace_usage,
        gc_stats,
        value_store_stats,
        runtime_eval_warnings,
    })
}

/// Agrega estatisticas do HIR merged: numero de items por tipo.
/// Conta RuntimeEval nao e feito aqui porque esse evento so e gerado
/// durante `typed_build`; usa-se `runtime_eval_warnings` do engine global.
fn collect_source_stats(hir: &crate::hir::nodes::HirModule) -> SourceStats {
    use crate::hir::nodes::HirItem;
    let mut stats = SourceStats::default();

    for item in &hir.items {
        match item {
            HirItem::Function(_) => stats.functions += 1,
            HirItem::Class(_) => stats.classes += 1,
            HirItem::Interface(_) => stats.interfaces += 1,
            HirItem::Import(_) => stats.imports += 1,
            HirItem::Statement(_) => {}
        }
    }

    // `functions` tambem inclui metodos de classe no merged_hir, mas como
    // cada metodo vira uma `HirFunction` separada durante o lower, contamos
    // apenas os items top-level. Para um numero mais preciso de "funcoes
    // totais no codegen" use `hir.functions.len()` — exposto separadamente.
    stats.functions = hir.functions.len();
    stats
}

/// Coleta callees por namespace varrendo o body das funcoes do HIR.
/// Como o HIR body ainda e `Vec<String>`, usamos um regex simples
/// procurando por `<ident>.<ident>(` no texto. Depois que a Etapa 6
/// quebrar o ciclo parse→string→reparse, isto pode ser feito em cima
/// do AST estruturado.
fn collect_namespace_usage(hir: &crate::hir::nodes::HirModule) -> Vec<NamespaceUsageEntry> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for function in &hir.functions {
        for stmt in &function.body {
            collect_namespace_calls_from_text(stmt, &mut counts);
        }
    }
    for item in &hir.items {
        if let crate::hir::nodes::HirItem::Statement(text) = item {
            collect_namespace_calls_from_text(text, &mut counts);
        }
    }

    counts
        .into_iter()
        .map(|(namespace, callee_count)| NamespaceUsageEntry {
            namespace,
            callee_count,
        })
        .collect()
}

/// Varre um trecho de texto procurando por `<ns>.<fn>(` — heuristica
/// leve que nao precisa de parser. Filtra por prefixos conhecidos de
/// namespace para reduzir falsos positivos (ex: `this.method()` nao conta).
fn collect_namespace_calls_from_text(
    text: &str,
    counts: &mut std::collections::BTreeMap<String, usize>,
) {
    let known_namespaces = crate::namespaces::namespace_names();
    for ns in known_namespaces {
        let needle = format!("{}.", ns);
        let mut idx = 0;
        while let Some(pos) = text[idx..].find(&needle) {
            let abs = idx + pos;
            // Check char before: must not be ident char (evitar `foo_io.print`)
            let prev_ok = abs == 0
                || !text.as_bytes()[abs - 1]
                    .is_ascii_alphanumeric()
                    && text.as_bytes()[abs - 1] != b'_';
            if prev_ok {
                *counts.entry(ns.to_string()).or_insert(0) += 1;
            }
            idx = abs + needle.len();
        }
    }
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
    println!("=== Analise de Fonte ===");
    println!("  input                {}", input.display());
    println!("  profile              {}", options.profile);
    println!("  frontend             {}", options.frontend_mode);
    println!("  modules              {}", report.module_count);
    println!(
        "  functions            {}",
        report.source_stats.functions
    );
    println!("  classes              {}", report.source_stats.classes);
    println!(
        "  interfaces           {}",
        report.source_stats.interfaces
    );
    println!("  imports              {}", report.source_stats.imports);
    if report.runtime_eval_warnings > 0 {
        println!(
            "  runtime_eval         {} (construcoes caindo em avaliacao dinamica)",
            report.runtime_eval_warnings
        );
    }
    println!();

    println!("=== Namespaces usados ===");
    if report.namespace_usage.is_empty() {
        println!("  (nenhum)");
    } else {
        for entry in &report.namespace_usage {
            println!(
                "  {:<20} {} callee(s)",
                entry.namespace, entry.callee_count
            );
        }
    }
    println!();

    println!("=== GC Arena ===");
    println!(
        "  allocated_bytes      {}",
        report.gc_stats.allocated_bytes
    );
    println!(
        "  generation           {} (collect_all count)",
        report.gc_stats.generation
    );
    println!("  live_slots           {}", report.gc_stats.live_slots);
    println!();

    println!("=== ValueStore (abi) ===");
    println!(
        "  values_len           {} (slots do Vec<RuntimeValue>)",
        report.value_store_stats.values_len
    );
    println!(
        "  bindings_len         {} (bindings nomeados registrados)",
        report.value_store_stats.bindings_len
    );
    println!();

    println!("=== Timeline (ms) ===");

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
