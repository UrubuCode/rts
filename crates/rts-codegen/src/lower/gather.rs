//! The parameters that arrive in no register: a fifth one and beyond, and `...rest`.
//!
//! The convention carries four argument slots, and a call passing more hands the
//! runtime a vector instead (`RuntimeOp::CallWithArgs`). `RestArguments` reads either
//! back -- the vector when the caller built one, the four slots when it did not -- from a
//! position the compiler fixes. So both shapes are the running emitter's
//! `bind_parameters`, said in the graph: one entry-point call, and a read by index for
//! each parameter past the slots.
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
        if rest.is_none() && written <= ARGUMENT_SLOTS {
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
            for (position, parameter) in function.parameters.iter().enumerate().skip(ARGUMENT_SLOTS) {
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
        if let Some(name) = rest {
            let gathered = self.rest_arguments(written as u32, &slots, &at);
            self.bind(name, gathered, Type::Object, &at)?;
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
