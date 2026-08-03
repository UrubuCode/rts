//! A function: the tables every handle indexes into.
//!
//! The function owns its values, blocks, instructions and constants. Handles are
//! indices into these tables and are meaningless against a different function —
//! the verifier rejects a program that mixes them rather than reading plausible
//! data from the wrong table.

use super::consts::ConstDecl;
use super::entity::{BlockId, ConstId, InstId, ValueId};
use super::inst::{BlockData, Inst, InstData, Terminator};
use crate::repr::Repr;
use crate::unwind::{RegionId, RegionTree};

/// What a function accepts and returns.
///
/// Multiple returns are expressible from the start. A signature model built
/// around a single return has to be rebuilt the day a client needs more, and
/// rebuilding an ABI is not a local change.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Signature {
    /// Parameter representations, in order.
    pub params: Vec<Repr>,
    /// Return representations, in order.
    pub returns: Vec<Repr>,
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
    block_regions: Vec<Option<RegionId>>,
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
            block_regions: Vec::new(),
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

    /// Appends an instruction to a block, binding its result when it has one.
    pub fn push_inst(&mut self, block: BlockId, inst: Inst, result: Option<Repr>) -> Option<ValueId> {
        let inst_id = InstId(self.insts.len() as u32);
        self.insts.push(InstData { inst, result: None });

        let value = result.map(|repr| self.push_value(repr, ValueOrigin::InstResult(inst_id)));
        self.insts[inst_id.index()].result = value;
        self.blocks[block.index()].insts.push(inst_id);
        value
    }

    /// Sets how control leaves a block.
    pub fn set_terminator(&mut self, block: BlockId, terminator: Terminator) {
        self.blocks[block.index()].terminator = Some(terminator);
    }

    /// Declares a constant and returns its handle.
    pub fn push_const(&mut self, decl: ConstDecl) -> ConstId {
        let id = ConstId(self.consts.len() as u32);
        self.consts.push(decl);
        id
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
        self.value(id).expect("value handle does not belong to this function").repr
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
        self.blocks.iter().enumerate().map(|(i, b)| (BlockId(i as u32), b))
    }

    /// How many blocks exist.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    fn push_value(&mut self, repr: Repr, origin: ValueOrigin) -> ValueId {
        let id = ValueId(self.values.len() as u32);
        self.values.push(ValueData { repr, origin });
        id
    }
}
