//! `await` and `yield`: reserved by where you are, not by what they spell.
//!
//! Neither is a keyword. `var await = 1;` is a perfectly good program, and so is
//! `function yield() {}` — until the code is inside an async function or a
//! generator, where the same text is a syntax error. So this cannot be decided
//! by the lexer, which sees only the word, nor by the tree, which holds an
//! identifier either way. It is decided by a walk that carries the context down.
//!
//! # What the context is, and where it changes
//!
//! Set by the *nearest* function — with two exceptions, and they are the whole
//! reason this is not a one-line check.
//!
//! An arrow inherits: `async function f() { const g = () => await; }` is an
//! error, because an arrow has no `await` of its own to shadow the outer one,
//! exactly as it has no `this`. An ordinary nested function resets, because it
//! does have one.
//!
//! A class static block reserves `await` too, and reserves it *less far*:
//! `ContainsAwait` does not descend into an arrow body, so
//! `static { (() => ({ await })); }` is a valid program. Two reasons to forbid
//! the same word that reach different distances — which is why [`Context`]
//! carries them separately.
//!
//! And a function *expression*'s own name belongs to the scope inside it, not
//! the one around it. `function*() { (function yield() {}); }` is valid;
//! `function* g() { function yield() {} }` is not.
//!
//! # What it looks at
//!
//! Every identifier that names something: a reference, a binding, a label. Not
//! a property key — `o.await` is a property, and always legal — which is why
//! this walks the tree rather than scanning the text.

use crate::names::{Name, Names};
use crate::syntax::{
    Catch, Class, ClassElement, ClassKey, Element, Expr, ExprKind, ForEachTarget, ForInit,
    Function, FunctionBody, Pattern, Property, PropertyKey, Stmt, StmtKind,
};

/// Which context a piece of code is in.
///
/// Three flags for two words, because `await` is reserved by two different
/// rules that reach different distances. Inside an async function it is
/// reserved all the way down through arrows, because an arrow has no `await` of
/// its own. Inside a class static block it is reserved only in the block —
/// `ContainsAwait` does not descend into an arrow body, so
/// `static { (() => ({ await })); }` is a valid program. Collapsing the two into
/// one flag refuses that program, which is how this was found.
#[derive(Clone, Copy)]
pub(super) struct Context {
    /// `await` may not name anything, here or in any arrow within.
    no_await: bool,
    /// `await` may not name anything here, but an arrow within is free of it.
    no_await_outside_arrow: bool,
    /// `yield` may not name anything.
    no_yield: bool,
}

impl Context {
    /// The context a script's top level is in: nothing is reserved.
    pub(super) fn sloppy() -> Self {
        Self {
            no_await: false,
            no_await_outside_arrow: false,
            no_yield: false,
        }
    }

    /// A class static block, and a field initialiser.
    fn static_block() -> Self {
        Self {
            no_await: false,
            no_await_outside_arrow: true,
            no_yield: false,
        }
    }

    /// The context inside a function, given the one it was written in.
    ///
    /// An arrow inherits what reaches through it and adds its own; anything
    /// else replaces both. That difference is the rule, and putting it here
    /// means no caller can get it half right.
    fn inside(self, function: &Function) -> Self {
        if function.captures_this {
            Self {
                no_await: self.no_await || function.is_async,
                no_await_outside_arrow: false,
                no_yield: self.no_yield || function.is_generator,
            }
        } else {
            Self {
                no_await: function.is_async,
                no_await_outside_arrow: false,
                no_yield: function.is_generator,
            }
        }
    }

    fn forbids_await(self) -> bool {
        self.no_await || self.no_await_outside_arrow
    }
}

/// Look for a reserved word used as a name, and say which.
pub(super) struct Scan<'a> {
    names: &'a Names,
    found: Option<&'static str>,
}

impl<'a> Scan<'a> {
    pub(super) fn new(names: &'a Names) -> Self {
        Self { names, found: None }
    }

    pub(super) fn finish(self) -> Option<&'static str> {
        self.found
    }

    /// One identifier, in the context it was written in.
    ///
    /// Compares the text rather than a pre-interned name, because interning is
    /// a mutation and this only reads. A program that never wrote either word
    /// pays two string comparisons per identifier for the privilege, which is
    /// what a checker costs.
    fn name(&mut self, name: Name, context: Context) {
        if self.found.is_some() {
            return;
        }
        match self.names.text(name) {
            "await" if context.forbids_await() => self.found = Some("await"),
            "yield" if context.no_yield => self.found = Some("yield"),
            _ => {}
        }
    }

    pub(super) fn stmts(&mut self, statements: &[Stmt], context: Context) {
        for statement in statements {
            self.stmt(statement, context);
        }
    }

    fn stmt(&mut self, statement: &Stmt, context: Context) {
        if self.found.is_some() {
            return;
        }
        match &statement.kind {
            StmtKind::Expr(expression) | StmtKind::Throw(expression) => {
                self.expr(expression, context);
            }
            StmtKind::Declare { bindings, .. } | StmtKind::Using { bindings, .. } => {
                for binding in bindings {
                    self.pattern(&binding.target, context);
                    if let Some(value) = &binding.value {
                        self.expr(value, context);
                    }
                }
            }
            StmtKind::Block(body) => self.stmts(body, context),
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition, context);
                self.stmt(then_branch, context);
                if let Some(otherwise) = else_branch {
                    self.stmt(otherwise, context);
                }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
                self.expr(condition, context);
                self.stmt(body, context);
            }
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                match init {
                    Some(ForInit::Declare { bindings, .. }) => {
                        for binding in bindings {
                            self.pattern(&binding.target, context);
                            if let Some(value) = &binding.value {
                                self.expr(value, context);
                            }
                        }
                    }
                    Some(ForInit::Expr(expression)) => self.expr(expression, context),
                    None => {}
                }
                if let Some(test) = test {
                    self.expr(test, context);
                }
                if let Some(update) = update {
                    self.expr(update, context);
                }
                self.stmt(body, context);
            }
            StmtKind::ForEach {
                target,
                subject,
                body,
                ..
            } => {
                match target {
                    ForEachTarget::Declare { target, .. } | ForEachTarget::Assign(target) => {
                        self.pattern(target, context);
                    }
                }
                self.expr(subject, context);
                self.stmt(body, context);
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value, context);
                }
            }
            // A label is an identifier and follows the same rule: `await: ;` in
            // an async function is an error for the same reason `var await` is.
            StmtKind::Break(label) | StmtKind::Continue(label) => {
                if let Some(label) = label {
                    self.name(*label, context);
                }
            }
            StmtKind::Labelled { label, body } => {
                self.name(*label, context);
                self.stmt(body, context);
            }
            StmtKind::Switch {
                discriminant: subject,
                clauses,
            } => {
                self.expr(subject, context);
                for clause in clauses {
                    if let Some(test) = &clause.test {
                        self.expr(test, context);
                    }
                    self.stmts(&clause.body, context);
                }
            }
            StmtKind::With {
                object: subject,
                body,
            } => {
                self.expr(subject, context);
                self.stmt(body, context);
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                self.stmts(body, context);
                if let Some(Catch { binding, body }) = catch {
                    if let Some(binding) = binding {
                        self.pattern(binding, context);
                    }
                    self.stmts(body, context);
                }
                if let Some(finally) = finally {
                    self.stmts(finally, context);
                }
            }
            StmtKind::Function(function) => self.function(function, context, true),
            StmtKind::Class(class) => self.class(class, context),
            StmtKind::Debugger | StmtKind::Empty => {}
        }
    }

    fn expr(&mut self, expression: &Expr, context: Context) {
        if self.found.is_some() {
            return;
        }
        match &expression.kind {
            ExprKind::Ident(name) => self.name(*name, context),

            ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
                self.expr(left, context);
                self.expr(right, context);
            }
            ExprKind::Unary { operand, .. }
            | ExprKind::Update {
                target: operand, ..
            } => {
                self.expr(operand, context);
            }
            ExprKind::Await(inner) | ExprKind::Chain(inner) => self.expr(inner, context),
            ExprKind::Yield { value, .. } => {
                if let Some(value) = value {
                    self.expr(value, context);
                }
            }

            // The object is an expression; the property is a key, and a key
            // named `await` is a property, not a name.
            ExprKind::Member { object, .. } => self.expr(object, context),
            ExprKind::Index { object, index, .. } => {
                self.expr(object, context);
                self.expr(index, context);
            }

            ExprKind::Call {
                callee, arguments, ..
            } => {
                self.expr(callee, context);
                for argument in arguments {
                    self.argument(argument, context);
                }
            }
            ExprKind::New { callee, arguments } => {
                self.expr(callee, context);
                for argument in arguments {
                    self.argument(argument, context);
                }
            }
            ExprKind::ImportCall { specifier, options } => {
                self.expr(specifier, context);
                if let Some(options) = options {
                    self.expr(options, context);
                }
            }

            // The literal pieces are text; only the substitutions hold names.
            ExprKind::Template { expressions, .. } => {
                for expression in expressions {
                    self.expr(expression, context);
                }
            }
            ExprKind::TaggedTemplate {
                tag, expressions, ..
            } => {
                self.expr(tag, context);
                for expression in expressions {
                    self.expr(expression, context);
                }
            }

            ExprKind::Object { properties } => {
                for property in properties {
                    self.property(property, context);
                }
            }
            ExprKind::Array { elements } => {
                for element in elements.iter().flatten() {
                    self.argument(element, context);
                }
            }

            ExprKind::Assign { target, value, .. } => {
                match target {
                    crate::syntax::AssignTarget::Place(place) => self.expr(place, context),
                    crate::syntax::AssignTarget::Pattern(pattern) => {
                        self.pattern(pattern, context);
                    }
                }
                self.expr(value, context);
            }
            ExprKind::Sequence { operands } => {
                for operand in operands {
                    self.expr(operand, context);
                }
            }
            ExprKind::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition, context);
                self.expr(then_branch, context);
                self.expr(else_branch, context);
            }

            ExprKind::Function(function) => self.function(function, context, false),
            ExprKind::Class(class) => self.class(class, context),

            ExprKind::SuperMember { property } => self.key(property, context),
            ExprKind::SuperCall { arguments } => {
                for argument in arguments {
                    self.argument(argument, context);
                }
            }

            ExprKind::Asserted { value, .. } => self.expr(value, context),

            // A literal names nothing, and `this`, `new.target`, `import.meta`
            // and a private name are each one fixed thing.
            ExprKind::Literal(_)
            | ExprKind::This
            | ExprKind::NewTarget
            | ExprKind::ImportMeta
            | ExprKind::PrivateName(_) => {}
        }
    }

    fn argument(&mut self, argument: &crate::syntax::Spreadable, context: Context) {
        match argument {
            crate::syntax::Spreadable::Single(expression)
            | crate::syntax::Spreadable::Spread(expression) => self.expr(expression, context),
        }
    }

    fn property(&mut self, property: &Property, context: Context) {
        match property {
            Property::Value { key, value, .. } => {
                self.key(key, context);
                self.expr(value, context);
            }
            Property::Method { key, function }
            | Property::Getter { key, function }
            | Property::Setter { key, function } => {
                self.key(key, context);
                self.function(function, context, false);
            }
            Property::Spread(expression) | Property::Prototype(expression) => {
                self.expr(expression, context);
            }
        }
    }

    fn key(&mut self, key: &PropertyKey, context: Context) {
        // Only a computed key holds an expression. A named one is a property
        // name, and `{ await: 1 }` is legal in an async function.
        if let PropertyKey::Computed(expression) = key {
            self.expr(expression, context);
        }
    }

    fn pattern(&mut self, pattern: &Pattern, context: Context) {
        match pattern {
            Pattern::Name(name) => self.name(*name, context),
            Pattern::Target(place) => self.expr(place, context),
            Pattern::Array(array) => {
                for element in array.elements.iter().flatten() {
                    self.element(element, context);
                }
                if let Some(rest) = &array.rest {
                    self.pattern(rest, context);
                }
            }
            Pattern::Object(object) => {
                for property in &object.properties {
                    self.key(&property.key, context);
                    self.element(&property.value, context);
                }
                if let Some(rest) = &object.rest {
                    self.pattern(rest, context);
                }
            }
        }
    }

    fn element(&mut self, element: &Element, context: Context) {
        self.pattern(&element.pattern, context);
        if let Some(default) = &element.default {
            self.expr(default, context);
        }
    }

    /// A function, in the context it creates rather than the one it sits in.
    ///
    /// `declares_outward` is where its own name lands, and the two answers are
    /// genuinely different. A declaration introduces its name where it is
    /// written, so `function* g() { function yield() {} }` is an error. A
    /// function *expression* binds its name only inside itself, in a scope
    /// nothing outside can see — so `function*() { (function yield() {}); }` is
    /// a valid program, and checking that name outward refuses it.
    ///
    /// Parameters and body are always the inner context, because that is where
    /// they are read.
    fn function(&mut self, function: &Function, outer: Context, declares_outward: bool) {
        let inner = outer.inside(function);
        if let Some(name) = function.name {
            self.name(name, if declares_outward { outer } else { inner });
        }
        for parameter in &function.parameters {
            self.pattern(&parameter.target, inner);
            if let Some(default) = &parameter.default {
                self.expr(default, inner);
            }
        }
        if let Some(rest) = &function.rest_parameter {
            self.pattern(rest, inner);
        }
        match &function.body {
            FunctionBody::Block(body) => self.stmts(body, inner),
            FunctionBody::Expression(value) => self.expr(value, inner),
        }
    }

    /// A class, whose parts are in three different contexts.
    ///
    /// The heritage is an expression where the class is written. A method makes
    /// its own context. A field initialiser and a static block make one that is
    /// neither the outer one nor a method's: `await` is reserved in both,
    /// unconditionally, and `yield` is not.
    fn class(&mut self, class: &Class, outer: Context) {
        if let Some(name) = class.name {
            self.name(name, outer);
        }
        if let Some(heritage) = &class.heritage {
            self.expr(heritage, outer);
        }

        let initialiser = Context::static_block();
        for element in &class.body {
            if let Some(ClassKey::Public(key)) = element.key() {
                self.key(key, outer);
            }
            match element {
                ClassElement::Method(method) => self.function(&method.function, outer, false),
                ClassElement::Field(field) => {
                    if let Some(value) = &field.value {
                        self.expr(value, initialiser);
                    }
                }
                ClassElement::StaticBlock(body) => self.stmts(body, initialiser),
            }
        }
    }
}
