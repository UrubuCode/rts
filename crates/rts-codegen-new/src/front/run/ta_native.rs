//! Native inline element access for level-B typed-array VIEWS (step 5b).
//!
//! When a `const a = new Uint8Array(ab)` local is proven a
//! [`HeapShape::TypedArrayView`](super::lower::HeapShape), `a[i]` and `a[i] = v`
//! lower here to raw memory ops against the buffer's base pointer instead of the
//! locked runtime call the dynamic path takes.
//!
//! The base pointer + element count come from ONE call to
//! `__rtsadp_ta_view_base_len` (which resolves the view and reads the buffer
//! under a single lock). The address is `base + (i << elem_log2)`, and the
//! load/store instruction is chosen from the element kind — all compile
//! constants. A bounds check (`idx <u count`, so a negative index is caught as a
//! huge unsigned) guards every access: an out-of-range READ yields `undefined`,
//! an out-of-range WRITE is a no-op, matching `taops::view_get`/`view_set`.
//!
//! GC safety: the base pointer is a derived interior pointer the conservative
//! scanner does not trace. The VIEW WORD stays live (it is the receiver value on
//! the stack), which keeps the buffer reachable across the access.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind, Value, types};

use super::lower::{Lowerer, Val};
use crate::front::error::FrontResult;
use crate::repr::Repr;
use crate::value::{self, PolyValue};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Emit the `(base, count)` resolution ONCE for a proven view local `name`
    /// and cache it in two `Variable`s. Called at the ctor site (step 5b
    /// hoisting), which dominates every access, so the later `use_var` reads are
    /// SSA-sound. A no-op if `name` is already cached.
    pub(super) fn cache_ta_view_base_len(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        name: &str,
        view_word: Value,
    ) -> FrontResult<()> {
        // Always OVERWRITE: a re-declaration (`const a` in a sibling scope, or a
        // fresh view bound to a reused name) must not serve a stale prior base.
        let (base, count) = self.ta_base_len(module, view_word)?;
        let base_var = self.builder.declare_var(types::I64);
        let count_var = self.builder.declare_var(types::I64);
        self.builder.def_var(base_var, base);
        self.builder.def_var(count_var, count);
        self.ta_view_base_len
            .insert(name.to_string(), (base_var, count_var));
        Ok(())
    }

    /// The cached `(base, count)` for a proven view local, or freshly resolved
    /// (uncached) for an anonymous view expression. When `local` is `Some` and
    /// cached, this is a pair of `use_var`s — no runtime call.
    fn ta_view_base_count(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        local: Option<&str>,
        view_word: Value,
    ) -> FrontResult<(Value, Value)> {
        if let Some(name) = local {
            if let Some(&(base_var, count_var)) = self.ta_view_base_len.get(name) {
                return Ok((self.builder.use_var(base_var), self.builder.use_var(count_var)));
            }
        }
        self.ta_base_len(module, view_word)
    }

    /// Resolve a view word to `(base_ptr, elem_count)` via the runtime helper,
    /// through two stack slots the emitted code owns. Both are i64 IR values.
    fn ta_base_len(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        view_word: Value,
    ) -> FrontResult<(Value, Value)> {
        let slot_base = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        let slot_count = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        let base_addr = self.builder.ins().stack_addr(types::I64, slot_base, 0);
        let count_addr = self.builder.ins().stack_addr(types::I64, slot_count, 0);
        // Return value (elem_bytes) is unused here — the kind is a compile
        // constant carried by the shape.
        self.call_runtime(
            module,
            "__rtsadp_ta_view_base_len",
            &[view_word, base_addr, count_addr],
        )?;
        let base = self
            .builder
            .ins()
            .load(types::I64, MemFlags::trusted(), base_addr, 0);
        let count = self
            .builder
            .ins()
            .load(types::I64, MemFlags::trusted(), count_addr, 0);
        Ok((base, count))
    }

    /// `addr = base + (idx << elem_log2)`.
    fn ta_addr(&mut self, base: Value, idx: Value, elem_log2: u8) -> Value {
        let off = if elem_log2 == 0 {
            idx
        } else {
            self.builder.ins().ishl_imm(idx, elem_log2 as i64)
        };
        self.builder.ins().iadd(base, off)
    }

    /// Inline read of `view[idx]` — returns the element boxed as a PolyValue
    /// number word, or `undefined` when out of range. `idx` is a proven i64.
    pub(super) fn emit_ta_view_get(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        local: Option<&str>,
        view_word: Value,
        idx: Value,
        elem_log2: u8,
        signed: bool,
        float: bool,
    ) -> FrontResult<Val> {
        let (base, count) = self.ta_view_base_count(module, local, view_word)?;
        // Unsigned compare: a negative idx becomes a huge unsigned and fails,
        // which is the JS `a[-1] === undefined` behaviour.
        let in_bounds = self.builder.ins().icmp(IntCC::UnsignedLessThan, idx, count);

        let load_blk = self.builder.create_block();
        let oob_blk = self.builder.create_block();
        let join_blk = self.builder.create_block();
        self.builder.append_block_param(join_blk, types::I64);
        self.builder
            .ins()
            .brif(in_bounds, load_blk, &[], oob_blk, &[]);

        // In-bounds: load by kind, convert to f64, box.
        self.builder.switch_to_block(load_blk);
        self.builder.seal_block(load_blk);
        let addr = self.ta_addr(base, idx, elem_log2);
        let f = MemFlags::trusted();
        let word = if float {
            let d = if elem_log2 == 2 {
                let f32v = self.builder.ins().load(types::F32, f, addr, 0);
                self.builder.ins().fpromote(types::F64, f32v)
            } else {
                self.builder.ins().load(types::F64, f, addr, 0)
            };
            value::emit_box_double(self.builder, d)
        } else {
            let raw = match (elem_log2, signed) {
                (0, false) => self.builder.ins().uload8(types::I64, f, addr, 0),
                (0, true) => self.builder.ins().sload8(types::I64, f, addr, 0),
                (1, false) => self.builder.ins().uload16(types::I64, f, addr, 0),
                (1, true) => self.builder.ins().sload16(types::I64, f, addr, 0),
                (2, false) => self.builder.ins().uload32(f, addr, 0),
                (2, true) => self.builder.ins().sload32(f, addr, 0),
                _ => self.builder.ins().load(types::I64, f, addr, 0),
            };
            // Every integer typed-array element is a JS number → f64. The loaded
            // i64 is already sign/zero-extended per the op above, so a signed
            // convert is correct (u32's max fits positive in i64).
            let d = self.builder.ins().fcvt_from_sint(types::F64, raw);
            value::emit_box_double(self.builder, d)
        };
        self.builder.ins().jump(join_blk, &[word.into()]);

        // Out of range → undefined.
        self.builder.switch_to_block(oob_blk);
        self.builder.seal_block(oob_blk);
        let undef = self
            .builder
            .ins()
            .iconst(types::I64, PolyValue::undefined().raw() as i64);
        self.builder.ins().jump(join_blk, &[undef.into()]);

        self.builder.switch_to_block(join_blk);
        self.builder.seal_block(join_blk);
        let res = self.builder.block_params(join_blk)[0];
        Ok(Val::new(res, Repr::Tagged))
    }

    /// Inline write of `view[idx] = value_word` — bounds-checked store, no-op out
    /// of range. `idx` is a proven i64; `value_word` is a PolyValue that is
    /// coerced ToNumber then narrowed to the element width.
    pub(super) fn emit_ta_view_set(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        local: Option<&str>,
        view_word: Value,
        idx: Value,
        value: Val,
        elem_log2: u8,
        float: bool,
    ) -> FrontResult<()> {
        // ToNumber the value ONCE, before the store (matches `view_set`).
        let d = self.coerce(value, Repr::Float64)?;

        let (base, count) = self.ta_view_base_count(module, local, view_word)?;
        let in_bounds = self.builder.ins().icmp(IntCC::UnsignedLessThan, idx, count);

        let store_blk = self.builder.create_block();
        let done_blk = self.builder.create_block();
        self.builder
            .ins()
            .brif(in_bounds, store_blk, &[], done_blk, &[]);

        self.builder.switch_to_block(store_blk);
        self.builder.seal_block(store_blk);
        let addr = self.ta_addr(base, idx, elem_log2);
        let f = MemFlags::trusted();
        if float {
            if elem_log2 == 2 {
                let f32v = self.builder.ins().fdemote(types::F32, d);
                self.builder.ins().store(f, f32v, addr, 0);
            } else {
                self.builder.ins().store(f, d, addr, 0);
            }
        } else {
            // ToInteger truncation toward zero, then the narrow store keeps the
            // low bytes = ToUintN low bits (matches `wrap` / the byte store).
            let i = self.builder.ins().fcvt_to_sint_sat(types::I64, d);
            match elem_log2 {
                0 => {
                    self.builder.ins().istore8(f, i, addr, 0);
                }
                1 => {
                    self.builder.ins().istore16(f, i, addr, 0);
                }
                2 => {
                    self.builder.ins().istore32(f, i, addr, 0);
                }
                _ => {
                    self.builder.ins().store(f, i, addr, 0);
                }
            }
        }
        self.builder.ins().jump(done_blk, &[]);

        self.builder.switch_to_block(done_blk);
        self.builder.seal_block(done_blk);
        Ok(())
    }
}
