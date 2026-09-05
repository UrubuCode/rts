//! What a program compiled to an OBJECT file carries beside its bytes.
//!
//! # Why these tests do not run the program, when rule 5 says a test here does
//!
//! Because running one needs a linker and an archive, and this crate has
//! neither: an object file's undefined names are resolved against
//! `rts-runtime`'s staticlib by `rts compile`, in another crate, against an
//! artefact `cargo test` does not build. The end-to-end claim is made where
//! that is possible — the blocking AOT smoke in `.github/workflows/build-artifacts.yml`,
//! which compiles, links and RUNS `tests/aot/graph.ts`.
//!
//! What is left for this file is the claim that smoke cannot localise: that the
//! manifest and the object AGREE. Every table crosses in two halves — the
//! compiler's, in the sidecar, and the linker's, as relocations — and if the
//! two disagree about how many entries there are, the runtime reads one list
//! against the other's length. That is a silent wrong answer, and it is exactly
//! the shape `docs/engine/lost-roots.md` warns about: two hand-kept lists.

use std::path::{Path, PathBuf};

use object::{Object, ObjectSection, ObjectSymbol};
use rts_host::object::{
    FRAME_TABLE_SYMBOL, FUNCTION_TABLE_SYMBOL, MODULE_TABLE_SYMBOL, ObjectProgram,
    compile_graph_to_object, compile_to_object, compile_to_object_with_html,
};

/// Writes a graph of files into a directory of its own and answers the entry.
///
/// Named after the test rather than randomised, so a failing run leaves
/// something a person can look at and re-compile by hand.
fn graph(named: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join("rts-aot-object").join(named);
    std::fs::create_dir_all(&dir).expect("a directory to write the graph into");
    let mut entry = PathBuf::new();
    for (name, source) in files {
        let path = dir.join(name);
        std::fs::write(&path, source).expect("a file to write");
        entry = path;
    }
    entry
}

/// One entry of an address table, in bytes.
fn width() -> u64 {
    size_of::<*const u8>() as u64
}

/// How many relocations the object carries inside the table named `symbol`.
///
/// Panics when the symbol is missing, which is itself the claim: a table that
/// is not in the object is a name the runtime archive fails to LINK against.
fn entries_in(bytes: &[u8], symbol: &str, expected: usize) -> usize {
    let file = object::File::parse(bytes).expect("a well-formed object file");
    let table = file
        .symbols()
        .find(|found| found.name() == Ok(symbol))
        .unwrap_or_else(|| panic!("`{symbol}` is not in the object — the archive would not link"));
    assert!(!table.is_undefined(), "`{symbol}` is defined by this object");
    // Past the COUNT word, which is a plain number this compilation wrote and
    // therefore carries no relocation — see
    // `rts_cranelift::target::AddressTable` for why the length travels inside
    // the table rather than beside it. The span ends where the manifest says
    // the entries do.
    let first = table.address() + width();
    let span = first..first + expected as u64 * width();
    let section = file
        .section_by_index(table.section_index().expect("the table is in a section"))
        .expect("the section it named exists");
    section
        .relocations()
        .filter(|(at, _)| span.contains(at))
        .count()
}

/// Every table's length, as the object states it, against what the manifest
/// says the runtime should read.
fn tables_agree_with(program: &ObjectProgram) {
    assert_eq!(
        entries_in(&program.bytes, MODULE_TABLE_SYMBOL, program.modules as usize),
        program.modules as usize,
        "the module table has one relocation per module the manifest counts"
    );
    assert_eq!(
        entries_in(&program.bytes, FRAME_TABLE_SYMBOL, program.frames.len()),
        program.frames.len(),
        "the frame table has one relocation per shape the manifest describes"
    );
    assert_eq!(
        entries_in(
            &program.bytes,
            FUNCTION_TABLE_SYMBOL,
            program.function_names.len()
        ),
        program.function_names.len(),
        "the function table has one relocation per name the manifest carries"
    );
}

#[test]
fn a_program_that_imports_a_file_compiles_to_one_object() {
    let entry = graph(
        "imports",
        &[
            ("lib.ts", "export function twice(n: number) { return n * 2; }\n"),
            (
                "main.ts",
                "import { twice } from \"./lib\";\nconsole.log(twice(21));\n",
            ),
        ],
    );
    let program = compile_graph_to_object(&entry).expect("a two-file graph compiles");

    assert_eq!(
        program.modules, 1,
        "one module runs before the entry: the file it imports"
    );
    assert_eq!(
        program.module_metas.len(),
        2,
        "`import.meta` answers for both files, not only the entry"
    );
    assert!(
        program.module_metas.last().expect("the entry is last").main,
        "the file the caller named is the one `import.meta.main` is true of"
    );
    assert!(!program.bytes.is_empty());
    tables_agree_with(&program);
}

#[test]
fn a_dependency_runs_before_the_module_that_imports_it() {
    let entry = graph(
        "order",
        &[
            ("bottom.ts", "export const N = 1;\n"),
            (
                "middle.ts",
                "import { N } from \"./bottom\";\nexport const M = N + 1;\n",
            ),
            (
                "top.ts",
                "import { M } from \"./middle\";\nconsole.log(M);\n",
            ),
        ],
    );
    let program = compile_graph_to_object(&entry).expect("a three-file chain compiles");

    assert_eq!(
        program.modules, 2,
        "both dependencies run before the entry, and the entry is not one of them"
    );
    // Post-order: the deepest file first, because a module publishes its
    // exports as its body finishes and the importer reads them when its own
    // body starts.
    let order: Vec<&str> = program
        .module_metas
        .iter()
        .map(|meta| {
            Path::new(&meta.specifier)
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a file name")
        })
        .collect();
    assert_eq!(order, vec!["bottom.ts", "middle.ts", "top.ts"]);
}

#[test]
fn a_generator_in_an_imported_module_gets_a_frame_the_linker_can_fill() {
    let entry = graph(
        "frames",
        &[
            (
                "gen.ts",
                "export function* upto(n: number) { for (let i = 0; i < n; i++) yield i; }\n",
            ),
            (
                "use.ts",
                "import { upto } from \"./gen\";\nlet total = 0;\nfor (const v of upto(3)) total += v;\nconsole.log(total);\n",
            ),
        ],
    );
    let program = compile_graph_to_object(&entry).expect("a graph with a generator compiles");

    assert!(
        !program.frames.is_empty(),
        "a generator body has a parked-frame shape, and an AOT binary that shipped none \
         answered `async function has no registered frame`"
    );
    for shape in &program.frames {
        assert_eq!(
            shape.code, 0,
            "the address is the linker's to write — the manifest carries no address at all"
        );
        assert!(shape.size > 0, "a frame occupies something");
    }
    tables_agree_with(&program);
}

#[test]
fn every_placed_function_is_named_so_that_length_and_name_can_be_answered() {
    let entry = graph(
        "named",
        &[
            (
                "fns.ts",
                "export function add(a: number, b: number) { return a + b; }\n",
            ),
            (
                "main.ts",
                "import { add } from \"./fns\";\nconsole.log(add.length, add.name);\n",
            ),
        ],
    );
    let program = compile_graph_to_object(&entry).expect("compiles");

    let add = program
        .function_names
        .iter()
        .find(|(name, ..)| name == "add")
        .expect("a declared function is in the table `declare_function_names` seeds");
    assert_eq!(add.1, 2, "`add.length` is its declared arity");
    tables_agree_with(&program);
}

/// A `require("./x")` is answered at RUN time, so its answer has to travel.
///
/// A static `import` is rewritten into the tree while compiling and needs
/// nothing here. `require` and dynamic `import()` ask the runtime, which asks
/// the host's resolver — and a JIT host answers by looking at the disk it just
/// read. An AOT binary has no such disk, so the loader's answers ride the
/// manifest; without them the program links, runs, and dies on
/// `cannot find module "./helper"`.
#[test]
fn a_require_of_a_sibling_file_carries_its_resolved_name() {
    let entry = graph(
        "commonjs",
        &[
            ("helper.js", "module.exports.shout = (s) => s.toUpperCase();\n"),
            (
                "main.ts",
                "const { shout } = require(\"./helper\");\nconsole.log(shout(\"hi\"));\n",
            ),
        ],
    );
    let program = compile_graph_to_object(&entry).expect("a CommonJS graph compiles");

    let (_, written, resolved) = program
        .resolutions
        .iter()
        .find(|(_, written, _)| written == "./helper")
        .expect("the specifier the program wrote is in the table");
    assert_eq!(written, "./helper");
    assert!(
        resolved.ends_with("helper.js"),
        "the loader resolved the extension, and THAT is what the module table is \
         keyed by — a resolver that answered `./helper` would find nothing: {resolved}"
    );
}

#[test]
fn a_single_file_program_still_takes_the_single_file_path() {
    let program = compile_to_object("console.log(1 + 1);\n").expect("compiles");

    assert_eq!(
        program.modules, 0,
        "nothing runs before the entry of a program that is one file"
    );
    assert!(
        program.module_metas.is_empty(),
        "a program compiled from source text has no file, so no `import.meta` to answer"
    );
    assert!(
        program.resolutions.is_empty(),
        "and no file to resolve a relative specifier against"
    );
    // The tables are still THERE, all three of them, and that is the point: the
    // runtime archive names them unconditionally, so a program with no modules
    // and no generators would fail to LINK if an empty table were left out.
    tables_agree_with(&program);
}

/// A program compiled with no `--html` writes the new table EMPTY rather than
/// omitting it — the shape `rts-runtime`'s generic, prebuilt facade needs:
/// `FUNCTION_TABLE_SYMBOL` and its manifest entries are unconditional, so the
/// table this batch adds has to be too, or a program with `--html` and one
/// without would need two different manifest shapes.
#[test]
fn a_program_with_no_html_writes_an_empty_page_scripts_table() {
    let program = compile_to_object("console.log(1 + 1);\n").expect("compiles");
    assert!(
        program.page_scripts.is_empty(),
        "nothing precompiled, nothing to find by hash"
    );
}

/// The claim `rts compile --html` exists to make true: a page `<script>`
/// becomes a function in the SAME object as the main program, findable by the
/// hash of its exact source at the position `FUNCTION_TABLE_SYMBOL` places its
/// address under — which is what `rts-runtime`'s `page_scripts` module reads
/// at run time to answer `context.eval_compiler_with_receiver`.
#[test]
fn a_page_script_is_placed_beside_the_main_program_and_found_by_hash() {
    let script = "document.getElementById(\"x\");\n".to_owned();
    let program = compile_to_object_with_html("console.log(1);\n", std::slice::from_ref(&script))
        .expect("a program with one page script compiles");

    assert_eq!(
        program.page_scripts.len(),
        1,
        "one `--html` script, one manifest entry"
    );
    let (hash, index) = program.page_scripts[0];
    assert_eq!(
        hash,
        rts_core::entry::source_hash(&script),
        "the manifest's hash is the SAME function both `rts compile` and an AOT \
         binary call — a hand-rolled hash here would prove nothing about that"
    );
    assert!(
        (index as usize) < program.function_names.len(),
        "the index names a real row of the function table, not one past its end"
    );
    // Not a NAMED function — `object::page`'s own comment on why it still
    // needs a `function_names` row — but a real one: `object::place`'s filter
    // only carries a `FuncId` that has a body AND a name into the address
    // table, so a missing row here would mean the address table and the
    // manifest's own function count silently disagreeing.
    tables_agree_with(&program);
}

/// Two page scripts share the SAME compilation as the main program and as
/// each other — the property `rts-core`'s README rule 3 is about: a second,
/// unseeded `KeyRegistry` for the scripts would number `document` (or
/// whatever property either reads) as key 0, disagreeing with whatever the
/// main program already calls key 0.
#[test]
fn two_page_scripts_and_the_main_program_share_one_key_numbering() {
    // `f(x)` and not a literal `{ shared: 1 }`: a property whose VALUE the
    // compiler can prove at compile time is exactly the shape this engine's
    // own constant-folding may erase before a key is ever minted for it — a
    // `shared` that disappeared that way would prove the fold worked, not
    // that three bodies number the property alike. Routing it through an
    // opaque function parameter keeps the object real.
    let wrapping = |letter: &str, value: &str| {
        format!("function f{letter}(x: number) {{ return {{ shared: x }}; }}\nconsole.log(f{letter}({value}).shared);\n")
    };
    let scripts = vec![wrapping("a", "1"), wrapping("b", "2")];
    let program = compile_to_object_with_html(&wrapping("c", "3"), &scripts)
        .expect("a main program with two page scripts compiles");

    assert_eq!(program.page_scripts.len(), 2, "both scripts are placed");
    let first = program.page_scripts[0].0;
    let second = program.page_scripts[1].0;
    assert_ne!(first, second, "two different sources hash differently");
    // `shared` was minted ONCE — by whichever of the three bodies asked for it
    // first — and every later use of the same text reuses that key rather
    // than minting a second one. `program.keys` is the compiler's own record
    // of that, in key order.
    let shared_count = program.keys.iter().filter(|key| *key == "shared").count();
    assert_eq!(
        shared_count, 1,
        "one property named `shared`, minted once, read by three bodies — \
         two entries would mean the scripts numbered it separately from the \
         main program"
    );
    tables_agree_with(&program);
}

/// The UMD shape that started this: a bundle writes `React` as a property of
/// `this` (`global.React = {}` inside `(function (global) { … })(this)`,
/// which never spells the bare name `React` anywhere in ITS OWN text), and a
/// SIBLING script reads `React` as a free identifier. Before the dynamic
/// window fallback this batch adds, that read had nowhere to resolve at all
/// — nothing in either script's own text ever assigns `React` bare, so
/// neither `enclosing`/`published` nor `sloppy::created` ever place it, and
/// compiling the sibling failed outright (`UnboundName`).
#[test]
fn a_name_a_sibling_script_writes_only_as_a_property_of_this_still_compiles() {
    let bundle = "(function (global) { global.React = {}; })(this);\n".to_owned();
    let app = "console.log(React);\n".to_owned();
    let program = compile_to_object_with_html("console.log(1);\n", &[bundle, app])
        .expect(
            "a page script reading a name only a SIBLING wrote as a property \
             of `this` must still compile — the language resolves it at run \
             time against the SAME window, not refuse it at build time",
        );
    assert_eq!(program.page_scripts.len(), 2);
    tables_agree_with(&program);
}

#[test]
fn an_import_cycle_is_refused_by_name_rather_than_half_compiled() {
    let entry = graph(
        "cycle",
        &[
            ("a.ts", "import { b } from \"./b\";\nexport const a = b;\n"),
            ("b.ts", "import { a } from \"./a\";\nexport const b = a;\n"),
        ],
    );
    let refused = compile_graph_to_object(&entry);
    assert!(
        refused.is_err(),
        "the object path refuses a cycle for the same reason the in-memory path does: \
         a live binding and its temporal dead zone do not exist here"
    );
}
