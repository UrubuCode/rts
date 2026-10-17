//! `x instanceof R` answered at compile time, where the whole chain is known.
//!
//! # The claim, and why each clause is there
//!
//! The ordinary `instanceof` walks `x`'s prototype chain looking for
//! `R.prototype`. That is a question about two things only: the chain `x`
//! starts with, and whether anything has changed a chain or `R.prototype` since.
//! A class with a layout has been proved for exactly the second half already —
//! declared once, never assigned, never handed over, `extends` only another such
//! class by a bare name — and `setters_reachable` has ruled out every spelling
//! that relinks a prototype or defines a property on one. So for a layout class
//! `C` the chain of `new C()` is `C.prototype`, `Parent.prototype`, …,
//! `Object.prototype`, and it is written in the declarations.
//!
//! The answer is folded only when ALL of these hold, each excluding a way the
//! runtime would answer differently:
//!
//! - **`R` and every class in `C`'s ancestry is an `ordinary` class with a
//!   layout.** `instanceof` defers to `R[Symbol.hasInstance]`, which a static
//!   subclass INHERITS, so the whole of `R`'s own `extends` chain is asked, not
//!   `R` alone. A class that spells a static computed key or a static block is
//!   not ordinary and is left to the runtime.
//! - **The chain is complete.** Each link is `extends Ident(parent)` of another
//!   such class; a heritage that is a call (`extends mixin(Base)`), `null` or
//!   an unknown name ends the walk with nothing recorded, and neither answer is
//!   given. `false` is only ever said when the chain ran to a root with no
//!   `extends`, so that `R` being absent from it is a fact and not a gap.
//! - **`x`'s class is syntactic.** `new C(…)` itself (or the born literal
//!   `{ __proto__: C.prototype, … }` the layout pass may have rewritten it to,
//!   which has the same prototype), or a `const`/`let` declared once in the
//!   program with exactly that initialiser and never written again. A parameter,
//!   a call result or a property read has an unknown prototype and is not asked.
//! - **Nothing relinks.** `Object` and `Reflect` appear in the program only as
//!   the base of a member read (`primordial::only_a_base`), and no program
//!   text spells `setPrototypeOf`, `__proto__`, `defineProperty` or a computed
//!   member of them (`setters_reachable`): so no alias of `Object` reaches a
//!   relink through a key built at run time. Both are checked by the caller
//!   through `layouts_allowed`, plus the `Object`/`Reflect` clause here.
//! - **Names are unique.** `R`, `C` and the binding are each declared exactly
//!   once in the whole program, so a spelling at the site names one thing; a
//!   name an import or an `export { X as Y }` can bind to something else is
//!   removed by the caller before this is built.
//!
//! A class declared inside a function is a different class per call. That is
//! harmless here: `R`, `C` and the binding are all lexical ancestors of the
//! site, so they resolve inside the same activation chain — a class and an
//! instance cannot come from different calls of the function that declares the
//! class.
//!
//! A binding read before its declaration runs is a `ReferenceError` in the
//! language and `undefined` here; the operand is still emitted, so that read
//! happens, but the answer is the folded one. That difference is the engine's
//! for every layout rewrite and is not widened by this.

use super::*;

/// What the fold knows: the full ancestry of every class that has one, and the
/// class each once-assigned binding holds.
#[derive(Default)]
pub(in crate::emit) struct Fold {
    /// `C` -> `[C, Parent, …, Root]`, only where every link qualified.
    chains: BTreeMap<Name, Vec<Name>>,
    /// `d` -> `C` for `const|let d = new C(…)` declared once and never written.
    bound: BTreeMap<Name, Name>,
}

impl Fold {
    /// What `left instanceof right` is, or `None` where the runtime must answer.
    pub(in crate::emit) fn decide(&self, left: &Expr, right: &Expr, names: &Names) -> Option<bool> {
        if self.chains.is_empty() {
            return None;
        }
        let ExprKind::Ident(wanted) = &right.kind else {
            return None;
        };
        // `R` must itself be a fully known ordinary class; its entry exists
        // only if its own ancestry (where a static `hasInstance` would hide)
        // qualified.
        self.chains.get(wanted)?;
        let class = self.class_of(left, names)?;
        let chain = self.chains.get(&class)?;
        Some(chain.contains(wanted))
    }

    fn class_of(&self, expr: &Expr, names: &Names) -> Option<Name> {
        match &expr.kind {
            ExprKind::New { callee, .. } => match &callee.kind {
                ExprKind::Ident(class) => Some(*class),
                _ => None,
            },
            ExprKind::Ident(binding) => self.bound.get(binding).copied(),
            // The born form: `{ __proto__: C.prototype, … }`.
            ExprKind::Object { properties } => match properties.first() {
                Some(Property::Prototype(Expr {
                    kind:
                        ExprKind::Member {
                            object,
                            property,
                            optional: false,
                        },
                    ..
                })) if names.text(*property) == "prototype" => match &object.kind {
                    ExprKind::Ident(class) => Some(*class),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
}

/// The fold's facts for `program`, or an empty fold where none can hold.
///
/// `excluded` are the names an import or a renaming export can bind to
/// something other than the class of that spelling.
pub(in crate::emit) fn facts(
    program: &[Stmt],
    layouts: &BTreeMap<Name, Rc<Layout>>,
    declared: &Declarations,
    writes: &Disturbed,
    ordinary: &BTreeSet<Name>,
    excluded: &BTreeSet<Name>,
) -> Fold {
    let mut found = Vec::new();
    for statement in program {
        classes_in(statement, &mut found);
    }
    let classes: BTreeMap<Name, &Class> = found
        .into_iter()
        .filter_map(|class| class.name.map(|name| (name, class)))
        .filter(|(name, _)| {
            layouts.contains_key(name)
                && declared.count(*name) == 1
                && writes.untouched(*name)
                && ordinary.contains(name)
                && !excluded.contains(name)
        })
        .collect();
    let mut chains = BTreeMap::new();
    for (name, class) in &classes {
        let mut chain = vec![*name];
        let mut at = *class;
        loop {
            match &at.heritage {
                None => {
                    chains.insert(*name, chain);
                    break;
                }
                Some(Expr {
                    kind: ExprKind::Ident(parent),
                    ..
                }) => {
                    // A cycle or an unqualified parent leaves the class out:
                    // its chain is not fully known.
                    let Some(next) = classes.get(parent) else {
                        break;
                    };
                    if chain.contains(parent) {
                        break;
                    }
                    chain.push(*parent);
                    at = next;
                }
                Some(_) => break,
            }
        }
    }
    let mut bound = BTreeMap::new();
    for statement in program {
        bindings_in(statement, &chains, declared, writes, excluded, &mut bound);
    }
    Fold { chains, bound }
}

fn bindings_in(
    statement: &Stmt,
    chains: &BTreeMap<Name, Vec<Name>>,
    declared: &Declarations,
    writes: &Disturbed,
    excluded: &BTreeSet<Name>,
    bound: &mut BTreeMap<Name, Name>,
) {
    if let StmtKind::Declare { kind, bindings } = &statement.kind
        && *kind != crate::syntax::BindingKind::Var
    {
        for binding in bindings {
            if let (
                Pattern::Name(name),
                Some(Expr {
                    kind: ExprKind::New { callee, .. },
                    ..
                }),
            ) = (&binding.target, &binding.value)
                && let ExprKind::Ident(class) = &callee.kind
                && chains.contains_key(class)
                && declared.count(*name) == 1
                && writes.untouched(*name)
                && !excluded.contains(name)
            {
                bound.insert(*name, *class);
            }
        }
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => bindings_in(inner, chains, declared, writes, excluded, bound),
        StmtChild::Catch(clause) => {
            for inner in &clause.body {
                bindings_in(inner, chains, declared, writes, excluded, bound);
            }
        }
        StmtChild::Function(function) => {
            if let FunctionBody::Block(body) = &function.body {
                for inner in body {
                    bindings_in(inner, chains, declared, writes, excluded, bound);
                }
            }
        }
        _ => {}
    });
}
