//! Binding identity: which declaration a name is, rather than how it is spelled.
//!
//! [`Name`] is an interned **spelling**. Nothing downstream of the parser could
//! tell two declarations that spell the same thing apart, so every analysis that
//! needed to recovered the answer by counting spellings over the whole program —
//! `inline::declarations_of`, `Inlinable::free_proved`, `primordial::untouched`,
//! `omit`, `escape` keyed by name. Each is weaker than the question it stands in
//! for, and measurably weaker in both directions at once:
//!
//! - refusing what is legal: a helper reading its own loop variable is refused in
//!   every program with two loops, because both spell it `i` — 233.67 against
//!   46.33 ns, measured 2026-08-30;
//! - accepting what is not: a substituted body resolved a free name against a
//!   `{ let i }` block of its own declarer and answered `11,110,100` where node
//!   answers `11,11,100`, measured 2026-09-20.
//!
//! This is stage E2 of `docs/engine/four-stages.md`. It assigns a [`BindingId`]
//! to every declaration and builds the scope tree those declarations live in, so
//! that a consumer asking *"can a call site in this body see a different binding
//! of this name?"* gets an answer about bindings instead of a count of spellings.
//!
//! # What it deliberately does not do yet
//!
//! It does not resolve REFERENCES. An identifier in the tree is `ExprKind::Ident(
//! Name)` and carries no site identity, so a reference→binding map would have to
//! be keyed by something the tree does not have. Giving it one is an AST change
//! that touches the parser and every consumer, and it is worth doing separately
//! from the analysis it enables — so this pass answers by SCOPE, which is what
//! the emitter already tracks as it walks.
//!
//! Nothing here is wired into emission by this commit. `resolve` is pure: it
//! reads the tree and allocates two arenas.
//!
//! # Where a `with` ends the question
//!
//! Any `with` in scope makes every enclosing binding unprovable — a free name in
//! its body may resolve to a property of an object nobody can see until it runs.
//! The scope tree records it as a scope of its own rather than refusing to build,
//! because the consumer that cares is the one that must refuse, and a consumer
//! that does not care about a name outside it should not be punished for one.

use std::collections::BTreeMap;

use rts_cranelift::fault::Position;

use crate::names::Name;
use crate::syntax::{
    Binding, BindingKind, Catch, Class, ClassElement, ExportDefault, ExportKind, Expr, ExprKind,
    ForEachTarget, ForInit, Function, FunctionBody, ImportBinding, ModuleItem, Pattern, Stmt,
    StmtKind, SwitchClause,
};

/// One declaration, wherever it was written.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct BindingId(u32);

impl BindingId {
    /// The id of the declaration at an index of the arena.
    ///
    /// For a consumer that walks every declaration — a report, or a test asserting
    /// about the whole program. There is no iterator instead because the arena is
    /// private and handing out a slice of it would let a caller index it by a
    /// number from somewhere else.
    pub fn from_index(held: usize) -> Self {
        Self(held as u32)
    }

    /// Which index this is.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One scope of the tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct ScopeId(u32);

/// What introduced a binding.
///
/// Kept apart from [`BindingKind`] because the tree's kinds are a `let`/`const`/
/// `var` distinction and three of the origins here are not written as a
/// declaration at all: a parameter, a `catch` clause's value, and the name a
/// function expression binds inside its own body.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// `var`, hoisted to the nearest function scope.
    Var,
    /// `let`, `const`, `using`, a `class` declaration, a loop target.
    Lexical,
    /// A parameter, including a rest parameter.
    Parameter,
    /// A `catch (e)` clause's binding.
    Caught,
    /// The name a function or class EXPRESSION binds inside its own body, and
    /// nowhere else.
    OwnName,
}

/// What kind of scope a node opened.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScopeKind {
    /// The program.
    Module,
    /// A function body, together with its parameters.
    Function,
    /// A block, a `switch` body, a `try` body.
    Block,
    /// The head of a `for` or `for`-`of`, which wraps the body scope.
    ForHead,
    /// A `catch` clause.
    CatchClause,
    /// A class body, for the name a class expression binds to itself.
    ClassBody,
    /// A `with` body. Every binding outside one is unprovable from inside it.
    With,
}

/// A declaration.
#[derive(Clone, Debug)]
pub struct BindingRecord {
    /// How it is spelled.
    pub name: Name,
    /// What introduced it.
    pub origin: Origin,
    /// The scope it belongs to. For a `var`, the nearest function scope.
    pub scope: ScopeId,
}

/// A scope.
#[derive(Clone, Debug)]
pub struct ScopeRecord {
    /// What opened it.
    pub kind: ScopeKind,
    /// The scope it is written inside, or `None` for the module.
    pub parent: Option<ScopeId>,
    /// The declarations it holds, in the order they were found.
    pub bindings: Vec<BindingId>,
}

/// The scope tree of one program, and every declaration in it.
#[derive(Clone, Debug, Default)]
pub struct Resolution {
    scopes: Vec<ScopeRecord>,
    bindings: Vec<BindingRecord>,
    /// The scope a function body opened, keyed by where the function was written.
    ///
    /// A position rather than a traversal index, because a consumer that holds a
    /// `&Function` has its position and has no traversal to count. Two functions
    /// cannot be written at one position, so this is total for source functions;
    /// a node the emitter synthesised has no entry, which is the honest answer
    /// since it has no source scope either.
    functions: BTreeMap<Position, ScopeId>,
    /// The scope a block statement opened, keyed the same way.
    ///
    /// Apart from [`Self::functions`] rather than one map over both, because a
    /// consumer walking the tree asks a different question of each: a function's
    /// scope is entered when its body is lowered, a block's when the statement is
    /// reached, and a single map would let a caller ask the wrong one and get an
    /// answer.
    blocks: BTreeMap<Position, ScopeId>,
    /// The scope a loop head opened, keyed the same way.
    heads: BTreeMap<Position, ScopeId>,
    /// The scope a `catch` clause opened, keyed by the `try` it belongs to.
    ///
    /// Keyed by the STATEMENT rather than by the clause, because a clause has no
    /// position of its own in the tree — and the `try` is what a consumer holds when it
    /// wants to know where the caught value is bound.
    catches: BTreeMap<Position, ScopeId>,
    /// Which bindings a nested function reads or writes, and what follows from it.
    /// `captured.rs` computes it and says why it is here rather than in a lowering.
    capture: captured::Capture,
}

mod captured;
pub use captured::Environment;

impl Resolution {
    /// The module scope, which every program has.
    pub fn module(&self) -> ScopeId {
        ScopeId(0)
    }

    /// One scope's record.
    pub fn scope(&self, scope: ScopeId) -> &ScopeRecord {
        &self.scopes[scope.0 as usize]
    }

    /// One binding's record.
    pub fn binding(&self, binding: BindingId) -> &BindingRecord {
        &self.bindings[binding.0 as usize]
    }

    /// How many declarations the whole program holds.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether the program holds none, which only an empty one does.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// The scope the body of the function written at `at` opened.
    pub fn function_scope(&self, at: Position) -> Option<ScopeId> {
        self.functions.get(&at).copied()
    }

    /// The scope the `catch` clause of the `try` at that position opened.
    pub fn catch_scope(&self, at: Position) -> Option<ScopeId> {
        self.catches.get(&at).copied()
    }

    /// The scope the loop head written at that position opened.
    ///
    /// A head wraps the body, which is what makes a lexical target one binding per
    /// pass rather than one for the loop.
    pub fn head_scope(&self, at: Position) -> Option<ScopeId> {
        self.heads.get(&at).copied()
    }

    /// The scope the block statement written at `at` opened.
    pub fn block_scope(&self, at: Position) -> Option<ScopeId> {
        self.blocks.get(&at).copied()
    }

    /// What `name` resolves to, seen from `scope`: the innermost declaration of
    /// that spelling on the way out to the module.
    ///
    /// `None` means no scope declares it, which is not an error — it is a global,
    /// resolved through the global object at run time.
    pub fn binding_in(&self, scope: ScopeId, name: Name) -> Option<BindingId> {
        let mut at = Some(scope);
        while let Some(here) = at {
            let record = self.scope(here);
            // In reverse, so that the innermost declaration of a scope that
            // declares one spelling twice wins — which the early-error rules
            // make impossible for two lexical ones, and possible for a `var`
            // beside a parameter.
            if let Some(found) = record
                .bindings
                .iter()
                .rev()
                .find(|held| self.binding(**held).name == name)
            {
                return Some(*found);
            }
            at = record.parent;
        }
        None
    }

    /// Whether more than one binding of `name` is reachable from within the
    /// function whose body is `scope`, without crossing into a nested function.
    ///
    /// # The question this exists for
    ///
    /// A substituted body is emitted in the CALLER's scope, so a free name it
    /// reads resolves against whatever is in scope AT THE CALL SITE. When the
    /// helper is only reachable from inside one function — which is what
    /// `omit::omittable` proves — the call sites are all in that function, so the
    /// hazard is exactly this: a block of that same function declaring the name
    /// again. `emit/omit.rs` approximates this by counting spellings in the body;
    /// this answers it.
    ///
    /// A nested function is not crossed because a call written inside one is a
    /// call that function's own emission substitutes, against its own scope.
    ///
    /// A `with` anywhere under the scope answers `true`, because a name inside
    /// one may resolve to a property instead of to any binding here.
    pub fn shadowed_within(&self, scope: ScopeId, name: Name) -> bool {
        let mut seen = 0usize;
        let mut pending = vec![scope];
        while let Some(here) = pending.pop() {
            let record = self.scope(here);
            if record.kind == ScopeKind::With {
                return true;
            }
            seen += record
                .bindings
                .iter()
                .filter(|held| self.binding(**held).name == name)
                .count();
            if seen > 1 {
                return true;
            }
            for (index, child) in self.scopes.iter().enumerate() {
                if child.parent == Some(here) && child.kind != ScopeKind::Function {
                    pending.push(ScopeId(index as u32));
                }
            }
        }
        false
    }
}

/// The scope tree and every declaration of one module.
///
/// An  declares at module scope like any other binding -- an immutable
/// one, which nothing here records yet because no consumer asks. A re-export
/// () declares NOTHING: the names are forwarded and this
/// module never sees them, which is why only the declaration form descends.
pub fn resolve_module(items: &[ModuleItem]) -> Resolution {
    let mut out = Resolution::default();
    let module = out.open(ScopeKind::Module, None);
    let mut walker = Walker {
        out: &mut out,
        function: module,
        in_loop: false,
        field_code: None,
        references: Vec::new(),
    };
    for item in items {
        match item {
            ModuleItem::Stmt(statement) => walker.statement(statement, module),
            ModuleItem::Import(import) => {
                for binding in &import.bindings {
                    let local = match binding {
                        ImportBinding::Default(name) | ImportBinding::Namespace(name) => *name,
                        ImportBinding::Named { local, .. } => *local,
                    };
                    walker.out.declare(local, Origin::Lexical, module);
                }
            }
            ModuleItem::Export(export) => match &export.kind {
                ExportKind::Declaration(statement) => walker.statement(statement, module),
                ExportKind::Default(ExportDefault::Declaration(statement)) => {
                    walker.statement(statement, module)
                }
                ExportKind::Default(ExportDefault::Expr(expr)) => walker.expression(expr, module),
                ExportKind::Named { .. } | ExportKind::All { .. } => {}
            },
        }
    }
    let references = std::mem::take(&mut walker.references);
    out.settle_capture(&references);
    out
}

/// The same, for a body of statements whose imports were split off beside it --
/// which is the shape the running emitter holds a program in.
///
/// The imports are declared FIRST, in the module scope, because that is where the
/// running emitter binds them (`emit/module.rs`) and where the language hoists them.
/// Leaving them out made every imported name read as a global -- `expect` alone was
/// the reason 3 822 functions of the corpus were kept off the MIR stage.
pub fn resolve_program(body: &[Stmt], imports: &[crate::syntax::Import]) -> Resolution {
    let mut out = Resolution::default();
    let module = out.open(ScopeKind::Module, None);
    for import in imports {
        for binding in &import.bindings {
            let local = match binding {
                ImportBinding::Default(name) | ImportBinding::Namespace(name) => *name,
                ImportBinding::Named { local, .. } => *local,
            };
            out.declare(local, Origin::Lexical, module);
        }
    }
    let mut walker = Walker {
        out: &mut out,
        function: module,
        in_loop: false,
        field_code: None,
        references: Vec::new(),
    };
    walker.statements(body, module);
    let references = std::mem::take(&mut walker.references);
    out.settle_capture(&references);
    out
}

/// The same, for a body of statements with no module items in it.
pub fn resolve(body: &[Stmt]) -> Resolution {
    resolve_program(body, &[])
}

impl Resolution {
    fn open(&mut self, kind: ScopeKind, parent: Option<ScopeId>) -> ScopeId {
        self.scopes.push(ScopeRecord {
            kind,
            parent,
            bindings: Vec::new(),
        });
        ScopeId(self.scopes.len() as u32 - 1)
    }

    fn declare(&mut self, name: Name, origin: Origin, scope: ScopeId) -> BindingId {
        self.bindings.push(BindingRecord {
            name,
            origin,
            scope,
        });
        let id = BindingId(self.bindings.len() as u32 - 1);
        self.scopes[scope.0 as usize].bindings.push(id);
        id
    }
}

/// The traversal, carrying the nearest FUNCTION scope so a `var` lands there.
struct Walker<'a> {
    out: &'a mut Resolution,
    function: ScopeId,
    /// Whether what is being walked runs once per pass of a loop of the SAME
    /// function. A scope opened while it holds is a fresh record per pass, which is
    /// what `captured::Capture::per_pass` reports.
    in_loop: bool,
    /// Every name USED, with the scope it was used in and the function it was used
    /// from -- resolved only once the walk is over, because a `var` or a function
    /// declared further down is already in scope at a use written above it.
    references: Vec<captured::Reference>,
    /// The class body whose field initialiser or static block is being walked. That
    /// code runs in a constructor or a class evaluation, not in the activation the
    /// class is written in, so a use there is attributed to the class body.
    field_code: Option<ScopeId>,
}

impl Walker<'_> {
    /// Opens a scope, remembering whether it is one record per loop pass.
    fn open(&mut self, kind: ScopeKind, parent: Option<ScopeId>) -> ScopeId {
        let scope = self.out.open(kind, parent);
        if self.in_loop && kind != ScopeKind::Function {
            self.out.capture.per_pass.insert(scope);
        }
        scope
    }

    /// Walks a loop's head and body, which run once per pass.
    fn in_a_loop(&mut self, walk: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.in_loop, true);
        walk(self);
        self.in_loop = outer;
    }

    /// Records a use of `name` from `scope`, to be resolved when the walk ends.
    fn used(&mut self, name: crate::names::Name, scope: ScopeId) {
        self.references.push(captured::Reference {
            name,
            scope,
            function: self.field_code.unwrap_or(self.function),
        });
    }

    /// The names a `for (target of …)` writes, and the expressions inside the
    /// pattern -- a computed key or a default is an ordinary read.
    fn assigned(&mut self, pattern: &Pattern, scope: ScopeId) {
        self.pattern_walk(pattern, scope, true);
    }

    fn statements(&mut self, body: &[Stmt], scope: ScopeId) {
        for statement in body {
            self.statement(statement, scope);
        }
    }

    fn statement(&mut self, statement: &Stmt, scope: ScopeId) {
        match &statement.kind {
            StmtKind::Declare { kind, bindings } => {
                let (origin, at) = self.destination(*kind, scope);
                self.bindings_of(bindings, origin, at, scope);
            }
            StmtKind::Using { bindings, .. } => {
                self.bindings_of(bindings, Origin::Lexical, scope, scope);
            }
            StmtKind::Function(function) => {
                // A function DECLARATION binds its name where it is written, in
                // the scope that holds the statement — never in the body.
                if let Some(name) = function.name {
                    self.out.declare(name, Origin::Lexical, scope);
                }
                self.function(function, scope, false);
            }
            StmtKind::Class(class) => {
                if let Some(name) = class.name {
                    self.out.declare(name, Origin::Lexical, scope);
                }
                self.class(class, scope);
            }
            StmtKind::Block(inner) => {
                let block = self.open(ScopeKind::Block, Some(scope));
                self.out.blocks.insert(statement.at, block);
                self.statements(inner, block);
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition, scope);
                self.statement(then_branch, scope);
                if let Some(branch) = else_branch {
                    self.statement(branch, scope);
                }
            }
            StmtKind::While { condition, body } => self.in_a_loop(|walker| {
                walker.expression(condition, scope);
                walker.statement(body, scope);
            }),
            StmtKind::DoWhile { body, condition } => self.in_a_loop(|walker| {
                walker.statement(body, scope);
                walker.expression(condition, scope);
            }),
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => self.in_a_loop(|walker| {
                // The HEAD is its own scope wrapping the body, which is what
                // makes each pass's copy of a lexical target a binding of its
                // own rather than the body's.
                let head = walker.open(ScopeKind::ForHead, Some(scope));
                walker.out.heads.insert(statement.at, head);
                match init {
                    Some(ForInit::Declare { kind, bindings }) => {
                        let (origin, at) = walker.destination(*kind, head);
                        walker.bindings_of(bindings, origin, at, head);
                    }
                    Some(ForInit::Expr(expr)) => walker.expression(expr, head),
                    None => {}
                }
                if let Some(expr) = test {
                    walker.expression(expr, head);
                }
                if let Some(expr) = update {
                    walker.expression(expr, head);
                }
                walker.statement(body, head);
            }),
            StmtKind::ForEach {
                target,
                subject,
                body,
                ..
            } => self.in_a_loop(|walker| {
                let head = walker.open(ScopeKind::ForHead, Some(scope));
                walker.out.heads.insert(statement.at, head);
                match target {
                    ForEachTarget::Declare { kind, target } => {
                        let (origin, at) = walker.destination(*kind, head);
                        walker.pattern(target, origin, at);
                    }
                    ForEachTarget::Dispose { target, .. } => {
                        walker.out.declare(*target, Origin::Lexical, head);
                    }
                    // An ASSIGNMENT target declares nothing: the loop writes a binding
                    // that already exists, wherever it exists -- which is a USE of it,
                    // and a nested function writing an outer one captures it.
                    ForEachTarget::Assign(target) => walker.assigned(target, head),
                }
                walker.expression(subject, head);
                walker.statement(body, head);
            }),
            StmtKind::Switch {
                discriminant,
                clauses,
            } => {
                self.expression(discriminant, scope);
                // ONE scope for every clause: a `let` in one case is visible from
                // the others, which is why the tree keeps the clauses flat.
                let block = self.open(ScopeKind::Block, Some(scope));
                for SwitchClause { test, body } in clauses {
                    if let Some(expr) = test {
                        self.expression(expr, block);
                    }
                    self.statements(body, block);
                }
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                let protected = self.open(ScopeKind::Block, Some(scope));
                self.statements(body, protected);
                if let Some(Catch {
                    binding,
                    body: handler,
                }) = catch
                {
                    let clause = self.open(ScopeKind::CatchClause, Some(scope));
                    self.out.catches.insert(statement.at, clause);
                    if let Some(pattern) = binding {
                        self.pattern(pattern, Origin::Caught, clause);
                        self.pattern_reads(pattern, clause);
                    }
                    self.statements(handler, clause);
                }
                if let Some(cleanup) = finally {
                    let after = self.open(ScopeKind::Block, Some(scope));
                    self.statements(cleanup, after);
                }
            }
            StmtKind::With { object, body } => {
                self.expression(object, scope);
                let inside = self.open(ScopeKind::With, Some(scope));
                self.statement(body, inside);
            }
            StmtKind::Labelled { body, .. } => self.statement(body, scope),
            StmtKind::Expr(expr) | StmtKind::Throw(expr) => self.expression(expr, scope),
            StmtKind::Return(Some(expr)) => self.expression(expr, scope),
            StmtKind::Return(None)
            | StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::Debugger
            | StmtKind::Empty => {} // NO fall-through arm. The match above is exhaustive, so a statement
                                    // kind added to the tree tomorrow fails to compile here instead of
                                    // being silently skipped -- which is how a whole class of binding
                                    // went missing from the counters this replaces. A wildcard with a
                                    // debug assertion was written first and the compiler reported it
                                    // unreachable, which is the stronger guarantee arriving for free.
        }
    }

    /// Where a declaration of `kind` lands, and what origin it has.
    ///
    /// A `var` belongs to the nearest FUNCTION scope however deeply the statement
    /// is nested — that asymmetry is the whole of the difference between the two
    /// declaration forms, and `check/scope.rs` states it for the early-error
    /// rules.
    fn destination(&self, kind: BindingKind, scope: ScopeId) -> (Origin, ScopeId) {
        match kind.is_block_scoped() {
            true => (Origin::Lexical, scope),
            false => (Origin::Var, self.function),
        }
    }

    fn bindings_of(
        &mut self,
        bindings: &[Binding],
        origin: Origin,
        at: ScopeId,
        // Where an initialiser is READ, which for a `var` is not where the name
        // lands: `{ var x = y }` declares `x` in the function and reads `y` in
        // the block.
        reads: ScopeId,
    ) {
        for binding in bindings {
            self.pattern(&binding.target, origin, at);
            self.pattern_reads(&binding.target, reads);
            if let Some(value) = &binding.value {
                self.expression(value, reads);
            }
        }
    }

    fn pattern(&mut self, pattern: &Pattern, origin: Origin, at: ScopeId) {
        let mut names = Vec::new();
        pattern.bound_names(&mut names);
        for name in names {
            self.out.declare(name, origin, at);
        }
    }

    /// The expressions a declaring pattern evaluates -- its defaults and computed keys
    /// -- read in `scope`.
    ///
    /// Apart from [`Self::pattern`] because the two scopes differ: a `var` declares in
    /// the function and reads where it is written. It was missing outright, so
    /// `function inner({ a = seen })` recorded no use of `seen`, the capture analysis
    /// called `seen` a register, and the closure found nothing there.
    fn pattern_reads(&mut self, pattern: &Pattern, scope: ScopeId) {
        self.pattern_walk(pattern, scope, false);
    }

    /// The walk both of those are: every expression inside a pattern, read in `scope`,
    /// and each name it holds as a USE where the pattern writes existing bindings.
    fn pattern_walk(&mut self, pattern: &Pattern, scope: ScopeId, writes: bool) {
        match pattern {
            Pattern::Name(name) if writes => self.used(*name, scope),
            Pattern::Name(_) => {}
            Pattern::Target(expr) => self.expression(expr, scope),
            Pattern::Object(object) => {
                for property in &object.properties {
                    if let crate::syntax::PropertyKey::Computed(key) = &property.key {
                        self.expression(key, scope);
                    }
                    self.pattern_walk(&property.value.pattern, scope, writes);
                    if let Some(default) = &property.value.default {
                        self.expression(default, scope);
                    }
                }
                if let Some(rest) = &object.rest {
                    self.pattern_walk(rest, scope, writes);
                }
            }
            Pattern::Array(array) => {
                for element in array.elements.iter().flatten() {
                    self.pattern_walk(&element.pattern, scope, writes);
                    if let Some(default) = &element.default {
                        self.expression(default, scope);
                    }
                }
                if let Some(rest) = &array.rest {
                    self.pattern_walk(rest, scope, writes);
                }
            }
        }
    }

    /// A function's own scope: its parameters and its body, together.
    fn function(&mut self, function: &Function, outside: ScopeId, expression: bool) {
        let body = self.open(ScopeKind::Function, Some(outside));
        self.out.functions.insert(function.at, body);
        // A function EXPRESSION binds its own name inside its own body and
        // nowhere else. `inline.rs` lost four assertions to exactly this: the
        // count saw the expression's name as a declaration, so a substituted
        // body carrying `fact` landed in a caller that declares no such thing.
        if expression && let Some(name) = function.name {
            self.out.declare(name, Origin::OwnName, body);
        }
        let outer = std::mem::replace(&mut self.function, body);
        // A body runs once per CALL, not once per pass of a loop around its
        // definition: what it declares is one record per activation.
        let looping = std::mem::replace(&mut self.in_loop, false);
        let fields = self.field_code.take();
        for parameter in &function.parameters {
            self.pattern(&parameter.target, Origin::Parameter, body);
            self.pattern_reads(&parameter.target, body);
            if let Some(default) = &parameter.default {
                self.expression(default, body);
            }
        }
        if let Some(rest) = &function.rest_parameter {
            self.pattern(rest, Origin::Parameter, body);
            self.pattern_reads(rest, body);
        }
        match &function.body {
            FunctionBody::Block(statements) => self.statements(statements, body),
            FunctionBody::Expression(expr) => self.expression(expr, body),
        }
        self.function = outer;
        self.in_loop = looping;
        self.field_code = fields;
    }

    fn class(&mut self, class: &Class, outside: ScopeId) {
        // A class binds its own name inside its body, declaration or expression
        // alike — which is what makes `static { … }` able to name it.
        let inside = self.open(ScopeKind::ClassBody, Some(outside));
        if let Some(name) = class.name {
            self.out.declare(name, Origin::OwnName, inside);
        }
        if let Some(heritage) = &class.heritage {
            self.expression(heritage, outside);
        }
        for element in &class.body {
            match element {
                ClassElement::Method(method) => self.function(&method.function, inside, false),
                ClassElement::Field(field) => {
                    if let Some(value) = &field.value {
                        let outer = self.field_code.replace(inside);
                        self.expression(value, inside);
                        self.field_code = outer;
                    }
                }
                ClassElement::StaticBlock(statements) => {
                    let block = self.open(ScopeKind::Block, Some(inside));
                    let outer = self.field_code.replace(inside);
                    self.statements(statements, block);
                    self.field_code = outer;
                }
            }
        }
    }

    fn expression(&mut self, expr: &Expr, scope: ScopeId) {
        match &expr.kind {
            ExprKind::Function(function) => {
                self.function(function, scope, true);
                return;
            }
            ExprKind::Class(class) => {
                self.class(class, scope);
                return;
            }
            ExprKind::Ident(name) => self.used(*name, scope),
            // A destructuring ASSIGNMENT writes the names at its leaves, and the shared
            // walk reports only the expressions inside a pattern -- its own comment
            // records the gap. Reported here rather than there, because a binding this
            // walk decides is not captured is one a closure reads from the wrong place.
            ExprKind::Assign {
                target: crate::syntax::AssignTarget::Pattern(pattern),
                ..
            } => {
                let mut names = Vec::new();
                pattern.bound_names(&mut names);
                for name in names {
                    self.used(name, scope);
                }
            }
            _ => {}
        }
        crate::emit::capture::walk_expr(expr, &mut |child| match child {
            crate::emit::capture::Child::Expr(inner) => self.expression(inner, scope),
            crate::emit::capture::Child::Function(function) => self.function(function, scope, true),
            crate::emit::capture::Child::Class(class) => self.class(class, scope),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::parse::parse_script;

    /// The scope tree of a script, with the interner that read it.
    fn tree(source: &str) -> (Resolution, Names) {
        let mut names = Names::new();
        let program = parse_script(source, &mut names).expect("the fixture parses");
        (resolve_module(&program.body), names)
    }

    /// Every binding of one spelling, anywhere in the program.
    fn all(out: &Resolution, names: &mut Names, spelled: &str) -> Vec<BindingId> {
        let name = names.intern(spelled);
        (0..out.len())
            .map(|at| BindingId(at as u32))
            .filter(|held| out.binding(*held).name == name)
            .collect()
    }

    #[test]
    fn two_blocks_that_spell_one_name_are_two_bindings() {
        let (out, mut names) = tree("let i = 1; { let i = 2; } { let i = 3; }");
        assert_eq!(all(&out, &mut names, "i").len(), 3);
    }

    /// The defect this pass exists for, asked as the question `omit` has to ask.
    #[test]
    fn a_block_of_the_declarer_shadowing_a_free_name_is_visible() {
        let (out, mut names) = tree(
            "function main() {
               let i = 1;
               const q = (x) => x + i;
               { let i = 100; q(10); }
             }",
        );
        let main = out
            .function_scope(
                // The one function whose scope is not an arrow's: it holds a
                // `const` and the arrow does not.
                *out.functions
                    .iter()
                    .map(|(at, _)| at)
                    .next()
                    .expect("the script declares a function"),
            )
            .expect("its body opened a scope");
        let i = names.intern("i");
        assert!(out.shadowed_within(main, i));
        let untouched = names.intern("q");
        assert!(!out.shadowed_within(main, untouched));
    }

    /// The case that must stay provable, and the reason the answer is not simply
    /// "the program spells it twice".
    #[test]
    fn a_name_spelled_again_in_a_different_function_is_not_shadowing() {
        let (out, mut names) = tree(
            "function held() { let zwq = 5; const q = (x) => x + zwq; q(1); }
             function other() { let zwq = 9; return zwq; }",
        );
        let first = *out.functions.keys().next().expect("two functions");
        let held = out.function_scope(first).expect("a body scope");
        let zwq = names.intern("zwq");
        assert!(!out.shadowed_within(held, zwq));
        assert_eq!(all(&out, &mut names, "zwq").len(), 2);
    }

    #[test]
    fn a_var_lands_in_the_function_and_a_let_stays_in_its_block() {
        let (out, mut names) = tree("function f() { { var hoisted = 1; let kept = 2; } }");
        let body = out
            .function_scope(*out.functions.keys().next().expect("one function"))
            .expect("a body scope");
        let hoisted = all(&out, &mut names, "hoisted");
        let kept = all(&out, &mut names, "kept");
        assert_eq!(out.binding(hoisted[0]).scope, body);
        assert_eq!(out.binding(hoisted[0]).origin, Origin::Var);
        assert_ne!(out.binding(kept[0]).scope, body);
        assert_eq!(out.binding(kept[0]).origin, Origin::Lexical);
    }

    #[test]
    fn a_function_expression_binds_its_own_name_only_inside_itself() {
        let (out, mut names) = tree("const f = function fact(n) { return n; };");
        let fact = all(&out, &mut names, "fact");
        assert_eq!(fact.len(), 1);
        assert_eq!(out.binding(fact[0]).origin, Origin::OwnName);
        let inside = out.binding(fact[0]).scope;
        assert_eq!(out.scope(inside).kind, ScopeKind::Function);
        // And it is NOT reachable from the module, which is the whole point: a
        // substituted body carrying the name would land where nothing declares
        // it. `inline.rs` lost four assertions to that.
        let name = names.intern("fact");
        assert!(out.binding_in(out.module(), name).is_none());
    }

    #[test]
    fn a_loop_target_is_a_binding_of_the_head_and_not_of_the_body() {
        let (out, mut names) = tree("let i = 7; for (let i = 0; i < 3; i++) { i; }");
        let both = all(&out, &mut names, "i");
        assert_eq!(both.len(), 2);
        let head = out.binding(both[1]).scope;
        assert_eq!(out.scope(head).kind, ScopeKind::ForHead);
        assert_eq!(out.scope(head).parent, Some(out.module()));
    }

    #[test]
    fn a_catch_binding_belongs_to_its_clause() {
        let (out, mut names) = tree("try { } catch (e) { e; }");
        let caught = all(&out, &mut names, "e");
        assert_eq!(caught.len(), 1);
        assert_eq!(out.binding(caught[0]).origin, Origin::Caught);
        assert_eq!(
            out.scope(out.binding(caught[0]).scope).kind,
            ScopeKind::CatchClause
        );
    }

    /// A `with` ends the question for every name under it, however it is spelled.
    #[test]
    fn a_with_makes_every_name_under_it_unprovable() {
        let (out, mut names) = tree("function f() { let v = 1; with (o) { v; } }");
        let body = out
            .function_scope(*out.functions.keys().next().expect("one function"))
            .expect("a body scope");
        let v = names.intern("v");
        assert!(out.shadowed_within(body, v));
        let never_written = names.intern("absent");
        assert!(out.shadowed_within(body, never_written));
    }

    #[test]
    fn a_name_no_scope_declares_resolves_to_nothing() {
        let (out, mut names) = tree("Math.abs(-1);");
        let math = names.intern("Math");
        assert!(out.binding_in(out.module(), math).is_none());
    }
}
