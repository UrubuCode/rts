use std::cell::RefCell;

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

// ── ANSI ──────────────────────────────────────────────────────────────────────

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

// ── Public C ABI ──────────────────────────────────────────────────────────────
// All symbols are pub so jit.rs can take their function pointers.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_SUITE_BEGIN(
    name_ptr: *const u8,
    name_len: usize,
) {
    let name = unsafe { str_from_raw(name_ptr, name_len) };
    RUNNER.with(|r| {
        let r = r.borrow();
        let indent = "  ".repeat(r.depth() + 1);
        eprintln!("{indent}{}", yellow(&bold(name)));
    });
    RUNNER.with(|r| r.borrow_mut().suite_stack.push(name.to_string()));
}

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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_BEGIN(
    name_ptr: *const u8,
    name_len: usize,
) {
    let name = unsafe { str_from_raw(name_ptr, name_len) };
    RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        r.case_name = Some(name.to_string());
        r.case_failed = false;
    });
}

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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_FAIL(
    msg_ptr: *const u8,
    msg_len: usize,
) {
    let msg = unsafe { str_from_raw(msg_ptr, msg_len) };
    RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        let indent = r.indent();
        let name = r.case_name.clone().unwrap_or_default();

        if !r.case_failed {
            // Print the ✗ line only on first failure of this case.
            eprintln!("{indent}{} {}", red("✗"), bold(&name));
            r.failed += 1;
        }
        r.case_failed = true;
        eprintln!("{indent}  {msg}");
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TEST_CORE_CASE_FAIL_DIFF(
    expected_ptr: *const u8,
    expected_len: usize,
    actual_ptr: *const u8,
    actual_len: usize,
) {
    let expected = unsafe { str_from_raw(expected_ptr, expected_len) };
    let actual = unsafe { str_from_raw(actual_ptr, actual_len) };

    RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        let indent = r.indent();
        let name = r.case_name.clone().unwrap_or_default();

        if !r.case_failed {
            eprintln!("{indent}{} {}", red("✗"), bold(&name));
            r.failed += 1;
        }
        r.case_failed = true;

        let prefix = format!("{indent}  ");
        eprintln!("{prefix}{} {}", dim("Expected:"), cyan(&format!("{expected:?}")));
        eprintln!("{prefix}{} {}", dim("Received:"), red(&format!("{actual:?}")));

        if expected.contains('\n') || actual.contains('\n') {
            print_line_diff(&prefix, expected, actual);
        }
    });
}

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
            if failed > 0 {
                eprintln!(
                    " {} {}",
                    red("✗"),
                    red(&format!("{failed} test{} failed", plural(failed)))
                );
            }
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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Naive line diff: prints +/- for lines that differ.
fn print_line_diff(prefix: &str, expected: &str, actual: &str) {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();
    let max = exp_lines.len().max(act_lines.len());

    eprintln!("{prefix}{}", dim("Diff:"));
    for i in 0..max {
        let e = exp_lines.get(i).copied();
        let a = act_lines.get(i).copied();
        match (e, a) {
            (Some(el), Some(al)) if el == al => {
                eprintln!("{prefix}  {}", dim(&format!("  {el}")));
            }
            (Some(el), _) => {
                eprintln!("{prefix}  {}", green(&format!("+ {el}")));
            }
            (None, Some(al)) => {
                eprintln!("{prefix}  {}", red(&format!("- {al}")));
            }
            _ => {}
        }
    }
}

unsafe fn str_from_raw<'a>(ptr: *const u8, len: usize) -> &'a str {
    if ptr.is_null() || len == 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr, len);
        std::str::from_utf8_unchecked(slice)
    }
}
