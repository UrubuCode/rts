//! A function: the tables every handle indexes into.
//!
//! The function owns its values, blocks, instructions and constants. Handles are
//! indices into these tables and are meaningless against a different function —
//! the verifier rejects a program that mixes them rather than reading plausible
//! data from the wrong table.

use super::consts::ConstDecl;
use super::entity::{BlockId, ConstId, InstId, ValueId};
use super::inst::{BlockData, Inst, InstData, Terminator};
use crate::abi::Convention;
use crate::repr::Repr;
use crate::unwind::{RegionId, RegionTree};

/// What a function accepts and returns.
///
/// Multiple returns are expressible from the start. A signature model built
/// around a single return has to be rebuilt the day a client needs more, and
/// rebuilding an ABI is not a local change.
// `Hash` derived alongside the existing `PartialEq`/`Eq`: every field is
// `Repr`, `Vec<Repr>`, `bool` or `Convention`, all of which derive `Hash`
// themselves with no hand-written `PartialEq` anywhere in the chain, so the
// derived `Hash` agrees with the derived `Eq` by construction. This is what
// lets `FuncRegistry::declare_signature` index signatures by a `HashMap`
// instead of scanning every signature interned so far (see its doc comment).
#[derive(Clone, PartialEq, Eq, Debug, Default, Hash)]
pub struct Signature {
    /// Parameter representations, in order.
    pub params: Vec<Repr>,
    /// Return representations, in order.
    pub returns: Vec<Repr>,
    /// Whether the function may park its frame.
    ///
    /// A property of the function, declared where it is defined. A caller does
    /// not re-declare it at each site, and a site does not choose it: whether a
    /// call parks the caller follows from what the callee is.
    pub may_suspend: bool,
    /// Which register and stack discipline it follows.
    pub convention: Convention,
}

impl Signature {
    /// Whether a tail call from this function to `callee` is legal.
    ///
    /// Both sides must permit tail calls and the return lists must match
    /// exactly. That makes a tail-recursive group a unit — the whole group
    /// compiles under the tail convention or none of it does — and it is checked
    /// here so that a call site cannot build an edge that would be rejected
    /// further down, where the cause is no longer visible.
    pub fn permits_tail_call_to(&self, callee: &Signature) -> bool {
        // The rule itself is `abi::tail_call_permitted`, stated once. This type
        // and the ABI's both need the answer, and both used to carry their own
        // copy of the three conjuncts — identical, and with nothing to keep
        // them that way.
        crate::abi::tail_call_permitted(
            self.convention,
            callee.convention,
            self.returns == callee.returns,
        )
    }
}

/// Where a value came from.
///
/// Recorded because the verifier needs to distinguish a value that exists
/// because a block receives it from one that exists because an instruction
/// produced it, and because a diagnostic that cannot say where a value came from
/// is a diagnostic nobody can act on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueOrigin {
    /// A parameter of the given block.
    BlockParam(BlockId),
    /// The result of the given instruction.
    InstResult(InstId),
}

/// A value's representation and origin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ValueData {
    /// How it exists on the machine.
    pub repr: Repr,
    /// Where it came from.
    pub origin: ValueOrigin,
}

/// A function under construction or under verification.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Function {
    /// What it accepts and returns.
    pub signature: Signature,
    /// The block control enters at.
    pub entry: BlockId,
    /// The protected regions this function declares.
    ///
    /// Held by the function rather than beside it because a region identifier is
    /// only meaningful against the tree that issued it, and the frame descriptor
    /// records one per program point.
    pub regions: RegionTree,
    values: Vec<ValueData>,
    blocks: Vec<BlockData>,
    insts: Vec<InstData>,
    consts: Vec<ConstDecl>,
    /// Where each declared constant already sits, so declaring one twice
    /// answers the first handle instead of appending a second row.
    ///
    /// # Why a second structure over the same data
    ///
    /// The pool has to stay a `Vec` in declaration order: `ConstId` indexes it,
    /// lowering walks it, and the text a person reads prints it — rule 13 says
    /// anything compared between builds is ordered deterministically, and a hash
    /// map's order is not. So this is the lookup and `consts` is the sequence,
    /// which is the usual pairing rather than two sources of one fact: nothing
    /// reads this map for content, only for a handle into the other.
    ///
    /// # What it removes
    ///
    /// Measured 2026-08-22 over `bench/analytic.ts` before this existed: **3 417
    /// pool rows holding 202 distinct values.** Every literal `0` a throw check
    /// compared against, every `undefined` a block answered, was its own row and
    /// its own `Inst::Const`. Rule 8 — derive what a client would otherwise have
    /// to remember — is the argument for putting it here rather than asking
    /// every emitter to keep its own memo: they would each keep a different one,
    /// and `emit/` has nineteen `declare_const` call sites.
    ///
    /// # What it does NOT remove
    ///
    /// The instructions. Each `use_const` still emits its own `Inst::Const`, and
    /// it must: a value has to dominate its uses, so one materialization cannot
    /// be shared across blocks that do not dominate each other. This collapses
    /// the pool, not the SSA.
    const_at: std::collections::HashMap<ConstDecl, ConstId>,
    caches: u32,
    block_regions: Vec<Option<RegionId>>,
    positions: Vec<crate::fault::Position>,
}

impl Function {
    /// Creates a function with an entry block whose parameters match the
    /// signature.
    ///
    /// The entry block's parameters are the function's parameters; there is no
    /// second way to name them, so they cannot disagree.
    pub fn new(signature: Signature) -> Self {
        let mut func = Function {
            entry: BlockId(0),
            signature,
            regions: RegionTree::new(),
            values: Vec::new(),
            blocks: Vec::new(),
            insts: Vec::new(),
            consts: Vec::new(),
            const_at: std::collections::HashMap::new(),
            caches: 0,
            block_regions: Vec::new(),
            positions: Vec::new(),
        };

        let entry = func.push_block();
        debug_assert_eq!(entry, func.entry);
        for i in 0..func.signature.params.len() {
            let repr = func.signature.params[i];
            let value = func.push_value(repr, ValueOrigin::BlockParam(entry));
            func.blocks[entry.index()].params.push(value);
        }
        func
    }

    /// Appends an empty block, outside any protected region.
    pub fn push_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(BlockData::default());
        self.block_regions.push(None);
        id
    }

    /// Places a block inside a protected region.
    ///
    /// Membership is a property of the block rather than of each instruction:
    /// every point in a block is protected by the same region, which is what
    /// makes the region a lexical fact the frame descriptor can record once per
    /// program point without recomputing a nesting depth.
    pub fn set_block_region(&mut self, block: BlockId, region: RegionId) {
        self.block_regions[block.index()] = Some(region);
    }

    /// Which region protects a block, if any.
    pub fn region_of(&self, block: BlockId) -> Option<RegionId> {
        self.block_regions.get(block.index()).copied().flatten()
    }

    /// Appends a parameter to a block and returns the value it binds.
    pub fn push_block_param(&mut self, block: BlockId, repr: Repr) -> ValueId {
        let value = self.push_value(repr, ValueOrigin::BlockParam(block));
        self.blocks[block.index()].params.push(value);
        value
    }

    /// Appends an instruction to a block, binding the values it defines.
    ///
    /// A list rather than an option because a call can return more than one
    /// value, and an instruction model built around at most one result has to be
    /// rebuilt the day that matters — which is the same reason the signature
    /// carries a list of returns.
    pub fn push_inst(&mut self, block: BlockId, inst: Inst, results: &[Repr]) -> Vec<ValueId> {
        let inst_id = InstId(self.insts.len() as u32);
        self.insts.push(InstData {
            inst,
            results: Vec::new(),
        });

        let values: Vec<_> = results
            .iter()
            .map(|&repr| self.push_value(repr, ValueOrigin::InstResult(inst_id)))
            .collect();
        self.positions.push(crate::fault::Position::UNKNOWN);
        self.insts[inst_id.index()].results = values.clone();
        self.blocks[block.index()].insts.push(inst_id);
        values
    }

    /// Says where in the client's program an instruction came from.
    ///
    /// Optional, and silent when omitted. An instruction with no position is not
    /// a mistake — plenty of emitted code belongs to no single place — and
    /// inventing one would be worse than admitting it.
    pub fn set_position(&mut self, inst: InstId, position: crate::fault::Position) {
        self.positions[inst.index()] = position;
    }

    /// Where an instruction came from, if anywhere.
    pub fn position_of(&self, inst: InstId) -> crate::fault::Position {
        self.positions
            .get(inst.index())
            .copied()
            .unwrap_or(crate::fault::Position::UNKNOWN)
    }

    /// Sets how control leaves a block.
    pub fn set_terminator(&mut self, block: BlockId, terminator: Terminator) {
        self.blocks[block.index()].terminator = Some(terminator);
    }

    /// Declares a constant and returns its handle.
    ///
    /// Declaring the same constant twice answers the same handle. A constant is
    /// immutable and identified by its content — nothing here can write through
    /// a `ConstId`, and no consumer distinguishes two rows holding the same
    /// declaration — so a second row would differ from the first in nothing but
    /// its number. See [`Function::const_at`] for what that was costing.
    pub fn push_const(&mut self, decl: ConstDecl) -> ConstId {
        if let Some(already) = self.const_at.get(&decl) {
            return *already;
        }
        let id = ConstId(self.consts.len() as u32);
        self.const_at.insert(decl.clone(), id);
        self.consts.push(decl);
        id
    }

    /// Declares somewhere a property read can remember what it last saw.
    pub fn push_cache(&mut self) -> crate::ir::CacheId {
        let id = crate::ir::CacheId(self.caches);
        self.caches += 1;
        id
    }

    /// How many sites in this function remember anything.
    pub fn cache_count(&self) -> usize {
        self.caches as usize
    }

    /// A value's representation and origin, or `None` if the handle is foreign.
    pub fn value(&self, id: ValueId) -> Option<ValueData> {
        self.values.get(id.index()).copied()
    }

    /// A value's representation.
    ///
    /// Panics on a handle this function did not issue, which means two functions
    /// were mixed — a wiring mistake, not a condition a caller handles.
    pub fn repr_of(&self, id: ValueId) -> Repr {
        self.value(id)
            .expect("value handle does not belong to this function")
            .repr
    }

    /// A block's contents, or `None` if the handle is foreign.
    pub fn block(&self, id: BlockId) -> Option<&BlockData> {
        self.blocks.get(id.index())
    }

    /// An instruction, or `None` if the handle is foreign.
    pub fn inst(&self, id: InstId) -> Option<&InstData> {
        self.insts.get(id.index())
    }

    /// A declared constant, or `None` if the handle is foreign.
    pub fn constant(&self, id: ConstId) -> Option<&ConstDecl> {
        self.consts.get(id.index())
    }

    /// Every block, in creation order.
    pub fn blocks(&self) -> impl Iterator<Item = (BlockId, &BlockData)> {
        self.blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (BlockId(i as u32), b))
    }

    fn push_value(&mut self, repr: Repr, origin: ValueOrigin) -> ValueId {
        let id = ValueId(self.values.len() as u32);
        self.values.push(ValueData { repr, origin });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::consts::ScalarBits;

    fn empty() -> Function {
        Function::new(Signature::default())
    }

    fn zero() -> ConstDecl {
        ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(0),
        }
    }

    #[test]
    fn one_constant_declared_twice_is_one_row() {
        // A constant is immutable and identified by its content, so a second row
        // holding the same declaration differs from the first in nothing but its
        // number — and the numbers are what lowering walks and what the reader
        // reads. `bench/analytic.ts` carried 3 417 rows for 202 distinct values
        // before this held.
        let mut func = empty();
        let first = func.push_const(zero());
        let again = func.push_const(zero());

        assert_eq!(first, again, "the same declaration is the same handle");
        assert_eq!(
            func.consts.len(),
            1,
            "and the pool holds it once, not once per site that asked"
        );
    }

    #[test]
    fn two_constants_that_differ_in_representation_are_two_rows() {
        // The bits alone do not identify a constant. `Repr` is what says how
        // they are read, so merging on bits would hand a site an integer where
        // it declared a double — the same eight bytes meaning two things.
        let mut func = empty();
        let integer = func.push_const(zero());
        let floating = func.push_const(ConstDecl::Scalar {
            repr: Repr::F64,
            bits: ScalarBits(0),
        });

        assert_ne!(
            integer, floating,
            "identical bits under different representations are different constants"
        );
        assert_eq!(func.consts.len(), 2);
    }

    #[test]
    fn the_pool_stays_in_declaration_order() {
        // Rule 13: anything a person compares between builds is ordered
        // deterministically. The memo is a hash map and the pool is not — this
        // is the assertion that says which of the two `ConstId` indexes.
        let mut func = empty();
        let first = func.push_const(zero());
        let second = func.push_const(ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(7),
        });
        let first_again = func.push_const(zero());

        assert_eq!(first, ConstId(0));
        assert_eq!(second, ConstId(1));
        assert_eq!(first_again, ConstId(0), "a repeat does not move anything");
        assert_eq!(
            func.constant(ConstId(1)),
            Some(&ConstDecl::Scalar {
                repr: Repr::I64,
                bits: ScalarBits(7)
            })
        );
    }
}
