//! The pass, over graphs the lowering makes.

use super::*;
use crate::names::Names;
use crate::parse::parse_script;

/// Lowers the only function of `source` and runs the pass over it.
fn replaced(source: &str) -> (Func, Js, Replaced) {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("the fixture parses");
    let resolution = crate::names::resolve::resolve_module(&program.body);
    let function = program
        .body
        .iter()
        .find_map(|item| match item {
            crate::syntax::ModuleItem::Stmt(crate::syntax::Stmt {
                kind: crate::syntax::StmtKind::Function(function),
                ..
            }) => Some(function),
            _ => None,
        })
        .expect("one function");
    let crate::lower::Lowered { mut func, domain } =
        crate::lower::lower(function, &resolution, rts_mir::Tier::Generic).expect("lowered");
    let out = replace_scalars(&mut func, &domain);
    assert_eq!(rts_mir::verify(&func), Ok(()));
    (func, domain, out)
}

fn listed(func: &Func, domain: &Js, which: JsPrim) -> usize {
    func.block_ids()
        .flat_map(|block| func.block(block).insts.clone())
        .filter(|inst| matches!(&func.inst(*inst).op, Op::Prim { prim, .. } if domain.meaning(*prim) == Some(which)))
        .count()
}

/// A literal read by fixed keys and seen by nothing else never becomes an object.
#[test]
fn a_literal_read_by_its_own_keys_leaves_the_graph() {
    let (func, domain, out) =
        replaced("function f(i) { const o = { x: i, y: i + 1 }; return o.x + o.y; }");
    assert_eq!(out, Replaced { allocations: 1, accesses: 2 });
    assert_eq!(listed(&func, &domain, JsPrim::NewObject), 0);
    assert_eq!(listed(&func, &domain, JsPrim::FieldRead), 0);
}

/// A write before a read in straight-line code is the value the read answers.
#[test]
fn a_write_then_a_read_in_one_block_is_the_written_value() {
    let (func, domain, out) =
        replaced("function f(i) { const o = { x: i }; o.y = i * 2; return o.y; }");
    assert_eq!(out.allocations, 1);
    assert_eq!(listed(&func, &domain, JsPrim::FieldWrite), 0);
}

/// Each way the object can be SEEN keeps it: returned, passed, stored, or read by a
/// key it does not own -- which would reach the prototype.
#[test]
fn a_literal_anything_else_can_see_is_kept() {
    for source in [
        "function f(i) { const o = { x: i }; return o; }",
        "function f(i, g) { const o = { x: i }; g(o); return o.x; }",
        "function f(i, k) { const o = { x: i }; k.o = o; return o.x; }",
        "function f(i) { const o = { x: i }; return o.toString; }",
        "function f(i, k) { const o = { x: i }; return o[k]; }",
    ] {
        let (_, _, out) = replaced(source);
        assert_eq!(out.allocations, 0, "{source}");
    }
}

/// An array literal read at constant indices inside it is its elements; a read past
/// its end, or of `length`, keeps it.
#[test]
fn an_array_literal_read_at_its_indices_is_its_elements() {
    let (func, domain, out) = replaced("function f(i) { const xs = [i, i + 1]; return xs[0] + xs[1]; }");
    assert_eq!(out.allocations, 1);
    assert_eq!(listed(&func, &domain, JsPrim::NewArray), 0);
    for source in [
        "function f(i) { const xs = [i]; return xs[3]; }",
        "function f(i) { const xs = [i]; return xs.length; }",
    ] {
        assert_eq!(replaced(source).2.allocations, 0, "{source}");
    }
}

/// A value read out of a replaced literal and then CALLED is the callee the call
/// names: the rewrite reaches a call's callee as well as its arguments. It did not,
/// and the call named a value the pass had removed (`tests/primordial_write.test.ts`).
#[test]
fn a_replaced_read_that_is_called_is_the_callee() {
    let (_, _, out) =
        replaced("function f(g) { let h; ({ h } = { h: g }); return h(1); }");
    assert_eq!(out.allocations, 1);
}
