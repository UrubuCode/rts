//! Lowering a function body.
//!
//! Blocks are created before anything is filled in, because a branch can name a
//! block that has not been built yet and a two-pass walk is simpler to be sure
//! of than an ordering rule.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{Block, InstBuilder, SourceLoc, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::error::{Capability, LowerError};
use super::memory;
use super::types::machine_type;
use super::value;
use crate::ir::{
    BitOp, BlockId, CmpOp, ConstDecl, Function, Inst, NumOp, ScalarBits, TrapCode, ValueId,
};
use crate::repr::Repr;
use crate::symbols::RtEntry;
use crate::target::{Declarations, FunctionRefs};
use cranelift_module::ModuleDeclarations;

/// What a function may name outside itself — all of it already decided.
///
/// # Why every field is a shared reference
///
/// Because lowering may not declare anything. Everything a body can name — its
/// callees, the seven entry points, its cache slots, the shapes its indirect
/// calls expect — is declared by a serial pre-pass in program order before any
/// function is lowered, and lowering only looks up. That is what makes a whole
/// program's bodies lowerable at once on a pool without any of them handing out
/// an identifier, which rule 13 (`rts-cranelift/README.md`) forbids.
///
/// The type carries the rule: there is no `&mut` here to declare *through*, so
/// "lowering declared something" is unrepresentable rather than merely
/// discouraged (rule 7).
pub struct Outside<'a> {
    /// What the module has been told about, in the module's own vocabulary.
    ///
    /// Immutable, and taken instead of the module itself — see
    /// [`crate::target::func_ref`] for why the module was not needed after all.
    pub machine: &'a ModuleDeclarations,
    /// Which of our identifiers the module has seen.
    pub declarations: &'a Declarations,
    /// The module's identifier for each runtime entry point.
    pub entries: &'a crate::symbols::EntryImports,
    /// Where each site in this function keeps what it last saw.
    ///
    /// Declared by whoever is compiling the function, because naming a piece of
    /// writable data needs a module and a name nothing else has taken — and the
    /// function knows how many sites it has but not what it is called.
    pub caches: &'a [cranelift_module::DataId],
}

/// Translates one function's body into the code generator's blocks.
pub struct Body<'a> {
    pub(super) func: &'a Function,
    pub(super) blocks: HashMap<BlockId, Block>,
    pub(super) values: HashMap<ValueId, Value>,
    /// What this function may name outside itself, and where to ask for it.
    ///
    /// Absent when lowering a function on its own. The two travel together
    /// because neither is any use without the other: knowing what has been
    /// declared is pointless without the module that declared it, and the module
    /// alone cannot say which of our identifiers means what.
    pub(super) outside: Option<Outside<'a>>,
    /// The heap this function may read and write.
    pub(super) heap: Option<super::Heap<'a>>,
    pub(super) refs: FunctionRefs,
}

impl<'a> Body<'a> {
    /// Lowers a function in the environment it is allowed to reach.
    /// Reports which runtime entry points the body ended up naming, because
    /// nothing else can: the entry points are all declared before lowering
    /// starts, so what a program reaches for is only knowable from what lowering
    /// asked for.
    pub fn lower_with(
        func: &'a Function,
        builder: &mut FunctionBuilder,
        environment: super::Environment<'a>,
    ) -> Result<crate::symbols::EntryTable, LowerError> {
        let mut body = Body {
            func,
            blocks: HashMap::new(),
            values: HashMap::new(),
            outside: environment.outside,
            heap: environment.heap,
            refs: FunctionRefs::new(),
        };
        body.create_blocks(builder);
        body.bind_entry_params(builder);

        // In an order where a definition is lowered before its uses, which is
        // reachability from the entry and NOT the order the blocks were created
        // in. Those two agree for anything a front end emits in one pass, and
        // they stop agreeing the moment a pass rewrites control flow: the frame
        // transform continues a split block in a block created after every block
        // the original body already had, so a use in one of those was lowered
        // before the definition it names and `self.values` had no entry for it.
        //
        // A block nothing reaches is lowered last rather than skipped. Dropping
        // it would make this pass decide what the program contains, which is a
        // decision no lowering gets to make; the code generator removes it.
        for id in reachable_first(func) {
            body.lower_block(builder, id)?;
        }
        builder.seal_all_blocks();
        Ok(body.refs.entries_used())
    }

    /// Creates every block with its parameters, before any is filled.
    fn create_blocks(&mut self, builder: &mut FunctionBuilder) {
        for (id, data) in self.func.blocks() {
            let block = builder.create_block();
            for &param in &data.params {
                let value = builder.append_block_param(block, machine_type(self.repr(param)));
                self.values.insert(param, value);
            }
            self.blocks.insert(id, block);
        }
    }

    /// Binds the entry block's parameters to the function's own.
    ///
    /// The entry block's parameters *are* the function's parameters — there is
    /// no second way to name them — so this is where the two become the same
    /// values rather than two lists that must agree.
    fn bind_entry_params(&mut self, builder: &mut FunctionBuilder) {
        let entry = self.blocks[&self.func.entry];
        builder.switch_to_block(entry);
    }

    fn lower_block(
        &mut self,
        builder: &mut FunctionBuilder,
        id: BlockId,
    ) -> Result<(), LowerError> {
        let data = self.func.block(id).expect("block belongs to this function");
        builder.switch_to_block(self.blocks[&id]);

        for &inst_id in &data.insts {
            let inst = self
                .func
                .inst(inst_id)
                .expect("instruction belongs to this function");

            // Said before the instruction is emitted, so that everything it
            // becomes carries it. This is the only moment the correspondence
            // between a program and its code exists; not recording it here is
            // recording it nowhere.
            builder.set_srcloc(SourceLoc::new(self.func.position_of(inst_id).raw()));
            self.lower_inst(builder, inst_id, &inst.inst, &inst.results)?;
        }
        // Back to saying nothing. Not the code generator's own default, which is
        // all ones and would read back as a position no client ever gave.
        builder.set_srcloc(SourceLoc::new(crate::fault::Position::UNKNOWN.raw()));

        match &data.terminator {
            Some(terminator) => self.lower_terminator(builder, id, terminator),
            None => Err(LowerError::UnterminatedBlock { block: id }),
        }
    }

    pub(super) fn lower_inst(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        inst: &Inst,
        results: &[ValueId],
    ) -> Result<(), LowerError> {
        let produced = match inst {
            Inst::Const(decl) => {
                let decl = self.func.constant(*decl).expect("constant belongs here");
                self.lower_const(builder, id, decl)?
            }

            Inst::IntArith(op, a, b) => {
                let (x, y) = (self.value(*a), self.value(*b));
                match op {
                    NumOp::Add => builder.ins().iadd(x, y),
                    NumOp::Sub => builder.ins().isub(x, y),
                    NumOp::Mul => builder.ins().imul(x, y),
                    NumOp::Div => builder.ins().sdiv(x, y),
                    // Total here only because the divisor was settled at
                    // construction — `fold::divisor_cannot_trap`. `srem` traps
                    // on `0` and on `INT_MIN % -1`, and nothing at this point
                    // can still see which value the divisor holds.
                    NumOp::Rem => builder.ins().srem(x, y),
                }
            }

            Inst::FloatArith(op, a, b) => {
                let (x, y) = (self.value(*a), self.value(*b));
                match op {
                    NumOp::Add => builder.ins().fadd(x, y),
                    NumOp::Sub => builder.ins().fsub(x, y),
                    NumOp::Mul => builder.ins().fmul(x, y),
                    NumOp::Div => builder.ins().fdiv(x, y),
                    // Reached only for a power-of-two divisor no smaller than
                    // one — the builder and the verifier both refuse anything
                    // else, and `fold::divisor_is_power_of_two` carries the
                    // proof that every step below is exact under that
                    // condition. NOT the general identity `NumOp` rejects.
                    //
                    //   q = x / d          exact: an exponent adjustment
                    //   t = trunc(q)       toward zero, as the language says
                    //   p = t * d          exact: |p| <= |x|, so no overflow
                    //   r = x - p          exact: |x - p| < |d|
                    //   fcopysign(r, x)    the ONE thing the algebra loses
                    //
                    // The last step is not a tidy-up. `-8 % 4` is `-0` in the
                    // language, and `x - p` answers `+0` for every dividend
                    // that is an exact multiple of the divisor — which is the
                    // common case for a mask, not a corner. It costs one
                    // instruction and is the difference between `-0` and `0`,
                    // observable through `1 / r`.
                    NumOp::Rem => {
                        // The third refusal, and it is not redundant with the
                        // builder's: a representation that never went through
                        // the builder reaches here, and the sequence below is
                        // exact ONLY for the divisors that predicate admits.
                        // Emitting it for any other is the approximation this
                        // crate refuses to make.
                        if !crate::ir::fold::divisor_is_power_of_two(self.func, *b) {
                            return Err(LowerError::RemainderNotInDomain {
                                inst: id,
                                found: self.func.repr_of(*a),
                            });
                        }
                        // `x * (1/d)` and not `x / d`. Exact for the same
                        // reason the whole sequence is — the reciprocal of a
                        // power of two is a power of two, so both are exponent
                        // adjustments and neither rounds — and `fdiv` costs
                        // roughly three times an `fmul` in latency on every
                        // target here, in the middle of a chain the loop
                        // carries. `divisor_is_power_of_two` refuses a divisor
                        // whose reciprocal is not normal, which is what makes
                        // reading it out here safe.
                        let reciprocal = self.reciprocal_of(builder, *b);
                        let q = builder.ins().fmul(x, reciprocal);
                        let t = builder.ins().trunc(q);
                        let p = builder.ins().fmul(t, y);
                        let r = builder.ins().fsub(x, p);
                        builder.ins().fcopysign(r, x)
                    }
                }
            }

            Inst::Bitwise(op, a, b) => {
                let (x, y) = (self.value(*a), self.value(*b));
                match op {
                    BitOp::And => builder.ins().band(x, y),
                    BitOp::Or => builder.ins().bor(x, y),
                    BitOp::Xor => builder.ins().bxor(x, y),
                    BitOp::Shl => builder.ins().ishl(x, y),
                    BitOp::Shr => builder.ins().sshr(x, y),
                }
            }

            Inst::Compare(op, a, b) => {
                let repr = self.repr(*a);
                let (x, y) = (self.value(*a), self.value(*b));
                let raw = if repr.is_float() {
                    builder.ins().fcmp(float_cc(*op), x, y)
                } else {
                    builder.ins().icmp(int_cc(*op), x, y)
                };
                value::to_machine_bool(builder, raw)
            }

            Inst::Widen(v) => {
                let repr = self.repr(*v);
                let raw = self.value(*v);
                value::widen(builder, raw, repr)?
            }

            Inst::ToInt32(v) => {
                // Truncate, bring inside 2^32, convert, and let the reduce do
                // the modulo. Branch-free, and each step is here because the
                // language asks for it:
                //
                //   t  = trunc(x)              toward zero, per ToInteger
                //   r  = t - trunc(t * 2^-32) * 2^32    |r| < 2^32, integral
                //   i  = fcvt_to_sint_sat(i64, r)       exact: r is in range
                //   i32 = ireduce(i32, i)               the modulo, and the
                //                                       wrap into signed
                //
                // NaN and both infinities need no case of their own: trunc
                // keeps them, the subtract turns an infinity into NaN, and the
                // saturating conversion answers zero for NaN — which is the
                // language's answer for all three.
                //
                // # Why the scale down is a MULTIPLY where it was a divide
                //
                // Because the two are bit-identical here and `divsd` is the
                // longest-latency instruction on this chain — ~13-14 cycles
                // against ~4 — and this whole sequence is serial, so its cost
                // is latency rather than throughput. Measured before the
                // change: `a = a | 0` carried on the loop's dependency chain
                // cost 11.6 ns against 2.98 ns for the same operator off it.
                //
                // Identical rather than merely close, and the reason is worth
                // stating because "replace a divide with a reciprocal multiply"
                // is normally exactly the transformation that is NOT sound.
                // Here it is: 2^-32 is a power of two and therefore exact, so
                // both spellings only adjust the exponent, and an exponent
                // adjustment rounds nothing UNLESS it reaches the subnormal
                // range. It cannot: `t` is `trunc(x)`, so `t` is an integer and
                // either zero or `|t| >= 1`, and `1 * 2^-32` is `2^-32` — 990
                // binades above where subnormals begin. Zero, both infinities
                // and NaN are unchanged by either spelling, signs included.
                let raw = self.value(*v);
                let truncated = builder.ins().trunc(raw);
                let scale = builder.ins().f64const(4294967296.0);
                // Written as the quotient rather than as `2.3283064365386963e-10`
                // so that being exactly 2^-32 is visible instead of trusted to a
                // decimal round trip. Both operands are exact powers of two, so
                // Rust folds this at compile time with no rounding.
                let reciprocal = builder.ins().f64const(1.0 / 4294967296.0);
                let quotient = builder.ins().fmul(truncated, reciprocal);
                let whole = builder.ins().trunc(quotient);
                let carried = builder.ins().fmul(whole, scale);
                let inside = builder.ins().fsub(truncated, carried);
                let wide = builder.ins().fcvt_to_sint_sat(types::I64, inside);
                builder.ins().ireduce(types::I32, wide)
            }

            Inst::FloatUnary(op, v) => {
                let raw = self.value(*v);
                match op {
                    crate::ir::FloatOp::Sqrt => builder.ins().sqrt(raw),
                    crate::ir::FloatOp::Floor => builder.ins().floor(raw),
                    crate::ir::FloatOp::Ceil => builder.ins().ceil(raw),
                    crate::ir::FloatOp::Trunc => builder.ins().trunc(raw),
                    crate::ir::FloatOp::Abs => builder.ins().fabs(raw),
                    crate::ir::FloatOp::Neg => builder.ins().fneg(raw),
                }
            }

            Inst::ToF64(v) => {
                let raw = self.value(*v);
                builder.ins().fcvt_from_sint(types::F64, raw)
            }

            // One load, no arithmetic. `trusted` says the address is valid and
            // that nothing else aliases the word — the second half is what lets
            // the code generator keep the value in a register between two reads
            // with no call in between, which is the entire point of the
            // instruction. The client asserts both by having asked for the
            // address; see `Inst::WordLoad` for why this layer cannot check it.
            Inst::WordLoad { address } => {
                let raw = self.value(*address);
                let flags = cranelift_codegen::ir::MemFlags::trusted();
                builder.ins().load(types::I64, flags, raw, 0)
            }

            // The bound first, and UNSIGNED: a negative index is a very large
            // one under that comparison, so one test rejects both ends where a
            // signed one would need two.
            //
            // `trapnz` rather than a branch to a trapping block: the check has
            // no join, and a block would ask this layer to build a merge where
            // there is nothing to merge.
            Inst::ElementLoad {
                base,
                index,
                length,
            } => {
                let base = self.value(*base);
                let index = self.value(*index);
                let length = self.value(*length);
                let past = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::UnsignedGreaterThanOrEqual,
                    index,
                    length,
                );
                builder
                    .ins()
                    .trapnz(past, trap_code(crate::ir::TrapCode::OutOfBounds));
                // Zero-extended and not sign-extended: the test above already
                // established that the index is below a non-negative length, so
                // its high bits are zero — widening any other way would
                // disagree with the comparison that let it through.
                let wide = builder.ins().uextend(types::I64, index);
                let offset = builder.ins().imul_imm(wide, 8);
                let at = builder.ins().iadd(base, offset);
                let flags = cranelift_codegen::ir::MemFlags::trusted();
                builder.ins().load(types::I64, flags, at, 0)
            }

            Inst::Narrow(v, to) => {
                let raw = self.value(*v);
                value::narrow(builder, raw, *to)?
            }

            // One comparison against a constant word. A singleton has exactly
            // one encoding, so nothing is loaded and no NaN case arises: the
            // encoded quadrant is disjoint from every double by construction,
            // which is what `BOX_BASE` reserves it for.
            Inst::IsSingleton { value, singleton } => {
                let word = self.value(*value);
                let raw = builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    word,
                    singleton.word() as i64,
                );
                value::to_machine_bool(builder, raw)
            }

            Inst::Generic(..) => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                });
            }
            Inst::FieldLoad { object, ty, field } => {
                let heap = self.heap.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                })?;
                let offset = crate::mem::ObjectLayout::field_offset_of(*ty, heap.types, *field)
                    .ok_or(LowerError::NoSuchField { inst: id })?;
                let repr = heap
                    .types
                    .layout(*ty)
                    .field(*field as usize)
                    .map(|f| f.repr);
                let repr = repr.ok_or(LowerError::NoSuchField { inst: id })?;

                let reference = self.value(*object);
                let address = memory::address_of(builder, reference, heap);
                memory::field_load(builder, address, offset, repr)
            }

            Inst::FieldStore {
                object,
                ty,
                field,
                value,
            } => {
                let heap = self.heap.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                })?;
                let offset = crate::mem::ObjectLayout::field_offset_of(*ty, heap.types, *field)
                    .ok_or(LowerError::NoSuchField { inst: id })?;

                // `Region::Shared` is fixed here rather than read from the
                // object, because nothing in this crate assigns an object to a
                // region yet (see `emit_barrier`'s doc for the tracking issue).
                // Fixing it to `Shared` is what keeps this call equivalent to
                // the inline check it replaces: `barrier_for` only distinguishes
                // `Shared` from `Local` when the field is traced, so a traced
                // field still always yields `CrossRegion` and an untraced one
                // still always yields `None` — the same two outcomes the old
                // code produced, from one predicate instead of a second copy of
                // its field-lookup.
                let region = crate::ir::Region::Shared;
                let kind = crate::gc::barrier_for(inst, heap.types, region, heap.bases.regions());
                let barrier = kind != crate::gc::BarrierKind::None;

                let reference = self.value(*object);
                let written = self.value(*value);
                let address = memory::address_of(builder, reference, heap);
                memory::field_store(builder, address, offset, written);

                if barrier {
                    self.emit_barrier(builder, id, reference, written)?;
                }
                return Ok(());
            }

            // Allocation is the one memory operation that is not arithmetic: it
            // has to ask a heap for space. So it is the one that is a call.
            Inst::Alloc { ty, .. } => {
                let size = {
                    let heap = self.heap.as_ref().ok_or(LowerError::NotYetLowered {
                        inst: id,
                        needs: Capability::Memory,
                    })?;
                    crate::mem::ObjectLayout::size_of(*ty, heap.types)
                };
                let outside = self.outside.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                })?;

                let allocate =
                    self.refs
                        .entry(outside.machine, outside.entries, builder.func, RtEntry::Alloc);
                let size = builder.ins().iconst(types::I64, i64::from(size));
                let ty_id = builder.ins().iconst(types::I64, ty.index() as i64);
                let call = builder.ins().call(allocate, &[size, ty_id]);
                return self.bind_results(builder, call, results);
            }
            Inst::Call { callee, args } => {
                let outside = self.outside.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                })?;
                let reference = self
                    .refs
                    .callee(outside.machine, builder.func, outside.declarations, *callee)
                    .ok_or(LowerError::UndeclaredCallee { inst: id })?;
                let args = self.args(args);
                let call = builder.ins().call(reference, &args);
                return self.bind_results(builder, call, results);
            }

            // Needs the same machinery a call does — a reference to the callee
            // in this function — and for the same reason: the address is a
            // relocation the destination fills in, not a number known here.
            // That is why it is refused without `Capability::Calls` rather than
            // being treated as a constant.
            Inst::FuncAddr { callee } => {
                let outside = self.outside.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                })?;
                let reference = self
                    .refs
                    .callee(outside.machine, builder.func, outside.declarations, *callee)
                    .ok_or(LowerError::UndeclaredCallee { inst: id })?;
                // `I64` because the builder gave the value `Repr::I64`. The two
                // are one decision and disagreeing about it would produce a
                // half-width address on any target where they differed.
                builder.ins().func_addr(types::I64, reference)
            }

            Inst::CallIndirect { callee, sig, args } => {
                let outside = self.outside.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                })?;
                let shape = self
                    .refs
                    .shape(builder.func, outside.declarations, *sig)
                    .ok_or(LowerError::UndeclaredCallee { inst: id })?;
                let target = self.value(*callee);
                let args = self.args(args);
                let call = builder.ins().call_indirect(shape, target, &args);
                return self.bind_results(builder, call, results);
            }
            Inst::PromiseNew => {
                let call = self.call_entry(builder, id, RtEntry::PromiseNew, &[])?;
                return self.bind_results(builder, call, results);
            }

            Inst::PromiseSettle {
                promise,
                value,
                rejected,
            } => {
                let promise = self.value(*promise);
                let value = self.value(*value);
                let rejected = builder.ins().iconst(types::I64, i64::from(*rejected));
                self.call_entry(
                    builder,
                    id,
                    RtEntry::PromiseSettle,
                    &[promise, value, rejected],
                )?;
                return Ok(());
            }

            Inst::Await { promise } => {
                let promise = self.value(*promise);
                let call = self.call_entry(builder, id, RtEntry::PromiseAwait, &[promise])?;
                return self.bind_results(builder, call, results);
            }

            // A bare suspension has no promise, so nothing decides when it
            // resumes. Expressing that needs the frame transformation, which is
            // the one piece of the three that is not a call — see the module doc.
            Inst::Suspend => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Suspension,
                });
            }
        };

        if let Some(&result) = results.first() {
            self.values.insert(result, produced);
        }
        Ok(())
    }

    fn lower_const(
        &self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        decl: &ConstDecl,
    ) -> Result<Value, LowerError> {
        match decl {
            ConstDecl::Scalar {
                repr,
                bits: ScalarBits(bits),
            } => Ok(match repr {
                Repr::F32 => builder.ins().f32const(f32::from_bits(*bits as u32)),
                Repr::F64 => builder.ins().f64const(f64::from_bits(*bits)),
                other => builder.ins().iconst(machine_type(*other), *bits as i64),
            }),
            // Everything else lives in a data section, which needs a module.
            _ => Err(LowerError::NotYetLowered {
                inst: id,
                needs: Capability::Memory,
            }),
        }
    }

    /// Emits a property read that remembers the last layout it saw.
    ///
    /// The shape of it is one comparison, one load, and a call that happens only
    /// when the comparison fails. Two details are worth stating because both
    /// looked optional and are not.
    ///
    /// The resolver refills the cell, so the load after it reads the answer that
    /// call just wrote. That is why there is one load reached from two paths
    /// rather than one on each: two would be two places for the addressing to be
    /// written, and the second one is the one that gets it wrong.
    ///
    /// The cell starts holding a layout no object has. It cannot start at zero:
    /// zero is a real layout, and a site that had never run would claim to
    /// recognize the first one ever declared.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_cached_get(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        object: ValueId,
        key: crate::shape::Key,
        cache: crate::ir::CacheId,
        hit: &crate::ir::BlockCall,
        miss: &crate::ir::BlockCall,
    ) -> Result<(), LowerError> {
        let heap = self.heap.ok_or(LowerError::TerminatorNotYetLowered {
            block,
            needs: Capability::Memory,
        })?;

        let reference = self.value(object);
        let address = memory::address_of(builder, reference, &heap);
        let header = memory::field_load(
            builder,
            address,
            crate::mem::HeaderLayout::TYPE_OFFSET,
            Repr::I64,
        );

        let cell = self.cache_address(builder, block, cache)?;
        let remembered = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            0,
        );
        let recognized = builder.ins().icmp(IntCC::Equal, header, remembered);

        let read = builder.create_block();
        let ask = builder.create_block();
        builder.ins().brif(recognized, read, &[], ask, &[]);

        // Not recognized: ask once, and the answer is written where the load
        // below will find it.
        builder.switch_to_block(ask);
        let key_value = builder.ins().iconst(types::I64, i64::from(key.0));
        let resolved = self.call_entry_at(
            builder,
            block,
            RtEntry::CacheResolve,
            &[reference, key_value, cell],
        )?;
        let answer = builder.inst_results(resolved)[0];
        let found = builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThanOrEqual, answer, 0);
        let miss_args = self.block_args(&miss.args);
        let miss_target = self.blocks[&miss.block];
        builder
            .ins()
            .brif(found, read, &[], miss_target, &miss_args);

        // Where the property is, read once, whichever path arrived here.
        // Where the property is, read once, whichever path arrived here — and
        // from WHICH base, which is the third word.
        //
        // Zero means the cell asked about, which is every ordinary read. A byte
        // offset means the answer is in the object's overflow, and that offset
        // is where the object keeps its address: load it, and the same
        // `base + offset` finishes the read. One load and one branch, on a path
        // that was already two loads.
        //
        // The alternative was `CachedGetIndirect`, and it cannot serve this:
        // that form validates a remembered ADDRESS by the header type it
        // carried, and every overflow block carries one type, so two objects of
        // one shape would share the site and the second would read the first's
        // overflow. An overflow is per object, so its address has to come from
        // the object.
        builder.switch_to_block(read);
        let offset = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            8,
        );
        let indirect = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            16,
        );
        let direct = builder.create_block();
        let through = builder.create_block();
        let based = builder.create_block();
        let base = builder.append_block_param(based, types::I64);
        builder
            .ins()
            .brif(indirect, through, &[], direct, &[]);

        builder.switch_to_block(direct);
        builder
            .ins()
            .jump(based, &[cranelift_codegen::ir::BlockArg::Value(address)]);

        builder.switch_to_block(through);
        let holder = builder.ins().iadd(address, indirect);
        let elsewhere = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            holder,
            0,
        );
        builder
            .ins()
            .jump(based, &[cranelift_codegen::ir::BlockArg::Value(elsewhere)]);

        builder.switch_to_block(based);
        let at = builder.ins().iadd(base, offset);
        let value = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            at,
            0,
        );

        let mut hit_args = vec![cranelift_codegen::ir::BlockArg::Value(value)];
        hit_args.extend(self.block_args(&hit.args));
        let hit_target = self.blocks[&hit.block];
        builder.ins().jump(hit_target, &hit_args);
        Ok(())
    }

    /// A cached read whose answer may live in a cell other than the one asked
    /// about.
    ///
    /// # The shape, and why the second guard is not on the fast path
    ///
    /// ```text
    ///   recognise the object ── no ──────────────────────────┐
    ///            │                                           │
    ///   is a second cell remembered? ── no ── read(object)   │
    ///            │ yes                          │            │
    ///   recognise that cell ── no ──────────────┼────────────┤
    ///            │                              │            │
    ///       read(that cell)                     │          ask once
    ///            │                              │            │
    ///            └──────────► load at the remembered offset ◄┘
    /// ```
    ///
    /// The obvious encoding is branchless: keep a mask word, compute
    /// `(address & mask) | remembered`, and test both layouts with one `band` of
    /// two comparisons. It was rejected on two counts. A fifth word makes the
    /// cell forty bytes, so four of every eight straddle a cache line, and the
    /// site pays that on every read including the ones that never use it. And a
    /// `band` of two comparisons materialises both with `setcc` rather than
    /// folding into two conditional jumps — there is no instruction scheduler at
    /// this optimisation level, so what is emitted is what runs.
    ///
    /// Behind a branch instead, a site whose answer is in the object it asked
    /// about — which the client says is the majority of them — pays one load and
    /// one branch that is monomorphic per site and therefore predicted.
    ///
    /// # Why the resolved path re-reads rather than reusing a value
    ///
    /// The call may have written a second cell where there was none. Recomputing
    /// the base from the cell afterwards is one load; threading it out of the
    /// call would mean the resolver reporting two things through one integer.
    ///
    /// # What was read from the code generator, and what was not
    ///
    /// That `brif` lowers to a compare and a conditional jump, and that `band`
    /// of two comparisons does not, is reported rather than verified here: it
    /// comes from the lowering rules rather than from a disassembly this crate
    /// produced. Rule 4 — say which. The block shape is correct either way; only
    /// the instruction count claim depends on it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_cached_get_indirect(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        object: ValueId,
        key: crate::shape::Key,
        cache: crate::ir::CacheId,
        hit: &crate::ir::BlockCall,
        miss: &crate::ir::BlockCall,
    ) -> Result<(), LowerError> {
        let heap = self.heap.ok_or(LowerError::TerminatorNotYetLowered {
            block,
            needs: Capability::Memory,
        })?;

        let flags = cranelift_codegen::ir::MemFlags::trusted();
        let reference = self.value(object);
        let address = memory::address_of(builder, reference, &heap);
        let header = memory::field_load(
            builder,
            address,
            crate::mem::HeaderLayout::TYPE_OFFSET,
            Repr::I64,
        );
        let cell = self.cache_address(builder, block, cache)?;

        // Every block below takes the base to read from as a parameter, so the
        // load happens once whichever path reached it — the same reason
        // `lower_cached_get` has one `read` block rather than one per path.
        let held_known = builder.create_block();
        let check_held = builder.create_block();
        let check_between = builder.create_block();
        let check_middle = builder.create_block();
        let ask = builder.create_block();
        let resolved = builder.create_block();
        let read = builder.create_block();
        let base = builder.append_block_param(read, types::I64);

        let remembered = builder.ins().load(types::I64, flags, cell, 0);
        let recognized = builder.ins().icmp(IntCC::Equal, header, remembered);
        builder.ins().brif(recognized, held_known, &[], ask, &[]);

        // The object is the layout this site remembers. Is the answer in it?
        builder.switch_to_block(held_known);
        let held = builder.ins().load(types::I64, flags, cell, 16);
        builder.ins().brif(
            held,
            check_held,
            &[],
            read,
            &[cranelift_codegen::ir::BlockArg::Value(address)],
        );

        // A second cell was remembered. It is an address rather than a
        // reference, so the only thing that can be checked about it is the
        // layout it carried when this site warmed — which is what the resolver
        // promised to make safe to load from.
        builder.switch_to_block(check_held);
        let held_header = builder.ins().load(
            types::I64,
            flags,
            held,
            i32::from(crate::mem::HeaderLayout::TYPE_OFFSET),
        );
        let held_remembered = builder.ins().load(types::I64, flags, cell, 24);
        let still = builder
            .ins()
            .icmp(IntCC::Equal, held_header, held_remembered);
        builder.ins().brif(still, check_between, &[], ask, &[]);

        // A cell BETWEEN the two, when the answer was two steps away. Its layout
        // has to be recognised as well, because the step from it to the holder
        // is a fact about IT: change what it reaches and the holder this site
        // remembers is no longer what the receiver would find, while both of the
        // comparisons above still succeed.
        //
        // Behind a test for its existence, so a site whose answer was one step
        // away pays a load and a predicted branch rather than a comparison
        // against a cell that is not there.
        builder.switch_to_block(check_between);
        let between = builder.ins().load(types::I64, flags, cell, 32);
        builder.ins().brif(
            between,
            check_middle,
            &[],
            read,
            &[cranelift_codegen::ir::BlockArg::Value(held)],
        );

        builder.switch_to_block(check_middle);
        let middle_header = builder.ins().load(
            types::I64,
            flags,
            between,
            i32::from(crate::mem::HeaderLayout::TYPE_OFFSET),
        );
        let middle_remembered = builder.ins().load(types::I64, flags, cell, 40);
        let unchanged = builder
            .ins()
            .icmp(IntCC::Equal, middle_header, middle_remembered);
        builder.ins().brif(
            unchanged,
            read,
            &[cranelift_codegen::ir::BlockArg::Value(held)],
            ask,
            &[],
        );

        builder.switch_to_block(ask);
        let key_value = builder.ins().iconst(types::I64, i64::from(key.0));
        let asked = self.call_entry_at(
            builder,
            block,
            RtEntry::CacheResolveIndirect,
            &[reference, key_value, cell],
        )?;
        let answer = builder.inst_results(asked)[0];
        let found = builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThanOrEqual, answer, 0);
        let miss_args = self.block_args(&miss.args);
        let miss_target = self.blocks[&miss.block];
        builder
            .ins()
            .brif(found, resolved, &[], miss_target, &miss_args);

        // The call may have written a second cell where there was none, so the
        // base is read back rather than assumed.
        builder.switch_to_block(resolved);
        let now_held = builder.ins().load(types::I64, flags, cell, 16);
        builder.ins().brif(
            now_held,
            read,
            &[cranelift_codegen::ir::BlockArg::Value(now_held)],
            read,
            &[cranelift_codegen::ir::BlockArg::Value(address)],
        );

        builder.switch_to_block(read);
        let offset = builder.ins().load(types::I64, flags, cell, 8);
        let at = builder.ins().iadd(base, offset);
        let value = builder.ins().load(types::I64, flags, at, 0);

        let mut hit_args = vec![cranelift_codegen::ir::BlockArg::Value(value)];
        hit_args.extend(self.block_args(&hit.args));
        let hit_target = self.blocks[&hit.block];
        builder.ins().jump(hit_target, &hit_args);
        Ok(())
    }

    /// A cached store: recognise the layout, or ask once, then write.
    ///
    /// The mirror of `lower_cached_get`, and it differs in three things worth
    /// naming rather than leaving to a reader to diff.
    ///
    /// **There is no value to hand on.** A store produces nothing, so the hit
    /// path takes no extra argument and the two paths meet without one.
    ///
    /// **The barrier.** A store of a reference the collector has not seen is the
    /// failure `emit_barrier` exists to prevent, and the reason it is emitted
    /// unconditionally is stated there: a barrier that was not needed costs a
    /// call, and one that was needed and skipped is a use-after-free that
    /// reproduces rarely and explains nothing.
    ///
    /// **The miss path is not a slower store.** A key the layout does not have
    /// is a shape transition, and a transition is not something a site can
    /// remember — the next object through it may be at a different layout
    /// entirely. The resolver answers that it cannot be reached this way, and
    /// what to do about it is the client's.
    ///
    /// **A different resolver.** [`RtEntry::CacheResolveStore`], not
    /// `CacheResolve`: the two answer the same question except when the runtime
    /// refuses the store outright, and which of them is asking is known here
    /// rather than passed as a flag. Its own documentation carries the reason.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_cached_set(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        object: ValueId,
        key: crate::shape::Key,
        cache: crate::ir::CacheId,
        value: ValueId,
        hit: &crate::ir::BlockCall,
        miss: &crate::ir::BlockCall,
    ) -> Result<(), LowerError> {
        let heap = self.heap.ok_or(LowerError::TerminatorNotYetLowered {
            block,
            needs: Capability::Memory,
        })?;

        let reference = self.value(object);
        let written = self.value(value);
        let address = memory::address_of(builder, reference, &heap);
        let header = memory::field_load(
            builder,
            address,
            crate::mem::HeaderLayout::TYPE_OFFSET,
            Repr::I64,
        );
        let cell = self.cache_address(builder, block, cache)?;
        let remembered = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            0,
        );
        let recognized = builder.ins().icmp(IntCC::Equal, header, remembered);

        let write = builder.create_block();
        let ask = builder.create_block();
        builder.ins().brif(recognized, write, &[], ask, &[]);

        builder.switch_to_block(ask);
        let key_value = builder.ins().iconst(types::I64, i64::from(key.0));
        let resolved = self.call_entry_at(
            builder,
            block,
            RtEntry::CacheResolveStore,
            &[reference, key_value, cell],
        )?;
        let answer = builder.inst_results(resolved)[0];
        let found = builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThanOrEqual, answer, 0);
        let miss_args = self.block_args(&miss.args);
        let miss_target = self.blocks[&miss.block];
        builder
            .ins()
            .brif(found, write, &[], miss_target, &miss_args);

        builder.switch_to_block(write);
        let offset = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            8,
        );
        let at = builder.ins().iadd(address, offset);
        builder
            .ins()
            .store(cranelift_codegen::ir::MemFlags::trusted(), written, at, 0);

        // The third word: the header this object must CARRY once the store has
        // happened, or zero when the store changes nothing about what it is.
        //
        // A write that ADDS a property cannot be cached any other way. The
        // layout the site has to recognise is the one from BEFORE the write and
        // the offset it has to answer only exists after, so remembering one of
        // them is remembering half the operation. Remembering the TRANSITION —
        // this header plus this key give that header and that offset — is what
        // makes a fresh object of the same starting shape hit rather than
        // resolve, which is every object built in a loop.
        //
        // Written unconditionally through a select rather than behind a branch:
        // the header was already loaded for the comparison above, so the
        // ordinary write pays one store of the value it already held.
        let grown = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            16,
        );
        let changes = builder.ins().icmp_imm(IntCC::NotEqual, grown, 0);
        let carried = builder.ins().select(changes, grown, header);
        memory::field_store(
            builder,
            address,
            crate::mem::HeaderLayout::TYPE_OFFSET,
            carried,
        );
        if self.regions_can_cross() {
            self.emit_barrier_at(builder, block, reference, written)?;
        }

        let hit_args = self.block_args(&hit.args);
        let hit_target = self.blocks[&hit.block];
        builder.ins().jump(hit_target, &hit_args);
        Ok(())
    }

    /// A barrier from a terminator rather than from an instruction.
    ///
    /// `emit_barrier` names an `InstId` in its error, and a terminator has none.
    /// The work is the same call; only what it can blame differs.
    fn emit_barrier_at(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        object: Value,
        written: Value,
    ) -> Result<(), LowerError> {
        let outside = self
            .outside
            .as_ref()
            .ok_or(LowerError::TerminatorNotYetLowered {
                block,
                needs: Capability::Memory,
            })?;
        let barrier = self.refs.entry(
            outside.machine,
            outside.entries,
            builder.func,
            RtEntry::WriteBarrier,
        );
        builder.ins().call(barrier, &[object, written]);
        Ok(())
    }

    /// The address of where a site keeps what it last saw.

    fn cache_address(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        cache: crate::ir::CacheId,
    ) -> Result<Value, LowerError> {
        let outside = self
            .outside
            .as_ref()
            .ok_or(LowerError::TerminatorNotYetLowered {
                block,
                needs: Capability::Memory,
            })?;
        let data = *outside
            .caches
            .get(cache.index())
            .ok_or(LowerError::UndeclaredTailCallee { block })?;

        let value = crate::target::data_ref(outside.machine, builder.func, data);
        Ok(builder.ins().global_value(types::I64, value))
    }

    /// Calls a runtime entry point from a terminator.
    fn call_entry_at(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        entry: RtEntry,
        args: &[Value],
    ) -> Result<cranelift_codegen::ir::Inst, LowerError> {
        let outside = self
            .outside
            .as_ref()
            .ok_or(LowerError::TerminatorNotYetLowered {
                block,
                needs: Capability::Calls,
            })?;
        let reference = self
            .refs
            .entry(outside.machine, outside.entries, builder.func, entry);
        Ok(builder.ins().call(reference, args))
    }

    /// Calls a runtime entry point.
    ///
    /// One place, so that declaring an entry point on first use and referring to
    /// it afterwards is a fact about the layer rather than a discipline each
    /// emission site follows.
    fn call_entry(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        entry: RtEntry,
        args: &[Value],
    ) -> Result<cranelift_codegen::ir::Inst, LowerError> {
        let outside = self.outside.as_ref().ok_or(LowerError::NotYetLowered {
            inst: id,
            needs: Capability::Calls,
        })?;
        let reference = self
            .refs
            .entry(outside.machine, outside.entries, builder.func, entry);
        Ok(builder.ins().call(reference, args))
    }

    /// Whether a store in this heap can make one region point at another.
    ///
    /// Forwards to [`crate::gc::crossing_is_possible`], which owns the rule and
    /// says why one region cannot point at another when there is no other. This
    /// is a lookup of the region count and nothing else — deciding here would be
    /// the flag `gc::barrier` refuses to have.
    ///
    /// Answers `true` when there is no heap at all, which is the conservative
    /// direction and also unreachable: a store that got this far has already
    /// resolved [`Capability::Memory`].
    fn regions_can_cross(&self) -> bool {
        match self.heap.as_ref() {
            Some(heap) => crate::gc::crossing_is_possible(heap.bases.regions()),
            None => true,
        }
    }

    /// Emits the barrier a reference store owes the collector.
    ///
    /// For every store into a traced field, in a heap where a crossing is
    /// possible at all — [`crate::gc::crossing_is_possible`] is the one case
    /// that is elided and why that case is arithmetic rather than a judgement.
    ///
    /// Otherwise unconditional. That is more than strictly necessary — a store
    /// into an object no other thread can reach makes nothing visible anywhere
    /// new, and could skip it — but deciding that needs to know the object's
    /// region, and a reference does not carry one. The ownership annotation that
    /// would answer it does not exist yet.
    ///
    /// So the direction of the error is chosen deliberately. A barrier that was
    /// not needed costs a call. A barrier that was needed and skipped produces a
    /// reference the collector never learns about, which becomes a use-after-free
    /// that reproduces rarely and explains nothing. The first is a bill; the
    /// second is a bug nobody can find.
    fn emit_barrier(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        object: Value,
        written: Value,
    ) -> Result<(), LowerError> {
        let outside = self.outside.as_ref().ok_or(LowerError::NotYetLowered {
            inst: id,
            needs: Capability::Memory,
        })?;
        let barrier = self.refs.entry(
            outside.machine,
            outside.entries,
            builder.func,
            RtEntry::WriteBarrier,
        );
        builder.ins().call(barrier, &[object, written]);
        Ok(())
    }

    /// Binds a call's results to the values the instruction defines.
    /// Binds what a call produced to the values this layer names.
    ///
    /// # Why a result may not be the value
    ///
    /// Because a foreign signature tells the truth about the callee, and the
    /// truth is narrower than a word for a boolean: the C convention defines
    /// only the low byte of the return register. `machine_signature` declares
    /// that, so what comes back is an `I8` where this layer's `Repr::Bool` is a
    /// word — and the widening happens here, once, rather than at every
    /// instruction that later consumes a boolean.
    ///
    /// Zero-extended and not sign-extended: a C boolean is 0 or 1, and a sign
    /// extension would turn a true into every bit set — which compares equal to
    /// nothing and would be a second wrong answer replacing the first.
    ///
    /// Written as a comparison of types rather than a test for `Repr::Bool`, so
    /// that any future narrowing at the boundary is widened by the same line
    /// instead of by a second one somebody has to remember to add.
    fn bind_results(
        &mut self,
        builder: &mut FunctionBuilder,
        call: cranelift_codegen::ir::Inst,
        results: &[ValueId],
    ) -> Result<(), LowerError> {
        let produced: Vec<cranelift_codegen::ir::Value> = builder.inst_results(call).to_vec();
        for (&ours, theirs) in results.iter().zip(produced) {
            let wanted = machine_type(self.repr(ours));
            let given = builder.func.dfg.value_type(theirs);
            let value = if given == wanted {
                theirs
            } else {
                builder.ins().uextend(wanted, theirs)
            };
            self.values.insert(ours, value);
        }
        Ok(())
    }

    pub(super) fn value(&self, id: ValueId) -> Value {
        self.values[&id]
    }

    pub(super) fn args(&self, ids: &[ValueId]) -> Vec<Value> {
        ids.iter().map(|&id| self.value(id)).collect()
    }

    /// Branch arguments, in the form a branch takes them.
    ///
    /// The code generator distinguishes a value passed along an edge from a
    /// value used in place, because an edge can also carry things a value cannot
    /// be. Everything this layer passes is an ordinary value, so the conversion
    /// is total — but it is written once here rather than at each branch.
    pub(super) fn block_args(&self, ids: &[ValueId]) -> Vec<cranelift_codegen::ir::BlockArg> {
        ids.iter()
            .map(|&id| cranelift_codegen::ir::BlockArg::Value(self.value(id)))
            .collect()
    }

    pub(super) fn repr(&self, id: ValueId) -> Repr {
        self.func.repr_of(id)
    }

    /// `1 / d` as a materialised constant, for a divisor already settled.
    ///
    /// The number comes from [`crate::ir::fold::power_of_two_reciprocal`],
    /// which is the same function that decided the divisor qualifies — asking
    /// somewhere else would be a second answer to the question that makes the
    /// sequence correct.
    ///
    /// # Panics
    ///
    /// If the divisor is not one that function admits. Not a defensive check:
    /// reaching here means the builder, the verifier and lowering's own guard
    /// all admitted it, so a mismatch is this crate disagreeing with itself.
    fn reciprocal_of(&self, builder: &mut FunctionBuilder, divisor: ValueId) -> Value {
        let reciprocal = crate::ir::fold::power_of_two_reciprocal(self.func, divisor)
            .expect("the divisor was settled to be a power of two with a normal reciprocal");
        builder.ins().f64const(reciprocal)
    }
}

/// The integer condition a comparison uses.
///
/// Signed throughout: the representations are signed integers, and choosing an
/// unsigned condition for a signed value is the kind of silent wrongness this
/// layer refuses elsewhere.
fn int_cc(op: CmpOp) -> IntCC {
    match op {
        CmpOp::Eq => IntCC::Equal,
        CmpOp::Ne => IntCC::NotEqual,
        CmpOp::Lt => IntCC::SignedLessThan,
        CmpOp::Le => IntCC::SignedLessThanOrEqual,
        CmpOp::Gt => IntCC::SignedGreaterThan,
        CmpOp::Ge => IntCC::SignedGreaterThanOrEqual,
    }
}

/// The floating-point condition a comparison uses.
///
/// Ordered, so that a comparison against a NaN is false rather than true — which
/// is what "less than" means when one side is not a number.
fn float_cc(op: CmpOp) -> FloatCC {
    match op {
        CmpOp::Eq => FloatCC::Equal,
        CmpOp::Ne => FloatCC::NotEqual,
        CmpOp::Lt => FloatCC::LessThan,
        CmpOp::Le => FloatCC::LessThanOrEqual,
        CmpOp::Gt => FloatCC::GreaterThan,
        CmpOp::Ge => FloatCC::GreaterThanOrEqual,
    }
}

/// The code generator's name for why a program stopped.
pub(super) fn trap_code(code: TrapCode) -> cranelift_codegen::ir::TrapCode {
    use cranelift_codegen::ir::TrapCode as Cl;
    match code {
        TrapCode::Unreachable => Cl::unwrap_user(1),
        TrapCode::OutOfBounds => Cl::HEAP_OUT_OF_BOUNDS,
        TrapCode::DivideByZero => Cl::INTEGER_DIVISION_BY_ZERO,
    }
}

/// Every block, reachable ones in an order where a definition precedes its uses.
///
/// Reverse post-order over the successor graph, which is what "a definition
/// dominates its uses" means as a traversal: a block is emitted only after every
/// path into it has been. Unreachable blocks follow, in the order they were
/// created, so that this answers EVERY block and the caller does not have to
/// decide what to do about the ones nothing jumps to.
fn reachable_first(func: &Function) -> Vec<BlockId> {
    let mut order = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // An explicit stack rather than recursion: a deeply nested body would be a
    // stack overflow in the compiler, which is the one failure a compiler must
    // not have.
    let mut stack = vec![(func.entry, false)];
    while let Some((id, expanded)) = stack.pop() {
        if expanded {
            order.push(id);
            continue;
        }
        if !seen.insert(id) {
            continue;
        }
        stack.push((id, true));
        if let Some(block) = func.block(id)
            && let Some(terminator) = &block.terminator
        {
            for successor in terminator.successors() {
                stack.push((successor, false));
            }
        }
    }
    order.reverse();
    order.extend(func.blocks().map(|(id, _)| id).filter(|id| !seen.contains(id)));
    order
}
