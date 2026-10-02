//! `./x` means `x.js` when both `x.js` and `x/` are there.
//!
//! Node's order is `LOAD_AS_FILE` and then `LOAD_AS_DIRECTORY`, and `extended`
//! had it the other way round on the stated grounds that a directory "can never
//! collide with the file candidates below". It can, and a real package does it:
//! `protobufjs/src/` holds BOTH `rpc.js` and `rpc/`, the directory has no
//! `index`, and `require.resolve("./rpc")` in node answers `rpc.js`.
//!
//! What the old order produced was not a wrong file — it was a wrong FAULT. The
//! directory had no `index`, so the search fell through to the directory itself
//! and the loader tried to read it: `Acesso negado. (os error 5)`, which names a
//! permission problem for what is an ordering one. That is how it was found,
//! loading `@whiskeysockets/baileys`.
//!
//! Why the old shape looked safe is worth keeping: the code branched on
//! `named.is_dir()`, so only ONE of the two candidate lists was ever built and
//! the collision was unrepresentable in the code while being real on disk.

use rts_host::graph::{resolve_written, Aliases};
use std::io::Write;
use std::path::{Path, PathBuf};

fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_file_before_dir_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

fn names(found: Option<PathBuf>, expected: &Path) {
    let found = found.expect("the specifier resolved");
    let found = found.canonicalize().unwrap_or(found);
    let expected = expected.canonicalize().expect("the expected file exists");
    assert_eq!(found, expected);
}

/// The `protobufjs` shape exactly: a file and a directory of the same name,
/// where the directory has no `index`.
#[test]
fn a_file_wins_over_a_directory_of_the_same_name() {
    let dir = fixture(
        "both",
        &[
            ("src/rpc.js", "module.exports = 1;\n"),
            ("src/rpc/service.js", "module.exports = 2;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    names(
        resolve_written(&entry, "./rpc", &Aliases::none()),
        &dir.join("src/rpc.js"),
    );
}

/// And it still wins when the directory DOES have an `index` — which is the
/// half that says this is an order and not a fall-back. `node`'s
/// `require.resolve` answers the file here too.
#[test]
fn a_file_wins_even_when_the_directory_has_an_index() {
    let dir = fixture(
        "both_with_index",
        &[
            ("src/thing.js", "module.exports = 1;\n"),
            ("src/thing/index.js", "module.exports = 2;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    names(
        resolve_written(&entry, "./thing", &Aliases::none()),
        &dir.join("src/thing.js"),
    );
}

/// A directory alone still answers its `index`, which is what `require("./lib")`
/// means and what must not have been lost.
#[test]
fn a_directory_alone_still_answers_its_index() {
    let dir = fixture(
        "directory_only",
        &[
            ("src/lib/index.js", "module.exports = 2;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    names(
        resolve_written(&entry, "./lib", &Aliases::none()),
        &dir.join("src/lib/index.js"),
    );
}

/// The extension order inside the file candidates is unchanged: `.ts` leads,
/// because this repository's own suite is TypeScript and every relative import
/// in it omits the extension.
#[test]
fn the_extension_order_is_unchanged() {
    let dir = fixture(
        "extensions",
        &[
            ("src/both.ts", "export const x = 1;\n"),
            ("src/both.js", "module.exports = 2;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    names(
        resolve_written(&entry, "./both", &Aliases::none()),
        &dir.join("src/both.ts"),
    );
}


/// A directory with no `index` and no file beside it: `resolve` still answers
/// the joined path, so a message can quote what was looked for — and the LOADER
/// is what must say which fault it is.
///
/// This test exists because the first version of it asserted that `resolve`
/// answers NOTHING, and that failed: the directory comes back by design, since
/// `resolve` returns a path and not an option. So the defect is not there, it is
/// in what the loader then says.
#[test]
fn a_directory_with_no_index_is_still_answered_as_a_path() {
    let dir = fixture(
        "empty_directory",
        &[
            ("src/bare/other.js", "module.exports = 2;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let found = resolve_written(&entry, "./bare", &Aliases::none()).expect("a path comes back");
    assert!(
        found.is_dir(),
        "the joined path is the directory, which is what a message can quote"
    );
}

/// And the loader NAMES it, rather than reporting the OS's refusal.
///
/// `std::fs::read_to_string` of a directory is `Acesso negado. (os error 5)` on
/// Windows and "Is a directory" elsewhere — a permission error for an ordering
/// bug, which is exactly the message `@whiskeysockets/baileys` produced while
/// loading `protobufjs`. One `is_dir` on a path the loader has already failed to
/// read buys the real fault.
#[test]
fn a_directory_reached_as_a_module_says_so() {
    let dir = fixture(
        "directory_as_module",
        &[
            ("src/bare/other.js", "module.exports = 2;\n"),
            ("app.ts", "import \"./src/bare\";\nexport const x = 1;\n"),
        ],
    );
    let entry = dir.join("app.ts");
    // `Loaded` is not `Debug`, so the `Err` is taken by match rather than by
    // `expect_err` — which also says, in the test, that the success arm is a
    // failure here.
    let message = match rts_host::graph::load(&entry) {
        Ok(_) => panic!("a directory is not a module, and load accepted it"),
        Err(error) => format!("{error:#?}"),
    };
    assert!(
        message.contains("is a directory, and names no module"),
        "the message must name the fault, not the OS's refusal: {message}"
    );
    assert!(
        message.contains("index.ts/js/cjs/mjs"),
        "and say what was looked for: {message}"
    );
}
