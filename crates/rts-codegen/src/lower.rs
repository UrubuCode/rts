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

use std::collections::{BTreeMap, BTreeSet};

use rts_mir::cfg::{Const, Func, FuncBuilder, Op, Terminator, ValueId};
use rts_mir::guard::Tier;
use rts_mir::{Domain, Effect};

use crate::domain::{Js, JsPrim, Type};
use crate::names::Name;
use crate::names::resolve::{BindingId, Resolution, ScopeId};
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Binding, Expr, ExprKind, Function, FunctionBody, Literal, Pattern, Stmt, StmtKind,
};
use crate::emit::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::values::Singleton;

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
pub struct Callees(std::collections::BTreeMap<BindingId, rts_mir::cfg::FuncId>);

impl Callees {
    /// The map for one module.
    pub fn of(
        items: &[crate::syntax::ModuleItem],
        functions: &[&Function],
        resolution: &Resolution,
    ) -> Self {
        let mut held = crate::lower_module::callee_map(functions, resolution);
        held.extend(crate::lower_module::bound_expressions(items, functions, resolution));
        Self(held)
    }

    /// The map, given one already built.
    pub fn from_map(held: std::collections::BTreeMap<BindingId, rts_mir::cfg::FuncId>) -> Self {
        Self(held)
    }

    /// Which function that binding is, if it is one.
    pub fn of_binding(&self, binding: BindingId) -> Option<rts_mir::cfg::FuncId> {
        self.0.get(&binding).copied()
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
    let func = lower_with(function, resolution, &Callees::default(), &mut domain, tier)?;
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
        lowering.bind(*name, value, Type::Anything)?;
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

/// One loop being lowered: where its test is, where leaving it goes, and which
/// bindings its header carries.
struct LoopFrame {
    header: rts_mir::BlockId,
    exit: rts_mir::BlockId,
    carried: Vec<BindingId>,
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
    scope: ScopeId,
    /// The loops enclosing what is being lowered, innermost last.
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
                    let Pattern::Name(name) = target else {
                        return Err(Unsupported::Pattern);
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
                    self.bind(*name, held, of)?;
                }
                Ok(false)
            }
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
            StmtKind::DoWhile { .. } | StmtKind::For { .. } => Err(Unsupported::Statement(
                "a do-while tests after the body and a for has a head; both are the
                 same shape once the header is decided, and neither is written yet",
            )),
            StmtKind::ForEach { .. } => Err(Unsupported::Statement("an iteration protocol")),
            StmtKind::Function(_) | StmtKind::Class(_) => {
                Err(Unsupported::Statement("a nested definition is its own graph"))
            }
            StmtKind::Try { .. } | StmtKind::Throw(_) => {
                Err(Unsupported::Statement("a protected region"))
            }
            other => Err(Unsupported::Statement(name_of(other))),
        }
    }

    /// Lowers an `if`, merging what the two arms disagree about.
    ///
    /// # Why the merge is computed and not declared
    ///
    /// The join block's parameters are exactly the bindings the two arms leave
    /// holding different values. Declaring one per live binding instead would be
    /// correct and would make every `if` in a program carry the whole scope
    /// through a parameter list, which is the shape `emit/merge.rs` records as the
    /// cost of not asking.
    ///
    /// Asking needs both arms lowered from the SAME starting map, which is why
    /// `before` is cloned twice rather than mutated through: an arm that rebinds a
    /// name must not be visible to the other arm, and the two orders would
    /// otherwise disagree.
    ///
    /// # Why an arm that returned contributes nothing
    ///
    /// Control does not arrive at the join from it, so its bindings are not a
    /// second opinion — they are no opinion. Joining them in would widen every
    /// type at the join for a path that cannot be taken, which is sound and is
    /// exactly the giving-up a type domain exists to avoid.
    fn branch(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
    ) -> Result<bool, Unsupported> {
        let tested = self.expression(condition)?;
        // A machine branch wants a machine boolean and a value of this language is
        // not one, so the language's truth rule is an operation. The domain folds
        // it where the type decides it.
        let tested = self.prim(JsPrim::Truthy, vec![tested], condition);

        let then_block = self.builder.block();
        let else_block = self.builder.block();
        let before = self.values.clone();
        self.builder.end(Terminator::Branch {
            condition: tested,
            then_block,
            then_args: Vec::new(),
            else_block,
            else_args: Vec::new(),
        });

        self.builder.switch_to(then_block);
        self.values = before.clone();
        let then_ended = self.statement(then_branch)?;
        let then_values = std::mem::replace(&mut self.values, before.clone());
        let then_exit = self.builder.current();

        self.builder.switch_to(else_block);
        let else_ended = match else_branch {
            Some(branch) => self.statement(branch)?,
            None => false,
        };
        let else_values = std::mem::take(&mut self.values);
        let else_exit = self.builder.current();

        // Both arms left the function, so nothing follows the `if` at all.
        if then_ended && else_ended {
            self.values = before;
            return Ok(true);
        }

        let join = self.builder.block();
        // One arm returning leaves the other as the only path, so there is nothing
        // to merge: its map IS the answer.
        if then_ended || else_ended {
            let (surviving, exit) = match then_ended {
                true => (else_values, else_exit),
                false => (then_values, then_exit),
            };
            self.values = surviving;
            self.builder.switch_to(exit);
            self.builder.end(Terminator::Jump {
                target: join,
                args: Vec::new(),
            });
            self.builder.switch_to(join);
            return Ok(false);
        }

        // Every binding the two arms disagree about, in a stable order: the map is
        // a `BTreeMap`, so this is the same list on every run, which is what keeps
        // one program compiling to one program.
        let merged: Vec<BindingId> = then_values
            .iter()
            .filter(|(binding, held)| else_values.get(binding).is_some_and(|other| other != *held))
            .map(|(binding, _)| *binding)
            .collect();

        let mut params = Vec::with_capacity(merged.len());
        for binding in &merged {
            let param = self.builder.param(join);
            // The type at the join is the join of the two types, which is the
            // domain doing the one thing a lattice is for. `Int32` from one arm
            // and `Double` from the other is a `Double` here — and in another
            // language it is neither.
            let of = self.domain.join(
                &self.type_of(then_values[binding]),
                &self.type_of(else_values[binding]),
            );
            self.types.insert(param, of);
            params.push(param);
        }

        self.builder.switch_to(then_exit);
        self.builder.end(Terminator::Jump {
            target: join,
            args: merged.iter().map(|held| then_values[held]).collect(),
        });
        self.builder.switch_to(else_exit);
        self.builder.end(Terminator::Jump {
            target: join,
            args: merged.iter().map(|held| else_values[held]).collect(),
        });

        // What each binding holds after the `if`: the parameter where the arms
        // disagreed, and what both said where they agreed.
        self.values = then_values;
        for (binding, param) in merged.iter().zip(params) {
            self.values.insert(*binding, param);
        }
        self.builder.switch_to(join);
        Ok(false)
    }


    /// Lowers a `while`, and every loop is this shape once its header is decided.
    ///
    /// # Why the carried set has to be computed BEFORE the body
    ///
    /// The header is a block with a predecessor that does not exist yet — the back
    /// edge — and a block's parameters must be declared before anything jumps to
    /// it. So the question "which bindings does this loop carry" cannot be answered
    /// the way the `if` join answers it, by comparing what two finished arms hold.
    /// It has to be answered from the tree.
    ///
    /// That is the whole structural difference between a branch and a loop, and it
    /// is why one of them needed a pre-pass and the other did not.
    ///
    /// # Why over-approximating is the safe direction
    ///
    /// [`Self::assigned_in`] resolves each assigned name in the scope the loop is
    /// written in, so a name the body shadows resolves to the OUTER binding and the
    /// loop carries one it did not need to. That costs a block parameter and
    /// nothing else: the value passes through unchanged on every edge.
    ///
    /// Under-approximating would be a wrong answer — a binding the body assigns and
    /// the header does not carry would be read across the back edge as the value
    /// from before the loop, for ever.
    ///
    /// # What the types at the header are, and why they are the domain's top
    ///
    /// A header parameter's type is the join of what arrives from before the loop
    /// and what arrives across the back edge, and the second is not known until the
    /// body has been lowered — which needs the parameter to exist. The loop is real
    /// and it is what [`rts_mir::infer`] exists to solve: it iterates to a fixed
    /// point over the finished graph and answers exactly that join.
    ///
    /// So this lowering records `top()` for a header parameter and the effects
    /// decided inside the body are pessimistic in consequence: arithmetic over a
    /// carried value is `CALLS_USER` even where inference will prove it numeric.
    /// That is sound and it is the one place this file knowingly leaves speed on
    /// the table — recovering it is a pass that recomputes effects from inference's
    /// answer, which is a pass over a finished graph and not a second traversal of
    /// the tree.
    fn loop_while(&mut self, condition: &Expr, body: &Stmt) -> Result<bool, Unsupported> {
        let carried: Vec<BindingId> = self.assigned_in(body)?.into_iter().collect();

        let header = self.builder.block();
        let into_body = self.builder.block();
        let exit = self.builder.block();

        // The values at the top of the loop, in the carried order.
        let entering: Vec<ValueId> = carried
            .iter()
            .map(|binding| self.values[binding])
            .collect();
        self.builder.end(Terminator::Jump {
            target: header,
            args: entering,
        });

        self.builder.switch_to(header);
        let mut params = Vec::with_capacity(carried.len());
        for binding in &carried {
            let param = self.builder.param(header);
            self.types.insert(param, self.domain.top());
            self.values.insert(*binding, param);
            params.push(param);
        }
        // The test is in the HEADER, which is what makes a `while` check before
        // each pass including the first, and what makes the value a carried
        // binding holds after the loop be the header's parameter.
        let tested = self.expression(condition)?;
        let tested = self.prim(JsPrim::Truthy, vec![tested], condition);
        let leaving: Vec<ValueId> = params.clone();
        self.builder.end(Terminator::Branch {
            condition: tested,
            then_block: into_body,
            then_args: Vec::new(),
            else_block: exit,
            else_args: leaving,
        });

        self.builder.switch_to(into_body);
        self.loops.push(LoopFrame {
            header,
            exit,
            carried: carried.clone(),
        });
        let ended = self.statement(body);
        self.loops.pop();
        let ended = ended?;
        // A body that left through a `return` or a `break` has already terminated
        // its block, so there is no back edge to write from here.
        if !ended {
            let back: Vec<ValueId> = carried
                .iter()
                .map(|binding| self.values[binding])
                .collect();
            self.builder.end(Terminator::Jump {
                target: header,
                args: back,
            });
        }

        self.builder.switch_to(exit);
        // After the loop, a carried binding holds what the exit block received --
        // which is the header's parameter, because that is where the test decided
        // to leave.
        let exiting: Vec<ValueId> = carried
            .iter()
            .map(|_| self.builder.param(exit))
            .collect();
        for (binding, param) in carried.iter().zip(&exiting) {
            self.types.insert(*param, self.domain.top());
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }

    /// Every binding an assignment in `body` may write.
    ///
    /// Over-approximating on purpose — see [`Self::loop_while`]. It walks through
    /// `emit::capture`'s traversal rather than matching statement kinds here, which
    /// is what keeps a statement added to the tree tomorrow from being silently
    /// skipped: that file's own header records how a second copy of the tree's
    /// shape is how a node comes to be walked by one analysis and missed by the
    /// other.
    fn assigned_in(&self, body: &Stmt) -> Result<BTreeSet<BindingId>, Unsupported> {
        let mut names = Vec::new();
        assigned_names_in_statement(body, &mut names);
        let mut found = BTreeSet::new();
        for name in names {
            // A NAME THIS SCOPE DOES NOT RESOLVE IS SKIPPED, and the first version
            // of this refused it as a global instead. That was wrong twice over.
            //
            // It is wrong about what the name is: a `let` inside the body is
            // declared in a scope this one cannot see, so a nested loop's own
            // counter looked like a global and every nested loop was refused.
            //
            // And it is wrong about where the refusal belongs. A name that really
            // is a global has no binding for a block parameter to hold, so there is
            // nothing to carry and skipping it is the correct answer here — and the
            // assignment itself is still refused, loudly, when the body reaches it
            // and asks `bind` for a binding that does not exist. The pre-pass
            // decides what to CARRY; what a program may do is not its question.
            if let Some(binding) = self.resolution.binding_in(self.scope, name) {
                found.insert(binding);
            }
        }
        Ok(found)
    }

    /// Leaves the innermost loop, or skips to its test.
    ///
    /// Both carry the same argument list, because the header and the exit declare
    /// the same parameters: one list of carried bindings per loop, so a jump from
    /// anywhere inside it needs no second convention.
    fn jump_out_of_loop(&mut self, to_header: bool) -> Result<bool, Unsupported> {
        let Some(frame) = self.loops.last() else {
            return Err(Unsupported::Statement(
                "a break or continue outside a loop is a label, which is not lowered",
            ));
        };
        let target = match to_header {
            true => frame.header,
            false => frame.exit,
        };
        let carried = frame.carried.clone();
        let args: Vec<ValueId> = carried
            .iter()
            .map(|binding| self.values[binding])
            .collect();
        self.builder.end(Terminator::Jump { target, args });
        Ok(true)
    }

    fn expression(&mut self, expr: &Expr) -> Result<ValueId, Unsupported> {
        match &expr.kind {
            ExprKind::Literal(literal) => self.literal(literal, expr),
            ExprKind::Ident(name) => {
                let Some(binding) = self.resolution.binding_in(self.scope, *name) else {
                    return Err(Unsupported::Global(*name));
                };
                match self.values.get(&binding) {
                    Some(value) => Ok(*value),
                    // Declared in this function and not yet reached: reading one
                    // before its declaration is the temporal dead zone, which
                    // needs a sentinel and a throw. `emit/scope.rs` records the
                    // same gap.
                    None => Err(Unsupported::Expression(
                        "a binding read before its declaration is in its dead zone",
                    )),
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
                let ExprKind::Ident(name) = &place.kind else {
                    return Err(Unsupported::Expression(
                        "an assignment to a property writes the heap",
                    ));
                };
                let held = self.expression(value)?;
                let of = self.type_of(held);
                self.bind(*name, held, of)?;
                // The value of an assignment is what was assigned, which is what
                // makes `a = b = 1` work.
                Ok(held)
            }
            // A CALL TO A FUNCTION THE MODULE NUMBERED.
            //
            // The callee is resolved through the binding and not through the
            // spelling, which is what makes this correct in a module that declares
            // `step` twice: `binding_in` answers which of the two this site reaches,
            // and the map answers which function that binding is.
            //
            // What is refused is everything else — a method, a call through a
            // parameter, a global. Each needs something this stage does not have: a
            // receiver proof, an indirect call with a signature, an entry point. The
            // refusals are separate so the survey can count them apart, which is how
            // this arm came to be written at all.
            ExprKind::Call {
                callee,
                arguments,
                optional: false,
            } => {
                let ExprKind::Ident(name) = &callee.kind else {
                    return Err(Unsupported::Expression(
                        "a call whose callee is not a plain name needs a receiver proof",
                    ));
                };
                let Some(binding) = self.resolution.binding_in(self.scope, *name) else {
                    return Err(Unsupported::Global(*name));
                };
                let Some(id) = self.callees.of_binding(binding) else {
                    return Err(Unsupported::Expression(
                        "a call to a binding that holds no function of this module",
                    ));
                };
                let mut args = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let crate::syntax::Spreadable::Single(value) = argument else {
                        return Err(Unsupported::Expression(
                            "a spread argument has a run-time count",
                        ));
                    };
                    args.push(self.expression(value)?);
                }
                // CALLS_USER and THROWS, because what the callee does is not known
                // here. An interprocedural pass is what narrows this — and
                // `passes::refine_effects` is already the shape that would apply the
                // answer, which is why the summary is on the instruction rather than
                // derived at every use.
                let held = self.builder.push(
                    Op::Call {
                        callee: rts_mir::cfg::Callee::Func(id),
                        args,
                    },
                    Effect::CALLS_USER.and(Effect::THROWS),
                    expr.at,
                );
                self.types.insert(held, self.domain.top());
                Ok(held)
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                let prim = match primitive(*op) {
                    Some(prim) => prim,
                    None => return Err(Unsupported::Operator(*op)),
                };
                Ok(self.prim(prim, vec![left, right], expr))
            }
            other => Err(Unsupported::Expression(expression_name(other))),
        }
    }

    fn literal(&mut self, literal: &Literal, at: &Expr) -> Result<ValueId, Unsupported> {
        let value = match literal {
            // An integral number that fits is an `Int::Int`, so the domain can
            // answer `Int32` for it and an addition of two of them can be proved.
            Literal::Number(held) => match held.fract() == 0.0 && i32::try_from(*held as i64).is_ok()
            {
                true => Const::Int(*held as i64),
                false => Const::Float(*held),
            },
            Literal::Boolean(held) => Const::Bool(*held),
            Literal::Singleton(which) => Const::Declared(*which as u32),
            Literal::String(_) => {
                return Err(Unsupported::Expression(
                    "a string literal needs the interner the machine holds",
                ));
            }
            _ => return Err(Unsupported::Expression("a literal of another kind")),
        };
        let of = self.domain.of_const(&value);
        let held = self
            .builder
            .push(Op::Const(value), Effect::PURE, at.at);
        self.types.insert(held, of);
        Ok(held)
    }

    fn singleton(&mut self, which: Singleton, at: &Stmt) -> ValueId {
        let value = Const::Declared(which as u32);
        let of = self.domain.of_const(&value);
        let held = self.builder.push(Op::Const(value), Effect::PURE, at.at);
        self.types.insert(held, of);
        held
    }

    /// Pushes a primitive, with the effect its operand types imply.
    ///
    /// This is where `domain::Js::effect_of` earns its shape: the same operator
    /// is `PURE` over two proven numbers and `CALLS_USER|THROWS` over two
    /// unknowns, so what a pass may later do with this instruction is decided
    /// here, from what is known here.
    fn prim(&mut self, which: JsPrim, args: Vec<ValueId>, at: &Expr) -> ValueId {
        let of_args: Vec<Type> = args.iter().map(|held| self.type_of(*held)).collect();
        let prim = self.domain.prim(which);
        let effect = self.domain.effect_of(prim, &of_args);
        let answered = self.domain.transfer(prim, &of_args);
        let held = self.builder.push(
            Op::Prim {
                prim,
                args,
            },
            effect,
            at.at,
        );
        self.types.insert(held, answered);
        held
    }

    /// Records what a declaration now holds.
    fn bind(&mut self, name: Name, value: ValueId, of: Type) -> Result<(), Unsupported> {
        let Some(binding) = self.resolution.binding_in(self.scope, name) else {
            return Err(Unsupported::Global(name));
        };
        self.values.insert(binding, value);
        self.types.insert(value, of);
        Ok(())
    }

    fn type_of(&self, value: ValueId) -> Type {
        self.types.get(&value).cloned().unwrap_or(Type::Anything)
    }
}

/// Every name an assignment or an increment in this statement writes, at any
/// depth, including inside a nested function.
///
/// A nested function is descended into deliberately. It cannot be lowered here
/// yet, so a body containing one is refused before this matters — but the day one
/// is, a closure that assigns an outer name across the back edge is exactly the
/// case a carried set must not miss, and a traversal that stopped at the function
/// boundary would miss it silently.
fn assigned_names_in_statement(statement: &Stmt, found: &mut Vec<Name>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => assigned_names_in_statement(inner, found),
        StmtChild::Expr(expr) => assigned_names_in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                assigned_names_in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                assigned_names_in_statement(inner, found);
            }
        }
        StmtChild::Function(function) => {
            if let FunctionBody::Block(statements) = &function.body {
                for inner in statements {
                    assigned_names_in_statement(inner, found);
                }
            }
        }
        StmtChild::Class(_) => {}
    });
}

fn assigned_names_in_expr(expr: &Expr, found: &mut Vec<Name>) {
    match &expr.kind {
        ExprKind::Assign {
            target: AssignTarget::Place(place),
            ..
        } => {
            if let ExprKind::Ident(name) = &place.kind {
                found.push(*name);
            }
        }
        ExprKind::Update { target, .. } => {
            if let ExprKind::Ident(name) = &target.kind {
                found.push(*name);
            }
        }
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => assigned_names_in_expr(inner, found),
        Child::Function(function) => {
            if let FunctionBody::Block(statements) = &function.body {
                for inner in statements {
                    assigned_names_in_statement(inner, found);
                }
            }
        }
        Child::Class(_) => {}
    });
}

/// The primitive an operator is, or `None` where this language's table has no row
/// for it yet.
fn primitive(op: BinaryOp) -> Option<JsPrim> {
    match op {
        BinaryOp::Add => Some(JsPrim::Add),
        BinaryOp::Sub => Some(JsPrim::Subtract),
        BinaryOp::Mul => Some(JsPrim::Multiply),
        BinaryOp::Div => Some(JsPrim::Divide),
        BinaryOp::Rem => Some(JsPrim::Remainder),
        BinaryOp::Less => Some(JsPrim::LessThan),
        BinaryOp::StrictEqual => Some(JsPrim::StrictEquals),
        // Every other operator is a row the table does not have. Deliberately not
        // expressed as `Greater` being `LessThan` with the operands swapped: the
        // two evaluate their operands in opposite orders, and `a > b` coercing
        // `a` first is observable.
        _ => None,
    }
}

/// The name a refusal reports for a statement.
fn name_of(kind: &StmtKind) -> &'static str {
    match kind {
        StmtKind::Break(_) => "break",
        StmtKind::Continue(_) => "continue",
        StmtKind::Labelled { .. } => "a label",
        StmtKind::Switch { .. } => "switch",
        StmtKind::With { .. } => "with",
        StmtKind::Using { .. } => "using",
        StmtKind::Debugger => "debugger",
        _ => "a statement kind",
    }
}

/// The name a refusal reports for an expression.
fn expression_name(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Call { .. } => "a call",
        ExprKind::Member { .. } | ExprKind::Index { .. } => "a property access",
        ExprKind::Assign { .. } => "an assignment",
        ExprKind::Update { .. } => "an increment",
        ExprKind::Function(_) => "a function expression",
        ExprKind::Object { .. } => "an object literal",
        ExprKind::Array { .. } => "an array literal",
        ExprKind::This => "this",
        ExprKind::Unary { .. } => "a unary operator",
        ExprKind::Logical { .. } => "a short-circuiting operator",
        ExprKind::Conditional { .. } => "a conditional",
        _ => "an expression kind",
    }
}

#[cfg(test)]
#[path = "lower_tests.rs"]
mod tests;
