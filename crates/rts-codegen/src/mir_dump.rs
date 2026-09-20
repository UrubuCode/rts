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
use rts_mir::passes::refine_effects;
use rts_mir::text::print;

use crate::lower::{Unsupported, lower};
use crate::names::Names;
use crate::names::resolve::resolve_module;
use crate::syntax::Function;

/// Every function of a module, lowered and printed.
///
/// Answers the text rather than writing it, so that the caller decides where it
/// goes — `rts ir` printed to stderr once, which made redirecting its output write
/// an empty file.
pub fn describe(source: &str) -> Result<String, String> {
    let mut names = Names::new();
    // AS A MODULE FIRST, and this order was a finding rather than a choice.
    //
    // The first version parsed a script, and running it over the corpus answered
    // `PARSE_FAIL` for 182 of 182 files: every one of them imports `rts:test`, so
    // the instrument measured nothing at all. A snippet typed at the command line
    // is usually a script, which is why the fallback exists — but a FILE is
    // usually a module, and the dump is for files.
    let program = match crate::parse::parse_module(source, &mut names) {
        Ok(program) => program,
        Err(module) => crate::parse::parse_script(source, &mut names)
            .map_err(|script| format!("as a module: {module}\nas a script: {script}"))?,
    };
    let resolution = resolve_module(&program.body);

    let lowered_module = crate::lower_module::lower_module(&program.body, &resolution, &names, Tier::Generic);
    let found = lowered_module.functions.len();
    let mut domain = lowered_module.domain;
    let mut out = String::new();
    let mut refused = 0usize;
    for entry in lowered_module.functions {
        match entry.result {
            Ok(mut func) => {
                // The pass runs BEFORE printing, and its count is printed with it.
                // Reading the unrefined form is what found the pass worth writing --
                // every operation of a numeric loop marked as possibly calling user
                // code -- but showing it now would be showing a graph no pass will
                // ever see.
                let refined = refine_effects(&mut func, &domain);
                out.push_str(&format!("fn {}
", entry.named));
                out.push_str(&print(&func, &Spelled((&domain, &names, &resolution))));
                if refined.narrowed > 0 || refined.refused > 0 {
                    out.push_str(&format!(
                        "; {} effect{} narrowed by inference{}
",
                        refined.narrowed,
                        match refined.narrowed {
                            1 => "",
                            _ => "s",
                        },
                        match refined.refused {
                            0 => String::new(),
                            held => format!(", {held} refused as wider"),
                        }
                    ));
                }
                out.push(0x0a as char);
            }
            Err(held) => {
                out.push_str(&format!("fn {} — NOT LOWERED: {}

", entry.named, why(&held, &names)));
                refused += 1;
            }
        }
    }
    let _ = &mut domain;

    if found == 0 {
        return Ok("no function in this program; the MIR stage lowers one function at a time
".to_owned());
    }
    out.push_str(&format!(
        "{} of {found} function{} lowered
",
        found - refused,
        match found {
            1 => "",
            _ => "s",
        }
    ));
    Ok(out)
}

/// The domain, plus the interner that can spell a key.
///
/// `domain::Js` implements the legend on its own and prints a key as its INDEX,
/// because it holds a `Name` and the text of a name lives in `Names`. That is the
/// honest answer for a crate that does not hold the interner, and it is nearly
/// useless to read: a dump full of `key#2` cannot say which property an access
/// touches. This wrapper has both, so the command prints the property.
///
/// A wrapper rather than putting the interner in the domain: the domain travels
/// with a graph into passes, and a pass has no business spelling anything.
struct Spelled<PLACE>(PLACE);

impl rts_mir::text::Legend
    for Spelled<(&crate::domain::Js, &Names, &crate::names::resolve::Resolution)>
{
    fn prim(&self, prim: rts_mir::Prim) -> String {
        rts_mir::text::Legend::prim(self.0.0, prim)
    }

    fn assertion(&self, assertion: rts_mir::Assertion) -> String {
        rts_mir::text::Legend::assertion(self.0.0, assertion)
    }

    fn entry(&self, entry: rts_mir::cfg::EntryId) -> String {
        rts_mir::text::Legend::entry(self.0.0, entry)
    }

    fn declared(&self, index: u32) -> String {
        match crate::domain::Js::declared(self.0.0, index) {
            Some(crate::domain::JsConst::Key(name)) => format!(".{}", self.0.1.text(*name)),
            Some(crate::domain::JsConst::Text(text)) => format!("{:?}", text.to_string()),
            Some(crate::domain::JsConst::Binding(held)) => {
                let binding = crate::names::resolve::BindingId::from_index(*held as usize);
                match self.0.2.len() > binding.index() {
                    true => format!("@{}", self.0.1.text(self.0.2.binding(binding).name)),
                    false => format!("@binding#{held}"),
                }
            }

            _ => rts_mir::text::Legend::declared(self.0.0, index),
        }
    }
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
        assert!(printed.contains("no function in this program"), "{printed}");
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
    /// The pass, end to end and in the form a reader sees. Before it existed this
    /// same loop printed three operations marked as possibly calling user code, on
    /// a program whose values are all numbers.
    #[test]
    fn a_numeric_loop_comes_out_with_no_pessimistic_effects_left() {
        let printed = describe(
            "function count() { let at = 0; while (at < 10) { at = at + 1; } return at; }",
        )
        .expect("parses");
        assert!(!printed.contains("calls"), "{printed}");
        assert!(printed.contains("2 effects narrowed by inference"), "{printed}");
    }

    /// And an operand that really is unknown keeps its effect, which is what says
    /// the pass narrows from evidence rather than by assumption.
    #[test]
    fn a_loop_over_a_parameter_keeps_the_effect_the_parameter_forces() {
        let printed = describe(
            "function total(n) { let at = 0; while (at < n) { at = at + 1; } return at; }",
        )
        .expect("parses");
        // The comparison against the parameter stays, the addition does not.
        assert!(printed.contains("lessthan(v2, v0)   ; calls|throws"), "{printed}");
        assert!(printed.contains("1 effect narrowed by inference"), "{printed}");
    }
}
