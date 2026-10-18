//! `...rest` in an object pattern: the own properties the pattern did not name, copied
//! into a fresh object.
//!
//! One runtime call, `ObjectRest(source, excluded)`, which is `CopyDataProperties`:
//! `[[OwnPropertyKeys]]` once, then per key not excluded a descriptor and a read,
//! interleaved. An excluded key is never described or READ, so a getter under it does
//! not run -- which is why this is not a spread followed by deletions. A loop in the
//! graph over `OwnKeys` could not interleave: `OwnKeys` over a proxy asks every
//! descriptor before the first read.
//!
//! The excluded keys are compared as property keys, so `{ [0]: a, ...r }` excludes
//! the key `"0"`, as the language says.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::JsPrim;
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
        let keys = self.prim(JsPrim::NewArray, excluded.to_vec(), at);
        Ok(self.entry(RuntimeOp::ObjectRest, vec![from, keys], at))
    }
}
