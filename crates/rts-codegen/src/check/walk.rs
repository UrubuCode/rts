//! The walk, and the rules it applies on the way.
//!
//! One traversal, not one per rule. Every rule here asks about a *scope* — the
//! names a statement list declares, the clauses a switch has — so they all want
//! the same shape of visit, and running each one over its own walk would mean
//! several places that must agree on what a scope is.
//!
//! The walk stops at the first error. A program with an early error is refused
//! whole, so continuing would be gathering facts about something that is not a
//! program.

use super::EarlyError;
use super::scope::{first_repeat, first_shared, lexical_names, var_names};
use crate::names::{Name, Names};
use crate::syntax::{
    Catch, ForEachTarget, ForInit, Function, FunctionBody, ModuleItem, Program, Stmt, StmtKind,
    SwitchClause,
};

pub(crate) struct Checker<'a> {
    names: &'a Names,
    /// The first thing found wrong, which is also the last thing looked for.
    error: Option<EarlyError>,
}

impl<'a> Checker<'a> {
    pub(crate) fn new(names: &'a Names) -> Self {
        Self { names, error: None }
    }

    pub(crate) fn finish(self) -> Option<EarlyError> {
        self.error
    }

    pub(crate) fn program(&mut self, program: &Program) {
        let statements: Vec<Stmt> = program
            .body
            .iter()
            .filter_map(|item| match item {
                ModuleItem::Stmt(statement) => Some(statement.clone()),
                _ => None,
            })
            .collect();
        self.scope(&statements);
    }

    /// Check one statement list as a scope, then descend into it.
    ///
    /// Both halves matter and neither implies the other: the rules below are
    /// about this list, and the statements in it contain lists of their own that
    /// the same rules apply to.
    fn scope(&mut self, statements: &[Stmt]) {
        if self.error.is_some() {
            return;
        }

        let lexical = lexical_names(statements);
        if let Some(name) = first_repeat(&lexical) {
            return self.fail(format!(
                "`{}` is declared twice in the same scope",
                self.names.text(name)
            ));
        }

        let mut vars = Vec::new();
        var_names(statements, &mut vars);
        if let Some(name) = first_shared(&lexical, &vars) {
            return self.fail(format!(
                "`{}` is declared both lexically and with `var` in the same scope",
                self.names.text(name)
            ));
        }

        for statement in statements {
            self.stmt(statement);
        }
    }

    fn stmt(&mut self, statement: &Stmt) {
        if self.error.is_some() {
            return;
        }
        match &statement.kind {
            StmtKind::Block(body) => self.scope(body),
            StmtKind::Labelled { body, .. } => self.stmt(body),
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.stmt(then_branch);
                if let Some(otherwise) = else_branch {
                    self.stmt(otherwise);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::With { body, .. } => self.stmt(body),

            StmtKind::For { init, body, .. } => {
                // The head shares the loop's scope, so a `let` there and a `var`
                // of the same name in the body collide. Wrapping both in one
                // list is how that question gets asked with the rule that
                // already knows how to ask it.
                if let Some(ForInit::Declare { kind, bindings }) = init
                    && kind.is_block_scoped()
                {
                    let mut declared = Vec::new();
                    for binding in bindings {
                        binding.target.bound_names(&mut declared);
                    }
                    if let Some(name) = first_repeat(&declared) {
                        return self.fail(format!(
                            "`{}` is declared twice in the same `for` head",
                            self.names.text(name)
                        ));
                    }
                }
                self.stmt(body);
            }

            StmtKind::ForEach { target, body, .. } => {
                if let ForEachTarget::Declare { kind, target } = target
                    && kind.is_block_scoped()
                {
                    let mut declared = Vec::new();
                    target.bound_names(&mut declared);
                    if let Some(name) = first_repeat(&declared) {
                        return self.fail(format!(
                            "`{}` is declared twice in the same `for` head",
                            self.names.text(name)
                        ));
                    }
                }
                self.stmt(body);
            }

            StmtKind::Switch { clauses, .. } => self.switch(clauses),

            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                self.scope(body);
                if let Some(catch) = catch {
                    self.catch(catch);
                }
                if let Some(finally) = finally {
                    self.scope(finally);
                }
            }

            StmtKind::Function(function) => self.function(function),

            // A class body is a list of methods, each of which is a function,
            // and a scope question about it is a question about those.
            StmtKind::Class(_) => {}

            _ => {}
        }
    }

    /// One switch, whose clauses are a single scope.
    ///
    /// The clauses share a scope — a `let` in one case is visible from the
    /// others, and in its temporal dead zone there — which is why they are
    /// flattened into one list rather than checked one at a time.
    ///
    /// A second `default` is deliberately *not* checked here. SWC already
    /// refuses it, and a rule here would be a second statement of something
    /// already decided — the copy that never runs being the one free to be
    /// wrong. Written, measured as unreachable, and removed.
    fn switch(&mut self, clauses: &[SwitchClause]) {
        let body: Vec<Stmt> = clauses
            .iter()
            .flat_map(|clause| clause.body.iter().cloned())
            .collect();
        self.scope(&body);
    }

    /// A `catch`, whose binding shares the block's scope.
    ///
    /// `try {} catch (e) { let e; }` is an error and `try {} catch (e) { var e; }`
    /// is not, which is exactly the lexical-versus-var line the scope rules
    /// already draw — so the binding is folded into the block's lexical names
    /// rather than checked by a rule of its own.
    fn catch(&mut self, catch: &Catch) {
        let mut bound = Vec::new();
        if let Some(binding) = &catch.binding {
            binding.bound_names(&mut bound);
            if let Some(name) = first_repeat(&bound) {
                return self.fail(format!(
                    "`{}` is bound twice by the same `catch`",
                    self.names.text(name)
                ));
            }
        }

        let lexical = lexical_names(&catch.body);
        if let Some(name) = first_shared(&bound, &lexical) {
            return self.fail(format!(
                "`{}` is declared in a `catch` block that already binds it",
                self.names.text(name)
            ));
        }

        self.scope(&catch.body);
    }

    fn function(&mut self, function: &Function) {
        let mut parameters: Vec<Name> = Vec::new();
        for parameter in &function.parameters {
            parameter.target.bound_names(&mut parameters);
        }
        if let Some(rest) = &function.rest_parameter {
            rest.bound_names(&mut parameters);
        }

        match &function.body {
            FunctionBody::Block(body) => {
                // A parameter and a `let` of the same name collide, always —
                // unlike two parameters of the same name, which only collide in
                // strict code or with a non-simple parameter list, and that is a
                // separate rule with separate conditions.
                let lexical = lexical_names(body);
                if let Some(name) = first_shared(&parameters, &lexical) {
                    return self.fail(format!(
                        "`{}` is a parameter and is declared again in the body",
                        self.names.text(name)
                    ));
                }
                self.scope(body);
            }
            FunctionBody::Expression(_) => {}
        }
    }

    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(EarlyError { message });
        }
    }
}
