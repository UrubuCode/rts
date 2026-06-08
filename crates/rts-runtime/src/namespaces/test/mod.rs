//! `test_core` namespace — low-level test runner primitives. The high-level
//! API lives in `bundle.ts` (`rts:test`).
//!
//! `reset_runner` / `runner_failed` / `runner_passed` are pub (driven by the
//! `rts test` command); `BUNDLE_TS` is the embedded high-level harness.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::cell::RefCell;

use rts_macro::rts_namespace;

pub const BUNDLE_TS: &str = include_str!("bundle.ts");

thread_local! {
    static RUNNER: RefCell<TestRunner> = RefCell::new(TestRunner::new());
}

struct TestRunner {
    suite_stack: Vec<String>,
    case_name: Option<String>,
    case_failed: bool,
    passed: usize,
    failed: usize,
}

impl TestRunner {
    fn new() -> Self {
        eprintln!();
        Self {
            suite_stack: Vec::new(),
            case_name: None,
            case_failed: false,
            passed: 0,
            failed: 0,
        }
    }

    fn depth(&self) -> usize {
        self.suite_stack.len()
    }

    fn indent(&self) -> String {
        "  ".repeat(self.depth() + 1)
    }
}

// ── ANSI ─────────────────────────────────────────────────────────────────────
fn green(s: &str) -> String {
    format!("\x1b[32m{s}\x1b[0m")
}
fn red(s: &str) -> String {
    format!("\x1b[31m{s}\x1b[0m")
}
fn yellow(s: &str) -> String {
    format!("\x1b[33m{s}\x1b[0m")
}
fn bold(s: &str) -> String {
    format!("\x1b[1m{s}\x1b[0m")
}
fn dim(s: &str) -> String {
    format!("\x1b[2m{s}\x1b[0m")
}
fn cyan(s: &str) -> String {
    format!("\x1b[36m{s}\x1b[0m")
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Naive line diff: +/- for lines that differ.
fn print_line_diff(prefix: &str, expected: &str, actual: &str) {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();
    let max = exp_lines.len().max(act_lines.len());

    eprintln!("{prefix}{}", dim("Diff:"));
    for i in 0..max {
        match (exp_lines.get(i).copied(), act_lines.get(i).copied()) {
            (Some(el), Some(al)) if el == al => eprintln!("{prefix}  {}", dim(&format!("  {el}"))),
            (Some(el), _) => eprintln!("{prefix}  {}", green(&format!("+ {el}"))),
            (None, Some(al)) => eprintln!("{prefix}  {}", red(&format!("- {al}"))),
            _ => {}
        }
    }
}

// ── Public Rust API (for the `rts test` command) ─────────────────────────────
/// Resets runner state between test files.
pub fn reset_runner() {
    RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        r.suite_stack.clear();
        r.case_name = None;
        r.case_failed = false;
        r.passed = 0;
        r.failed = 0;
    });
}

pub fn runner_failed() -> usize {
    RUNNER.with(|r| r.borrow().failed)
}

pub fn runner_passed() -> usize {
    RUNNER.with(|r| r.borrow().passed)
}

/// Low-level test runner primitives. Use rts:test for the high-level API.
#[rts_namespace(test_core)]
impl TestCore {
    /// Opens a named test suite block. Nested calls increase indent.
    #[rts_fn]
    pub fn suite_begin(name: Str) {
        RUNNER.with(|r| {
            let r = r.borrow();
            let indent = "  ".repeat(r.depth() + 1);
            eprintln!("{indent}{}", yellow(&bold(name)));
        });
        RUNNER.with(|r| r.borrow_mut().suite_stack.push(name.to_string()));
    }

    /// Closes the innermost test suite block.
    #[rts_fn]
    pub fn suite_end() {
        RUNNER.with(|r| {
            let mut r = r.borrow_mut();
            r.suite_stack.pop();
            if r.suite_stack.is_empty() {
                eprintln!();
            }
        });
    }

    /// Starts a named test case and resets the failure flag.
    #[rts_fn]
    pub fn case_begin(name: Str) {
        RUNNER.with(|r| {
            let mut r = r.borrow_mut();
            r.case_name = Some(name.to_string());
            r.case_failed = false;
        });
    }

    /// Ends the current case. Prints ✓ if no failures, updates counters.
    #[rts_fn]
    pub fn case_end() {
        RUNNER.with(|r| {
            let mut r = r.borrow_mut();
            let indent = r.indent();
            let name = r.case_name.take().unwrap_or_default();
            if r.case_failed {
                r.failed += 1;
            } else {
                r.passed += 1;
                eprintln!("{indent}{} {}", green("✓"), dim(&name));
            }
            r.case_failed = false;
        });
    }

    /// Marks current case as failed and emits message in red.
    #[rts_fn]
    pub fn case_fail(msg: Str) {
        RUNNER.with(|r| {
            let mut r = r.borrow_mut();
            let indent = r.indent();
            let name = r.case_name.clone().unwrap_or_default();
            if !r.case_failed {
                eprintln!("{indent}{} {}", red("✗"), bold(&name));
            }
            r.case_failed = true;
            eprintln!("{indent}  {msg}");
        });
    }

    /// Marks current case as failed and prints an expected/received diff.
    #[rts_fn]
    pub fn case_fail_diff(expected: Str, actual: Str) {
        RUNNER.with(|r| {
            let mut r = r.borrow_mut();
            let indent = r.indent();
            let name = r.case_name.clone().unwrap_or_default();
            if !r.case_failed {
                eprintln!("{indent}{} {}", red("✗"), bold(&name));
            }
            r.case_failed = true;

            let prefix = format!("{indent}  ");
            eprintln!(
                "{prefix}{} {}",
                dim("Expected:"),
                cyan(&format!("{expected:?}"))
            );
            eprintln!(
                "{prefix}{} {}",
                dim("Received:"),
                red(&format!("{actual:?}"))
            );
            if expected.contains('\n') || actual.contains('\n') {
                print_line_diff(&prefix, expected, actual);
            }
        });
    }

    /// Prints pass/fail counts. Call once at the end of the test file.
    #[rts_fn]
    pub fn print_summary() {
        RUNNER.with(|r| {
            let r = r.borrow();
            let passed = r.passed;
            let failed = r.failed;
            let total = passed + failed;

            eprintln!("{}", dim(&"─".repeat(40)));
            if failed == 0 {
                eprintln!(
                    " {} {}",
                    green("✓"),
                    green(&format!("{total} test{} passed", plural(total)))
                );
            } else {
                eprintln!(
                    " {} {}",
                    red("✗"),
                    red(&format!("{failed} test{} failed", plural(failed)))
                );
                if passed > 0 {
                    eprintln!(
                        " {} {}",
                        green("✓"),
                        green(&format!("{passed} test{} passed", plural(passed)))
                    );
                }
                eprintln!(" {} {total} total", dim("·"));
            }
            eprintln!();
        });
    }
}
