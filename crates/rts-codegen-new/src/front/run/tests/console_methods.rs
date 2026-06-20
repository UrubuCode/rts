//! `console.*` beyond `log`. The console object is resolved through the
//! `console` Registry namespace (not a hardcoded-per-method switch): each method
//! formats its args the same way `log` does, and the SINK (stdout vs stderr) is
//! read from the member's `io.*` symbol (`PRINT` → stdout, `EPRINT` → stderr).
//!
//! The in-process capture buffer (`assert_stdout`) joins BOTH streams — it reads
//! rendered text regardless of the sink — so these tests pin the FORMATTING /
//! routing-resolves path. The stdout-vs-stderr SPLIT itself is validated
//! end-to-end against `bun` by the fixture harness.

use super::assert_stdout;

#[test]
fn info_debug_alias_log() {
    assert_stdout(
        r#"console.info("a", 1);
           console.debug("b", 2);"#,
        "a 1\nb 2\n",
    );
}

#[test]
fn warn_and_error_render() {
    // warn/error route to stderr, but the capture buffer reads both streams; we
    // assert the rendered lines (the SPLIT is a fixture-level e2e concern).
    assert_stdout(
        r#"console.error("boom", 42);
           console.warn("careful");"#,
        "boom 42\ncareful\n",
    );
}

#[test]
fn dir_single_arg() {
    assert_stdout(r#"console.dir("inspect-me");"#, "inspect-me\n");
}

#[test]
fn mixed_methods_order_preserved() {
    assert_stdout(
        r#"console.log("L");
           console.info("I");
           console.warn("W");
           console.error("E");"#,
        "L\nI\nW\nE\n",
    );
}
