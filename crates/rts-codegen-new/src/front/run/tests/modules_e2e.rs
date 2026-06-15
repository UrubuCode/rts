//! M1b end-to-end: a REAL multi-file TS program on disk runs through the new
//! engine via [`crate::front::run::render_path`], captured stdout asserted exact.
//!
//! Each test writes a small program into a UNIQUE self-cleaning temp dir (no
//! `tempfile` dep — name = `std::process::id()` + a static atomic counter, the
//! same pattern as the M1a `modules::tests`), runs `render_path` on the entry,
//! and asserts the captured `console.log` output.
//!
//! Coverage: a NAMED cross-file import, an ALIASED import (`import { a as b }`),
//! a 3-file dependency chain (A→B→C), a re-run (the Registry/JIT path is
//! re-entrant), a cross-file CLASS import, and the EXPLICIT bail for an unwired
//! builtin import (`rts:<ns>`) — the honest boundary M1b leaves for follow-up.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::front::run::render_path;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A self-cleaning unique temp dir (mirrors `modules::tests::TempDir`).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rts_m1b_{}_{}", std::process::id(), n));
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

#[test]
fn named_import_two_files() {
    let t = TempDir::new();
    t.write(
        "lib.ts",
        "export function add(a: number, b: number): number { return a + b; }\n",
    );
    t.write(
        "main.ts",
        "import { add } from \"./lib\";\nconsole.log(add(2, 3));\n",
    );

    let out = render_path(&t.path("main.ts")).expect("run ok");
    assert_eq!(out, "5\n");
}

#[test]
fn aliased_import() {
    let t = TempDir::new();
    t.write(
        "lib.ts",
        "export function add(a: number, b: number): number { return a + b; }\n",
    );
    // `add` is imported under the local alias `plus`; the rename pass remaps
    // every `plus` reference back to the flattened declaration `add`.
    t.write(
        "main.ts",
        "import { add as plus } from \"./lib\";\nconsole.log(plus(10, 7));\n",
    );

    let out = render_path(&t.path("main.ts")).expect("run ok");
    assert_eq!(out, "17\n");
}

#[test]
fn three_file_chain() {
    let t = TempDir::new();
    // C exports `base`; B imports it and exports `mid` built on it; A imports
    // `mid` and prints. Tests transitive resolution + flatten post-order.
    t.write("c.ts", "export function base(): number { return 100; }\n");
    t.write(
        "b.ts",
        "import { base } from \"./c\";\nexport function mid(x: number): number { return base() + x; }\n",
    );
    t.write(
        "a.ts",
        "import { mid } from \"./b\";\nconsole.log(mid(23));\n",
    );

    let out = render_path(&t.path("a.ts")).expect("run ok");
    assert_eq!(out, "123\n");
}

#[test]
fn rerun_is_stable() {
    let t = TempDir::new();
    t.write(
        "lib.ts",
        "export function twice(n: number): number { return n * 2; }\n",
    );
    t.write(
        "main.ts",
        "import { twice } from \"./lib\";\nconsole.log(twice(21));\n",
    );

    // Re-running the same program must produce the same output (the leaked
    // Registry + a fresh JITModule per run are re-entrant).
    let entry = t.path("main.ts");
    assert_eq!(render_path(&entry).expect("run 1"), "42\n");
    assert_eq!(render_path(&entry).expect("run 2"), "42\n");
}

#[test]
fn cross_file_class_import() {
    let t = TempDir::new();
    t.write(
        "point.ts",
        "export class Point {\n  x: number;\n  constructor(x: number) { this.x = x; }\n  getX(): number { return this.x; }\n}\n",
    );
    t.write(
        "main.ts",
        "import { Point } from \"./point\";\nconst p = new Point(9);\nconsole.log(p.getX());\n",
    );

    let out = render_path(&t.path("main.ts")).expect("run ok");
    assert_eq!(out, "9\n");
}

#[test]
fn builtin_import_runs() {
    let t = TempDir::new();
    // `rts:io` namespace-member dispatch IS now wired (M1b builtin-import): the
    // imported `print` calls the real `__RTS_FN_NS_IO_PRINT`. Detailed coverage
    // (math, aliases, the honest bails) lives in `builtin_import.rs`.
    t.write(
        "main.ts",
        "import { print } from \"rts:io\";\nprint(\"hi\");\n",
    );

    // `io.print` writes to the REAL stdout (uncaptured); the run succeeding proves
    // the import resolved to the real symbol. See `builtin_import.rs` for the
    // captured-output (math) coverage.
    render_path(&t.path("main.ts")).expect("builtin import runs");
}
