//! The walk, and every rule that rides it.
//!
//! One traversal for all of them. The rules divide into two shapes — the ones
//! about a *set* (the names a scope declares, the members a class has) and the
//! ones about a *context* (where `await` may name something, where `super`
//! reaches anything) — and they used to have a walk each. The set rules' walk
//! visited statement lists and never entered an expression, so no function
//! expression and no class method was ever checked by them. They are the same
//! rules; what they were missing is the walk that already reaches everything.
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

mod scopes;
mod words;

use words::{is_reserved, is_reserved_in_strict, legacy_escape_in_a_strict_prologue};

use crate::check::scope::{first_repeat, first_shared, lexical_names};
use crate::names::{Name, Names};
use crate::syntax::{
    Class, ClassElement, ClassKey, Element, Expr, ExprKind, ForEachTarget,
    ForInit, Function, FunctionBody, Goal, Pattern, Program, Property, PropertyKey, Stmt, StmtKind,
    UnaryOp,
};

/// What a statement list is a scope *of*.
///
/// The one difference is where a function declaration lands. In a block it is a
/// lexical name, so `{ function f() {} let f; }` is not a program. At the top of
/// a script or a function body it is var-declared, so `function f() {} var f;`
/// is one. Nothing else about the two differs, and a checker that could not tell
/// them apart would have to refuse one of those two programs wrongly.
#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    /// A block, a `switch` body, a `catch` body.
    Block,
    /// The top of a script or module, and a function body.
    VarRoot,
}

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
    /// `super.x` reaches a home object from here.
    super_property: bool,
    /// `super()` reaches a parent constructor from here.
    super_call: bool,
    /// Whether this code is strict.
    ///
    /// Carried rather than asked for, because strictness is inherited: a
    /// `"use strict"` at the top of a function makes every function written
    /// inside it strict too, and a class body is strict whatever surrounds it.
    /// Nothing in the tree records the answer, so the walk is what remembers.
    strict: bool,
}

/// What `super` means where a function was written.
///
/// A function does not decide this for itself — being a method is a fact about
/// where it appears, not about how it is spelled, and the same
/// `function () {}` is a method in `{ m: … }`'s place and not one two
/// characters away. So the walk hands it down.
#[derive(Clone, Copy, PartialEq)]
enum Home {
    /// Not a method. `super` reaches nothing at all.
    None,
    /// A method definition: `super.x` has a home object, `super()` has no
    /// parent constructor to call.
    Method,
    /// A derived class's constructor, the one place `super()` exists.
    DerivedConstructor,
}

impl Context {
    /// The context a whole program's top level is in.
    ///
    /// A module is strict and reserves `await` at its top level — top-level
    /// `await` is an operator there, so the word cannot also be a name. A
    /// script is neither, unless it opens with the directive.
    pub(super) fn top(program: &Program) -> Self {
        let module = program.goal == Goal::Module;
        Self {
            no_await: module,
            no_await_outside_arrow: false,
            no_yield: false,
            super_property: false,
            super_call: false,
            strict: module || program.directives.iter().any(|d| d.is_use_strict()),
        }
    }

    /// A class static block, and a field initialiser.
    ///
    /// `super.x` works in both — they have the class as a home object — and
    /// `super()` does not: a field initialiser runs *after* the parent
    /// constructor has already been called, and a static block never has one.
    fn static_block(self) -> Self {
        Self {
            no_await: false,
            no_await_outside_arrow: true,
            no_yield: false,
            super_property: true,
            super_call: false,
            // A class body is strict code, always, whatever surrounds it.
            strict: true,
        }
    }

    /// The context inside a function, given the one it was written in.
    ///
    /// An arrow inherits what reaches through it and adds its own; anything
    /// else replaces both. That difference is the rule, and putting it here
    /// means no caller can get it half right.
    ///
    /// `home` is what the function is *written as*, and an arrow ignores it:
    /// an arrow has no home object of its own, exactly as it has no `this`, so
    /// its `super` is whatever the enclosing method's was. That is what makes
    /// `class D extends B { constructor() { (() => super())(); } }` legal and
    /// the same arrow at the top level not.
    fn inside(self, function: &Function, home: Home) -> Self {
        let strict = self.strict || function.directives.iter().any(|d| d.is_use_strict());
        if function.captures_this {
            Self {
                no_await: self.no_await || function.is_async,
                no_await_outside_arrow: false,
                no_yield: self.no_yield || function.is_generator,
                super_property: self.super_property,
                super_call: self.super_call,
                strict,
            }
        } else {
            Self {
                no_await: function.is_async,
                no_await_outside_arrow: false,
                no_yield: function.is_generator,
                super_property: home != Home::None,
                super_call: home == Home::DerivedConstructor,
                strict,
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
        self.names.text(name).starts_with("@@#")
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
        let word = self.names.text(name);
        if is_reserved(word) {
            // Reaching here at all means the word was written with a unicode
            // escape. `var break = 1` does not parse — the lexer sees the
            // keyword — so the only way a reserved word arrives as an
            // *identifier* is `var break = 1`, which is the program this
            // refuses. Which is why the rule needs no source text: the tree
            // holding this node is itself the evidence.
            self.found = Some(format!(
                "`{word}` is a reserved word and cannot name anything"
            ));
            return;
        }
        if context.strict && is_reserved_in_strict(word) {
            self.found = Some(format!("`{word}` cannot name anything in strict code"));
            return;
        }
        match word {
            "await" if context.forbids_await() => {
                self.found = Some("`await` cannot name anything here".to_owned());
            }
            "yield" if context.no_yield => {
                self.found = Some("`yield` cannot name anything here".to_owned());
            }
            _ => {}
        }
    }

    fn fail(&mut self, message: String) {
        if self.found.is_none() {
            self.found = Some(message);
        }
    }


    /// The whole program, from its top level.
    ///
    /// A module's top level takes the block's rule rather than the script's: a
    /// function declared there is a lexical name, so `function f() {} var f;`
    /// is a program in a script and is not one in a module.
    pub(super) fn program(&mut self, program: &Program) {
        let context = Context::top(program);
        let statements = super::module::statements(program);
        let (kind, extra) = if program.goal == Goal::Module {
            (ScopeKind::Block, super::module::imported_names(program))
        } else {
            (ScopeKind::VarRoot, Vec::new())
        };

        if let Some(message) = super::module::duplicate_export(program, self.names) {
            return self.fail(message);
        }

        // A script's top level is not a scope anything disposes of — it ends
        // when the host says so — so there is no moment at which a `using`
        // there would run its cleanup. A module's top level does end, and takes
        // one.
        if program.goal == Goal::Script
            && statements
                .iter()
                .any(|statement| matches!(statement.kind, StmtKind::Using { .. }))
        {
            return self.fail("`using` cannot be declared at the top level of a script".to_owned());
        }

        if let Some(message) = legacy_escape_in_a_strict_prologue(&program.directives) {
            return self.fail(message);
        }
        self.scope_with(&statements, kind, &extra, context);
        self.stmts(&statements, context);
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
            StmtKind::Block(body) => {
                self.scope(body, ScopeKind::Block, context);
                self.stmts(body, context);
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition, context);
                self.statement_body(then_branch, true, context);
                self.stmt(then_branch, context);
                if let Some(otherwise) = else_branch {
                    self.statement_body(otherwise, true, context);
                    self.stmt(otherwise, context);
                }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
                self.expr(condition, context);
                self.statement_body(body, false, context);
                self.stmt(body, context);
            }
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                match init {
                    Some(ForInit::Declare { kind, bindings }) => {
                        if kind.is_block_scoped() {
                            let mut declared = Vec::new();
                            for binding in bindings {
                                binding.target.bound_names(&mut declared);
                            }
                            self.loop_head(&declared, body);
                        }
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
                self.statement_body(body, false, context);
                self.stmt(body, context);
            }
            StmtKind::ForEach {
                target,
                subject,
                body,
                ..
            } => {
                match target {
                    ForEachTarget::Declare { kind, target } => {
                        if kind.is_block_scoped() {
                            let mut declared = Vec::new();
                            target.bound_names(&mut declared);
                            self.loop_head(&declared, body);
                        }
                        self.pattern(target, context);
                    }
                    ForEachTarget::Assign(target) => self.pattern(target, context),
                    ForEachTarget::Dispose { target, .. } => self.name(*target, context),
                }
                self.expr(subject, context);
                self.statement_body(body, false, context);
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
            // A label may name a declaration in exactly one case: a plain
            // function declaration in sloppy code, which Annex B keeps alive.
            StmtKind::Labelled { label, body } => {
                self.name(*label, context);
                self.statement_body(body, true, context);
                self.stmt(body, context);
            }
            StmtKind::Switch {
                discriminant: subject,
                clauses,
            } => {
                self.expr(subject, context);
                // The clauses share one scope — a `let` in one case is visible
                // from the others, and in its temporal dead zone there — so
                // they are asked the scope question as a single list.
                let body: Vec<Stmt> = clauses
                    .iter()
                    .flat_map(|clause| clause.body.iter().cloned())
                    .collect();
                self.scope(&body, ScopeKind::Block, context);
                // `using` is refused directly in a clause because the clauses
                // share one scope and a clause is not one: the disposal would
                // have to happen at the end of the whole `switch`, from a
                // declaration that looks like it belongs to one case. Wrapping
                // the case body in a block says which, and is legal.
                if body
                    .iter()
                    .any(|statement| matches!(statement.kind, StmtKind::Using { .. }))
                {
                    return self.fail(
                        "`using` cannot be declared directly in a `switch` clause".to_owned(),
                    );
                }
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
                self.statement_body(body, false, context);
                self.stmt(body, context);
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                self.scope(body, ScopeKind::Block, context);
                self.stmts(body, context);
                if let Some(catch) = catch {
                    self.catch(catch, context);
                }
                if let Some(finally) = finally {
                    self.scope(finally, ScopeKind::Block, context);
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

            // `super` is not an expression that resolves at run time and fails:
            // outside a method there is no home object for `super.x` to read
            // from and no parent constructor for `super()` to reach, so the
            // text does not name anything at all. Which is why it is refused
            // here rather than compiled into something that throws.
            ExprKind::SuperMember { property } => {
                if !context.super_property {
                    self.found = Some("`super` is only available in a method".to_owned());
                    return;
                }
                self.key(property, context);
            }
            ExprKind::SuperCall { arguments } => {
                if !context.super_call {
                    self.found = Some(
                        "`super()` is only available in the constructor of a derived class"
                            .to_owned(),
                    );
                    return;
                }
                for argument in arguments {
                    self.argument(argument, context);
                }
            }

            ExprKind::Asserted { value, .. } => self.expr(value, context),

            // A literal names nothing, and `this`, `new.target`, `import.meta`
            // and a private name are each one fixed thing.
            // Every literal but one is a value and nothing else. A regular
            // expression carries a program in its text, and the rules about it
            // are early errors: `/(?<a>x)(?<a>y)/` is not a pattern that fails
            // to match, it is not a program.
            ExprKind::Literal(crate::syntax::Literal::Regex { pattern, flags }) => {
                if let Some(message) = super::regexp::check(pattern, flags) {
                    self.found = Some(message);
                }
            }
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
            // An object literal's method has a home object too — the object —
            // so `({ m() { super.toString(); } })` is a program. `super()` is
            // not: there is no parent constructor anywhere in sight.
            Property::Method { key, function }
            | Property::Getter { key, function }
            | Property::Setter { key, function } => {
                self.key(key, context);
                self.function_as(function, context, false, Home::Method);
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
        self.function_as(function, outer, declares_outward, Home::None);
    }

    /// The same, for a function that is a method of something.
    fn function_as(
        &mut self,
        function: &Function,
        outer: Context,
        declares_outward: bool,
        home: Home,
    ) {
        let inner = outer.inside(function, home);
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
        let mut parameters: Vec<Name> = Vec::new();
        for parameter in &function.parameters {
            parameter.target.bound_names(&mut parameters);
        }
        if let Some(rest) = &function.rest_parameter {
            rest.bound_names(&mut parameters);
        }

        // `eval` and `arguments` may not be *bound* in strict code — reading
        // them is fine, and that asymmetry is the point: strict mode takes away
        // the ability to shadow the two names whose meaning it depends on.
        //
        // The strictness that decides it is the function's own, which is what
        // makes `function eval() { "use strict"; }` an error: the directive is
        // inside the body and the name is outside it, and the language reads
        // the body first.
        if inner.strict
            && let Some(name) = function
                .name
                .into_iter()
                .chain(parameters.iter().copied())
                .find(|name| matches!(self.names.text(*name), "eval" | "arguments"))
        {
            return self.fail(format!(
                "`{}` cannot be bound in strict code",
                self.names.text(name)
            ));
        }

        if let Some(message) = legacy_escape_in_a_strict_prologue(&function.directives) {
            return self.fail(message);
        }

        // Two parameters of one name are legal in exactly one shape: an
        // ordinary sloppy function whose parameter list is simple. Everything
        // else takes UniqueFormalParameters — an arrow, a method, a class
        // member — and so does any list with a default, a pattern or a rest,
        // because the second binding would have to be initialised twice.
        let unique_required = inner.strict
            || function.captures_this
            || home != Home::None
            || !function.has_simple_parameter_list();
        if unique_required && let Some(name) = first_repeat(&parameters) {
            return self.fail(format!(
                "`{}` is a parameter twice, where every parameter must be distinct",
                self.names.text(name)
            ));
        }

        // `"use strict"` is refused rather than ignored on a non-simple list.
        // The directive would decide how the defaults are evaluated, and they
        // are evaluated before the body the directive is written in — so the
        // language forbids the question instead of answering it.
        if !function.has_simple_parameter_list()
            && function.directives.iter().any(|d| d.is_use_strict())
        {
            return self.fail(
                "a function with a default, a pattern or a rest parameter cannot declare \
                 `\"use strict\"`"
                    .to_owned(),
            );
        }

        match &function.body {
            FunctionBody::Block(body) => {
                // A parameter and a `let` of the same name collide, always —
                // unlike two parameters of the same name, which the rule above
                // allows in a sloppy function with a simple list.
                let lexical = lexical_names(body);
                if let Some(name) = first_shared(&parameters, &lexical) {
                    return self.fail(format!(
                        "`{}` is a parameter and is declared again in the body",
                        self.names.text(name)
                    ));
                }
                self.scope(body, ScopeKind::VarRoot, inner);
                self.stmts(body, inner);
            }
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

        // Everything from here down is inside the body, which is strict code
        // however the surrounding program is written.
        let inside = Context {
            strict: true,
            ..outer
        };
        let initialiser = inside.static_block();
        for element in &class.body {
            if let Some(ClassKey::Public(key)) = element.key() {
                self.key(key, outer);
            }
            match element {
                ClassElement::Method(method) => {
                    // Only the constructor of a class that extends something
                    // may call `super()`. A method of the same class may not:
                    // the parent constructor runs once, when the object is
                    // made, and a method runs on one that already exists.
                    let home = if method.is_constructor(self.names) && class.heritage.is_some() {
                        Home::DerivedConstructor
                    } else {
                        Home::Method
                    };
                    self.function_as(&method.function, inside, false, home);
                }
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
            ExprKind::Member { property, .. } => self.names.text(*property).starts_with("@@#"),
            // `?.` around it does not change what is being removed.
            ExprKind::Chain(inner) => self.reaches_a_private_name(inner),
            _ => false,
        }
    }
}
