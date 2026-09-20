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

use crate::domain::{Js, JsPrim, Type};
use crate::names::Name;
use crate::names::resolve::{BindingId, Resolution, ScopeId};
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Binding, Expr, ExprKind, Function, FunctionBody, Literal, Pattern, Stmt, StmtKind,
};
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
        domain: Js::new(),
        values: BTreeMap::new(),
        types: BTreeMap::new(),
        resolution,
        scope,
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

    Ok(Lowered {
        func: lowering.builder.finish(),
        domain: lowering.domain,
    })
}

/// The state one function's lowering carries.
struct Lowering<'a> {
    builder: FuncBuilder,
    domain: Js,
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
            StmtKind::While { .. } | StmtKind::DoWhile { .. } | StmtKind::For { .. } => {
                Err(Unsupported::Statement("a loop needs a back edge"))
            }
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
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::names::resolve::resolve_module;
    use crate::parse::parse_script;
    use crate::syntax::ModuleItem;
    use rts_mir::verify::verify;

    /// The first function of a script, lowered.
    fn only(source: &str) -> Result<Lowered, Unsupported> {
        let mut names = Names::new();
        let program = parse_script(source, &mut names).expect("the fixture parses");
        let resolution = resolve_module(&program.body);
        let function = program
            .body
            .iter()
            .find_map(|item| match item {
                ModuleItem::Stmt(Stmt {
                    kind: StmtKind::Function(function),
                    ..
                }) => Some(function),
                _ => None,
            })
            .expect("the fixture declares a function");
        lower(function, &resolution, Tier::Generic)
    }

    #[test]
    fn a_straight_line_body_lowers_to_a_well_formed_graph() {
        let lowered = only("function f(x) { const two = 2; return x - two; }")
            .expect("the subset covers this");
        assert_eq!(verify(&lowered.func), Ok(()));
        assert_eq!(lowered.func.blocks.len(), 1);
    }

    /// The whole pipeline composing: the tree gives a graph, the graph gives
    /// types, and the types are this language's answers.
    #[test]
    fn the_graph_infers_this_languages_types() {
        let lowered = only("function f() { const a = 2; const b = 3; return a / b; }")
            .expect("the subset covers this");
        assert_eq!(verify(&lowered.func), Ok(()));
        let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
        let last = lowered.func.insts.last().expect("three instructions");
        // Division answers a number even of two integers, which is this
        // language's rule and the toy domain's too -- for different reasons.
        assert_eq!(*types.of(last.result), Type::Double);
    }

    /// The effect is decided from what is known where the instruction is pushed,
    /// which is what makes a proven arithmetic operation movable.
    #[test]
    fn an_operation_over_proven_numbers_is_pure_and_one_over_a_parameter_is_not() {
        let proven =
            only("function f() { const a = 1; const b = 2; return a + b; }").expect("covered");
        let last = proven.func.insts.last().expect("an addition");
        assert!(last.effect.is_pure());

        let unknown = only("function f(x) { const b = 2; return x + b; }").expect("covered");
        let last = unknown.func.insts.last().expect("an addition");
        assert!(last.effect.has(Effect::CALLS_USER));
        assert!(last.effect.has(Effect::THROWS));
    }

    #[test]
    fn a_global_is_refused_by_name_and_not_treated_as_a_local() {
        let refused =
            only("function f() { return Math; }").expect_err("a global needs an entry point");
        assert!(matches!(refused, Unsupported::Global(_)));
    }

    /// Greater-than is NOT less-than with the operands swapped, and the refusal
    /// records why rather than silently doing it.
    #[test]
    fn an_operator_with_no_row_is_refused_by_name() {
        let refused = only("function f(a, b) { return a > b; }").expect_err("no row for >");
        assert_eq!(refused, Unsupported::Operator(BinaryOp::Greater));
    }

    #[test]
    fn a_parked_frame_is_refused_before_anything_is_built() {
        let refused = only("async function f() { return 1; }").expect_err("async parks");
        assert!(matches!(refused, Unsupported::Shape(_)));
        let refused = only("function* f() { return 1; }").expect_err("a generator parks");
        assert!(matches!(refused, Unsupported::Shape(_)));
    }

    #[test]
    fn a_body_with_no_return_answers_nothing_and_is_still_well_formed() {
        let lowered = only("function f() { const a = 1; }").expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        assert_eq!(
            lowered.func.block(lowered.func.entry()).terminator,
            Some(Terminator::Return(None))
        );
    }

    /// The declared-constant numbering is a coupling between two files, so it is
    /// pinned rather than trusted: `lower` writes a `Singleton`'s discriminant
    /// into `Const::Declared` and `domain` reads it back.
    ///
    /// Reordering the enum would silently make `undefined` mean `null` — and both
    /// are falsy, both answer `false` to every truth question, and neither is a
    /// number or a string. So no assertion about behaviour would catch it, which
    /// is the shape of defect this repository calls silent.
    #[test]
    fn the_singleton_numbering_is_the_one_the_domain_reads() {
        let domain = Js::new();
        assert_eq!(
            domain.of_const(&Const::Declared(Singleton::Undefined as u32)),
            Type::Undefined
        );
        assert_eq!(
            domain.of_const(&Const::Declared(Singleton::Null as u32)),
            Type::Null
        );
    }

    /// A declaration with no initialiser is `undefined`, through that same
    /// numbering.
    #[test]
    fn a_declaration_with_no_value_holds_undefined() {
        let lowered = only("function f() { let a; return a; }").expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
        let first = lowered.func.insts.first().expect("one constant");
        assert_eq!(*types.of(first.result), Type::Undefined);
    }

    /// A branch with both arms rebinding one local: the join carries exactly that
    /// one binding, and the type at the join is the domain's join of the two.
    #[test]
    fn a_branch_merges_only_what_the_arms_disagree_about() {
        let lowered = only(
            "function f(c) {
               let a = 1;
               const kept = 9;
               if (c) { a = 2; } else { a = 3; }
               return a;
             }",
        )
        .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));

        // Four blocks: entry, the two arms, the join.
        assert_eq!(lowered.func.blocks.len(), 4);
        // And the join declares ONE parameter -- `a`, not `kept` and not `c`.
        let join = lowered
            .func
            .block_ids()
            .find(|held| lowered.func.predecessors(*held).len() == 2)
            .expect("a join block");
        assert_eq!(lowered.func.block(join).params.len(), 1);
    }

    /// The types the two arms leave are joined by the DOMAIN, which is the one
    /// thing a lattice is for.
    #[test]
    fn the_join_takes_the_domains_answer_and_not_the_irs() {
        let lowered = only(
            "function f(c) {
               let a = 1;
               if (c) { a = 2; } else { a = 0.5; }
               return a;
             }",
        )
        .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
        let join = lowered
            .func
            .block_ids()
            .find(|held| lowered.func.predecessors(*held).len() == 2)
            .expect("a join block");
        let carried = lowered.func.block(join).params[0];
        // An integer from one arm and a fraction from the other. THIS language has
        // one numeric type, so the answer is a number; the toy domain's answer for
        // the same shape is its top.
        assert_eq!(*types.of(carried), Type::Double);
    }

    /// An arm that returned contributes nothing to the join, because control does
    /// not arrive from it.
    #[test]
    fn an_arm_that_returned_is_not_a_second_opinion() {
        let lowered = only(
            "function f(c) {
               let a = 1;
               if (c) { return 0; } else { a = 2; }
               return a;
             }",
        )
        .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        let join = lowered
            .func
            .block_ids()
            .find(|held| lowered.func.predecessors(*held).len() == 1 && held.0 != 0)
            .expect("a join reached from one arm only");
        assert!(lowered.func.block(join).params.is_empty());
    }

    /// Both arms leaving means nothing follows the `if`, and the function is still
    /// well formed -- no empty block left behind without a terminator.
    #[test]
    fn both_arms_returning_ends_the_statement_run() {
        let lowered = only("function f(c) { if (c) { return 1; } else { return 2; } }")
            .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
    }

    /// A block is a scope walked in step, and a binding inside it is a different
    /// `BindingId` -- which is why nothing here decides a shadowing rule.
    #[test]
    fn a_block_scope_is_walked_and_its_binding_is_its_own() {
        let lowered = only(
            "function f() {
               let i = 1;
               { let i = 2; }
               return i;
             }",
        )
        .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
        let returned = match &lowered.func.block(lowered.func.entry()).terminator {
            Some(Terminator::Return(Some(value))) => *value,
            other => panic!("expected a returned value, got {other:?}"),
        };
        // The OUTER `i` is returned. Both are `Int32`, so the type cannot tell them
        // apart -- what says it is the right one is the value: it is the constant
        // `1`, which is the first instruction.
        assert_eq!(*types.of(returned), Type::Int32);
        assert_eq!(lowered.func.insts[0].result, returned);
    }

    /// The compound form is refused rather than rewritten, because `a += b`
    /// evaluates its target once and `a = a + b` evaluates it twice.
    #[test]
    fn a_compound_assignment_is_refused_rather_than_rewritten() {
        let refused =
            only("function f(a) { a += 1; return a; }").expect_err("no lowering for compound");
        assert!(matches!(refused, Unsupported::Expression(_)));
    }

    /// An assignment answers what was assigned, so a chain works.
    #[test]
    fn an_assignment_answers_the_value_it_assigned() {
        let lowered = only("function f() { let a = 0; let b = 0; a = b = 7; return a; }")
            .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
        let returned = match &lowered.func.block(lowered.func.entry()).terminator {
            Some(Terminator::Return(Some(value))) => *value,
            other => panic!("expected a returned value, got {other:?}"),
        };
        assert_eq!(*types.of(returned), Type::Int32);
    }

    /// The truth rule is an operation of the LANGUAGE, and the domain folds it
    /// where the type decides it.
    #[test]
    fn a_branch_over_something_always_true_is_folded_by_the_domain() {
        let lowered = only("function f() { if (1 === 1) { return 1; } return 2; }")
            .expect("covered");
        assert_eq!(verify(&lowered.func), Ok(()));
        let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
        // `===` answers a boolean of unknown value, so the truth of it is unknown
        // too: nothing is folded here, and that is the honest answer.
        let tested = lowered
            .func
            .insts
            .iter()
            .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if *prim == lowered.domain.prim(JsPrim::Truthy)))
            .expect("a truthiness test");
        assert_eq!(*types.of(tested.result), Type::Bool(None));
    }
}
