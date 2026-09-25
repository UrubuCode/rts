//! Protected regions: where a throwing instruction goes.
//!
//! # Why this is neutral, and what is left to the language
//!
//! A region is structural. Every language with exceptions needs to say *these
//! instructions are protected, and control goes THERE when one of them raises*, and
//! none of them needs that said differently.
//!
//! What is not neutral is **what a handler catches**. One language catches everything
//! with one clause; another matches on a type; a third has a tag per raise site.
//! `rts_cranelift::unwind::Handler` carries a `Tag` for exactly that reason — so the
//! tag is the language's, and it arrives through `MachineOps` at lowering time rather
//! than being a field here.
//!
//! # The handler receives the value
//!
//! As its first block parameter, which is the discipline the machine already states
//! for its own handlers: *"a handler that did not receive the value would have to find
//! it somewhere else, and somewhere else is a side channel that outlives the frame it
//! belongs to."* `verify` refuses a handler block with no parameters for that reason.
//!
//! # Why membership is per block and not per instruction
//!
//! Because a region is a span of CONTROL, not a set of operations. An instruction that
//! throws sends control to the handler of the region its BLOCK is in, and a block is
//! either inside the `try` or it is not — there is no half. Marking instructions
//! instead would let a block hold two answers, and the first thing that breaks is a
//! call in the middle of one.

use crate::cfg::BlockId;

/// A protected region of a function.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct RegionId(pub u32);

/// What a region protects with.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Region {
    /// The region this one is written inside, for a `try` inside a `try`.
    ///
    /// A raise looks at the innermost region first and walks out, so the chain is
    /// what makes a nested `try` catch before the one around it.
    pub parent: Option<RegionId>,
    /// Where control goes when something in the region raises.
    ///
    /// Receives the raised value as its first block parameter.
    pub handler: Option<BlockId>,
    /// Where control goes on the way out, however it leaves.
    ///
    /// `None` here is not "no cleanup needed" — it is "this region has none", which is
    /// what a `try`/`catch` with no `finally` is.
    pub cleanup: Option<BlockId>,
    /// Where a resumption that RETURNS, injected at a suspension inside the region,
    /// carries on instead of returning -- the machine's `Region::resume_return`, and
    /// its reason: a return a client writes is a jump the client routes, and this one
    /// is written nowhere, so the region is the only place its destination can be
    /// stated. Receives the delivered value as its first parameter, as a handler does.
    pub resume_return: Option<BlockId>,
}

#[cfg(test)]
mod tests {
    use crate::Effect;
    use crate::cfg::{Const, FuncBuilder, Op, Terminator};
    use crate::guard::Tier;
    use crate::verify::{Malformed, verify};

    #[test]
    fn a_block_inside_a_region_says_which_one() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let handler = build.block();
        build.param(handler);
        let region = build.open_region(Some(handler), None);
        let inside = build.block();
        build.switch_to(inside);
        let held = build.push(Op::Const(Const::Int(1)), Effect::THROWS, Default::default());
        build.end(Terminator::Return(Some(held)));
        build.close_region();
        let outside = build.block();
        build.switch_to(outside);
        build.end(Terminator::Return(None));
        build.switch_to(build.entry_block());
        build.end(Terminator::Jump {
            target: inside,
            args: Vec::new(),
        });
        build.switch_to(handler);
        build.end(Terminator::Return(None));

        let func = build.finish();
        assert_eq!(func.region_of(inside), Some(region));
        assert_eq!(func.region_of(outside), None);
        assert_eq!(verify(&func), Ok(()));
    }

    /// A handler that does not receive the raised value would have to find it
    /// somewhere else, and somewhere else is a side channel that outlives the frame.
    #[test]
    fn a_handler_with_no_parameter_is_refused() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let handler = build.block();
        build.open_region(Some(handler), None);
        build.end(Terminator::Return(None));
        build.close_region();
        build.switch_to(handler);
        build.end(Terminator::Return(None));
        assert_eq!(
            verify(&build.finish()),
            Err(Malformed::HandlerTakesNoValue(handler))
        );
    }

    /// A region opened inside another records it, which is what makes the inner `try`
    /// catch first.
    #[test]
    fn a_nested_region_records_its_parent() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let outer_handler = build.block();
        build.param(outer_handler);
        let inner_handler = build.block();
        build.param(inner_handler);
        let outer = build.open_region(Some(outer_handler), None);
        let inner = build.open_region(Some(inner_handler), None);
        build.close_region();
        build.close_region();
        build.end(Terminator::Return(None));
        build.switch_to(outer_handler);
        build.end(Terminator::Return(None));
        build.switch_to(inner_handler);
        build.end(Terminator::Return(None));

        let func = build.finish();
        assert_eq!(func.region(inner).parent, Some(outer));
        assert_eq!(func.region(outer).parent, None);
    }
}
