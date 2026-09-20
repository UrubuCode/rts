//! `rts mir` — every function of a program as MIR, or why it does not lower yet.
//!
//! `rts ir` reads the machine's representation and `rts prove` counts over it.
//! This is the same instrument one stage earlier, and the stage matters: after
//! lowering, a type domain's proof has become an offset and a guard has become a
//! branch, so the question "what did the analysis know here" has no answer left to
//! read. Before it, the answer is the graph.
//!
//! # Why the refusals are printed and not hidden
//!
//! The lowering covers a subset and names what it does not cover
//! ([`crate::lower::Unsupported`]). Printing those beside the graphs makes the
//! output a work queue: what a real program is refused for, counted, is which
//! lowering to write next. A dump that showed only what worked would report
//! coverage the stage does not have — which `lower.rs`'s own header calls a claim
//! wearing a measurement's clothes.

use rts_mir::guard::Tier;
use rts_mir::text::print;

use crate::lower::{Unsupported, lower};
use crate::names::Names;
use crate::names::resolve::resolve_module;
use crate::syntax::{Function, ModuleItem, Stmt, StmtKind};

/// Every function of a module, lowered and printed.
///
/// Answers the text rather than writing it, so that the caller decides where it
/// goes — `rts ir` printed to stderr once, which made redirecting its output write
/// an empty file.
pub fn describe(source: &str) -> Result<String, String> {
    let mut names = Names::new();
    let program = crate::parse::parse_script(source, &mut names).map_err(|held| format!("{held}"))?;
    let resolution = resolve_module(&program.body);

    let mut out = String::new();
    let mut found = 0usize;
    let mut refused = Vec::new();
    for item in &program.body {
        let ModuleItem::Stmt(Stmt {
            kind: StmtKind::Function(function),
            ..
        }) = item
        else {
            continue;
        };
        found += 1;
        let named = match function.name {
            Some(name) => names.text(name).to_owned(),
            None => "<anonymous>".to_owned(),
        };
        match lower(function, &resolution, Tier::Generic) {
            Ok(lowered) => {
                out.push_str(&format!("fn {named}\n"));
                out.push_str(&print(&lowered.func, &lowered.domain));
                out.push('\n');
            }
            Err(held) => {
                out.push_str(&format!("fn {named} — NOT LOWERED: {}\n\n", why(&held, &names)));
                refused.push(held);
            }
        }
    }

    if found == 0 {
        return Ok("no top-level function declarations; the MIR stage lowers one function at a time\n".to_owned());
    }
    out.push_str(&format!(
        "{} of {found} function{} lowered\n",
        found - refused.len(),
        match found {
            1 => "",
            _ => "s",
        }
    ));
    Ok(out)
}

/// A refusal, as a sentence naming what it was.
fn why(held: &Unsupported, names: &Names) -> String {
    match held {
        Unsupported::Statement(what) => format!("statement — {what}"),
        Unsupported::Expression(what) => format!("expression — {what}"),
        Unsupported::Operator(op) => format!("operator — {op:?} has no row in the primitive table"),
        Unsupported::Pattern => "a destructuring target".to_owned(),
        Unsupported::Global(name) => {
            format!("`{}` is a global, which needs an entry point", names.text(*name))
        }
        Unsupported::Shape(what) => format!("shape — {what}"),
        Unsupported::NoScope => "no scope was resolved for it".to_owned(),
    }
}

/// The same, for one function held directly.
///
/// Here so that a caller with a `&Function` — a test, or a future pass driver —
/// does not have to go through a source string to reach the graph.
pub fn describe_function(
    function: &Function,
    resolution: &crate::names::resolve::Resolution,
) -> Result<String, Unsupported> {
    let lowered = lower(function, resolution, Tier::Generic)?;
    Ok(print(&lowered.func, &lowered.domain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lowered_function_prints_its_graph_with_this_languages_names() {
        let printed = describe("function f(x) { const two = 2; return x - two; }")
            .expect("the fixture parses");
        assert!(printed.contains("fn f"), "{printed}");
        // The primitive is named by THIS language's table, which is the other half
        // of the IR refusing to interpret an index.
        assert!(printed.contains("subtract(v0, v1)"), "{printed}");
        assert!(printed.contains("1 of 1 function lowered"), "{printed}");
    }

    /// An operation over an unknown operand carries its effect, which is the thing
    /// this form exists to make readable.
    #[test]
    fn the_effect_of_an_unproven_operation_is_visible_in_the_dump() {
        let printed = describe("function f(x) { return x + 1; }").expect("parses");
        assert!(printed.contains("calls|throws"), "{printed}");
    }

    /// And over proven operands it is absent, so the two read differently at a
    /// glance -- which is the whole reason the suffix is omitted when pure.
    #[test]
    fn a_proven_operation_prints_no_effect_at_all() {
        let printed = describe("function f() { return 1 + 2; }").expect("parses");
        assert!(!printed.contains("calls"), "{printed}");
        assert!(printed.contains("add(v0, v1)"), "{printed}");
    }

    #[test]
    fn a_refusal_is_printed_as_a_work_queue_and_counted() {
        let printed = describe(
            "function ok() { return 1; }
             function nope(a, b) { return a > b; }",
        )
        .expect("parses");
        assert!(printed.contains("fn nope — NOT LOWERED: operator"), "{printed}");
        assert!(printed.contains("1 of 2 functions lowered"), "{printed}");
    }

    #[test]
    fn a_global_is_named_in_its_refusal() {
        let printed = describe("function f() { return Math; }").expect("parses");
        assert!(printed.contains("`Math` is a global"), "{printed}");
    }

    #[test]
    fn a_program_with_no_function_says_so_rather_than_printing_nothing() {
        let printed = describe("const a = 1;").expect("parses");
        assert!(printed.contains("no top-level function"), "{printed}");
    }

    /// A loop's header, its back edge and its exit, all readable -- which is what
    /// the command is for.
    #[test]
    fn a_loop_prints_its_header_and_its_back_edge() {
        let printed = describe(
            "function f() { let at = 0; while (at < 3) { at = at + 1; } return at; }",
        )
        .expect("parses");
        assert!(printed.contains("branch"), "{printed}");
        assert!(printed.contains("lessthan"), "{printed}");
        // The header is jumped to from two places, and the dump says so.
        assert!(printed.contains("; from b0, b"), "{printed}");
    }
}
