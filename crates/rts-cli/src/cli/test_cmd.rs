//! `rts test [path]` — discover and run `.test.ts` / `.spec.ts` files.
//!
//! Each test file is compiled and executed via Cranelift JIT with the full
//! module graph resolved (supports `import { describe, test } from "rts:test"`).
//! The Rust-side test runner in `namespaces::test::runner` tracks pass/fail
//! counts and prints ANSI output; this command resets state between files,
//! auto-prints per-file summaries, and exits non-zero if any tests failed.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::compile_options::CompileOptions;
use crate::namespaces::test as runner;

pub fn command(path: Option<String>) -> Result<()> {
    let root = match path {
        Some(ref p) => PathBuf::from(p),
        None => std::env::current_dir()?,
    };

    let files = if root.is_file() {
        vec![root.clone()]
    } else {
        discover_test_files(&root)
    };

    if files.is_empty() {
        eprintln!("no test files found in {}", root.display());
        return Ok(());
    }

    // Single-file mode: roda in-process. Tambem e' o modo usado pelo
    // pai ao spawnar `rts test <file>` para isolar fixtures que
    // segfault/OOM (#314) — assim um crash em um arquivo nao aborta a
    // suite inteira.
    if files.len() == 1 {
        return run_single_in_process(&files[0], &root);
    }

    // Multi-file: isola cada fixture em subprocess. Crash (segfault,
    // OOM, panic na thread main) vira "1 failed file" e o runner segue
    // para o proximo arquivo. Ver #314.
    let mut total_files = 0usize;
    let mut failed_files = 0usize;
    let mut grand_passed = 0usize;
    let mut grand_failed = 0usize;
    let exe = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("failed to locate rts executable: {e}"))?;

    // Run the per-file children CONCURRENTLY. They are already isolated
    // processes (#314 — a segfaulting fixture must not abort the suite), so
    // running several at once changes no semantics; it only stops leaving every
    // core but one idle. Output is buffered per file and emitted below in the
    // ORIGINAL file order, so the report is identical to the serial one.
    //
    // `RTS_TEST_JOBS` overrides the worker count (`1` = the old serial
    // behavior, for a fixture whose timing goes flaky under load).
    let jobs = test_jobs(files.len());
    let mut outputs = run_children_parallel(&files, &exe, jobs);
    // SERIAL RETRY. A fixture that is timing-sensitive (timers, async settle
    // ordering) can fail purely because N children were competing for the CPU —
    // `promise_rejection_basic` did exactly that. Re-run every failing file
    // ALONE and keep the retry's result. A genuinely broken test fails both
    // times, so this removes load-induced noise WITHOUT hiding a real failure;
    // the files that needed a retry are named in the summary so the flakiness
    // stays visible instead of being silently laundered.
    let flaky = retry_failures_serially(&files, &exe, &mut outputs);

    let mut timed_out_files: Vec<PathBuf> = Vec::new();

    for (file, run) in files.iter().zip(outputs) {
        total_files += 1;
        let label = relative_label(file, &root);
        eprintln!("\n{}", dim(&label));

        let timed_out = run.timed_out;
        let output = match run.output {
            Ok(o) => o,
            Err(e) => {
                eprintln!("  failed to spawn child: {e}");
                failed_files += 1;
                grand_failed += 1;
                continue;
            }
        };
        if timed_out {
            timed_out_files.push(file.clone());
        }

        // Re-emit child stderr (onde vai o output do rts test e' panics
        // do Rust). Sem o header de arquivo — ja' imprimimos acima.
        let stderr = String::from_utf8_lossy(&output.stderr);
        emit_child_output(&stderr, &label);

        // Parseia linha de resumo: " X tests passed" / " Y tests failed".
        let (file_passed, file_failed) = parse_summary_counts(&stderr);
        grand_passed += file_passed;
        grand_failed += file_failed;

        // A HUNG file counts as failed regardless of what it managed to print
        // before the kill — a test that never finishes has not passed.
        if timed_out {
            grand_failed += 1;
            failed_files += 1;
            eprintln!("  {} {}", red("✗"), red("timed out — killed"));
            continue;
        }

        let crashed = !output.status.success() && file_failed == 0 && file_passed == 0;
        if crashed {
            // Crash sem nenhum teste reportado — conta como 1 failed.
            grand_failed += 1;
            failed_files += 1;
            let code = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            // Emite stdout do filho (prints do user TS antes do crash)
            // e stderr completo pra dar contexto. stderr ja' foi
            // re-emitido acima — aqui so' o stdout pra nao duplicar.
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                eprintln!("  {} child stdout (last output before crash):", dim("│"));
                for line in stdout.lines() {
                    eprintln!("  {} {line}", dim("│"));
                }
            }
            eprintln!(
                "  {} {} — codigo {} (Windows: -1073741819=ACCESS_VIOLATION, -1073740791=HEAP_CORRUPTION)",
                red("✗"),
                red(&format!("crashed")),
                code,
            );
        } else if !output.status.success() || file_failed > 0 {
            failed_files += 1;
        }
    }

    // Grand summary when more than one file ran.
    if total_files > 1 {
        let sep = format!("\x1b[2m{}\x1b[0m", "─".repeat(40));
        eprintln!("\n{sep}");
        eprintln!(" Files  {} passed, {} failed, {} total",
            grand_passed_label(failed_files == 0, total_files - failed_files),
            failed_label(failed_files),
            total_files,
        );
        let total_tests = grand_passed + grand_failed;
        eprintln!(" Tests  {} passed, {} failed, {} total",
            grand_passed_label(grand_failed == 0, grand_passed),
            failed_label(grand_failed),
            total_tests,
        );
        // Name the files that only passed once re-run alone: they are counted as
        // passing (they do pass), but their result depended on machine load, and
        // that is a fact about the suite worth seeing rather than swallowing.
        // A hang is the failure mode most worth naming: it is invisible in a
        // pass/fail count and it used to stall the entire run.
        if !timed_out_files.is_empty() {
            eprintln!(
                " Hung   {} killed on timeout: {}",
                timed_out_files.len(),
                timed_out_files
                    .iter()
                    .map(|f| relative_label(f, &root))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !flaky.is_empty() {
            eprintln!(
                " Flaky  {} passed only on serial retry: {}",
                flaky.len(),
                flaky
                    .iter()
                    .map(|f| relative_label(f, &root))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        eprintln!();
    }

    if failed_files > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Worker count for the per-file child processes. `RTS_TEST_JOBS` wins when set
/// (`1` restores the fully serial behavior); otherwise one per available core,
/// never more than the number of files.
fn test_jobs(file_count: usize) -> usize {
    let requested = std::env::var("RTS_TEST_JOBS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        });
    requested.max(1).min(file_count.max(1))
}

/// Spawn `rts test <file>` for every file across `jobs` workers, returning each
/// child's captured output INDEXED BY THE INPUT ORDER (slot `i` belongs to
/// `files[i]` no matter which worker ran it, so the caller's report order does
/// not depend on scheduling).
fn run_children_parallel(
    files: &[PathBuf],
    exe: &Path,
    jobs: usize,
) -> Vec<ChildRun> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    // One slot per file. `Mutex<Vec<Option<_>>>` rather than channels so a
    // worker writes straight into its file's slot — no reordering step.
    let slots: Mutex<Vec<Option<ChildRun>>> =
        Mutex::new((0..files.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    // Inherited by every child: `RUST_BACKTRACE` gives a native backtrace on a
    // codegen/runtime crash (without it a segfault is a bare exit code), and
    // `RTS_TEST_TRACE_STAGES` prints `[trace] invoking __RTS_MAIN` so a crash
    // says exactly where it died (#314). A user-set RUST_BACKTRACE is kept.
    let backtrace = std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "1".to_string());
    let timeout = child_timeout();

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(i) else {
                    break;
                };
                let run = run_child(exe, file, &backtrace, timeout);
                slots.lock().expect("test slots mutex").ordered_put(i, run);
            });
        }
    });

    slots
        .into_inner()
        .expect("test slots mutex")
        .into_iter()
        .map(|slot| slot.expect("every slot filled: one worker per index"))
        .collect()
}

/// One child run: its captured output, plus whether we had to KILL it for
/// exceeding the deadline. The flag is carried separately because a hung child
/// may already have printed passing tests before wedging — judging it by its
/// output alone would count a hang as a pass.
struct ChildRun {
    output: std::io::Result<std::process::Output>,
    timed_out: bool,
}

/// Per-file wall-clock budget for a child. `RTS_TEST_TIMEOUT` (seconds)
/// overrides; `0` disables the deadline entirely.
///
/// A hung child used to stall the WHOLE suite forever: the runner blocked in
/// `.output()`, which waits for EOF on the child's pipes, and a wedged child
/// never closes them. That is not hypothetical — it stalled a full-suite run on
/// `node_child_process_full` and another on `node_util_parseargs`. A test that
/// hangs is a FAILING test, and the suite must say so and move on.
fn child_timeout() -> Option<std::time::Duration> {
    let secs = std::env::var("RTS_TEST_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(120);
    (secs > 0).then(|| std::time::Duration::from_secs(secs))
}

/// Run `rts test <file>` as a child, killing it if it outruns `timeout`.
///
/// The pipes are drained by their own threads so a child that fills a pipe
/// buffer cannot deadlock against us while we poll for exit (the classic
/// `wait()`-before-draining hang), and so we still recover whatever it printed
/// before being killed.
fn run_child(
    exe: &Path,
    file: &Path,
    backtrace: &str,
    timeout: Option<std::time::Duration>,
) -> ChildRun {
    use std::io::Read;
    use std::process::Stdio;

    let spawned = std::process::Command::new(exe)
        .arg("test")
        .arg(file)
        .env("RUST_BACKTRACE", backtrace)
        .env("RTS_TEST_TRACE_STAGES", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            return ChildRun {
                output: Err(e),
                timed_out: false,
            }
        }
    };

    let mut out_pipe = child.stdout.take().expect("stdout piped");
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = timeout.map(|t| std::time::Instant::now() + t);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Err(e) => {
                return ChildRun {
                    output: Err(e),
                    timed_out: false,
                }
            }
            Ok(None) => {}
        }
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            let _ = child.kill();
            timed_out = true;
            // The kill closes the pipes, so the readers finish and `wait`
            // reaps immediately.
            match child.wait() {
                Ok(status) => break status,
                Err(e) => {
                    return ChildRun {
                        output: Err(e),
                        timed_out: true,
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };

    let stdout = out_reader.join().unwrap_or_default();
    let mut stderr = err_reader.join().unwrap_or_default();
    if timed_out {
        let secs = timeout.map(|t| t.as_secs()).unwrap_or(0);
        stderr.extend_from_slice(
            format!("\n  timed out after {secs}s — killed (RTS_TEST_TIMEOUT to change)\n")
                .as_bytes(),
        );
    }

    ChildRun {
        output: Ok(std::process::Output {
            status,
            stdout,
            stderr,
        }),
        timed_out,
    }
}

/// Re-run every file whose parallel result reported a failure or a crash, ALONE
/// and serially, replacing its output with the retry's. Returns the labels of
/// the files that FAILED in parallel but PASSED alone — i.e. the ones whose
/// result depended on machine load, which the summary names so the flakiness is
/// reported rather than hidden.
fn retry_failures_serially(files: &[PathBuf], exe: &Path, outputs: &mut [ChildRun]) -> Vec<PathBuf> {
    let backtrace = std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "1".to_string());
    let timeout = child_timeout();
    let mut flaky = Vec::new();
    for (i, file) in files.iter().enumerate() {
        if !outputs[i].failed() {
            continue;
        }
        let retry = run_child(exe, file, &backtrace, timeout);
        if !retry.failed() {
            flaky.push(file.clone());
        }
        outputs[i] = retry;
    }
    flaky
}

impl ChildRun {
    /// Did this child report a failing test, get killed for hanging, or die
    /// without reporting anything?
    ///
    /// The `timed_out` check comes FIRST and is independent of the output: a
    /// child killed mid-run may already have printed `N tests passed` for the
    /// cases it got through, and judging by the summary alone would score a
    /// hang as a pass.
    fn failed(&self) -> bool {
        if self.timed_out {
            return true;
        }
        match &self.output {
            Err(_) => true,
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let (passed, failed) = parse_summary_counts(&stderr);
                failed > 0 || (!o.status.success() && passed == 0)
            }
        }
    }
}

/// Tiny helper so the worker body stays one line (and the lock is dropped
/// immediately after the write rather than held across the next spawn).
trait OrderedPut<T> {
    fn ordered_put(&mut self, index: usize, value: T);
}

impl<T> OrderedPut<T> for Vec<Option<T>> {
    fn ordered_put(&mut self, index: usize, value: T) {
        self[index] = Some(value);
    }
}

fn run_single_in_process(file: &Path, root: &Path) -> Result<()> {
    let label = relative_label(file, root);
    eprintln!("\n{}", dim(&label));

    runner::reset_runner();
    crate::namespaces::gc::error::__RTS_FN_RT_ERROR_CLEAR();
    crate::namespaces::gc::stack::reset_stack_depth();
    // Drain the engine's process-global tables before compiling the next file.
    // This is the quiescent top-level boundary `reset_codegen_state` documents:
    // the previous file's program has finished (its objects are dead) and the next
    // has not compiled, so clearing the shape registry + side-tables is safe and
    // bounds their growth across a whole multi-file suite run.
    rts_codegen_new::state::reset_codegen_state();

    // Cutover: run the test file through the NEW engine (resolves the import graph
    // + the `rts:test` prelude, executes via JIT). The Rust-side runner tracks
    // pass/fail and prints the summary below.
    let run_result = rts_codegen_new::front::run::run_path(file).map_err(|e| anyhow::anyhow!("{e}"));

    runner::__RTS_FN_NS_TEST_CORE_PRINT_SUMMARY();

    let file_failed = runner::runner_failed();

    if run_result.is_err() || file_failed > 0 {
        if let Err(ref e) = run_result {
            let use_color = crate::diagnostics::reporter::stderr_supports_color();
            let engine = crate::diagnostics::reporter::global_engine();
            let msg = if engine.has_errors() {
                engine.render_all(use_color)
            } else {
                crate::diagnostics::reporter::format_anyhow_error(e, use_color)
            };
            for line in msg.lines() {
                eprintln!("  {line}");
            }
        }
        std::process::exit(1);
    }
    Ok(())
}

/// Re-imprime output do child suprimindo a primeira linha em branco e
/// o header "<label>" que ele imprimiu — o pai ja' escreveu o cabecalho.
fn emit_child_output(stderr: &str, label: &str) {
    let mut lines = stderr.lines();
    // Pula primeira linha em branco e o header se baterem.
    let mut peeked: Vec<&str> = Vec::with_capacity(2);
    for _ in 0..2 {
        if let Some(l) = lines.next() {
            peeked.push(l);
        }
    }
    let label_line_re = format!("\x1b[2m{label}\x1b[0m");
    if peeked.first().map(|l| l.is_empty()).unwrap_or(false)
        && peeked.get(1).map(|l| *l == label_line_re).unwrap_or(false)
    {
        // header ja' impresso pelo pai
    } else {
        for l in peeked {
            eprintln!("{l}");
        }
    }
    for l in lines {
        eprintln!("{l}");
    }
}

/// Extrai contadores de "X test(s) passed" / "Y test(s) failed" do output.
fn parse_summary_counts(text: &str) -> (usize, usize) {
    let mut passed = 0usize;
    let mut failed = 0usize;
    for line in text.lines() {
        // Formato emitido pelo runner: "  ✓ <N> test(s) passed",
        // "  ✗ <N> test(s) failed". Codigos ANSI sao removidos antes.
        let plain = strip_ansi(line);
        if let Some(n) = find_count_with_suffix(&plain, "passed") {
            passed = n;
        }
        if let Some(n) = find_count_with_suffix(&plain, "failed") {
            failed = n;
        }
    }
    (passed, failed)
}

/// Procura padrao "<N> test(s) <suffix>" em qualquer posicao da linha.
fn find_count_with_suffix(line: &str, suffix: &str) -> Option<usize> {
    let needle_pl = format!(" tests {suffix}");
    let needle_sg = format!(" test {suffix}");
    let (pos, key_len) = if let Some(p) = line.find(&needle_pl) {
        (p, needle_pl.len())
    } else if let Some(p) = line.find(&needle_sg) {
        (p, needle_sg.len())
    } else {
        return None;
    };
    // Sufixo deve terminar na linha (ou seguido de whitespace), pra
    // nao casar "passed_files" hipoteticos.
    let end = pos + key_len;
    if end != line.len() && !line.as_bytes()[end].is_ascii_whitespace() {
        return None;
    }
    let prefix = &line[..pos];
    let last_token = prefix.rsplit_terminator(|c: char| !c.is_ascii_digit()).next()?;
    last_token.parse::<usize>().ok()
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < bytes.len() && !(bytes[i] as char).is_alphabetic() {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn discover_test_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_dir(root, &mut files);
    files.sort();
    files
}

fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if name_str == "node_modules" || name_str.starts_with('.') {
                continue;
            }
            walk_dir(&path, out);
        } else if name_str.ends_with(".test.ts") || name_str.ends_with(".spec.ts") {
            out.push(path);
        }
    }
}

fn relative_label(file: &Path, root: &Path) -> String {
    if root.is_file() {
        return file.display().to_string();
    }
    file.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| file.display().to_string())
}

fn dim(s: &str) -> String {
    format!("\x1b[2m{s}\x1b[0m")
}

fn green(s: &str) -> String {
    format!("\x1b[32m{s}\x1b[0m")
}

fn red(s: &str) -> String {
    format!("\x1b[31m{s}\x1b[0m")
}

fn grand_passed_label(all_ok: bool, n: usize) -> String {
    if all_ok {
        green(&n.to_string())
    } else {
        n.to_string()
    }
}

fn failed_label(n: usize) -> String {
    if n > 0 { red(&n.to_string()) } else { n.to_string() }
}
