//! Turning SOURCE TEXT into a script's body — the one shape every piece of
//! script code this crate compiles takes.
//!
//! Split out of [`crate::run::front_end_agreeing`], which used to hold this
//! inline as its own non-module branch, once a second caller needed exactly
//! the same wrapping: [`crate::object::page`] compiles several page
//! `<script>`s into one AOT object, each one wrapped by this same rule. A
//! second copy of the format string below is a second place for the trailing
//! newline or the async-wrapper choice to be gotten wrong — both were, once,
//! against real files in the corpus, which is why the comment on each survives
//! the move rather than being trimmed.

use rts_codegen::names::Names;
use rts_codegen::parse::parse_script;
use rts_codegen::syntax::{FunctionBody, ModuleItem, Stmt, StmtKind};

use crate::link::HostError;
use crate::run::SCRIPT;

/// Wraps SOURCE TEXT — already stripped of a `#!` line — as a script body and
/// parses it, answering the statements inside: `function __rts_script() { … }`
/// (or the `async` form, when the text awaits at its own top level).
///
/// Never called for a MODULE — `import`/`export` are syntax errors inside the
/// function this wraps with, which is why [`crate::run::front_end_agreeing`]
/// parses one directly instead of reaching here. A page `<script>` never
/// takes that door either: `type="module"` is one of the types `rts-dom`'s
/// `__runScriptAt` still runs as an ordinary script, a pre-existing
/// simplification this function does not change — see
/// `docs/engine/aot-page-scripts.md`.
pub(crate) fn wrap_and_parse_script(source: &str, names: &mut Names) -> Result<Vec<Stmt>, HostError> {
    let wrapper = match source.contains("await ") {
        true => "async function",
        false => "function",
    };
    // The newline before the closing brace is load-bearing. A file ending in a
    // `//` comment with no trailing newline put that brace INSIDE the comment,
    // so the wrapper never closed and the parser reported `Expected '}', got
    // '<eof>'` — twelve files in the corpus, refused for a character this host
    // wrote rather than for anything they contained.
    let wrapped = format!("{wrapper} {SCRIPT}() {{ {source}
 }}");
    let program = parse_script(&wrapped, names)
        .map_err(|error| HostError::Parse(format!("{error:?}")))?;
    // Anything other than the one function declaration means the wrapping did
    // not produce what it was written to produce, which is a defect here
    // rather than in the source.
    let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
        return Err(HostError::Parse(
            "the wrapper did not produce one statement".to_owned(),
        ));
    };
    let StmtKind::Function(function) = &statement.kind else {
        return Err(HostError::Parse(
            "the wrapper did not produce a function".to_owned(),
        ));
    };
    let FunctionBody::Block(body) = &function.body else {
        return Err(HostError::Parse(
            "a declaration always has a block body".to_owned(),
        ));
    };
    Ok(body.clone())
}
