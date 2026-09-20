//! Tree to MIR: the first half of E4, and the first place the three pieces meet.
//!
//! `names::resolve` says which binding a declaration is, `domain::Js` says what
//! this language's types and primitives are, and `rts_mir` holds the graph. This
//! turns a function body into one of those graphs.
//!
//! # A deliberately small subset, and why refusing is the shape of the work
//!
//! What lowers here is straight-line code: declarations of plain names,
//! expressions over literals and locals, the arithmetic and comparison operators,
//! and a `return`. Everything else answers [`Unsupported`] with the reason.
//!
//! That is the honest shape for a stage being built underneath a working
//! compiler. The emitter this will eventually replace handles the whole language,
//! so a partial lowering is only useful if it says exactly where it stops — and a
//! refusal that names itself is what lets the next piece be chosen by measurement
//! rather than by guess. A lowering that fell back silently would report coverage
//! it does not have, which is the failure the honesty floor calls a claim wearing
//! a measurement's clothes.
//!
//! # Why a binding maps to a value and not to a slot
//!
//! `emit/scope.rs` already made this decision and its reasoning transfers whole:
//! giving every local a stack slot is what a machine does, and it is wrong to
//! start with, because undoing it later is a rewrite rather than an optimisation —
//! every read has become a memory operation some pass has to prove away.
//!
//! What is different here is that the map is keyed by [`BindingId`] rather than by
//! a spelling, so two bindings that spell the same thing are two entries. That is
//! the whole of what E2 bought, and it is why this file needs no shadowing rules.

use std::collections::BTreeMap;

use rts_mir::cfg::{Const, Func, FuncBuilder, Op, Terminator, ValueId};
use rts_mir::guard::Tier;
use rts_mir::{Domain, Effect};

use crate::domain::{Js, JsConst, JsPrim, Type};
use crate::names::Name;
use crate::names::resolve::{BindingId, Resolution, ScopeId};
use named::{expression_name, name_of, primitive};
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Binding, UpdateOp, UpdatePosition, Expr, ExprKind, Function, FunctionBody, Pattern, Stmt, StmtKind,
};
use crate::values::Singleton;

mod branch;
mod class;
mod choice;
mod destructure;
mod switch;
mod calls;
mod named;
mod push;
mod protect;
mod loops;

/// What this lowering does not do yet, and where.
///
/// One variant per reason rather than a single "unsupported", because the list is
/// the work queue: what appears most often in a real program is what to lower
/// next, and that is a question only a named refusal can answer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unsupported {
    /// A statement kind with no lowering here.
    Statement(&'static str),
    /// An expression kind with no lowering here.
    Expression(&'static str),
    /// An operator with no primitive in [`crate::domain`]'s table.
    Operator(BinaryOp),
    /// A destructuring or otherwise non-name declaration target.
    Pattern,
    /// A name no scope declares. It is a global, which needs the entry point this
    /// does not call yet — not an error in the program.
    Global(Name),
    /// The function's body is not a shape this reads: a generator, an async
    /// function, or a parameter list with a default or a rest.
    Shape(&'static str),
    /// A resolution was not built for this function, so no binding has an
    /// identity here.
    ///
    /// Reachable only for a synthesised function, which has no source position
    /// and therefore no scope — see `Resolution::function_scope`.
    NoScope,
}

/// Which binding of a module calls which function.
///
/// Built by [`crate::lower_module`] over the whole module before any of it is
/// lowered, because a call may name a function written later in the file --
/// `function a() { return b(); } function b() {}` is ordinary code.
///
/// Keyed by [`BindingId`] and not by a spelling, which is the whole of why this is
/// a map and not a search: two functions may be called `step` in one module, and
/// the binding says which one a given call reaches.
#[derive(Debug, Default)]
pub struct Callees {
    by_binding: std::collections::BTreeMap<BindingId, rts_mir::cfg::FuncId>,
    /// Which function was written at which position.
    ///
    /// A function EXPRESSION usually binds no name, so the binding map cannot reach
    /// it — and a function VALUE has to be nameable whether or not anything holds
    /// it. The position is what the two views share, which `bound_expressions`
    /// already relied on.
    by_position: std::collections::BTreeMap<rts_cranelift::fault::Position, rts_mir::cfg::FuncId>,
}

impl Callees {
    /// The map for one module.
    pub fn of(
        items: &[crate::syntax::ModuleItem],
        functions: &[&Function],
        resolution: &Resolution,
    ) -> Self {
        let mut held = crate::lower_module::callee_map(functions, resolution);
        held.extend(crate::lower_module::bound_expressions(items, functions, resolution));
        Self {
            by_binding: held,
            by_position: functions
                .iter()
                .enumerate()
                .map(|(at, function)| (function.at, rts_mir::cfg::FuncId(at as u32)))
                .collect(),
        }
    }

    /// Which function was written at that position, if the module numbered one.
    pub fn of_position(&self, at: rts_cranelift::fault::Position) -> Option<rts_mir::cfg::FuncId> {
        self.by_position.get(&at).copied()
    }

    /// Which function that binding is, if it is one.
    pub fn of_binding(&self, binding: BindingId) -> Option<rts_mir::cfg::FuncId> {
        self.by_binding.get(&binding).copied()
    }
}

/// One function, lowered.
///
/// The domain travels with it because the graph's `Prim` and `Assertion` indices
/// mean nothing without the tables that minted them: handing a `Func` to a pass
/// without its domain is the one way to use this API wrongly.
#[derive(Debug)]
pub struct Lowered {
    /// The graph.
    pub func: Func,
    /// The tables its indices refer to.
    pub domain: Js,
}

/// Lowers a function body into MIR, or says where it stopped.
///
/// `at` is the position the function was written at, which is how the scope tree
/// is asked for its scope — `Resolution::function_scope`'s own documentation says
/// why a position rather than a traversal index.
pub fn lower(
    function: &Function,
    resolution: &Resolution,
    tier: Tier,
) -> Result<Lowered, Unsupported> {
    let mut domain = Js::new();
    // A door with no module behind it, so no numbering and no interner of its own: the
    // empty one answers every text question with nothing, which is what a caller that
    // lowers ONE function out of context is entitled to.
    let names = crate::names::Names::new();
    let func = lower_with(
        function,
        resolution,
        &Callees::default(),
        &mut domain,
        &names,
        tier,
    )?;
    Ok(Lowered { func, domain })
}

/// The same, against a MODULE's tables.
///
/// The domain is the module's rather than this function's, because an assertion
/// index minted for one function has to mean the same thing in the next -- a guard
/// hoisted out of a call, later, is one assertion in two graphs. And the callee map
/// is the module's by definition: a call names something outside the function it
/// is written in.
pub fn lower_with(
    function: &Function,
    resolution: &Resolution,
    callees: &Callees,
    domain: &mut Js,
    names: &crate::names::Names,
    tier: Tier,
) -> Result<Func, Unsupported> {
    if function.is_async {
        return Err(Unsupported::Shape("an async function parks its frame"));
    }
    if function.is_generator {
        return Err(Unsupported::Shape("a generator parks its frame"));
    }
    if function.rest_parameter.is_some() {
        return Err(Unsupported::Shape("a rest parameter gathers at run time"));
    }
    let Some(scope) = resolution.function_scope(function.at) else {
        return Err(Unsupported::NoScope);
    };

    let mut lowering = Lowering {
        builder: FuncBuilder::new(tier),
        domain,
        callees,
        values: BTreeMap::new(),
        types: BTreeMap::new(),
        resolution,
        names,
        scope,
        loops: Vec::new(),
    };

    for parameter in &function.parameters {
        if parameter.default.is_some() {
            return Err(Unsupported::Shape("a parameter default is an expression"));
        }
        let Pattern::Name(name) = &parameter.target else {
            return Err(Unsupported::Pattern);
        };
        let entry = lowering.builder.current();
        let value = lowering.builder.param(entry);
        // A parameter is this function's by definition, so the `at` is only ever
        // read if the binding were outer -- which it cannot be.
        let at = Expr {
            kind: ExprKind::Ident(*name),
            at: function.at,
        };
        lowering.bind(*name, value, Type::Anything, &at)?;
    }

    match &function.body {
        FunctionBody::Expression(expr) => {
            let value = lowering.expression(expr)?;
            lowering.builder.end(Terminator::Return(Some(value)));
        }
        FunctionBody::Block(statements) => {
            let answered = lowering.statements(statements)?;
            if !answered {
                lowering.builder.end(Terminator::Return(None));
            }
        }
    }

    Ok(lowering.builder.finish())
}

/// Which construct a frame belongs to.
///
/// `break` leaves the innermost of EITHER, and `continue` names a loop — so a
/// `continue` inside a `switch` inside a loop must reach the loop. Without this the
/// two would be one stack and a `continue` would jump to the switch's exit, which is
/// a wrong answer that compiles: the program would leave the loop instead of taking
/// its next pass, and nothing about the graph would look malformed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FrameKind {
    /// A loop: both `break` and `continue` reach it.
    Loop,
    /// A `switch`: `break` reaches it and `continue` passes through.
    Switch,
}

/// One loop or switch being lowered: where its test is, where leaving it goes, and
/// which bindings its header carries.
pub(super) struct LoopFrame {
    pub(super) kind: FrameKind,
    pub(super) header: rts_mir::BlockId,
    pub(super) exit: rts_mir::BlockId,
    pub(super) carried: Vec<BindingId>,
}

/// The state one function's lowering carries.
struct Lowering<'a> {
    builder: FuncBuilder,
    domain: &'a mut Js,
    /// Which function of the module a name calls, where one does.
    callees: &'a Callees,
    /// What each binding currently holds. Keyed by IDENTITY, which is why there
    /// are no shadowing rules in this file.
    values: BTreeMap<BindingId, ValueId>,
    /// What is known about each value, so that an operation's effect can be asked
    /// of its operands.
    ///
    /// A local map rather than `rts_mir::infer`'s answer, because the effect has
    /// to be decided when the instruction is PUSHED and inference runs over a
    /// finished graph. The two agree wherever this one is not `Anything`; where it
    /// is, inference knows more and a later pass may recompute the effect. That is
    /// a refinement and never a correction: this map errs towards knowing less,
    /// and knowing less only ever makes an effect wider.
    types: BTreeMap<ValueId, Type>,
    resolution: &'a Resolution,
    /// The interner, for the two questions that are about TEXT rather than about a
    /// binding: whether a class member is the constructor, and what a name is called in
    /// a report. Immutable, so nothing here can mint a name -- which is why a key the
    /// language fixes is a `JsConst::WellKnown` and not an interned string.
    names: &'a crate::names::Names,
    scope: ScopeId,
    /// The loops and switches enclosing what is being lowered, innermost last.
    loops: Vec<LoopFrame>,
}

impl Lowering<'_> {
    /// Lowers a run of statements, answering whether it ended the block.
    fn statements(&mut self, statements: &[Stmt]) -> Result<bool, Unsupported> {
        for statement in statements {
            if self.statement(statement)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Lowers one statement, answering whether it terminated the block.
    fn statement(&mut self, statement: &Stmt) -> Result<bool, Unsupported> {
        match &statement.kind {
            StmtKind::Empty => Ok(false),
            StmtKind::Expr(expr) => {
                self.expression(expr)?;
                Ok(false)
            }
            StmtKind::Declare { bindings, .. } => {
                for Binding { target, value, .. } in bindings {
                    // A PATTERN needs a value to read from, so a declaration with
                    // no initialiser cannot have one -- and the language agrees: a
                    // destructuring declaration must be initialised.
                    if !matches!(target, Pattern::Name(_)) {
                        let Some(expr) = value else {
                            return Err(Unsupported::Statement(
                                "a destructuring declaration with no initialiser",
                            ));
                        };
                        let held = self.expression(expr)?;
                        self.destructure(target, held, expr)?;
                        continue;
                    }
                    let Pattern::Name(name) = target else {
                        unreachable!("the arm above took every other shape")
                    };
                    let (held, of) = match value {
                        Some(expr) => {
                            let held = self.expression(expr)?;
                            let of = self.type_of(held);
                            (held, of)
                        }
                        // `let x;` is `undefined`, which is a value like any
                        // other.
                        None => {
                            let held = self.singleton(Singleton::Undefined, statement);
                            (held, Type::Undefined)
                        }
                    };
                    // A DECLARATION is always this function's -- a `let` binds
                    // here -- so the position is only carried for the arm that
                    // cannot be reached from one.
                    let at = Expr {
                        kind: ExprKind::Ident(*name),
                        at: statement.at,
                    };
                    self.bind(*name, held, of, &at)?;
                }
                Ok(false)
            }
            StmtKind::Switch {
                discriminant,
                clauses,
            } => self.switch(discriminant, clauses),
            StmtKind::Break(None) => self.jump_out_of_loop(false),
            StmtKind::Continue(None) => self.jump_out_of_loop(true),
            StmtKind::Return(value) => {
                let answered = match value {
                    Some(expr) => Some(self.expression(expr)?),
                    None => None,
                };
                self.builder.end(Terminator::Return(answered));
                Ok(true)
            }
            StmtKind::Block(inner) => {
                // The scope tree is walked in step with the statements, keyed by
                // where the block was written. Nothing about shadowing has to be
                // decided here: a binding inside is a different `BindingId`, so
                // the map that tracks what each holds cannot collide.
                let Some(scope) = self.resolution.block_scope(statement.at) else {
                    return Err(Unsupported::NoScope);
                };
                let outer = std::mem::replace(&mut self.scope, scope);
                let ended = self.statements(inner)?;
                self.scope = outer;
                Ok(ended)
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.branch(condition, then_branch, else_branch.as_deref()),
            StmtKind::While { condition, body } => self.loop_while(condition, body),
            StmtKind::DoWhile { body, condition } => self.loop_do_while(body, condition),
            StmtKind::For { init, test, update, body } => self.loop_for(
                statement.at,
                init.as_ref(),
                test.as_ref(),
                update.as_ref(),
                body,
            ),
            StmtKind::ForEach { .. } => Err(Unsupported::Statement("an iteration protocol")),
            // A NESTED DEFINITION is a closure bound to a name, and the name is this
            // function's -- so it is an ordinary rebind and no environment is written.
            //
            // It was refused as "its own graph", which was true and was not the whole
            // truth: the graph is lowered by `lower_module` like every other, and what
            // this statement does is make a VALUE of it.
            StmtKind::Function(function) => {
                let Some(name) = function.name else {
                    return Err(Unsupported::Statement(
                        "a function declaration with no name",
                    ));
                };
                let Some(id) = self.callees.of_position(function.at) else {
                    return Err(Unsupported::Statement(
                        "a nested definition needs the module's numbering",
                    ));
                };
                let at = Expr {
                    kind: ExprKind::Ident(name),
                    at: function.at,
                };
                let held = self.closure(id, &at);
                let of = self.type_of(held);
                self.bind(name, held, of, &at)?;
                Ok(false)
            }
            StmtKind::Class(class) => {
                let at = Expr {
                    kind: ExprKind::This,
                    at: statement.at,
                };
                self.class(class, &at)
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => self.protect(body, catch.as_ref(), finally.as_ref(), statement),
            // A THROW is a raise, and it is refused apart from the region that catches
            // one: the region is control flow this lowering builds, and raising is an
            // operation the runtime performs -- an entry point, which this stage does
            // not call yet.
            StmtKind::Throw(_) => Err(Unsupported::Statement(
                "a throw raises, which is an entry point rather than control flow",
            )),
            other => Err(Unsupported::Statement(name_of(other))),
        }
    }

    fn expression(&mut self, expr: &Expr) -> Result<ValueId, Unsupported> {
        match &expr.kind {
            ExprKind::Literal(literal) => self.literal(literal, expr),
            ExprKind::Ident(name) => {
                match self.resolution.binding_in(self.scope, *name) {
                    Some(binding) => self.read_binding(binding, *name, expr),
                    // NO SCOPE DECLARES IT, so it is a global -- read through the
                    // global object, which is what the language does with one.
                    None => Ok(self.global(*name, expr)),
                }
            }
            // An assignment to a plain local is a REBIND, which is what SSA makes
            // of one: the binding now holds a different value and no store
            // happens. A compound form (`a += b`) is refused rather than rewritten
            // to `a = a + b`, because the target is evaluated once and rewriting
            // would evaluate it twice — the tree carries the operator for exactly
            // that reason.
            ExprKind::Assign {
                target,
                value,
                op: AssignOp::Plain,
            } => {
                let AssignTarget::Place(place) = target else {
                    return Err(Unsupported::Pattern);
                };
                // A WRITE TO A PROPERTY is a write to the heap and not a rebind, so
                // it is a primitive rather than an entry in the binding map. Its
                // answer is the value written, which is what makes `o.x = o.y = 1`
                // work.
                if let ExprKind::Member {
                    object,
                    property,
                    optional: false,
                } = &place.kind
                {
                    let receiver = self.expression(object)?;
                    let key = self.domain.constant(JsConst::Key(*property));
                    let key = self.declared(key, expr);
                    let held = self.expression(value)?;
                    self.prim(JsPrim::FieldWrite, vec![receiver, key, held], expr);
                    return Ok(held);
                }
                if let ExprKind::Index {
                    object,
                    index,
                    optional: false,
                } = &place.kind
                {
                    let receiver = self.expression(object)?;
                    let at = self.expression(index)?;
                    let held = self.expression(value)?;
                    self.prim(JsPrim::IndexWrite, vec![receiver, at, held], expr);
                    return Ok(held);
                }
                let ExprKind::Ident(name) = &place.kind else {
                    return Err(Unsupported::Expression(
                        "an assignment whose target is neither a name nor a property",
                    ));
                };
                let held = self.expression(value)?;
                let of = self.type_of(held);
                self.bind(*name, held, of, expr)?;
                // The value of an assignment is what was assigned, which is what
                // makes `a = b = 1` work.
                Ok(held)
            }
            // A COMPOUND ASSIGNMENT to a plain local.
            //
            // `a += b` is not `a = a + b` and the tree says so by carrying the
            // operator: the target is evaluated ONCE. For a plain name that
            // distinction costs nothing — reading a binding has no effect to
            // duplicate — so the rewrite is legal here and only here. A member
            // target is refused below for exactly the reason the tree gives:
            // `a[i()] += 1` calls `i` a single time.
            ExprKind::Assign {
                target,
                value,
                op: AssignOp::Compound(op),
            } => {
                let AssignTarget::Place(place) = target else {
                    return Err(Unsupported::Pattern);
                };
                let ExprKind::Ident(name) = &place.kind else {
                    return Err(Unsupported::Expression(
                        "a compound assignment to a property reads and writes the heap, once",
                    ));
                };
                let Some(prim) = primitive(*op) else {
                    return Err(Unsupported::Operator(*op));
                };
                let held = self.expression(place)?;
                let with = self.expression(value)?;
                let answered = self.prim(prim, vec![held, with], expr);
                let of = self.type_of(answered);
                self.bind(*name, answered, of, expr)?;
                Ok(answered)
            }
            // READING A PROPERTY BY NAME.
            //
            // The key is a constant of this language's table rather than an operand
            // of the IR, which is what lets two reads of one property compare equal
            // by their indices — a pass asking whether two accesses touch the same
            // field compares numbers, never names.
            //
            // The effect comes from the RECEIVER's type: a shaped one is a load at a
            // position the layout decided, and an unshaped one goes through the
            // runtime, where a getter may run. Nothing here proves a shape — this
            // language has no shape source yet, so every receiver is unshaped and
            // every read carries the wide effect. What changes that is an object
            // literal minting a layout, which is the next piece and the one that
            // makes a guard worth emitting at all.
            ExprKind::Member {
                object,
                property,
                optional: false,
            } => {
                let held = self.expression(object)?;
                let key = self.domain.constant(JsConst::Key(*property));
                let key = self.declared(key, expr);
                Ok(self.prim(JsPrim::FieldRead, vec![held, key], expr))
            }
            // READING BY A COMPUTED KEY, which is a different operation and not a
            // `FieldRead` with a clever argument: the key is a VALUE, so even a
            // shaped receiver needs the runtime to turn it into a position. A pass
            // that proves the index constant is what merges the two, and it is a
            // pass rather than something the lowering can see.
            ExprKind::Index {
                object,
                index,
                optional: false,
            } => {
                let held = self.expression(object)?;
                let at = self.expression(index)?;
                Ok(self.prim(JsPrim::IndexRead, vec![held, at], expr))
            }
            // `this` is an operation that reads the receiver of this activation.
            //
            // Where that receiver lives is the calling convention, which the machine
            // decides -- the same answer the outer binding got, and the other end of
            // the receiver field on a call.
            ExprKind::This => Ok(self.prim(JsPrim::ThisValue, Vec::new(), expr)),
            ExprKind::Unary { op, operand } => {
                // `delete` REMOVES a property, so its operand is a place and not a
                // value: lowering the operand first would evaluate what is about to
                // be deleted. It keeps its refusal by name.
                if matches!(op, crate::syntax::UnaryOp::Delete) {
                    return Err(Unsupported::Expression(
                        "delete removes a property, so its operand is a place",
                    ));
                }
                let held = self.expression(operand)?;
                let which = match op {
                    crate::syntax::UnaryOp::Negate => JsPrim::Negate,
                    // Unary plus IS `ToNumber` and nothing else, which is why it has
                    // no row of its own: `+a` and the coercion an increment performs
                    // are the same operation, and two rows would let a pass fold one
                    // and miss the other.
                    crate::syntax::UnaryOp::Plus => JsPrim::ToNumber,
                    crate::syntax::UnaryOp::Not => JsPrim::Not,
                    crate::syntax::UnaryOp::BitNot => JsPrim::BitwiseNot,
                    crate::syntax::UnaryOp::TypeOf => JsPrim::TypeOf,
                    // `void a` evaluates its operand and answers `undefined`. The
                    // operand was lowered above, which is the whole of what it does.
                    crate::syntax::UnaryOp::Void => {
                        return Ok(self.singleton_at(Singleton::Undefined, expr));
                    }
                    // A CHECK a loop expansion mints, not an operator a program can
                    // write. It answers its operand unchanged and exists so that
                    // raising the loop's `TypeError` needs no binding a program could
                    // shadow -- so refusing it here would refuse `for`-`of`, and
                    // lowering it as a no-op would drop the check. Named, and left
                    // for the entry point that raises.
                    crate::syntax::UnaryOp::IteratorResult => {
                        return Err(Unsupported::Expression(
                            "the iterator-result check a loop expansion mints",
                        ));
                    }
                    crate::syntax::UnaryOp::Delete => unreachable!("refused above"),
                };
                Ok(self.prim(which, vec![held], expr))
            }
            // A FUNCTION EXPRESSION is a closure value naming which function it is.
            ExprKind::Function(function) => match self.callees.of_position(function.at) {
                Some(id) => Ok(self.closure(id, expr)),
                // Unreachable through `lower_module`, which numbers every function of
                // the module before lowering any of it. Reachable through the
                // single-function door, where there is no numbering at all -- so it
                // says that rather than pretending a number.
                None => Err(Unsupported::Expression(
                    "a function value needs the module's numbering",
                )),
            },
            ExprKind::Conditional {
                condition,
                then_branch,
                else_branch,
            } => self.conditional(condition, then_branch, else_branch, expr),
            ExprKind::Logical { op, left, right } => self.logical(*op, left, right, expr),
            // A TYPE ASSERTION is the operand and nothing else.
            //
            // `a as number` narrows nothing in the domain, and that is rule 4 of this
            // crate rather than laziness: a type annotation is EVIDENCE, not proof.
            // TypeScript's assertion is unchecked — a program may assert what is
            // false, and the standard says the value is unchanged — so narrowing on
            // one would make a guard that asserts nothing and a fast path that is
            // wrong whenever the program lied.
            //
            // What an annotation IS good for is choosing which specialisation to
            // build, where the guard still checks. That is a pass reading the claim,
            // not a lowering trusting it.
            ExprKind::Asserted { value, .. } => self.expression(value),
            // A CONSTRUCTION, with the constructor as the first argument.
            ExprKind::New { callee, arguments } => {
                let held = self.expression(callee)?;
                let mut args = vec![held];
                args.extend(self.arguments(arguments)?);
                Ok(self.prim(JsPrim::Construct, args, expr))
            }
            // AN OBJECT LITERAL, as pairs of a declared key and a value.
            //
            // In SOURCE ORDER, and that is load-bearing rather than tidy: the order
            // properties are added is what decides the layout, which the tree's own
            // comment on this node says, so reordering the pairs here would mint a
            // different shape at run time and nothing would report it.
            //
            // No shape is asserted. `Type::Shaped` carries the reason — the shape
            // tree that decides layouts is the RUNTIME's, and claiming a number only
            // it mints is what this crate's rules 1 and 2 forbid by name.
            //
            // A method, a getter, a setter and a spread are each refused apart. A
            // method is not a value under a key: it is installed with a home object,
            // which is what `super.x` inside it reads from, and a function stored
            // under a key has none. Collapsing the two would compile and would make
            // `super` mean nothing.
            ExprKind::Object { properties } => {
                let mut pairs = Vec::with_capacity(properties.len() * 2);
                for property in properties {
                    let crate::syntax::Property::Value { key, value, .. } = property else {
                        return Err(Unsupported::Expression(
                            "an object literal with a method, an accessor or a spread",
                        ));
                    };
                    let crate::syntax::PropertyKey::Named(name) = key else {
                        return Err(Unsupported::Expression(
                            "a computed key is a value, so the layout is not the one written",
                        ));
                    };
                    let index = self.domain.constant(JsConst::Key(*name));
                    pairs.push(self.declared(index, expr));
                    pairs.push(self.expression(value)?);
                }
                Ok(self.prim(JsPrim::NewObject, pairs, expr))
            }
            // AN ARRAY LITERAL, which is one primitive over its elements.
            //
            // A hole is refused rather than lowered as `undefined`: the two are
            // different, and the tree's own comment says why — a hole is skipped by
            // some operations and read as `undefined` by others, so collapsing them
            // loses that. A spread is refused because the element count would stop
            // being the count written.
            ExprKind::Array { elements } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    let Some(crate::syntax::Spreadable::Single(held)) = element else {
                        return Err(Unsupported::Expression(
                            "an array literal with a hole or a spread has a run-time length",
                        ));
                    };
                    values.push(self.expression(held)?);
                }
                Ok(self.prim(JsPrim::NewArray, values, expr))
            }
            ExprKind::Call {
                callee,
                arguments,
                optional: false,
            } => {
                // A METHOD CALL: the receiver is read once, the callee is read from
                // it, and the receiver travels as itself.
                //
                // Once is the whole point. `o.m()` evaluates `o` a single time, so
                // lowering it as "read o, read o.m, call with o" would evaluate it
                // twice and call a getter twice — which is observable and is the
                // same mistake `a[i()] += 1` is refused for one arm up.
                //
                // The receiver is a FIELD of the call and not its first argument.
                // `rts_mir::cfg::Op::Call` carries the reason: a convention held in
                // two places drifts, and a field cannot.
                if let ExprKind::Member {
                    object,
                    property,
                    optional: false,
                } = &callee.kind
                {
                    let receiver = self.expression(object)?;
                    let key = self.domain.constant(JsConst::Key(*property));
                    let key = self.declared(key, expr);
                    let held = self.prim(JsPrim::FieldRead, vec![receiver, key], expr);
                    let args = self.arguments(arguments)?;
                    return Ok(self.call(
                        rts_mir::cfg::Callee::Dynamic(held),
                        Some(receiver),
                        args,
                        expr,
                    ));
                }
                let ExprKind::Ident(name) = &callee.kind else {
                    return Err(Unsupported::Expression(
                        "a call whose callee is neither a name nor a property",
                    ));
                };
                let binding = self.resolution.binding_in(self.scope, *name);
                let args = self.arguments(arguments)?;
                // A CALL TO A GLOBAL: read it, then call what it held. No receiver
                // travels -- parseInt(x) passes none, and the global object is not
                // a receiver.
                let Some(binding) = binding else {
                    let held = self.global(*name, expr);
                    return Ok(self.call(rts_mir::cfg::Callee::Dynamic(held), None, args, expr));
                };
                // A NAME THAT HOLDS NO FUNCTION OF THIS MODULE is still a call — an
                // imported binding, or a parameter holding a function. It reaches
                // whatever the value is, which is exactly `Callee::Dynamic`, and it
                // passes no receiver.
                //
                // It was refused before this, and the refusal was about the MIR not
                // having a receiver rather than about this shape: a dynamic call was
                // already expressible. 218 refusals in `tests/` said so.
                let callee = match self.callees.of_binding(binding) {
                    Some(id) => rts_mir::cfg::Callee::Func(id),
                    None => {
                        let held = self.read_binding(binding, *name, expr)?;
                        rts_mir::cfg::Callee::Dynamic(held)
                    }
                };
                Ok(self.call(callee, None, args, expr))
            }
            // AN INCREMENT of a local, which a `for` header needs and which is not
            // `x = x + 1`.
            //
            // The difference is what the expression ANSWERS. `i++` answers the
            // number the target held — coerced, so a string target answers 5 and
            // not "5" — and `++i` answers the sum. Both rebind. Written with an
            // explicit `ToNumber` rather than leaving the coercion to the addition,
            // because the addition's answer is not what a postfix form gives back.
            ExprKind::Update { op, position, target } => {
                let ExprKind::Ident(name) = &target.kind else {
                    return Err(Unsupported::Expression(
                        "an increment of a property writes the heap",
                    ));
                };
                let held = self.expression(target)?;
                let before = self.prim(JsPrim::ToNumber, vec![held], expr);
                let one = {
                    let value = Const::Int(1);
                    let of = self.domain.of_const(&value);
                    let pushed = self.builder.push(Op::Const(value), Effect::PURE, expr.at);
                    self.types.insert(pushed, of);
                    pushed
                };
                let after = match op {
                    UpdateOp::Increment => self.prim(JsPrim::Add, vec![before, one], expr),
                    UpdateOp::Decrement => self.prim(JsPrim::Subtract, vec![before, one], expr),
                };
                let of = self.type_of(after);
                self.bind(*name, after, of, expr)?;
                Ok(match position {
                    UpdatePosition::Prefix => after,
                    UpdatePosition::Postfix => before,
                })
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                match op {
                    // `!==` AND `!=` ARE A NEGATION, and this is the case where rewriting is
                    // legal — which is worth stating beside the comparisons, where it was not.
                    //
                    // `a > b` could not become `b < a` because the two coerce their operands
                    // in opposite orders, and that is observable. `a !== b` becoming
                    // `!(a === b)` changes nothing: the same operands are evaluated in the
                    // same order by the same operation, and the negation applies to a boolean
                    // the operation already produced. So one costs a table row and the other
                    // costs two instructions a pass can fold.
                    BinaryOp::StrictNotEqual | BinaryOp::LooseNotEqual => {
                        let which = match op {
                            BinaryOp::StrictNotEqual => JsPrim::StrictEquals,
                            _ => JsPrim::LooseEquals,
                        };
                        let equal = self.prim(which, vec![left, right], expr);
                        Ok(self.prim(JsPrim::Not, vec![equal], expr))
                    }
                    _ => {
                        let Some(prim) = primitive(*op) else {
                            return Err(Unsupported::Operator(*op));
                        };
                        Ok(self.prim(prim, vec![left, right], expr))
                    }
                }
            }
            other => Err(Unsupported::Expression(expression_name(other))),
        }
    }

    /// Records what a declaration now holds.
    ///
    /// A binding of THIS function is a rebind — SSA makes a write into a new value
    /// and no store happens. A binding outside it is a cell somewhere, so the write
    /// is an operation; which cell, and where it lives, is the machine question rule
    /// 2 leaves below.
    fn bind(&mut self, name: Name, value: ValueId, of: Type, at: &Expr) -> Result<(), Unsupported> {
        let Some(binding) = self.resolution.binding_in(self.scope, name) else {
            return Err(Unsupported::Global(name));
        };
        if !self.declared_in_this_function(binding) {
            self.outer(binding, JsPrim::OuterWrite, Some(value), at);
            return Ok(());
        }
        self.values.insert(binding, value);
        self.types.insert(value, of);
        Ok(())
    }

    fn type_of(&self, value: ValueId) -> Type {
        self.types.get(&value).cloned().unwrap_or(Type::Anything)
    }
}

#[cfg(test)]
#[path = "../lower_tests.rs"]
mod tests;
