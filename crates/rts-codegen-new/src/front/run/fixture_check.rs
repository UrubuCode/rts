//! Mini cross-runtime harness — the FIRST real measurement of the new engine on
//! actual fixtures.
//!
//! Honest and subset-only: it walks `tests/cross-runtime/**/*.ts`, runs each
//! through [`super::render_source`] (console.log captured to a `String` via the
//! adapter's real-pool-backed sink), and — for the ones the new engine actually
//! runs — compares the captured stdout to `bun <file>`. Most fixtures are
//! Unsupported today (objects, strings methods, classes, regex, …); the harness
//! reports the REAL count it runs and matches, never inflates it.
//!
//! It is `#[ignore]`d by default so the normal `cargo test` run does NOT depend
//! on `bun` being installed. Run it explicitly with:
//!
//! ```text
//! cargo test -p rts-codegen-new -- --ignored fixture_harness --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use super::render_source;

/// Root of the committed cross-runtime fixtures, resolved from this crate.
fn fixtures_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/rts-codegen-new
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("../../tests/cross-runtime")
}

/// Collect every `*.ts` fixture under `root`, skipping `node_modules` and the
/// shared `support/` helpers (which are imported, not run standalone).
fn collect_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "node_modules" || name == "support" {
                    continue;
                }
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("ts") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Run a fixture through `bun` and return its stdout (None if bun fails/missing).
fn bun_stdout(path: &Path) -> Option<String> {
    let output = Command::new("bun").arg("run").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(normalize(&String::from_utf8_lossy(&output.stdout)))
}

/// Normalize line endings (CRLF→LF) for a fair host-vs-bun comparison.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[derive(Default)]
struct Tally {
    total: usize,
    ran: usize,        // run_source returned Ok
    matched: usize,    // ran AND equals bun
    diverged: usize,   // ran but != bun
    unsupported: usize, // run_source bailed
    bun_unavailable: usize,
}

#[test]
#[ignore = "shells out to `bun`; run explicitly with --ignored"]
fn fixture_harness() {
    let root = fixtures_root();
    if !root.is_dir() {
        eprintln!("cross-runtime fixtures not found at {}", root.display());
        return;
    }
    // Probe bun once; if unavailable, report and stop (the test is ignored by
    // default, so this only fires on an explicit, bun-less run).
    if Command::new("bun").arg("--version").output().is_err() {
        eprintln!("`bun` not on PATH — skipping the cross-runtime comparison.");
        return;
    }

    let fixtures = collect_fixtures(&root);
    let mut t = Tally::default();
    let mut matched_names: Vec<String> = Vec::new();
    let mut diverged_names: Vec<(String, String, String)> = Vec::new();

    for path in &fixtures {
        t.total += 1;
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Guard against runaway loops in a fixture: each run is in-process, so a
        // genuinely infinite program would hang. We accept that risk for the
        // ignored harness (the subset the engine runs is loop-bounded in
        // practice); a timeout wrapper is a follow-up.
        match render_source(&src) {
            Ok(out) => {
                t.ran += 1;
                match bun_stdout(path) {
                    Some(expected) => {
                        if normalize(&out) == expected {
                            t.matched += 1;
                            matched_names.push(rel(&root, path));
                        } else {
                            t.diverged += 1;
                            diverged_names.push((rel(&root, path), normalize(&out), expected));
                        }
                    }
                    None => t.bun_unavailable += 1,
                }
            }
            Err(_) => t.unsupported += 1,
        }
    }

    eprintln!("\n=== new-engine cross-runtime harness (honest, subset-only) ===");
    eprintln!("fixtures scanned : {}", t.total);
    eprintln!("ran (Ok)         : {}", t.ran);
    eprintln!("  matched bun    : {}", t.matched);
    eprintln!("  diverged       : {}", t.diverged);
    eprintln!("  bun unavailable: {}", t.bun_unavailable);
    eprintln!("unsupported (bail): {}", t.unsupported);
    eprintln!("\nmatched fixtures:");
    for n in &matched_names {
        eprintln!("  ✓ {n}");
    }
    eprintln!("\ndiverged fixtures (new-engine vs bun):");
    for (n, got, want) in &diverged_names {
        eprintln!("  ✗ {n}\n     got : {got:?}\n     bun : {want:?}");
    }
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------------------
// Bail histogram — measurement-only, no engine behavior change, no `bun`.
//
// Walks every cross-runtime fixture, runs it through `render_source` (in-process,
// captured stdout), and for the ones that bail collects the reason. Each reason
// is normalized to a coarse CATEGORY (specific identifiers/numbers stripped) so
// similar bails cluster, then printed sorted by frequency with example fixtures.
//
// Its purpose is to tell the build which features to add next for maximum
// whole-fixture ROL: a fixture only "flips" to running once ALL its bail reasons
// are cleared, so it also estimates how many fixtures are blocked by exactly one
// distinct construct ("closest" fixtures).
//
// `#[ignore]`d like the bun harness, but it does NOT shell out — run with:
//   cargo test -p rts-codegen-new -- --ignored bail_histogram --nocapture
// ---------------------------------------------------------------------------

/// Map a raw `Unsupported` reason string to a coarse, identifier-free category.
/// The match order matters: more specific phrases first. The goal is that two
/// bails on "the same kind of construct" land in the same bucket regardless of
/// the concrete identifier/number/repr embedded in the message.
fn categorize(reason: &str) -> &'static str {
    let r = reason;
    let has = |needle: &str| r.contains(needle);

    // Parse / lowering-plumbing failures (not a language feature per se).
    if has("parse error:") {
        return "parse error";
    }
    if has("declare ") || has("define ") || has("finalize module") {
        return "jit/module plumbing error";
    }

    // Strings.
    if has("string literal") || has("returning StrPtr") {
        return "string literal / string value";
    }

    // --- high-signal phrases discovered from the raw dump ---
    if has("is not a user class in this program") {
        return "global/Registry class (new C() / extends builtin)";
    }
    if has("whose shape is not statically proven") {
        return "object: property access on unproven shape (param/return/reassign)";
    }
    if has("CAPTURING callback") {
        return "array method with capturing-closure callback";
    }
    if has("no Registry entry for") {
        return "array/collection method with callback (.map/.filter/.reduce)";
    }
    if has("private field") {
        return "class: private #field";
    }
    if has("private method") {
        return "class: private #method";
    }
    if has("on a whole object/array operand") || has("ToPrimitive coercion") {
        return "operator on object/array operand (ToPrimitive)";
    }
    if has("differing/unknown kind") {
        return "equality across Unknown/Str/Number kinds";
    }
    if has("init block") || has("static block") {
        return "class: static/init block";
    }
    if has("more than one constructor") {
        return "class: multiple constructors";
    }
    if has("extends unknown class") {
        return "class: extends unknown user class";
    }
    if has("uses a variadic / defaulted parameter") {
        return "param: variadic/default (ctor/method)";
    }
    if has("expects ") && has(" args, got ") {
        return "function arity mismatch";
    }

    // Null / undefined literals.
    if has("null literal") {
        return "null literal";
    }
    if has("undefined literal") {
        return "undefined literal";
    }

    // Objects / shapes / property access.
    if has("adding a new key") || has("transition tree") {
        return "object: add new key (transition tree)";
    }
    if has("object literal with a duplicated key") {
        return "object literal (duplicated key)";
    }
    if has("property/index access on a non-identifier object")
        || has("unknown shape")
        || has("no such field")
        || has("no field")
    {
        return "object: property access on unproven shape";
    }
    if has("computed index on a non-array") || has("indexed write on a non-array") {
        return "object: computed/dynamic key o[k]";
    }

    // Arrays.
    if has("spread in an array literal") || has("spread") {
        return "spread";
    }
    if has("array member `.") || has("write to array member") {
        return "array: unsupported member access";
    }
    if has("array method") || has("array-method arg") || has("array receiver") {
        return "array: unsupported method";
    }
    if has("non-integer array index") || has("number index but got") {
        return "array: non-integer/typed index";
    }

    // Method dispatch / calls.
    if has("method call `.") || has("call of member `.") || has("not statically dispatchable") {
        return "method call on non-dispatchable receiver";
    }
    if has("cannot marshal a method arg") || has("method arg") || has("a method returning") {
        return "method arg/return marshaling";
    }
    if has("call to unknown function") {
        return "call to unknown function/global";
    }
    if has("call of a non-identifier callee") {
        return "call of non-identifier callee";
    }
    if has("cross-function calls are a later increment") {
        return "cross-function call (hir_lower path)";
    }

    // Classes / inheritance.
    if has("abstract class") {
        return "class: abstract";
    }
    if has("super(") || has("super.") || has("superclass") {
        return "class: super(...)/super.method";
    }
    if has("static field") || has("static method") {
        return "class: static member";
    }
    if has("accessor") || has("getter") || has("setter") {
        return "class: getter/setter";
    }
    if has("class `") || has("on class `") {
        return "class: other unsupported class shape";
    }

    // Identifiers / globals / captures.
    if has("unbound identifier") || has("unbound `") || has("globals/captures") {
        return "unbound identifier (global/capture)";
    }
    if has("variadic/defaulted") {
        return "param: variadic/default";
    }

    // Control flow / statements.
    if has("may fall through without returning") {
        return "fn may fall through (no return)";
    }
    if has("unreachable statement after") {
        return "unreachable after terminator";
    }
    if has("`return;`") {
        return "bare `return;`";
    }
    if has("without an initializer") {
        return "`let` without initializer";
    }
    if has("non-numeric type") {
        return "`let` non-numeric type";
    }
    if has("unrecognized statement") {
        return "unrecognized statement (Raw)";
    }
    if has("statement ") {
        // catch the `statement <name>` family (for-of, for-in, switch, try, throw...)
        return categorize_statement(r);
    }
    if has("condition of repr") {
        return "branch condition repr";
    }

    // Operators / coercions.
    if has("compound-assign") {
        return "compound-assign operator";
    }
    if has("`++`/`--`") {
        return "++/-- on non-int repr";
    }
    if has("assignment target must be") || has("assignment to unbound") {
        return "assignment target not a simple ident";
    }
    if has("float remainder") || has("needs runtime fmod") {
        return "float `%` (fmod)";
    }
    if has("arithmetic on a boolean") || has("ordering comparison on a boolean") {
        return "arithmetic/compare on boolean";
    }
    if has("logical") && has("non-boolean") {
        return "logical op on non-boolean";
    }
    if has("logical `!` on a non-boolean") {
        return "`!` on non-boolean";
    }
    if has("unary `-`") || has("unary `~`") || has("unary operator") || has("unary `!`") {
        return "unary op on non-numeric repr";
    }
    if has("ternary arms") {
        return "ternary arms incompatible reprs";
    }
    if has("strict-eq op") || has("generic relational op") || has("comparison op") {
        return "comparison op on generic/tagged";
    }
    if has("bitwise op") {
        return "bitwise op (unsupported)";
    }
    if has("generic arithmetic") || has("arithmetic op") {
        return "arithmetic op on generic/tagged";
    }
    if has("binary operator") {
        return "binary operator (other)";
    }
    if has("cannot coerce") {
        return "coercion between reprs";
    }
    if has("thunk cannot unbox") {
        return "thunk param unbox repr";
    }
    if has("expression ") {
        return categorize_expression(r);
    }

    "uncategorized"
}

/// `statement <name>` bails — distinguish the construct named after "statement ".
/// The names come from `stmt_name` and are lowercase (`try`, `for-of`, `switch`…).
fn categorize_statement(r: &str) -> &'static str {
    let name = r.rsplit("statement ").next().unwrap_or("").trim();
    match name {
        "for-of" | "for-in" => "for-of/for-in",
        "for" => "for-loop (C-style)",
        "while" | "do-while" => "while/do-while",
        "switch" => "switch",
        "try" => "try/catch/finally",
        "throw" => "throw",
        "break" | "continue" => "break/continue",
        "labeled" => "labeled statement",
        "block" => "bare block statement",
        _ => "statement (other)",
    }
}

/// `expression <name>` bails — distinguish the construct named after "expression ".
/// Names come from `expr_variant_name` (lowercase: `await`, `arrow`, `spread`…).
fn categorize_expression(r: &str) -> &'static str {
    let name = r.rsplit("expression ").next().unwrap_or("").trim();
    match name {
        "raw/unrecognized" => "raw/unmodeled expr (template/regex/bigint/optional-chain/etc)",
        "await" => "async/await",
        "arrow" => "closure/arrow value",
        "new" => "new expression",
        "spread" => "spread",
        "object-literal" => "object literal expr",
        "array-literal" => "array literal expr",
        "assignment" | "compound-assignment" => "assignment expr",
        "sequence" => "sequence expr (a, b)",
        "ternary" => "ternary expr",
        "cast" => "type cast expr",
        _ => "expression (other)",
    }
}

#[test]
#[ignore = "measurement-only histogram; run explicitly with --ignored bail_histogram --nocapture"]
fn bail_histogram() {
    use std::collections::BTreeMap;

    let root = fixtures_root();
    if !root.is_dir() {
        eprintln!("cross-runtime fixtures not found at {}", root.display());
        return;
    }

    let fixtures = collect_fixtures(&root);

    // category -> (count, example fixture paths)
    let mut buckets: BTreeMap<&'static str, (usize, Vec<String>)> = BTreeMap::new();
    // per-fixture: the set of DISTINCT categories it bailed on (only the first
    // reason is observable per run, but we record it; "closest" is estimated as
    // fixtures whose single observed blocker is the sole category — see note).
    let mut ran = 0usize;
    let mut bailed = 0usize;
    let mut errored_read = 0usize;
    // fixture -> its single observed bail category (the engine bails on the FIRST
    // unsupported construct it reaches, so each fixture contributes exactly one).
    let mut fixture_cat: Vec<(String, &'static str)> = Vec::new();

    for path in &fixtures {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                errored_read += 1;
                continue;
            }
        };
        match render_source(&src) {
            Ok(_) => ran += 1,
            Err(u) => {
                bailed += 1;
                let cat = categorize(u.reason());
                let name = rel(&root, path);
                let entry = buckets.entry(cat).or_insert((0, Vec::new()));
                entry.0 += 1;
                if entry.1.len() < 5 {
                    entry.1.push(name.clone());
                }
                fixture_cat.push((name, cat));
            }
        }
    }

    // Sort categories by frequency desc.
    let mut sorted: Vec<(&'static str, usize, Vec<String>)> = buckets
        .into_iter()
        .map(|(cat, (count, examples))| (cat, count, examples))
        .collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    eprintln!("\n=== new-engine bail histogram (measurement-only, no bun) ===");
    eprintln!("fixtures scanned : {}", fixtures.len());
    eprintln!("ran (Ok)         : {ran}");
    eprintln!("bailed (Err)     : {bailed}");
    if errored_read > 0 {
        eprintln!("unreadable files : {errored_read}");
    }
    eprintln!("\n--- bail categories (sorted by frequency) ---");
    for (cat, count, examples) in &sorted {
        eprintln!("{count:>4}  {cat}");
        for ex in examples {
            eprintln!("        e.g. {ex}");
        }
    }

    // "Closest" estimate: the engine bails on the FIRST unsupported construct, so
    // every fixture shows exactly ONE blocker here. A fixture is "closest" (one
    // feature away from running) when that single blocker is, in fact, its only
    // distinct unsupported construct — which we cannot fully prove without
    // re-running after each feature lands. As an honest upper-bound proxy we
    // report, per category, how many fixtures have it as their (first) blocker:
    // landing that one feature is NECESSARY (not always sufficient) to flip them.
    eprintln!("\n--- 'closest' proxy: fixtures whose FIRST blocker is this category ---");
    eprintln!("(landing the feature is necessary for these; some may reveal a second blocker)");
    let mut by_cat: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (_, cat) in &fixture_cat {
        *by_cat.entry(cat).or_insert(0) += 1;
    }
    let mut cc: Vec<(&'static str, usize)> = by_cat.into_iter().collect();
    cc.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (cat, n) in &cc {
        eprintln!("{n:>4}  {cat}");
    }

    eprintln!("\n--- totals ---");
    eprintln!("run / bail = {ran} / {bailed}  (of {} scanned)", fixtures.len());
}
