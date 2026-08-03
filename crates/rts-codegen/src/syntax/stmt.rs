//! Statements, declarations, and functions.

use rts_cranelift::fault::Position;

use super::pattern::Pattern;
use super::{Claim, Class, Expr};
use crate::names::Name;

/// How a binding behaves.
///
/// Three, not two, because the language has three and they differ in ways a
/// lowering has to know: whether the name exists before its declaration runs,
/// whether it can be assigned again, and what a closure capturing it in a loop
/// captures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingKind {
    /// `var`: reaches the enclosing function, and exists — as `undefined` —
    /// from the moment that function is entered.
    Var,
    /// `let`: reaches the enclosing block, and reading it before its declaration
    /// runs is an error rather than `undefined`.
    Let,
    /// `const`: like `let`, and assigning to it again is an error.
    ///
    /// It says the *binding* does not change, not the value. A `const` object
    /// can have its properties written all day, and a lowering that concluded
    /// otherwise would be optimizing against the language.
    Const,
}

impl BindingKind {
    /// Whether the name exists before its declaration is reached.
    pub fn exists_before_declared(self) -> bool {
        matches!(self, BindingKind::Var)
    }

    /// Whether it may be assigned again.
    pub fn is_assignable(self) -> bool {
        !matches!(self, BindingKind::Const)
    }

    /// Whether it belongs to the enclosing block rather than the function.
    pub fn is_block_scoped(self) -> bool {
        !matches!(self, BindingKind::Var)
    }
}

/// A statement, and where it came from.
#[derive(Clone, PartialEq, Debug)]
pub struct Stmt {
    /// What it is.
    pub kind: StmtKind,
    /// Where in the program it was written.
    pub at: Position,
}

impl Stmt {
    /// A statement at a position.
    pub fn new(kind: StmtKind, at: Position) -> Self {
        Self { kind, at }
    }
}

/// What a statement is.
#[derive(Clone, PartialEq, Debug)]
pub enum StmtKind {
    /// An expression evaluated for what it does.
    Expr(Expr),

    /// One or more bindings introduced together.
    Declare {
        /// How they behave.
        kind: BindingKind,
        /// The bindings.
        bindings: Vec<Binding>,
    },

    /// A sequence, and a scope if what it holds is block-scoped.
    Block(Vec<Stmt>),

    /// A condition and one or two branches.
    If {
        /// The condition, read for truthiness rather than compared to anything.
        condition: Expr,
        /// Run when it is truthy.
        then_branch: Box<Stmt>,
        /// Run otherwise, if there is one.
        else_branch: Option<Box<Stmt>>,
    },

    /// A condition checked before each pass.
    While {
        /// The condition.
        condition: Expr,
        /// The body.
        body: Box<Stmt>,
    },

    /// A body run at least once, then a condition.
    ///
    /// Its own node rather than a `While` with the body copied ahead of it: the
    /// copy would duplicate every `continue` target and every declaration in the
    /// body, and a `continue` in a `do`/`while` jumps to the *condition*, not to
    /// the top.
    DoWhile {
        /// The body.
        body: Box<Stmt>,
        /// Checked after each pass.
        condition: Expr,
    },

    /// `for (init; test; update) body` — each of the three slots independently
    /// absent.
    ///
    /// The header is not three statements. A lexical `init` introduces bindings
    /// in a scope that wraps the loop, and — the part that is easy to get wrong
    /// — each pass gets a *fresh copy* of them, which is why a closure made in
    /// the body captures that pass's value rather than the last one. `var` does
    /// not do this, which is the whole of the difference people notice.
    For {
        /// What runs once before the loop.
        init: Option<ForInit>,
        /// Checked before each pass. Absent means it never stops on its own.
        test: Option<Expr>,
        /// Run after each pass, including after a `continue`.
        update: Option<Expr>,
        /// The body.
        body: Box<Stmt>,
    },

    /// `for (x in obj)`, `for (x of xs)`, `for await (x of xs)`.
    ///
    /// One node for the three because they differ in where the values come from
    /// and in nothing else. Which is not to say the difference is small: `in`
    /// walks string keys up the prototype chain, `of` steps an iterator, and
    /// `await of` awaits each step.
    ForEach {
        /// Where the values come from.
        source: ForEachSource,
        /// What each value is bound or assigned to.
        target: ForEachTarget,
        /// What is walked or iterated.
        subject: Expr,
        /// The body.
        body: Box<Stmt>,
    },

    /// Leaving a function, with a value or without.
    ///
    /// Without is not the same as returning nothing: it returns `undefined`,
    /// which is a value. Modelled as an absent expression rather than an implicit
    /// one so that the place that decides what it means is the lowering.
    Return(Option<Expr>),

    /// Leaving a loop, or a labelled statement of any kind.
    ///
    /// A label reaches anything labelled, not only loops: `outer: { break
    /// outer; }` leaves a block. Without one it leaves the nearest loop or
    /// `switch`.
    Break(Option<Name>),

    /// Skipping to the next pass of a loop.
    ///
    /// A label here must name a *loop*, unlike `break`.
    Continue(Option<Name>),

    /// `name: statement`.
    Labelled {
        /// The label.
        label: Name,
        /// What it labels.
        body: Box<Stmt>,
    },

    /// `switch (d) { case … }`.
    ///
    /// One scope for the whole block, not one per clause: a `let` in one case is
    /// visible — and in its temporal dead zone — from the others. Which is why
    /// the clauses are a flat list here rather than each carrying a body that
    /// looks like a block.
    Switch {
        /// What each case is compared against, with `===`.
        discriminant: Expr,
        /// The clauses, in source order.
        ///
        /// `default` keeps its position, because falling through into it and out
        /// of it follows the written order. It is matched last and executed
        /// where it sits.
        clauses: Vec<SwitchClause>,
    },

    /// `debugger;`.
    Debugger,

    /// `with (obj) body`.
    ///
    /// Normative — it is in the main grammar, not Annex B — and a SyntaxError in
    /// strict code. Represented so it can be *parsed and rejected with a reason*
    /// rather than failing to parse, which is the difference between a
    /// diagnostic and a mystery.
    ///
    /// It is also the single construct that makes every enclosing binding
    /// unprovable: any free name in the body might resolve to a property of an
    /// object nobody can see until it runs.
    With {
        /// What is pushed onto the scope chain.
        object: Expr,
        /// The body.
        body: Box<Stmt>,
    },

    /// `using x = e` / `await using x = e`.
    ///
    /// A lexical declaration that also registers the value for disposal when the
    /// scope ends — in reverse declaration order, like a stack, and even when
    /// the scope is left by a throw.
    ///
    /// Its own statement rather than a third [`BindingKind`], because the extra
    /// behaviour is not about how the *name* behaves: `using` is exactly `const`
    /// for binding purposes, and everything that differs happens on the way out
    /// of the scope.
    Using {
        /// The bindings. Each must be a plain name — `using` takes no pattern.
        bindings: Vec<Binding>,
        /// Whether disposal is awaited (`await using`).
        is_async: bool,
    },

    /// Raising a value.
    Throw(Expr),

    /// A protected region, with a handler, a cleanup, or both.
    Try {
        /// What is protected.
        body: Vec<Stmt>,
        /// What catches, and what it calls the value it caught.
        ///
        /// The binding is optional: `catch {}` without one is legal, and a tree
        /// that required one would have to invent a name nothing can refer to.
        catch: Option<Catch>,
        /// What runs on the way out, however that happens.
        finally: Option<Vec<Stmt>>,
    },

    /// A function declared by name.
    Function(Box<Function>),

    /// A class declared by name.
    Class(Box<Class>),

    /// Nothing.
    Empty,
}

/// What runs once at the top of a three-part `for`.
#[derive(Clone, PartialEq, Debug)]
pub enum ForInit {
    /// A declaration. If it is lexical, the loop owns a scope and each pass gets
    /// a fresh copy of the bindings.
    Declare {
        /// How the bindings behave.
        kind: BindingKind,
        /// The bindings.
        bindings: Vec<Binding>,
    },
    /// An expression, evaluated and discarded.
    Expr(Expr),
}

impl ForInit {
    /// Whether the loop owns a scope whose bindings are copied per pass.
    ///
    /// True only for a lexical declaration. This is the difference behind the
    /// oldest closure-in-a-loop surprise in the language, and it is a property
    /// of the header rather than of the body.
    pub fn copies_per_pass(&self) -> bool {
        matches!(self, ForInit::Declare { kind, .. } if kind.is_block_scoped())
    }
}

/// Where a `for`-each loop takes its values from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ForEachSource {
    /// `for (k in obj)` — enumerable string keys, **including inherited ones**,
    /// which is the trap: it walks the prototype chain, and it yields strings
    /// even for array indices.
    In,
    /// `for (v of xs)` — steps `[Symbol.iterator]()`, and owes an
    /// `IteratorClose` on `break`, `return` or `throw`.
    Of,
    /// `for await (v of xs)` — steps `[Symbol.asyncIterator]()`, falling back to
    /// the sync one wrapped. Legal only where `await` is.
    AwaitOf,
}

impl ForEachSource {
    /// Whether leaving this loop early obliges the lowering to close an
    /// iterator.
    pub fn owes_iterator_close(self) -> bool {
        !matches!(self, ForEachSource::In)
    }

    /// Whether this loop may suspend the frame.
    pub fn suspends(self) -> bool {
        matches!(self, ForEachSource::AwaitOf)
    }
}

/// What a `for`-each loop puts each value into.
#[derive(Clone, PartialEq, Debug)]
pub enum ForEachTarget {
    /// `for (const x of xs)` — a fresh binding per pass, which is what makes a
    /// closure made in the body capture that pass's value.
    Declare {
        /// How it behaves.
        kind: BindingKind,
        /// What it introduces.
        target: Pattern,
    },
    /// `for (x of xs)` / `for ([a, b] of pairs)` — writes to something that
    /// already exists, once per pass, with no fresh binding at all.
    Assign(Pattern),
}

/// One `case` or `default` of a switch.
#[derive(Clone, PartialEq, Debug)]
pub struct SwitchClause {
    /// What it matches, or `None` for `default`.
    ///
    /// Compared with `===`, so `case "1"` does not match `1`, and `case NaN`
    /// matches nothing at all — including `NaN`.
    pub test: Option<Expr>,
    /// What runs, and keeps running into the next clause unless something
    /// leaves.
    pub body: Vec<Stmt>,
}

/// One binding introduced by a declaration.
#[derive(Clone, PartialEq, Debug)]
pub struct Binding {
    /// What it introduces — a name, or a pattern that introduces several.
    ///
    /// Must satisfy [`Pattern::is_valid_binding`]: a declaration cannot write
    /// into `obj.x`, though a destructuring assignment can.
    pub target: Pattern,
    /// What it starts as, if anything.
    ///
    /// Absent means `undefined` for `var` and `let`, and is not legal for
    /// `const` — nor for any pattern, which has nothing to destructure. Both are
    /// rules about the language rather than about the tree, so the tree can
    /// express them and something else rejects them.
    pub value: Option<Expr>,
    /// What the program claimed it holds.
    pub claim: Option<Claim>,
}

/// What catches a thrown value.
#[derive(Clone, PartialEq, Debug)]
pub struct Catch {
    /// What it calls the value, if it names it.
    ///
    /// A pattern, because `catch ({ message })` is legal. Absent because
    /// `catch {}` is too.
    pub binding: Option<Pattern>,
    /// What it does.
    pub body: Vec<Stmt>,
}

/// A function, however it was written.
///
/// One shape for declarations, expressions and arrows, because they differ in
/// two facts rather than in structure — what `this` means inside them, and
/// whether the name exists before the declaration runs. Two shapes would mean
/// two of everything that walks one.
#[derive(Clone, PartialEq, Debug)]
pub struct Function {
    /// What it is called, if it has a name.
    pub name: Option<Name>,
    /// Its parameters, in order.
    pub parameters: Vec<Parameter>,
    /// `...rest`, which gathers every argument past the declared ones.
    ///
    /// Its own field rather than a flag on the last parameter, so that "rest is
    /// last" and "rest has no default" are facts about the type instead of rules
    /// somebody has to enforce.
    pub rest_parameter: Option<Pattern>,
    /// A directive prologue, if the body opened with one.
    ///
    /// Only meaningful for a block body: a concise arrow body has no place to
    /// put one, and a non-simple parameter list forbids `"use strict"` here
    /// outright — see [`Function::has_simple_parameter_list`].
    pub directives: Vec<Directive>,
    /// Its body.
    pub body: FunctionBody,
    /// What the program claimed it returns.
    pub returns: Option<Claim>,
    /// Whether it takes `this` from where it was written rather than from how it
    /// is called.
    ///
    /// The one thing arrows actually change. Everything else about them is
    /// spelling.
    pub captures_this: bool,
    /// Whether it may park its frame.
    pub is_async: bool,
    /// Whether it may yield.
    pub is_generator: bool,
    /// Where it was written.
    pub at: Position,
}

/// A string expression statement at the top of a body, which may be a
/// directive.
///
/// `"use strict"` is not a string being evaluated and discarded — it changes how
/// the code around it is *compiled*. Only a string literal, only in the run of
/// them at the very start of a body, and only with no escapes: `"use strict"`
/// is a string, because the directive is matched against the raw text.
///
/// Kept as its own thing so the rule has one home. A `Vec<Stmt>` where the first
/// element happens to be a string cannot express "with no escapes", and a
/// lowering that checked the cooked value would accept the escaped spelling.
#[derive(Clone, PartialEq, Debug)]
pub struct Directive {
    /// The text exactly as written, between the quotes.
    pub raw: String,
}

impl Directive {
    /// Whether this is the strict-mode directive.
    ///
    /// Matches the raw text, which is what makes `"use strict"` an ordinary
    /// string statement rather than a directive that was spelled cleverly.
    pub fn is_use_strict(&self) -> bool {
        self.raw == "use strict"
    }
}

/// What a function does when called.
#[derive(Clone, PartialEq, Debug)]
pub enum FunctionBody {
    /// `{ … }` — statements, and a `return` if it means to produce anything.
    Block(Vec<Stmt>),

    /// `x => x + 1` — one expression, whose value is returned.
    ///
    /// Kept as an expression rather than wrapped in a synthetic `return`,
    /// because a concise body is not a block: no `{` to be a scope, and — the
    /// part a rewrite loses — `x => ({ a: 1 })` needs its parentheses precisely
    /// because a `{` there would have been a block. Recording which was written
    /// keeps that distinction available to a diagnostic.
    Expression(Box<Expr>),
}

impl FunctionBody {
    /// The statements, if this is a block.
    pub fn statements(&self) -> Option<&[Stmt]> {
        match self {
            FunctionBody::Block(body) => Some(body),
            FunctionBody::Expression(_) => None,
        }
    }

    /// Whether the body's value is returned without a `return` being written.
    pub fn returns_implicitly(&self) -> bool {
        matches!(self, FunctionBody::Expression(_))
    }
}

/// One parameter.
#[derive(Clone, PartialEq, Debug)]
pub struct Parameter {
    /// What it introduces.
    pub target: Pattern,
    /// What it is when the caller passed nothing.
    ///
    /// A default is evaluated at the call rather than at the declaration, and
    /// only when the argument was `undefined` — which is why it is an expression
    /// kept here rather than a value computed once. Passing `undefined`
    /// explicitly triggers it; passing `null` does not.
    pub default: Option<Expr>,
    /// What the program claimed it holds.
    pub claim: Option<Claim>,
}

impl Function {
    /// Whether every parameter is a plain name with no default and there is no
    /// rest parameter.
    ///
    /// The spec calls this a "simple parameter list", and it is not a style
    /// question. A non-simple list forbids a `"use strict"` directive in the
    /// body — because the directive would change how the parameters themselves
    /// are parsed, after they have already been parsed — and it decouples the
    /// `arguments` object from the parameters, so writing `arguments[0]` stops
    /// being visible through the parameter name.
    pub fn has_simple_parameter_list(&self) -> bool {
        self.rest_parameter.is_none()
            && self
                .parameters
                .iter()
                .all(|p| p.default.is_none() && matches!(p.target, Pattern::Name(_)))
    }

    /// Every name the parameter list introduces, in order.
    pub fn parameter_names(&self) -> Vec<Name> {
        let mut names = Vec::new();
        for parameter in &self.parameters {
            parameter.target.bound_names(&mut names);
        }
        if let Some(rest) = &self.rest_parameter {
            rest.bound_names(&mut names);
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_var_exists_before_its_declaration_runs() {
        assert!(BindingKind::Var.exists_before_declared());
        assert!(!BindingKind::Let.exists_before_declared());
        assert!(!BindingKind::Const.exists_before_declared());
    }

    #[test]
    fn const_binds_the_name_and_says_nothing_about_the_value() {
        assert!(!BindingKind::Const.is_assignable());
        assert!(
            BindingKind::Const.is_block_scoped(),
            "it is let with one more rule, not a third kind of scope"
        );
    }
}
