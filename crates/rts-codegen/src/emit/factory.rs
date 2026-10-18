//! A call to an object-returning FACTORY, rewritten to the literal it builds.
//!
//! `function mk(x, y) { const o = { a: x, b: y }; return o; }` and then
//! `const r = mk(i, i + 1); s += r.a + r.b` inlines (`inline.rs`) but the object is
//! still allocated: `escape::analyse` is a per-body AST pass and the inlined `o`
//! is not in the caller's tree, so nothing is scalar-replaced (measured ~32.8 ns
//! against ~4 for the same loop written with the literal in place).
//!
//! This pass puts the literal in the caller's tree, so the existing escape
//! analysis can decide. It is the same two-pass shape as `class_layout::unseen`:
//! rewrite every candidate site, ask `escape` about the rewritten body, and put
//! back every site whose result it did not flatten.
//!
//! # Soundness
//!
//! The rewrite alone is meaning-preserving, which is why escaping results stay
//! correct: `const r = mk(a, b)` and `const r = { k1: a, k2: b }` both build ONE
//! fresh ordinary object with the same keys in the same order. The conditions
//! that make them the same program:
//!
//! - `mk` is a candidate of `inline::candidates` (declared once program-wide,
//!   never written, call-free, no free names) AND a top-level function
//!   declaration, so it is bound at every site in the module.
//! - Its answer is a literal with only named `key: value` properties, written
//!   either directly (`return {..}`) or through one local (`const o = {..};
//!   return o`), where every value is a parameter name or a constant.
//! - The parameters appear in the values exactly once each, in parameter
//!   order, so the arguments are evaluated once each in the order they were
//!   written and the constants have no effect to reorder.
//! - The call passes exactly as many plain (non-spread) arguments as there are
//!   parameters, and no parameter has a default.
//!
//! The scalar replacement itself is `escape.rs`'s, whose default is "escapes".

use std::collections::BTreeSet;
use std::rc::Rc;

use super::inline::Inlinable;
use crate::names::Name;
use crate::syntax::{
    BindingKind, Expr, ExprKind, Pattern, Property, PropertyKey, Spreadable, Stmt, StmtKind,
};

type Table = std::collections::BTreeMap<Name, Rc<Inlinable>>;

/// `body` with the factory calls whose result nothing sees written out as
/// literals, or `None` where there is nothing to do.
pub(super) fn rewritten(table: &Table, body: &[Stmt], parameters: &[Name]) -> Option<Vec<Stmt>> {
    if !table.values().any(|held| held.top_level && factory(held).is_some()) {
        return None;
    }
    let mut tried = body.to_vec();
    let mut names = BTreeSet::new();
    in_list(&mut tried, table, None, &mut names);
    if names.is_empty() {
        return None;
    }
    let candidates: Vec<Name> = names.iter().copied().collect();
    let captured = super::capture::captured(&tried, &candidates, &BTreeSet::new());
    let flattened = super::escape::analyse(&tried, parameters, &captured);
    let kept: BTreeSet<Name> = names
        .iter()
        .copied()
        .filter(|name| flattened.properties(*name).is_some())
        .collect();
    if kept.is_empty() {
        return None;
    }
    if kept.len() == names.len() {
        return Some(tried);
    }
    let mut settled = body.to_vec();
    in_list(&mut settled, table, Some(&kept), &mut BTreeSet::new());
    Some(settled)
}

/// The literal a factory answers, if it is one of the admitted shapes.
fn factory(candidate: &Inlinable) -> Option<&[Property]> {
    if candidate.rest_length.is_some()
        || !candidate.free.is_empty()
        || candidate.defaults.iter().any(Option::is_some)
    {
        return None;
    }
    let properties = match (candidate.statements.as_slice(), &candidate.body.kind) {
        ([], ExprKind::Object { properties }) => properties,
        (
            [
                Stmt {
                    kind: StmtKind::Declare { kind, bindings },
                    ..
                },
            ],
            ExprKind::Ident(answered),
        ) => {
            if matches!(kind, BindingKind::Var) {
                return None;
            }
            let [binding] = bindings.as_slice() else {
                return None;
            };
            let Pattern::Name(local) = &binding.target else {
                return None;
            };
            let Some(Expr {
                kind: ExprKind::Object { properties },
                ..
            }) = &binding.value
            else {
                return None;
            };
            if local != answered || candidate.parameters.contains(local) {
                return None;
            }
            properties
        }
        _ => return None,
    };
    // Parameters once each, in order; everything else a constant.
    let mut next = 0;
    let mut keys: Vec<Name> = Vec::new();
    for property in properties {
        let Property::Value {
            key: PropertyKey::Named(key),
            value,
            ..
        } = property
        else {
            return None;
        };
        if keys.contains(key) {
            return None;
        }
        keys.push(*key);
        match &value.kind {
            ExprKind::Ident(name) if candidate.parameters.contains(name) => {
                if candidate.parameters.get(next) != Some(name) {
                    return None;
                }
                next += 1;
            }
            ExprKind::Literal(crate::syntax::Literal::Number(_))
            | ExprKind::Literal(crate::syntax::Literal::String(_))
            | ExprKind::Literal(crate::syntax::Literal::Boolean(_))
            | ExprKind::Literal(crate::syntax::Literal::Singleton(_)) => {}
            _ => return None,
        }
    }
    (next == candidate.parameters.len()).then_some(properties.as_slice())
}

/// `const r = mk(args)` as the literal, when it is a factory call.
fn site(statement: &mut Stmt, table: &Table, only: Option<&BTreeSet<Name>>, names: &mut BTreeSet<Name>) {
    let StmtKind::Declare { kind, bindings } = &mut statement.kind else {
        return;
    };
    if !kind.is_block_scoped() {
        return;
    }
    for binding in bindings.iter_mut() {
        let Pattern::Name(local) = &binding.target else {
            continue;
        };
        if only.is_some_and(|kept| !kept.contains(local)) {
            continue;
        }
        let Some(Expr {
            kind:
                ExprKind::Call {
                    callee,
                    arguments,
                    optional: false,
                },
            at,
        }) = &binding.value
        else {
            continue;
        };
        let ExprKind::Ident(callee) = &callee.kind else {
            continue;
        };
        let Some(candidate) = table.get(callee) else {
            continue;
        };
        if !candidate.top_level || arguments.len() != candidate.parameters.len() {
            continue;
        }
        let Some(properties) = factory(candidate) else {
            continue;
        };
        let mut plain = Vec::with_capacity(arguments.len());
        for argument in arguments {
            match argument {
                Spreadable::Single(value) => plain.push(value),
                Spreadable::Spread(_) => break,
            }
        }
        if plain.len() != arguments.len() {
            continue;
        }
        let at = *at;
        let mut used = 0;
        let built: Vec<Property> = properties
            .iter()
            .map(|property| match property {
                Property::Value {
                    key,
                    value:
                        Expr {
                            kind: ExprKind::Ident(_),
                            ..
                        },
                    ..
                } => {
                    let value = plain[used].clone();
                    used += 1;
                    Property::Value {
                        key: key.clone(),
                        value,
                        shorthand: false,
                    }
                }
                other => other.clone(),
            })
            .collect();
        binding.value = Some(Expr {
            kind: ExprKind::Object { properties: built },
            at,
        });
        names.insert(*local);
    }
}

fn in_list(list: &mut [Stmt], table: &Table, only: Option<&BTreeSet<Name>>, names: &mut BTreeSet<Name>) {
    for statement in list {
        site(statement, table, only, names);
        match &mut statement.kind {
            StmtKind::Block(inner) => in_list(inner, table, only, names),
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                in_list(std::slice::from_mut(&mut **then_branch), table, only, names);
                if let Some(other) = else_branch {
                    in_list(std::slice::from_mut(&mut **other), table, only, names);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. }
            | StmtKind::Labelled { body, .. } => {
                in_list(std::slice::from_mut(&mut **body), table, only, names);
            }
            StmtKind::Switch { clauses, .. } => {
                for clause in clauses {
                    in_list(&mut clause.body, table, only, names);
                }
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                in_list(body, table, only, names);
                if let Some(catch) = catch {
                    in_list(&mut catch.body, table, only, names);
                }
                if let Some(finally) = finally {
                    in_list(finally, table, only, names);
                }
            }
            _ => {}
        }
    }
}
