//! The rules about a *set* of names, riding the walk next door.
//!
//! Every one of these asks about a scope — the names a statement list declares,
//! the binding a `catch` adds, what a loop head puts in reach of its body — and
//! a scope is not a node, which is why none of them can live on one. They are
//! methods of [`Scan`] rather than free functions because the answer is a
//! refusal, and there is exactly one place a refusal is recorded.

use super::{Context, Scan, ScopeKind};
use crate::check::scope::{
    Declared, first_illegal_repeat, first_repeat, first_shared, function_names, lexical_names,
    var_names,
};
use crate::names::Name;
use crate::syntax::{Catch, Stmt, StmtKind};

impl Scan<'_> {
    /// One statement list, asked the questions a scope answers.
    ///
    /// Two questions, and they are separate because the sets are: no name may
    /// be declared lexically twice, and no lexically declared name may also be
    /// var-declared anywhere the list reaches. Where function declarations
    /// count is [`ScopeKind`]'s whole subject.
    pub(super) fn scope(&mut self, statements: &[Stmt], kind: ScopeKind, context: Context) {
        self.scope_with(statements, kind, &[], context);
    }

    /// The same, plus names declared by something that is not a statement.
    ///
    /// Only a module has any: its imports bind names at the top level while
    /// being module items rather than statements, so a rule that read the
    /// statement list alone would not see them collide with anything.
    pub(super) fn scope_with(
        &mut self,
        statements: &[Stmt],
        kind: ScopeKind,
        extra: &[Declared],
        context: Context,
    ) {
        if self.found.is_some() {
            return;
        }

        let functions = function_names(statements);
        let mut lexical: Vec<Declared> = extra
            .iter()
            .copied()
            .chain(lexical_names(statements).into_iter().map(|name| Declared {
                name,
                relaxable: false,
            }))
            .collect();
        if kind == ScopeKind::Block {
            lexical.extend(functions.iter().copied());
        }

        if let Some(name) = first_illegal_repeat(&lexical, context.strict) {
            return self.fail(format!(
                "`{}` is declared twice in the same scope",
                self.names.text(name)
            ));
        }

        let mut vars = Vec::new();
        var_names(statements, &mut vars);
        if kind == ScopeKind::VarRoot {
            vars.extend(functions.iter().map(|declared| declared.name));
        }

        let names: Vec<Name> = lexical.iter().map(|declared| declared.name).collect();
        if let Some(name) = first_shared(&names, &vars) {
            return self.fail(format!(
                "`{}` is declared both lexically and with `var` in the same scope",
                self.names.text(name)
            ));
        }
    }

    /// A loop head that declares names, and the body that may not repeat them.
    ///
    /// `for (let x of []) { var x; }` is not a program: the head's names are
    /// lexical and belong to the loop's own scope, which the body is inside —
    /// so a `var` there, which belongs to the enclosing function, would be two
    /// bindings of one name in scopes that nest.
    pub(super) fn loop_head(&mut self, declared: &[Name], body: &Stmt) {
        if self.found.is_some() {
            return;
        }
        if let Some(name) = first_repeat(declared) {
            return self.fail(format!(
                "`{}` is declared twice in the same `for` head",
                self.names.text(name)
            ));
        }
        let mut vars = Vec::new();
        var_names(std::slice::from_ref(body), &mut vars);
        if let Some(name) = first_shared(declared, &vars) {
            return self.fail(format!(
                "`{}` is declared in a `for` head and with `var` in its body",
                self.names.text(name)
            ));
        }
    }

    /// A position that takes a Statement, where a Declaration is not one.
    ///
    /// The body of `if`, of every loop and of `with` is a *Statement*, and
    /// `let`, `const`, `class` and `function` are Declarations — so
    /// `while (false) let [a] = 0;` is not a program even though every piece of
    /// it parses. The rule is not decoration: a declaration there would have a
    /// scope nobody can name, since the body of a loop is not a block.
    ///
    /// `plain_function_allowed` is Annex B B.3.4, which is real and narrow: a
    /// bare `function f() {}` is allowed as the body of an `if` in sloppy code,
    /// and nothing else is allowed anywhere. Not a generator, not an async
    /// function, and not the same declaration behind a label — which is why the
    /// label case recurses with the permission withdrawn.
    pub(super) fn statement_body(&mut self, body: &Stmt, plain_function_allowed: bool, context: Context) {
        if self.found.is_some() {
            return;
        }
        match &body.kind {
            // `var` is a *Statement*, which is why it is the one declaring form
            // allowed here: `for (;;) var x;` is a program and `for (;;) let x;`
            // is not. The two spellings look alike and the grammar puts them in
            // different productions.
            StmtKind::Declare { kind, .. } if kind.is_block_scoped() => {
                self.fail("a lexical declaration cannot be the body of this statement".to_owned());
            }
            StmtKind::Using { .. } | StmtKind::Class(_) => {
                self.fail("a declaration cannot be the body of this statement".to_owned());
            }
            StmtKind::Function(function) => {
                let plain = !function.is_generator && !function.is_async;
                if !(plain_function_allowed && plain && !context.strict) {
                    self.fail(
                        "a function declaration cannot be the body of this statement".to_owned(),
                    );
                }
            }
            StmtKind::Labelled { body, .. } => self.statement_body(body, false, context),
            _ => {}
        }
    }

    /// A `catch`, whose binding shares the block's scope.
    ///
    /// `try {} catch (e) { let e; }` is an error and `try {} catch (e) { var e; }`
    /// is not, which is exactly the lexical-versus-var line the scope rules
    /// already draw — so the binding is folded into the block's lexical names
    /// rather than checked by a rule of its own.
    pub(super) fn catch(&mut self, catch: &Catch, context: Context) {
        let mut bound = Vec::new();
        if let Some(binding) = &catch.binding {
            binding.bound_names(&mut bound);
            if let Some(name) = first_repeat(&bound) {
                return self.fail(format!(
                    "`{}` is bound twice by the same `catch`",
                    self.names.text(name)
                ));
            }
            self.pattern(binding, context);
        }

        let lexical = lexical_names(&catch.body);
        if let Some(name) = first_shared(&bound, &lexical) {
            return self.fail(format!(
                "`{}` is declared in a `catch` block that already binds it",
                self.names.text(name)
            ));
        }

        self.scope(&catch.body, ScopeKind::Block, context);
        self.stmts(&catch.body, context);
    }

}
