//! The deep walk: the rules that need to reach every node.
//!
//! The scope rules in `walk` ask about statement lists and never look inside an
//! expression. These do the opposite — they visit everything, carrying a
//! context down — so they share one traversal rather than having one each. A
//! second walk of the same shape is a second place that has to agree on what an
//! arrow inherits, and it would not.
//!
//! # `await` and `yield`: reserved by where you are, not by what they spell
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
    Function, FunctionBody, Pattern, Property, PropertyKey, Stmt, StmtKind, UnaryOp,
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

/// Look for the first thing wrong, and say what it was.
pub(super) struct Scan<'a> {
    names: &'a Names,
    found: Option<String>,
    /// The private names each enclosing class body declares, innermost last.
    ///
    /// A stack rather than a set, because a private name's scope is the class
    /// body that declares it and nested classes nest: an inner class can reach
    /// its own `#x` and an outer one's, and an outer class cannot reach the
    /// inner's. Flattening them would make the last of those legal.
    private_scopes: Vec<Vec<Name>>,
}

impl<'a> Scan<'a> {
    pub(super) fn new(names: &'a Names) -> Self {
        Self {
            names,
            found: None,
            private_scopes: Vec::new(),
        }
    }

    /// One use of a private name, which must be declared by a class we are in.
    ///
    /// Unlike every other identifier, this cannot be resolved at run time and
    /// fall back to `undefined`: `this.#m` where no enclosing class declares
    /// `#m` is not a missing property, it is not a program. The name has no
    /// meaning outside the body that introduced it.
    fn private_use(&mut self, name: Name) {
        if self.found.is_some() {
            return;
        }
        if !self
            .private_scopes
            .iter()
            .any(|scope| scope.contains(&name))
        {
            self.found = Some(format!(
                "`{}` is not declared by any enclosing class",
                self.names.text(name)
            ));
        }
    }

    /// Whether a name was interned as a private one.
    fn is_private(&self, name: Name) -> bool {
        self.names.text(name).starts_with('#')
    }

    pub(super) fn finish(self) -> Option<String> {
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
            word @ "await" if context.forbids_await() => {
                self.found = Some(format!("`{word}` cannot name anything here"));
            }
            word @ "yield" if context.no_yield => {
                self.found = Some(format!("`{word}` cannot name anything here"));
            }
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
            // `delete this.#x` is an early error however it is written, and
            // `delete (((g().#m)))` is the same program — which is why this
            // asks the tree rather than looking at the text. A private name is
            // not a property, so there is nothing to remove.
            ExprKind::Unary {
                op: UnaryOp::Delete,
                operand,
            } if self.reaches_a_private_name(operand) => {
                self.found = Some("`delete` cannot remove a private name".to_owned());
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
            ExprKind::Member {
                object, property, ..
            } => {
                if self.is_private(*property) {
                    self.private_use(*property);
                }
                self.expr(object, context);
            }
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
            ExprKind::Literal(_) | ExprKind::This | ExprKind::NewTarget | ExprKind::ImportMeta => {}

            // `#x in obj` — the only place a private name stands alone, and it
            // needs declaring exactly as `this.#x` does.
            ExprKind::PrivateName(name) => self.private_use(*name),
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
        // The rules about the body as a *set* — one constructor, no static
        // `prototype` — ride this walk rather than one of their own, because
        // reaching every class written anywhere in a program is exactly what
        // this walk already does.
        if self.found.is_none()
            && let Some(message) = super::class::check(class, self.names)
        {
            self.found = Some(message);
            return;
        }

        if let Some(name) = class.name {
            self.name(name, outer);
        }
        if let Some(heritage) = &class.heritage {
            self.expr(heritage, outer);
        }

        // Pushed before the body and popped after — and after the heritage,
        // which is evaluated where the class is written and cannot see them.
        self.private_scopes.push(
            class
                .body
                .iter()
                .filter_map(|element| match element.key() {
                    Some(ClassKey::Private(name)) => Some(*name),
                    _ => None,
                })
                .collect(),
        );

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

        self.private_scopes.pop();
    }
}

/// Whether a `delete` operand ends at a private name.
///
/// Only the outermost member access matters: `delete a.#b.c` removes `c`, which
/// is an ordinary property, and is legal. `delete (a.#b)` removes `#b` and is
/// not. Parentheses are already gone by the time the tree exists, so the two
/// spellings of the same program reach here identically — which is the reason
/// this reads the tree instead of the source.
impl Scan<'_> {
    fn reaches_a_private_name(&self, operand: &Expr) -> bool {
        match &operand.kind {
            ExprKind::Member { property, .. } => self.names.text(*property).starts_with('#'),
            // `?.` around it does not change what is being removed.
            ExprKind::Chain(inner) => self.reaches_a_private_name(inner),
            _ => false,
        }
    }
}
