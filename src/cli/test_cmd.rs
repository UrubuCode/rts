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
use crate::namespaces::test::runner;

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

    for file in &files {
        total_files += 1;
        let label = relative_label(file, &root);
        eprintln!("\n{}", dim(&label));

        let output = std::process::Command::new(&exe)
            .arg("test")
            .arg(file)
            .output();
        let output = match output {
            Ok(o) => o,
            Err(e) => {
                eprintln!("  failed to spawn child: {e}");
                failed_files += 1;
                grand_failed += 1;
                continue;
            }
        };

        // Re-emit child stderr (where rts test imprime tudo) sem o
        // header de arquivo — ja' imprimimos acima. Pulamos a primeira
        // linha em branco + header pra evitar duplicar.
        let stderr = String::from_utf8_lossy(&output.stderr);
        emit_child_output(&stderr, &label);

        // Parseia linha de resumo: " X tests passed" / " Y tests failed".
        let (file_passed, file_failed) = parse_summary_counts(&stderr);
        grand_passed += file_passed;
        grand_failed += file_failed;

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
            eprintln!("  {} {}", red("✗"), red(&format!("crashed (exit {code})")));
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
        eprintln!();
    }

    if failed_files > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn run_single_in_process(file: &Path, root: &Path) -> Result<()> {
    let label = relative_label(file, root);
    eprintln!("\n{}", dim(&label));

    runner::reset_runner();
    crate::namespaces::gc::error::__RTS_FN_RT_ERROR_CLEAR();
    crate::namespaces::gc::stack::reset_stack_depth();

    let options = CompileOptions::default();
    let run_result = crate::pipeline::run_jit_with_imports(file, options);

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
