//! The parameters that arrive in no register: a fifth one and beyond, and `...rest`.
//!
//! The convention carries four argument slots, and a call passing more hands the
//! runtime a vector instead (`RuntimeOp::CallWithArgs`). `RestArguments` reads either
//! back -- the vector when the caller built one, the four slots when it did not -- from a
//! position the compiler fixes. So both shapes are the running emitter's
//! `bind_parameters`, said in the graph: one entry-point call, and a read by index for
//! each parameter past the slots.
//!
//! A DEFAULT is the other thing a parameter list does at run time, and it is here for
//! that reason: [`Lowering::defaults`].
//!
//! What the graph needs for it is all four slots, whether or not the program named them,
//! so a function that gathers declares every slot as an entry parameter and binds the
//! ones the program wrote.

use rts_mir::cfg::{Callee, Const, Op, ValueId};
use rts_mir::{Domain as _, Effect};

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim, Type};
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{Expr, ExprKind, Function, Pattern};

impl Lowering<'_> {
    /// Binds the parameters past the slots, and the rest parameter, if there are any.
    pub(super) fn gather(&mut self, function: &Function) -> Result<(), Unsupported> {
        let written = function.parameters.len();
        let rest = match &function.rest_parameter {
            Some(Pattern::Name(name)) => Some(*name),
            Some(_) => return Err(Unsupported::Pattern),
            None => None,
        };
        // `arguments`, where a function that has one -- not an arrow, which sees its
        // enclosing function's -- mentions it: the same test `emit/function.rs` makes.
        let wants_arguments = !function.captures_this
            && match &function.body {
                crate::syntax::FunctionBody::Block(body) => self
                    .names
                    .find("arguments")
                    .is_some_and(|named| crate::emit::capture::mentions(body, named)),
                crate::syntax::FunctionBody::Expression(_) => false,
            };
        if rest.is_none() && written <= ARGUMENT_SLOTS && !wants_arguments {
            return Ok(());
        }
        let at = Expr {
            kind: ExprKind::This,
            at: function.at,
        };
        // EVERY SLOT, the ones the loop over the parameters did not declare included.
        let entry = self.builder.entry_block();
        let declared = written.min(ARGUMENT_SLOTS);
        let mut slots: Vec<ValueId> = self.builder.params_of(entry)[..declared].to_vec();
        while slots.len() < ARGUMENT_SLOTS {
            slots.push(self.builder.param(entry));
        }

        if written > ARGUMENT_SLOTS {
            let all = self.rest_arguments(0, &slots, &at);
            for (position, parameter) in function.parameters.iter().enumerate().skip(ARGUMENT_SLOTS)
            {
                let Pattern::Name(name) = &parameter.target else {
                    return Err(Unsupported::Pattern);
                };
                let index = {
                    let value = Const::Int(position as i64);
                    let of = self.domain.of_const(&value);
                    let pushed = self.builder.push(Op::Const(value), Effect::PURE, at.at);
                    self.types.insert(pushed, of);
                    pushed
                };
                let value = self.prim(JsPrim::IndexRead, vec![all, index], &at);
                self.bind(*name, value, Type::Anything, &at)?;
            }
        }
        if wants_arguments {
            let entry = self.domain.entry_point(RuntimeOp::ArgumentsObject);
            self.arguments = Some(self.call(Callee::Entry(entry), None, slots.clone(), &at));
        }
        if let Some(name) = rest {
            let gathered = self.rest_arguments(written as u32, &slots, &at);
            self.bind(name, gathered, Type::Object, &at)?;
        }
        Ok(())
    }

    /// Each parameter with a default, as `if (p === void 0) p = default;` -- the
    /// language's own definition of one, lowered by the statements that already exist.
    ///
    /// AFTER the environment is open and not with the parameters: a default may read a
    /// captured binding, and a captured parameter it writes lives there by then. In
    /// order, so a default reads the parameters before it with their defaults applied.
    ///
    /// `void 0` and not `undefined`, because `undefined` is a name a program may bind.
    /// A default that WRITES a function is declined elsewhere rather than here: its
    /// function is outside the body, so nothing numbered it, and the closure refuses.
    ///
    /// A PATTERN parameter is taken apart here too, in the same order and for the same
    /// reason -- `patterns` holds the value each one arrived as, by position.
    pub(super) fn defaults(
        &mut self,
        function: &Function,
        patterns: &[(usize, ValueId)],
    ) -> Result<(), Unsupported> {
        for (position, parameter) in function.parameters.iter().enumerate() {
            if let Some((_, arrived)) = patterns.iter().find(|(at, _)| *at == position) {
                let at = Expr {
                    kind: ExprKind::This,
                    at: function.at,
                };
                self.destructure(&parameter.target, *arrived, &at)?;
                continue;
            }
            let Some(default) = &parameter.default else {
                continue;
            };
            let Pattern::Name(name) = &parameter.target else {
                return Err(Unsupported::Pattern);
            };
            let at = default.at;
            let expr = |kind| Expr { kind, at };
            let absent = expr(ExprKind::Unary {
                op: crate::syntax::UnaryOp::Void,
                operand: Box::new(expr(ExprKind::Literal(crate::syntax::Literal::Number(0.0)))),
            });
            let written = crate::syntax::Stmt {
                kind: crate::syntax::StmtKind::If {
                    condition: expr(ExprKind::Binary {
                        op: crate::syntax::BinaryOp::StrictEqual,
                        left: Box::new(expr(ExprKind::Ident(*name))),
                        right: Box::new(absent),
                    }),
                    then_branch: Box::new(crate::syntax::Stmt {
                        kind: crate::syntax::StmtKind::Expr(expr(ExprKind::Assign {
                            target: crate::syntax::AssignTarget::Place(Box::new(expr(
                                ExprKind::Ident(*name),
                            ))),
                            value: Box::new(default.clone()),
                            op: crate::syntax::AssignOp::Plain,
                        })),
                        at,
                    }),
                    else_branch: None,
                },
                at,
            };
            self.statement(&written)?;
        }
        Ok(())
    }

    /// `RestArguments` from position `from`, over the four slots.
    fn rest_arguments(&mut self, from: u32, slots: &[ValueId], at: &Expr) -> ValueId {
        let count = self.domain.constant(JsConst::Count(from));
        let count = self.declared(count, at);
        let mut args = vec![count];
        args.extend_from_slice(slots);
        let entry = self.domain.entry_point(RuntimeOp::RestArguments);
        self.call(Callee::Entry(entry), None, args, at)
    }
}
