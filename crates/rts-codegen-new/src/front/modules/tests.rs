//! Unit tests for the M1a module-resolution subsystem.
//!
//! Each test writes a small program into a UNIQUE temp dir (no `tempfile` dep:
//! name = `std::process::id()` + a static atomic counter), runs
//! [`super::load_program`], asserts on the flattened program / bindings / error,
//! and cleans the temp dir up.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rts_ast::ast::Item;

use super::flatten::Binding;
use super::load_program;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A self-cleaning unique temp dir.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rts_m1a_{}_{}", std::process::id(), n));
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

/// All top-level function names in a program (order-preserving).
fn fn_names(program: &rts_ast::ast::Program) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect()
}

/// No item in the flattened program should be an Import/ExportNamespace.
fn no_imports(program: &rts_ast::ast::Program) -> bool {
    !program
        .items
        .iter()
        .any(|it| matches!(it, Item::Import(_) | Item::ExportNamespace(_)))
}

#[test]
fn a_two_user_files_import_function() {
    let t = TempDir::new();
    t.write(
        "lib.ts",
        "export function add(a: number, b: number): number { return a + b; }\n",
    );
    t.write(
        "main.ts",
        "import { add } from \"./lib\";\nconst x = add(1, 2);\n",
    );

    let resolved = load_program(&t.path("main.ts")).expect("load ok");

    let names = fn_names(&resolved.program);
    assert!(names.contains(&"add".to_string()), "flattened: {names:?}");
    assert!(no_imports(&resolved.program), "imports must be erased");

    // local name `add` binds to the user-module export `add`.
    match resolved.bindings.get("add") {
        Some(Binding::Local { name }) => assert_eq!(name, "add"),
        other => panic!("expected Local binding, got {other:?}"),
    }
}

#[test]
fn b_relative_index_resolution() {
    let t = TempDir::new();
    // `./util` should resolve to `./util/index.ts`.
    t.write(
        "util/index.ts",
        "export function helper(): number { return 7; }\n",
    );
    t.write("main.ts", "import { helper } from \"./util\";\nhelper();\n");

    let resolved = load_program(&t.path("main.ts")).expect("load ok");
    assert!(fn_names(&resolved.program).contains(&"helper".to_string()));
    assert!(matches!(
        resolved.bindings.get("helper"),
        Some(Binding::Local { .. })
    ));
}

#[test]
fn c_builtin_import_no_disk() {
    let t = TempDir::new();
    // No `rts:io` file exists on disk — resolution must NOT touch disk.
    t.write(
        "main.ts",
        "import { print } from \"rts:io\";\nprint(\"hi\");\n",
    );

    let resolved = load_program(&t.path("main.ts")).expect("load ok");
    match resolved.bindings.get("print") {
        Some(Binding::Builtin { ns, member }) => {
            assert_eq!(ns, "io");
            assert_eq!(member, "print");
        }
        other => panic!("expected Builtin binding, got {other:?}"),
    }
}

#[test]
fn d_cycle_is_explicit_error() {
    let t = TempDir::new();
    t.write(
        "a.ts",
        "import { b } from \"./b\";\nexport function a(): number { return 1; }\n",
    );
    t.write(
        "b.ts",
        "import { a } from \"./a\";\nexport function b(): number { return 2; }\n",
    );

    let err = load_program(&t.path("a.ts")).expect_err("cycle must error");
    let msg = err.reason();
    assert!(
        msg.contains("circular") || msg.contains("cycle"),
        "unexpected message: {msg}"
    );
}

#[test]
fn e_import_non_exported_name_errors() {
    let t = TempDir::new();
    // `lib` defines `secret` but does NOT export it.
    t.write(
        "lib.ts",
        "function secret(): number { return 0; }\nexport function shown(): number { return 1; }\n",
    );
    t.write("main.ts", "import { secret } from \"./lib\";\n");

    let err = load_program(&t.path("main.ts")).expect_err("missing export must error");
    let msg = err.reason();
    assert!(msg.contains("secret"), "unexpected message: {msg}");
    assert!(msg.contains("not exported"), "unexpected message: {msg}");
}

#[test]
fn f_top_level_name_collision_errors() {
    let t = TempDir::new();
    // Both modules define a top-level `dup`; flattening them clashes.
    t.write("x.ts", "export function dup(): number { return 1; }\n");
    t.write("y.ts", "export function dup(): number { return 2; }\n");
    t.write(
        "main.ts",
        "import { dup } from \"./x\";\nimport { dup as dup2 } from \"./y\";\n",
    );

    let err = load_program(&t.path("main.ts")).expect_err("collision must error");
    let msg = err.reason();
    assert!(msg.contains("dup"), "unexpected message: {msg}");
    assert!(msg.contains("collision"), "unexpected message: {msg}");
}

#[test]
fn entry_not_found_errors() {
    let t = TempDir::new();
    let err = load_program(&t.path("nope.ts")).expect_err("missing entry must error");
    assert!(err.reason().contains("not found"));
}

/// Sanity: a relative import with an explicit extension resolves too.
#[test]
fn relative_with_explicit_ext() {
    let t = TempDir::new();
    t.write("lib.ts", "export function k(): number { return 3; }\n");
    let _: HashMap<String, Binding> = {
        t.write("main.ts", "import { k } from \"./lib.ts\";\n");
        load_program(&t.path("main.ts")).expect("load ok").bindings
    };
}

/// `Path` is used only to type the entry argument; keep the import live.
#[allow(dead_code)]
fn _type_anchor(_: &Path) {}
