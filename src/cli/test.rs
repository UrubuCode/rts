use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::compile_options::CompileOptions;

struct Theme {
    green: &'static str,
    red: &'static str,
    cyan: &'static str,
    dim: &'static str,
    bold: &'static str,
    reset: &'static str,
}

impl Theme {
    fn detect() -> Self {
        let enabled = is_tty_stderr()
            && std::env::var("NO_COLOR").is_err()
            && std::env::var("TERM").as_deref() != Ok("dumb");

        if enabled {
            Self {
                green: "\x1b[32m",
                red: "\x1b[31m",
                cyan: "\x1b[36m",
                dim: "\x1b[2m",
                bold: "\x1b[1m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                green: "", red: "", cyan: "", dim: "", bold: "", reset: "",
            }
        }
    }
}

fn is_tty_stderr() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

struct TestSuite {
    files: BTreeMap<PathBuf, Vec<TestResult>>,
    total: usize,
    passed: usize,
    failed: usize,
    start_time: Instant,
}

struct TestResult {
    name: String,
    passed: bool,
    duration_ms: f64,
    error: Option<String>,
}

pub fn command(path_arg: Option<String>, options: CompileOptions) -> Result<()> {
    let search_root = path_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let test_files = collect_test_files(&search_root);

    if test_files.is_empty() {
        eprintln!("No test files found in {}", search_root.display());
        return Ok(());
    }

    let theme = Theme::detect();
    let mut suite = TestSuite {
        files: BTreeMap::new(),
        total: 0,
        passed: 0,
        failed: 0,
        start_time: Instant::now(),
    };

    // Print header
    print_header(&theme);

    // Run all tests
    for file in &test_files {
        run_file_tests(file, &mut suite, &options, &theme);
    }

    // Print summary
    print_summary(&suite, &theme);

    if suite.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn run_file_tests(
    file: &Path,
    suite: &mut TestSuite,
    options: &CompileOptions,
    theme: &Theme,
) {
    let label = relative_display(file);
    
    // Print file header
    eprintln!("\n{}{}:{}", theme.bold, label, theme.reset);
    
    let file_start = Instant::now();
    let outcome = super::run::execute_file(file, *options);
    let duration_ms = file_start.elapsed().as_secs_f64() * 1000.0;
    
    // Parse test results from output
    // This assumes your test runner outputs test names in a parseable format
    let test_name = extract_test_name(file);
    
    let passed = outcome.is_ok();
    let error = outcome.err().map(|e| format!("{:#}", e));
    
    let result = TestResult {
        name: test_name,
        passed,
        duration_ms,
        error,
    };
    
    // Print test result
    if passed {
        eprintln!(
            "{}{}✓{} {} {}{}[{:.2}ms]{}",
            theme.dim, theme.green, theme.reset,
            result.name,
            theme.dim, theme.cyan,
            result.duration_ms,
            theme.reset
        );
        suite.passed += 1;
    } else {
        eprintln!(
            "{}{}✗{} {} {}{}[{:.2}ms]{}",
            theme.dim, theme.red, theme.reset,
            result.name,
            theme.dim, theme.cyan,
            result.duration_ms,
            theme.reset
        );
        
        // Print error with indentation
        if let Some(ref err) = result.error {
            for line in err.lines().take(10) {
                eprintln!("  {}  {}{}", theme.dim, theme.red, line);
            }
        }
        
        suite.failed += 1;
    }
    
    suite.files.entry(file.to_path_buf()).or_default().push(result);
    suite.total += 1;
}

fn extract_test_name(file: &Path) -> String {
    // Extract a meaningful test name from the file
    file.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.replace('_', " ").replace('-', " "))
        .unwrap_or_else(|| "unnamed test".to_string())
}

fn print_header(theme: &Theme) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        "{}{}rts test v{}{}",
        theme.bold, theme.cyan, version, theme.reset
    );
}

fn print_summary(suite: &TestSuite, theme: &Theme) {
    let total_duration = suite.start_time.elapsed().as_secs_f64();
    
    eprintln!();
    
    if suite.failed == 0 {
        eprintln!(
            "{}{}  {} pass{}",
            theme.bold, theme.green,
            suite.passed,
            theme.reset
        );
    } else {
        eprintln!(
            "{}{}  {} pass{}  {}{} fail{}",
            theme.bold, theme.green,
            suite.passed,
            theme.reset,
            theme.red,
            suite.failed,
            theme.reset
        );
        
        // List failed tests summary
        eprintln!();
        for (file, results) in &suite.files {
            let failed_tests: Vec<_> = results.iter()
                .filter(|r| !r.passed)
                .collect();
            
            if !failed_tests.is_empty() {
                eprintln!("{}{}:{}", theme.bold, relative_display(file), theme.reset);
                for test in failed_tests {
                    eprintln!(
                        "{}{}✗{} {} {}{}[{:.2}ms]{}",
                        theme.dim, theme.red, theme.reset,
                        test.name,
                        theme.dim, theme.cyan,
                        test.duration_ms,
                        theme.reset
                    );
                    if let Some(ref err) = test.error {
                        if let Some(first_line) = err.lines().next() {
                            eprintln!("  {}  {}{}", theme.dim, theme.red, first_line);
                        }
                    }
                }
                eprintln!();
            }
        }
    }
    
    eprintln!(
        "{}Ran {} tests across {} files. {}{}[{:.2}s]{}",
        theme.dim,
        suite.total,
        suite.files.len(),
        theme.cyan,
        theme.bold,
        total_duration,
        theme.reset
    );
}

fn relative_display(path: &Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    path.strip_prefix(&cwd)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn collect_test_files(root: &Path) -> Vec<PathBuf> {
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(root)
    };

    let mut files = Vec::new();
    collect_recursive(&root, &mut files);
    files.sort();
    files
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            collect_recursive(&path, out);
        } else if is_test_file(&path) {
            out.push(path);
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    let (ext_ts, ext_js) = (name.ends_with(".ts"), name.ends_with(".js"));
    if !ext_ts && !ext_js {
        return false;
    }
    let stem = &name[..name.len() - 3];
    stem.ends_with(".test") || stem.ends_with(".__test__")
}