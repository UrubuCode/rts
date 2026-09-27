//! `...rest` in an object pattern: the own properties the pattern did not name, copied
//! into a fresh object.
//!
//! The running emitter's shape (`emit/destructure/mod.rs::object_rest`), built as a
//! loop in the graph: the keys `OwnKeys` answers, each compared with the keys the
//! pattern already read, and every other one read from the source and written into
//! the rest. An excluded key is never READ, so a getter under it does not run -- which
//! is why this is not a spread followed by deletions.
//!
//! The same named divergence as there: a computed key is compared as the value it
//! evaluated to, with `===`, so `{ [0]: a, ...r }` does not exclude the key `"0"`.

use rts_mir::Domain;
use rts_mir::cfg::{Terminator, ValueId};

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::runtime::RuntimeOp;
use crate::syntax::Expr;

impl Lowering<'_> {
    /// The rest object of `from`, less the keys in `excluded` -- each either a text
    /// constant of a written name or a computed key's value.
    pub(super) fn object_rest(
        &mut self,
        from: ValueId,
        excluded: &[ValueId],
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let none = self.domain.constant(JsConst::Count(0));
        let none = self.declared(none, at);
        let rest = self.entry(RuntimeOp::ObjectNew, vec![none], at);
        let keys = self.entry(RuntimeOp::OwnKeys, vec![from], at);
        let length = self.entry(RuntimeOp::ArrayLength, vec![keys], at);
        let zero = self.integer(0, at);

        let header = self.builder.block();
        let checking = self.builder.block();
        let copying = self.builder.block();
        let step = self.builder.block();
        let exit = self.builder.block();
        self.builder.end(Terminator::Jump {
            target: header,
            args: vec![zero],
        });

        self.builder.switch_to(header);
        let counter = self.builder.param(header);
        self.types.insert(counter, self.domain.top());
        let more = self.prim(JsPrim::LessThan, vec![counter, length], at);
        let more = self.prim(JsPrim::Truthy, vec![more], at);
        self.builder.end(Terminator::Branch {
            condition: more,
            then_block: checking,
            then_args: Vec::new(),
            else_block: exit,
            else_args: Vec::new(),
        });

        // A KEY THE PATTERN NAMED is skipped, straight to the step; each comparison is
        // its own branch, so the first match leaves.
        self.builder.switch_to(checking);
        let key = self.entry(RuntimeOp::ElementAt, vec![keys, counter], at);
        for named in excluded {
            let same = self.prim(JsPrim::StrictEquals, vec![key, *named], at);
            let next = self.builder.block();
            self.builder.end(Terminator::Branch {
                condition: same,
                then_block: step,
                then_args: Vec::new(),
                else_block: next,
                else_args: Vec::new(),
            });
            self.builder.switch_to(next);
        }
        self.builder.end(Terminator::Jump {
            target: copying,
            args: Vec::new(),
        });

        self.builder.switch_to(copying);
        let value = self.prim(JsPrim::IndexRead, vec![from, key], at);
        self.prim(JsPrim::IndexWrite, vec![rest, key, value], at);
        self.builder.end(Terminator::Jump {
            target: step,
            args: Vec::new(),
        });

        self.builder.switch_to(step);
        let one = self.integer(1, at);
        let next = self.prim(JsPrim::Add, vec![counter, one], at);
        self.builder.end(Terminator::Jump {
            target: header,
            args: vec![next],
        });

        self.builder.switch_to(exit);
        Ok(rest)
    }
}
