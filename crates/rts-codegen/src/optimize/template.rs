//! A template's conversions, removed where the inference proved they run nothing.
//!
//! `lower/template.rs` converts each substitution with `StringOf` right after it is
//! evaluated and hands the strings to `TemplateJoin`. The order is the language's:
//! ToString over an object runs its `toString`, and a later substitution may read what
//! that did. But over a PRIMITIVE it runs nothing, and `TemplateJoin` converts what it
//! is handed itself -- so each such conversion is a crossing and a string allocated to
//! be read once and dropped, where handing the value on costs neither.
//!
//! `bench/analytic.ts` `template 2 holes` is the row that said so: 792 ns through this
//! stage against 314 through the running emitter, whose `emit/template.rs` hands a
//! proven number straight to the join and converts nothing first.
//!
//! # After the inference, and why that is not the other passes' order
//!
//! Every other pass here runs BEFORE the inference, and `optimize` says why: so it
//! reads the graph the machine will. This one cannot, because what it removes is only
//! removable where a type was PROVED, and a loop's counter is only a number once the
//! back edge has been joined. So the door infers, runs this, and infers again when it
//! changed anything.
//!
//! Rejected: making the machine skip the call when it lowers `StringOf`. Its result has
//! to be a string for every reader but the join, so the machine would have to ask who
//! reads it -- a use count held in the one layer that has no business counting.

use std::collections::{BTreeMap, BTreeSet};

use rts_mir::cfg::{Callee, Func, InstId, Op, ValueId};
use rts_mir::infer::Types;

use crate::domain::{Js, Type};
use crate::runtime::RuntimeOp;

/// Hands every proven primitive straight to its template's join, and says how many
/// conversions that removed.
pub fn fuse_templates(func: &mut Func, domain: &Js, types: &Types<Type>) -> usize {
    let entry_of = |func: &Func, inst: InstId| match &func.inst(inst).op {
        Op::Call {
            callee: Callee::Entry(entry),
            ..
        } => domain.entry_meaning(*entry),
        _ => None,
    };
    let mut defined: BTreeMap<ValueId, InstId> = BTreeMap::new();
    let mut readers: BTreeMap<ValueId, usize> = BTreeMap::new();
    for block in func.block_ids() {
        for &inst in &func.block(block).insts {
            defined.insert(func.inst(inst).result, inst);
            for read in func.reads(inst) {
                *readers.entry(read).or_default() += 1;
            }
        }
        if let Some(end) = &func.block(block).terminator {
            for read in end.reads() {
                *readers.entry(read).or_default() += 1;
            }
        }
    }

    let mut dropped = BTreeSet::new();
    for block in func.block_ids() {
        for inst in func.block(block).insts.clone() {
            if entry_of(func, inst) != Some(RuntimeOp::TemplateJoin) {
                continue;
            }
            let Op::Call { args, .. } = &func.inst(inst).op else {
                continue;
            };
            // The site and the count first, then the substitutions.
            let mut handed = args.clone();
            for slot in handed.iter_mut().skip(2) {
                let Some(&conversion) = defined.get(slot) else {
                    continue;
                };
                if entry_of(func, conversion) != Some(RuntimeOp::StringOf)
                    || readers.get(slot) != Some(&1)
                {
                    continue;
                }
                let Op::Call { args: of, .. } = &func.inst(conversion).op else {
                    continue;
                };
                let [value] = of.as_slice() else {
                    continue;
                };
                if !runs_nothing(types.of(*value)) {
                    continue;
                }
                *slot = *value;
                dropped.insert(conversion);
            }
            if let Op::Call { args, .. } = &mut func.insts[inst.0 as usize].op {
                *args = handed;
            }
        }
    }
    rts_mir::passes::unlist(func, &dropped);
    dropped.len()
}

/// Whether ToString over a value of this type is a spelling rather than a call: every
/// primitive but a symbol, which raises -- and which this lattice does not name, so it
/// is inside `Anything` and never here.
fn runs_nothing(of: &Type) -> bool {
    matches!(
        of,
        Type::Nothing
            | Type::Undefined
            | Type::Null
            | Type::Bool(_)
            | Type::Int32
            | Type::Double
            | Type::Str
            | Type::BigInt
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{ExprKind, ModuleItem, Stmt, StmtKind};

    /// Lowers the one function of `source`, with a join site minted for the template its
    /// last statement returns, then infers and fuses.
    fn fused(source: &str) -> (usize, Func, Js) {
        let mut names = crate::names::Names::new();
        let program = crate::parse::parse_script(source, &mut names).expect("parses");
        let resolution = crate::names::resolve::resolve_module(&program.body);
        let function = program
            .body
            .iter()
            .find_map(|item| match item {
                ModuleItem::Stmt(Stmt {
                    kind: StmtKind::Function(function),
                    ..
                }) => Some(function),
                _ => None,
            })
            .expect("one function");
        let Some(Stmt {
            kind: StmtKind::Return(Some(returned)),
            ..
        }) = function.body.statements().and_then(|body| body.last())
        else {
            panic!("the function ends by returning its template");
        };
        assert!(matches!(returned.kind, ExprKind::Template { .. }));
        let callees = crate::lower::Callees::default()
            .with_templates(BTreeMap::from([(returned.at, 0)]));
        let mut domain = Js::new();
        let mut func = crate::lower::lower_with(
            function,
            &resolution,
            &callees,
            &mut domain,
            &names,
            rts_mir::Tier::Generic,
        )
        .expect("lowers");
        let types = rts_mir::infer::infer(&func, &domain);
        let removed = fuse_templates(&mut func, &domain, &types);
        assert_eq!(rts_mir::verify(&func), Ok(()));
        (removed, func, domain)
    }

    fn conversions(func: &Func, domain: &Js) -> usize {
        func.block_ids()
            .flat_map(|block| func.block(block).insts.clone())
            .filter(|&inst| match &func.inst(inst).op {
                Op::Call {
                    callee: Callee::Entry(entry),
                    ..
                } => domain.entry_meaning(*entry) == Some(RuntimeOp::StringOf),
                _ => false,
            })
            .count()
    }

    /// A number and a string are handed to the join as they are: converting either
    /// first runs nothing and allocates a string read once.
    #[test]
    fn a_proven_primitive_reaches_the_join_unconverted() {
        let (removed, func, domain) =
            fused("function f() { let i = 0; i = i + 1; return `v=${i}!${'s'}?`; }");
        assert_eq!(removed, 2);
        assert_eq!(conversions(&func, &domain), 0);
    }

    /// A parameter may be an object whose `toString` a later substitution observes, so
    /// its conversion stays where the language evaluates it -- between the two.
    #[test]
    fn a_value_that_may_be_an_object_keeps_its_conversion_in_order() {
        let (removed, func, domain) = fused("function f(o) { return `${o}${1}`; }");
        assert_eq!(removed, 1);
        assert_eq!(conversions(&func, &domain), 1);
    }
}
