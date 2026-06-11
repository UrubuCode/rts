//! `test_core` namespace — low-level test runner primitives. The high-level
//! API lives in `bundle.ts` (`rts:test`).
//!
//! `reset_runner` / `runner_failed` / `runner_passed` are pub (driven by the
//! `rts test` command); `BUNDLE_TS` is the embedded high-level harness.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::cell::RefCell;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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

// ── ABI surface (extern "C" runner primitives) ───────────────────────────────

/// Opens a named test suite block. Nested calls increase indent.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_SUITE_BEGIN(name_ptr: *const u8, name_len: i64) {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return,
    };
    RUNNER.with(|r| {
        let r = r.borrow();
        let indent = "  ".repeat(r.depth() + 1);
        eprintln!("{indent}{}", yellow(&bold(name)));
    });
    RUNNER.with(|r| r.borrow_mut().suite_stack.push(name.to_string()));
}

/// Closes the innermost test suite block.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_SUITE_END() {
    RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        r.suite_stack.pop();
        if r.suite_stack.is_empty() {
            eprintln!();
        }
    });
}

/// Starts a named test case and resets the failure flag.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_BEGIN(name_ptr: *const u8, name_len: i64) {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return,
    };
    RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        r.case_name = Some(name.to_string());
        r.case_failed = false;
    });
}

/// Ends the current case. Prints ✓ if no failures, updates counters.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_END() {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_FAIL(msg_ptr: *const u8, msg_len: i64) {
    let msg = match unsafe { rts_engine::abi::str_abi::from_abi(msg_ptr, msg_len) } {
        Some(s) => s,
        None => return,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_FAIL_DIFF(
    expected_ptr: *const u8,
    expected_len: i64,
    actual_ptr: *const u8,
    actual_len: i64,
) {
    let expected = match unsafe { rts_engine::abi::str_abi::from_abi(expected_ptr, expected_len) } {
        Some(s) => s,
        None => return,
    };
    let actual = match unsafe { rts_engine::abi::str_abi::from_abi(actual_ptr, actual_len) } {
        Some(s) => s,
        None => return,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_PRINT_SUMMARY() {
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

/// Função `test_core.f(args)` — todas as primitivas são void, não-pure.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `test_core` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("test_core")
        .doc("Low-level test runner primitives. Use rts:test for the high-level API.")
        .member(func(
            "suite_begin",
            "__RTS_FN_NS_TEST_CORE_SUITE_BEGIN",
            Sig::new(vec![AbiType::StrPtr], AbiType::Void),
            "suite_begin(name: string): void",
            "Opens a named test suite block. Nested calls increase indent.",
            __RTS_FN_NS_TEST_CORE_SUITE_BEGIN as *const u8,
        ))
        .member(func(
            "suite_end",
            "__RTS_FN_NS_TEST_CORE_SUITE_END",
            Sig::new(Vec::new(), AbiType::Void),
            "suite_end(): void",
            "Closes the innermost test suite block.",
            __RTS_FN_NS_TEST_CORE_SUITE_END as *const u8,
        ))
        .member(func(
            "case_begin",
            "__RTS_FN_NS_TEST_CORE_CASE_BEGIN",
            Sig::new(vec![AbiType::StrPtr], AbiType::Void),
            "case_begin(name: string): void",
            "Starts a named test case and resets the failure flag.",
            __RTS_FN_NS_TEST_CORE_CASE_BEGIN as *const u8,
        ))
        .member(func(
            "case_end",
            "__RTS_FN_NS_TEST_CORE_CASE_END",
            Sig::new(Vec::new(), AbiType::Void),
            "case_end(): void",
            "Ends the current case. Prints ✓ if no failures, updates counters.",
            __RTS_FN_NS_TEST_CORE_CASE_END as *const u8,
        ))
        .member(func(
            "case_fail",
            "__RTS_FN_NS_TEST_CORE_CASE_FAIL",
            Sig::new(vec![AbiType::StrPtr], AbiType::Void),
            "case_fail(msg: string): void",
            "Marks current case as failed and emits message in red.",
            __RTS_FN_NS_TEST_CORE_CASE_FAIL as *const u8,
        ))
        .member(func(
            "case_fail_diff",
            "__RTS_FN_NS_TEST_CORE_CASE_FAIL_DIFF",
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Void),
            "case_fail_diff(expected: string, actual: string): void",
            "Marks current case as failed and prints an expected/received diff.",
            __RTS_FN_NS_TEST_CORE_CASE_FAIL_DIFF as *const u8,
        ))
        .member(func(
            "print_summary",
            "__RTS_FN_NS_TEST_CORE_PRINT_SUMMARY",
            Sig::new(Vec::new(), AbiType::Void),
            "print_summary(): void",
            "Prints pass/fail counts. Call once at the end of the test file.",
            __RTS_FN_NS_TEST_CORE_PRINT_SUMMARY as *const u8,
        ))
        .done();
}
