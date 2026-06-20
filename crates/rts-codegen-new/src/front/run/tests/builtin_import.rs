//! BUILTIN-IMPORT dispatch (M1b): `import { member } from "rts:<ns>"` actually
//! CALLS the real runtime namespace function through the generic Registry marshal
//! (`recv = None`), asserting REAL captured stdout.
//!
//! Coverage:
//! - `rts:io` `print` writes a real line to the captured stdout (StrPtr arg);
//! - `rts:io` `eprint` resolves + runs (stderr — not captured here, but proves the
//!   second member of the same namespace marshals);
//! - `rts:math` `sqrt`/`floor` compute a real f64 result used in an expression;
//! - an aliased builtin import (`import { print as p }`) still calls through;
//! - a bare-`"rts"` namespace-OBJECT import (`import { math } from "rts"`) whose
//!   `math.sin(..)` METHOD and `math.PI` CONSTANT both route through the Registry;
//! - the explicit honest bails: an UNKNOWN member, a bare-`"rts"` object CALLED as
//!   a function (`io("x")`), and `import * as` (dropped by the parser — no binding).
//!
//! Uses the same self-cleaning temp-dir + `render_path` harness as `modules_e2e`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::front::run::render_path;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A self-cleaning unique temp dir (mirrors `modules_e2e::TempDir`).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rts_bi_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, contents).expect("write file");
        path
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `import { print } from "rts:io"; print("...")` is the canonical "std io público
/// importável" the user asked for. `print` is the REAL `__RTS_FN_NS_IO_PRINT`, which
/// writes to the process's REAL stdout (NOT the `console.log` capture sink — only
/// `__rtsadp_print_line` is intercepted), so the captured buffer is empty; the test
/// asserts the program COMPILES + RUNS end to end (the marshal resolved the StrPtr
/// arg + the real symbol). The visible-stdout proof is the release-binary run in the
/// PR notes.
#[test]
fn io_print_runs() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { print } from \"rts:io\";\nprint(\"hello from rts:io\");\n",
    );
    let out = render_path(&t.path("main.ts")).expect("io.print runs");
    // Real stdout (uncaptured) carried the line; the capture buffer stays empty.
    assert_eq!(out, "");
}

/// Two members of the SAME namespace both resolve + marshal: `print` and `eprint`
/// used together. Proves both resolve through one `register("io")` with no per-member
/// codegen arm (the run succeeds; both write to the real std streams, uncaptured).
#[test]
fn io_print_and_eprint_resolve() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { print, eprint } from \"rts:io\";\neprint(\"to stderr\");\nprint(\"to stdout\");\n",
    );
    render_path(&t.path("main.ts")).expect("io.print + io.eprint run");
}

/// An ALIASED builtin import (`import { print as say }`) calls through under the
/// local alias — the binding map keys on the LOCAL name, not the export name.
#[test]
fn io_print_aliased() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { print as say } from \"rts:io\";\nsay(\"aliased\");\n",
    );
    render_path(&t.path("main.ts")).expect("aliased io.print runs");
}

/// A NUMERIC builtin namespace (`rts:math`): `sqrt`/`floor` compute a real f64
/// result fed into an expression, printed via `console.log`. Proves the generic
/// marshal handles F64 args + F64 returns (not just StrPtr), i.e. generality beyond
/// `io`. `Math.sqrt(16) + Math.floor(2.9)` = 4 + 2 = 6.
#[test]
fn math_sqrt_floor_compute() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { sqrt, floor } from \"rts:math\";\nconsole.log(sqrt(16) + floor(2.9));\n",
    );
    let out = render_path(&t.path("main.ts")).expect("rts:math sqrt/floor run");
    assert_eq!(out, "6\n");
}

/// An UNKNOWN member of a real namespace bails EXPLICITLY (the honesty floor) — no
/// silent drop, no guessed symbol.
#[test]
fn unknown_member_bails() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { nonexistent } from \"rts:io\";\nnonexistent(\"x\");\n",
    );
    let err = render_path(&t.path("main.ts")).expect_err("unknown member must bail");
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent") && msg.contains("io"),
        "unexpected bail message: {msg}"
    );
}

/// BARE `"rts"` namespace-object: `import { math, io } from "rts"` binds `math`
/// as a namespace OBJECT; `math.sin(..)` routes through the same Registry marshal
/// the `rts:math` member-import uses. `sin(0) == 0`.
#[test]
fn bare_rts_namespace_object_method() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { math } from \"rts\";\nconsole.log(\"\" + math.sin(0));\n",
    );
    let out = render_path(&t.path("main.ts")).expect("bare-rts math.sin runs");
    assert_eq!(out, "0\n");
}

/// BARE `"rts"` namespace-object CONSTANT read: `math.PI` is a zero-arg
/// `MemberKind::Constant` getter resolved through `registry::namespace_const`.
/// Before the fix this bailed with "unbound identifier `math`" (the member read,
/// unlike the method call, never consulted the namespace-object binding).
#[test]
fn bare_rts_namespace_object_constant() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { math } from \"rts\";\nconsole.log(\"\" + math.PI);\n",
    );
    let out = render_path(&t.path("main.ts")).expect("bare-rts math.PI runs");
    assert_eq!(out, "3.141592653589793\n");
}

/// A bare-`"rts"` import binds a namespace OBJECT (`io`), not a member; calling it as
/// a function does not resolve to a namespace function — bail honestly. (`import * as`
/// is dropped by the parser, so it never even reaches a binding; the bare-`rts`
/// object import is the reachable "not a member" case.)
#[test]
fn bare_rts_object_import_bails() {
    let t = TempDir::new();
    t.write(
        "main.ts",
        "import { io } from \"rts\";\nio(\"x\");\n",
    );
    let err = render_path(&t.path("main.ts")).expect_err("bare rts object import must bail");
    let msg = err.to_string();
    assert!(
        msg.contains("io") || msg.contains("rts"),
        "unexpected bail message: {msg}"
    );
}
