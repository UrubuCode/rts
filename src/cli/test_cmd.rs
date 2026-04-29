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

    let options = CompileOptions::default();
    let mut total_files = 0usize;
    let mut failed_files = 0usize;
    let mut grand_passed = 0usize;
    let mut grand_failed = 0usize;

    for file in &files {
        total_files += 1;

        // Print file header (relative path when possible).
        let label = relative_label(file, &root);
        eprintln!("\n{}", dim(&label));

        runner::reset_runner();
        // Reseta state global entre fixtures: erro slot + stack depth.
        // Sem isso fixtures que comecam erro pendente ou depth acumulada
        // de fixture anterior batem em RangeError espurio.
        crate::namespaces::gc::error::__RTS_FN_RT_ERROR_CLEAR();
        crate::namespaces::gc::stack::reset_stack_depth();

        let run_result = crate::pipeline::run_jit_with_imports(file, options);

        // Auto-print per-file summary (calls the same Rust fn the TS
        // `printSummary()` export delegates to, so output is identical).
        // SAFETY: extern "C" Rust function with no invariants.
        runner::__RTS_FN_NS_TEST_CORE_PRINT_SUMMARY();

        let file_passed = runner::runner_passed();
        let file_failed = runner::runner_failed();
        grand_passed += file_passed;
        grand_failed += file_failed;

        if run_result.is_err() || file_failed > 0 {
            failed_files += 1;
            if let Err(ref e) = run_result {
                let use_color = crate::diagnostics::reporter::stderr_supports_color();
                let engine = crate::diagnostics::reporter::global_engine();
                let msg = if engine.has_errors() {
                    engine.render_all(use_color)
                } else {
                    crate::diagnostics::reporter::format_anyhow_error(e, use_color)
                };
                // Indent each line by 2 spaces to align with test output
                for line in msg.lines() {
                    eprintln!("  {line}");
                }
            }
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
