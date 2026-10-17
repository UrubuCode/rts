//! `a OP b` between two instances of one operator class, as a direct method call.
//!
//! # The claim
//!
//! `import { operators } from "rts"` makes `class Vec { [operators.add](o, r) { … } }`
//! answer `a + b` (`rts-core/src/entry/overload.rs`): the runtime asks the LEFT
//! operand for `left[add]` and, if it is callable, returns `left[add](right, false)`
//! untouched; only a left that lacks it hands the question to the right. When both
//! operands are provably instances of the SAME class `C` that defines the method,
//! the left always has it, so that whole protocol is the one expression
//! `a[add](b, false)` — and this pass emits exactly that, as an ordinary method
//! call, dropping the `__rts_add` crossing, the operand classification and the probe.
//!
//! The same protocol serves every operator whose answer is the method's RAW result
//! (`overload::binary` is one function over the operator): `+ - * / % **`, the
//! symbols `add sub mul div mod pow`. Each is proven independently per class — a
//! class may define several, and `a - b` needs `sub`, not `add`. The relational and
//! equality operators (`< <= > >= ==`, whose answer goes through ToBoolean and `==`
//! through an extra both-objects gate) and unary `-` (`neg`) are NOT handled here;
//! they stay on the runtime path.
//!
//! # Why the direct call is the runtime's answer, clause by clause
//!
//! The result is required to be byte-identical, so every clause below excludes one
//! way the two could differ; where a clause cannot be shown, no site is rewritten.
//!
//! - **The operands are concrete `C` instances.** `new C(…)` itself, or a
//!   `const`/`let` declared once in the program with that initialiser, never
//!   written, read where its declaration has run (the emitter's scope says so: a
//!   register in this very function, outside the dead zone). A `new` of a class with
//!   no `extends` and no `return <value>` in its constructor is an ordinary object
//!   whose prototype is `C.prototype`: not a proxy, so the runtime's `any_proxy`
//!   guard (which only routes to the generic Get) has nothing to route and no trap
//!   can intervene; and an object, so the runtime's objectness test is true.
//! - **`C.prototype` is stable.** `C` is declared once, never written through its
//!   name and never read as a value (`handed_over`: no alias can reach its
//!   prototype), and the program-wide gate is the layout pass's: `Object` and
//!   `Reflect` only as bases, no `setPrototypeOf`/`__proto__`/`defineProperty`
//!   spelling anywhere. So the chain of a `C` instance is `C.prototype`,
//!   `Object.prototype`, and the former holds the method for the program's life.
//! - **Nothing can shadow or replace the method.** Every spelling of a handled
//!   symbol (`add sub mul div mod pow`) in the program is the key of a non-static method of a class declaration
//!   (`operators.add` anywhere else — a store, an alias, a field key, a getter —
//!   disables the pass), `operators` itself is only ever the base of a member read
//!   (never `operators[k]`, never aliased or passed on), no other import of `rts`
//!   (`* as r`, default) or dynamic `import`/`require` can reach the symbol, and no
//!   `getOwnPropertySymbols`/`ownKeys` is spelled to recover it from an object. The
//!   symbol is therefore unobtainable by any code that could write it, so the
//!   property `C.prototype[add]` is the method's own function, a data property, with
//!   no instance shadow and no getter: the Get `a[add]` is the runtime's `resolve`.
//! - **`C` defines it exactly once.** One non-static `Normal` method with that key
//!   (per operator: a class that defines `add` twice loses `add`, not `sub`), no
//!   static block in the class (arbitrary code with `this = C`).
//! - **Same call.** `this = a` and `(b, false)` are the runtime's receiver and first
//!   two arguments. The runtime also passes two more `undefined` slots, which no
//!   method can observe unless it reads `arguments` or a rest parameter — a method
//!   that mentions either is left to the runtime.
//! - **Same throws.** A throw in the method (or in evaluating an operand) propagates
//!   from the call exactly as the runtime's early `Some(throw)` return is re-raised
//!   at the compiled site.
//! - **Same order.** Left operand, then the key (a pure read of the namespace member),
//!   then the right operand. The runtime evaluates both operands and then reads
//!   `left[add]`; the read moving before the right operand is invisible because
//!   nothing can write that property (above).
//!
//! The method is reached by an ordinary computed-key call, so the function called is
//! whatever the class definition stored under the symbol — the one the runtime finds.
//!
//! Scope: `+ - * / % **`, both operands the same class, and that class defines THAT
//! operator's symbol. The `@operator("+")` spelling is not parsed.

use std::collections::{BTreeMap, BTreeSet};

use super::capture::{Child, StmtChild, names_in_function, walk_expr, walk_pattern_exprs, walk_stmt};
use super::inline::Declarations;
use super::primordial::Disturbed;
use super::scope::{Binding, Scope};
use crate::names::{Name, Names};
use crate::syntax::{
    BinaryOp, Class, ClassElement, ClassKey, Expr, ExprKind, ForEachTarget, Function, FunctionBody, Import,
    ImportBinding, Literal, MethodKind, Pattern, PropertyKey, Spreadable, Stmt, StmtKind,
};

/// The operators handled, by the name of their `rts` symbol.
const OPS: [&str; 6] = ["add", "sub", "mul", "div", "mod", "pow"];

/// The symbol name of a binary operator this pass handles, or `None` for every other
/// (relational, equality, bitwise, … — those stay on the runtime path).
pub(in crate::emit) fn symbol_of(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("add"),
        BinaryOp::Sub => Some("sub"),
        BinaryOp::Mul => Some("mul"),
        BinaryOp::Div => Some("div"),
        BinaryOp::Rem => Some("mod"),
        BinaryOp::Exponent => Some("pow"),
        _ => None,
    }
}

/// What the pass knows: the `operators` local, the key expression to reuse per
/// operator, and the proven classes with the bindings that hold a fresh instance of one.
#[derive(Default)]
pub(in crate::emit) struct Direct {
    /// The local bound by `import { operators } from "rts"`.
    local: Option<Name>,
    /// `operators.<name>`, as a class writes it — cloned into the call.
    keys: BTreeMap<&'static str, Expr>,
    /// Per class, the operators whose `[operators.<name>]` is proven stable.
    classes: BTreeMap<Name, BTreeSet<&'static str>>,
    /// `d` -> `C` for `const|let d = new C(…)` declared once and never written.
    bound: BTreeMap<Name, Name>,
}

impl Direct {
    /// The direct call `left[operators.<op>](right, false)` for `left OP right`, or `None`
    /// where the runtime's operator must run.
    pub(in crate::emit) fn call(
        &self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        scope: &Scope,
    ) -> Option<Expr> {
        let symbol = symbol_of(op)?;
        let (local, key) = (self.local?, self.keys.get(symbol)?);
        // The key reads `operators` at the site: it must resolve here (a closure that
        // did not capture it cannot, and then the runtime path stays).
        scope.lookup(local)?;
        let class = self.class_of(left, scope, symbol)?;
        (self.class_of(right, scope, symbol)? == class).then_some(())?;
        let at = left.at;
        Some(Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Index {
                        object: Box::new(left.clone()),
                        index: Box::new(key.clone()),
                        optional: false,
                    },
                    at,
                )),
                arguments: vec![
                    Spreadable::Single(right.clone()),
                    Spreadable::Single(Expr::new(ExprKind::Literal(Literal::Boolean(false)), at)),
                ],
                optional: false,
            },
            at,
        ))
    }

    /// The class `expr` is a fresh instance of, if that class's `symbol` is proven.
    fn class_of(&self, expr: &Expr, scope: &Scope, symbol: &str) -> Option<Name> {
        let proven = |class: &Name| self.classes.get(class).is_some_and(|ops| ops.contains(symbol));
        match &bare(expr).kind {
            ExprKind::New { callee, .. } => match &bare(callee).kind {
                ExprKind::Ident(class) if proven(class) => Some(*class),
                _ => None,
            },
            // A binding is trusted only as a register of THIS activation, already
            // declared: a captured one (or one read before its `const` ran, which the
            // engine answers `undefined` for) is left to the runtime.
            ExprKind::Ident(binding) => {
                let class = *self.bound.get(binding)?;
                proven(&class).then_some(())?;
                matches!(scope.lookup(*binding), Some(Binding::Value(_)))
                    .then_some(())
                    .filter(|_| !scope.in_dead_zone(*binding))?;
                Some(class)
            }
            _ => None,
        }
    }
}

/// The pass's facts for one module, or an empty (inert) one where any clause fails.
#[allow(clippy::too_many_arguments)]
pub(in crate::emit) fn facts(
    program: &[Stmt],
    imports: &[&Import],
    excluded: &BTreeSet<Name>,
    names: &mut Names,
    layouts_allowed: bool,
    eval: Name,
    global_this: Name,
    handed_over: &BTreeSet<Name>,
    primordial_ok: impl FnOnce(&mut Names) -> bool,
) -> Direct {
    let none = Direct::default();
    if !layouts_allowed {
        return none;
    }
    // The `operators` import: exactly one, and no other way into the `rts` module.
    let mut local = None;
    for import in imports {
        if import.source != "rts" {
            continue;
        }
        for binding in &import.bindings {
            match binding {
                ImportBinding::Named { exported, local: bound } if exported == "operators" => {
                    if local.replace(*bound).is_some() {
                        return none;
                    }
                }
                ImportBinding::Named { .. } => {}
                // `r.operators.add` / a default export would spell the symbol unseen.
                ImportBinding::Default(_) | ImportBinding::Namespace(_) => return none,
            }
        }
    }
    let Some(local) = local else { return none };
    // No other import, from anywhere, binds that name: the site's `operators` is this one.
    let binds = |binding: &ImportBinding| match binding {
        ImportBinding::Default(bound) | ImportBinding::Namespace(bound) => *bound == local,
        ImportBinding::Named { local: bound, .. } => *bound == local,
    };
    if imports.iter().flat_map(|import| import.bindings.iter()).filter(|b| binds(b)).count() != 1 {
        return none;
    }
    // Only now (a program that uses operators) are further names interned.
    if !primordial_ok(names) {
        return none;
    }
    let declared = Declarations::of(program);
    let writes = super::primordial::disturbed(program, eval, global_this);
    if declared.count(local) != 0 || !writes.untouched(local) {
        return none;
    }
    let mut audit = Audit {
        names,
        local,
        idents: 0,
        bases: 0,
        stray: 0,
        leaks: false,
        found: Vec::new(),
        sanctioned: Vec::new(),
    };
    for statement in program {
        audit.stmt(statement);
    }
    // Every appearance of `operators` is the base of a non-optional member read, and
    // the handled symbols are spelled only as method keys.
    if audit.leaks || audit.idents != audit.bases || audit.stray != 0 {
        return none;
    }
    let found = std::mem::take(&mut audit.found);
    let sanctioned = std::mem::take(&mut audit.sanctioned);
    let names: &Names = audit.names;
    let mut classes: BTreeMap<Name, BTreeSet<&'static str>> = BTreeMap::new();
    let mut keys: BTreeMap<&'static str, Expr> = BTreeMap::new();
    for class in found {
        let Some(name) = class.name else { continue };
        if excluded.contains(&name) || !class_stable(class, name, names, &declared, &writes, handed_over) {
            continue;
        }
        // Per operator: defined exactly once, and its method observes no extra slot.
        for symbol in OPS {
            let mut defs = sanctioned
                .iter()
                .filter(|(owner, op, _, _)| std::ptr::eq(*owner, class) && *op == symbol);
            let (Some(def), None) = (defs.next(), defs.next()) else { continue };
            if !method_plain(def.2, names) {
                continue;
            }
            classes.entry(name).or_default().insert(symbol);
            keys.entry(symbol).or_insert_with(|| def.3.clone());
        }
    }
    if classes.is_empty() {
        return none;
    }
    let mut bound = BTreeMap::new();
    for statement in program {
        bindings_in(statement, &classes, &declared, &writes, excluded, &mut bound);
    }
    Direct { local: Some(local), keys, classes, bound }
}

/// Whether `class` (named `name`) meets the per-class clauses of the module doc.
fn class_stable(
    class: &Class,
    name: Name,
    names: &Names,
    declared: &Declarations,
    writes: &Disturbed,
    handed_over: &BTreeSet<Name>,
) -> bool {
    if class.heritage.is_some()
        || declared.count(name) != 1
        || !writes.untouched(name)
        || handed_over.contains(&name)
        || class.body.iter().any(|element| matches!(element, ClassElement::StaticBlock(_)))
    {
        return false;
    }
    // A constructor that returns a value can hand back something that is not a `C`.
    if let Some(constructor) = class.constructor(names)
        && let FunctionBody::Block(body) = &constructor.function.body
        && body.iter().any(returns_a_value)
    {
        return false;
    }
    true
}

/// Whether an operator method cannot observe the two extra `undefined` argument slots
/// the runtime passes: no rest parameter and no mention of `arguments`.
fn method_plain(method: &Function, names: &Names) -> bool {
    if method.rest_parameter.is_some() {
        return false;
    }
    let mut mentioned = BTreeSet::new();
    names_in_function(method, &mut mentioned);
    !mentioned.iter().any(|seen| names.text(*seen) == "arguments")
}

/// Whether a statement (not entering nested functions) is `return <expr>`.
fn returns_a_value(statement: &Stmt) -> bool {
    if matches!(&statement.kind, StmtKind::Return(Some(_))) {
        return true;
    }
    let mut found = false;
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => found |= returns_a_value(inner),
        StmtChild::Catch(clause) => found |= clause.body.iter().any(returns_a_value),
        _ => {}
    });
    found
}

/// `const|let d = new C(…)` declared once, never written, and not in a `switch` case
/// (where a read can run before the declaration without leaving the scope).
fn bindings_in(
    statement: &Stmt,
    classes: &BTreeMap<Name, BTreeSet<&'static str>>,
    declared: &Declarations,
    writes: &Disturbed,
    excluded: &BTreeSet<Name>,
    bound: &mut BTreeMap<Name, Name>,
) {
    if matches!(statement.kind, StmtKind::Switch { .. }) {
        return;
    }
    if let StmtKind::Declare { kind, bindings } = &statement.kind
        && *kind != crate::syntax::BindingKind::Var
    {
        for binding in bindings {
            if let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value)
                && let ExprKind::New { callee, .. } = &bare(value).kind
                && let ExprKind::Ident(class) = &bare(callee).kind
                && classes.contains_key(class)
                && declared.count(*name) == 1
                && writes.untouched(*name)
                && !excluded.contains(name)
            {
                bound.insert(*name, *class);
            }
        }
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => bindings_in(inner, classes, declared, writes, excluded, bound),
        StmtChild::Catch(clause) => {
            for inner in &clause.body {
                bindings_in(inner, classes, declared, writes, excluded, bound);
            }
        }
        StmtChild::Function(function) => {
            if let FunctionBody::Block(body) = &function.body {
                for inner in body {
                    bindings_in(inner, classes, declared, writes, excluded, bound);
                }
            }
        }
        _ => {}
    });
}

/// Members whose only use is to recover a symbol from an object.
const RECOVERS: [&str; 3] = ["getOwnPropertySymbols", "ownKeys", "getOwnPropertyDescriptors"];

/// One walk over the whole program, counting the appearances of `operators`.
struct Audit<'a, 'n> {
    names: &'n Names,
    local: Name,
    /// Every `Ident(operators)` met.
    idents: u32,
    /// Those that are the object of a non-optional member read.
    bases: u32,
    /// `operators.<handled>` read anywhere but as a sanctioned method key.
    stray: u32,
    /// A way to obtain the symbol that does not go through `operators`.
    leaks: bool,
    /// Every class declaration, in order.
    found: Vec<&'a Class>,
    /// (class, symbol name, its method's function, the key expression) per sanctioned key.
    sanctioned: Vec<(&'a Class, &'static str, &'a Function, &'a Expr)>,
}

impl<'a, 'n> Audit<'a, 'n> {
    fn handled_key(&self, key: &'a ClassKey) -> Option<(&'static str, &'a Expr)> {
        let ClassKey::Public(PropertyKey::Computed(expr)) = key else {
            return None;
        };
        match &expr.kind {
            ExprKind::Member { object, property, optional: false }
                if matches!(&object.kind, ExprKind::Ident(base) if *base == self.local)
                    && let Some(symbol) = OPS.iter().find(|op| **op == self.names.text(*property)) =>
            {
                Some((*symbol, expr))
            }
            _ => None,
        }
    }

    fn stmt(&mut self, statement: &'a Stmt) {
        if let StmtKind::Class(class) = &statement.kind {
            self.found.push(class);
        }
        if let StmtKind::ForEach { target, .. } = &statement.kind {
            match target {
                ForEachTarget::Declare { target, .. } | ForEachTarget::Assign(target) => {
                    self.pattern(target)
                }
                ForEachTarget::Dispose { .. } => {}
            }
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.stmt(inner),
            StmtChild::Expr(expr) => self.expr(expr),
            StmtChild::Binding(binding) => {
                self.pattern(&binding.target);
                if let Some(value) = &binding.value {
                    self.expr(value);
                }
            }
            StmtChild::Catch(clause) => {
                if let Some(binding) = &clause.binding {
                    self.pattern(binding);
                }
                for inner in &clause.body {
                    self.stmt(inner);
                }
            }
            StmtChild::Function(function) => self.function(function),
            StmtChild::Class(class) => self.class(class),
        });
    }

    fn pattern(&mut self, pattern: &'a Pattern) {
        walk_pattern_exprs(pattern, &mut |child| {
            if let Child::Expr(inner) = child {
                self.expr(inner);
            }
        });
    }

    fn expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Ident(seen) if *seen == self.local => self.idents += 1,
            ExprKind::Ident(seen) if self.names.text(*seen) == "require" => self.leaks = true,
            ExprKind::ImportCall { .. } => self.leaks = true,
            ExprKind::Member { object, property, optional } => {
                if matches!(&object.kind, ExprKind::Ident(base) if *base == self.local) {
                    if !*optional {
                        self.bases += 1;
                    }
                    if OPS.contains(&self.names.text(*property)) {
                        self.stray += 1;
                    }
                }
                if RECOVERS.contains(&self.names.text(*property)) {
                    self.leaks = true;
                }
            }
            _ => {}
        }
        walk_expr(expr, &mut |child| match child {
            Child::Expr(inner) => self.expr(inner),
            Child::Function(function) => self.function(function),
            Child::Class(class) => self.class(class),
        });
    }

    fn function(&mut self, function: &'a Function) {
        for parameter in &function.parameters {
            self.pattern(&parameter.target);
            if let Some(default) = &parameter.default {
                self.expr(default);
            }
        }
        if let Some(rest) = &function.rest_parameter {
            self.pattern(rest);
        }
        match &function.body {
            FunctionBody::Block(body) => {
                for statement in body {
                    self.stmt(statement);
                }
            }
            FunctionBody::Expression(value) => self.expr(value),
        }
    }

    fn key(&mut self, key: &'a ClassKey) {
        if let ClassKey::Public(PropertyKey::Computed(expr)) = key {
            self.expr(expr);
        }
    }

    fn class(&mut self, class: &'a Class) {
        if let Some(heritage) = &class.heritage {
            self.expr(heritage);
        }
        for element in &class.body {
            match element {
                ClassElement::Method(method) => {
                    match self.handled_key(&method.key) {
                        // The one place a handled symbol may appear: it is a
                        // DEFINITION, and its `operators` is the import (both counted
                        // here so the base/ident tally stays whole).
                        Some((symbol, key)) if !method.is_static && method.kind == MethodKind::Normal => {
                            self.idents += 1;
                            self.bases += 1;
                            self.sanctioned.push((class, symbol, &method.function, key));
                        }
                        _ => self.key(&method.key),
                    }
                    self.function(&method.function);
                }
                ClassElement::Field(field) => {
                    self.key(&field.key);
                    if let Some(value) = &field.value {
                        self.expr(value);
                    }
                }
                ClassElement::StaticBlock(body) => {
                    for statement in body {
                        self.stmt(statement);
                    }
                }
            }
        }
    }
}

/// The expression under any TypeScript wrappers (`x as T`, `x!`): claims carry no
/// value and evaluate nothing.
fn bare(expr: &Expr) -> &Expr {
    let mut seen = expr;
    while let ExprKind::Asserted { value, .. } = &seen.kind {
        seen = value;
    }
    seen
}
