//! Mechanical translation MirFunc → Cranelift Function.
//!
//! For each block in the MIR we create a Cranelift `Block` with matching
//! block params, then walk every `Inst` and emit a 1:1 `builder.ins().*`
//! call, finally translating the `Terminator`. Block ids are preserved
//! (mir block N → cranelift block N) since both use dense u32-indexed
//! tables.
//!
//! The function is appended to a caller-supplied `Module`. Caller is
//! responsible for `define_function` / `finalize`.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types as cl, AbiParam, Block, InstBuilder, MemFlags, Signature, Value};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use rts_hir::ir::HirType;
use rts_mir::ir::*;

use super::hint_bridge::hir_to_cl;

/// Declare a `MirFunc`'s signature in `module` without lowering its body.
/// Used by the two-pass driver when several functions can call each other:
/// pass 1 declares all of them, pass 2 lowers each body with the resulting
/// `decls` map so `Inst::CallUser` resolves.
pub fn declare_mir_func(
    module: &mut dyn Module,
    mir: &MirFunc,
) -> Result<cranelift_module::FuncId> {
    let sig = build_signature(module, mir);
    module
        .declare_function(&mir.name, Linkage::Local, &sig)
        .map_err(|e| anyhow!("declare_function {}: {e}", mir.name))
}

/// Lowers a `MirFunc` into a defined function inside `module` with no
/// known external user-fn declarations. `Inst::CallUser` will fail to lower.
pub fn lower_mir_func(
    module: &mut dyn Module,
    mir: &MirFunc,
) -> Result<cranelift_module::FuncId> {
    let empty: HashMap<String, cranelift_module::FuncId> = HashMap::new();
    lower_mir_func_with_decls(module, mir, &empty)
}

/// Lowers a `MirFunc` with access to a name → FuncId map of other user
/// functions already declared in the module. Each `Inst::CallUser` resolves
/// its target through this map and imports the `FuncId` as a `FuncRef` for
/// the duration of the function being built.
pub fn lower_mir_func_with_decls(
    module: &mut dyn Module,
    mir: &MirFunc,
    decls: &HashMap<String, cranelift_module::FuncId>,
) -> Result<cranelift_module::FuncId> {
    let sig = build_signature(module, mir);
    // Reuse an existing FuncId when the caller (e.g. compile_program) has
    // already declared this fn with a mangled symbol name. Declaring a new
    // function with `mir.name` would split the JIT into two unrelated
    // symbols and the caller's calls go unresolved.
    let func_id = if let Some(&existing) = decls.get(&mir.name) {
        existing
    } else {
        module
            .declare_function(&mir.name, Linkage::Local, &sig)
            .map_err(|e| anyhow!("declare_function {}: {e}", mir.name))?
    };

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    // Pre-import every user-fn target referenced by `Inst::CallUser` AND
    // every extern symbol referenced by `Inst::CallExtern`. Also pre-allocate
    // a data segment per `Inst::StrLit` so the literal's pointer is available
    // as a `GlobalValue` once the FunctionBuilder is live.
    //
    // All pre-import work is done before the `FunctionBuilder` takes a
    // mutable borrow of `ctx.func` since we still need `module` access here.
    let mut func_ref_cache: HashMap<String, cranelift_codegen::ir::FuncRef> = HashMap::new();
    let mut strlit_cache: HashMap<usize, (cranelift_codegen::ir::GlobalValue, usize)> =
        HashMap::new();
    let mut strlit_inst_idx: usize = 0;
    for block in &mir.blocks {
        for inst in &block.insts {
            match inst {
                Inst::CallUser { name, .. } => {
                    if func_ref_cache.contains_key(name) {
                        continue;
                    }
                    let target_id = decls.get(name).copied().ok_or_else(|| {
                        anyhow!("mir_codegen: CallUser to '{}' but no FuncId declared", name)
                    })?;
                    let fref = module.declare_func_in_func(target_id, &mut ctx.func);
                    func_ref_cache.insert(name.clone(), fref);
                }
                Inst::CallExtern { sym, param_tys, ret_ty, .. } => {
                    if func_ref_cache.contains_key(sym) {
                        continue;
                    }
                    let mut ext_sig = Signature::new(
                        if cfg!(windows) { CallConv::WindowsFastcall } else { CallConv::SystemV }
                    );
                    for ty in param_tys {
                        ext_sig.params.push(AbiParam::new(hir_to_cl(ty)));
                    }
                    if !matches!(ret_ty, HirType::Void) {
                        ext_sig.returns.push(AbiParam::new(hir_to_cl(ret_ty)));
                    }
                    let target_id = module
                        .declare_function(sym, Linkage::Import, &ext_sig)
                        .map_err(|e| anyhow!("declare extern {}: {e}", sym))?;
                    let fref = module.declare_func_in_func(target_id, &mut ctx.func);
                    func_ref_cache.insert(sym.clone(), fref);
                }
                Inst::StrLit { value, .. } => {
                    // Allocate a unique anonymous data segment for each
                    // occurrence — same-value sharing isn't worth complicating
                    // the lookup map; LLVM-style merging can come later.
                    let data_name =
                        format!("__rts_mir_str_{}_{}", mir.name, strlit_inst_idx);
                    let data_id = module
                        .declare_data(
                            &data_name,
                            cranelift_module::Linkage::Local,
                            false, // writable
                            false, // tls
                        )
                        .map_err(|e| anyhow!("declare_data {}: {e}", data_name))?;
                    let mut data = cranelift_module::DataDescription::new();
                    data.define(value.as_bytes().to_vec().into_boxed_slice());
                    module
                        .define_data(data_id, &data)
                        .map_err(|e| anyhow!("define_data {}: {e}", data_name))?;
                    let gv = module.declare_data_in_func(data_id, &mut ctx.func);
                    strlit_cache.insert(strlit_inst_idx, (gv, value.len()));
                    strlit_inst_idx += 1;
                }
                _ => {}
            }
        }
    }

    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

    // Pre-create one Cranelift Block per MIR block so jumps can target by index.
    let cl_blocks: Vec<Block> = (0..mir.blocks.len()).map(|_| builder.create_block()).collect();

    // Append params on the entry block matching the function signature.
    if let Some(&entry) = cl_blocks.first() {
        builder.append_block_params_for_function_params(entry);
    }

    // Append block params for non-entry blocks (= phi sources via terminator args).
    for (i, bb) in mir.blocks.iter().enumerate().skip(1) {
        for (_, ty) in &bb.params {
            let cl_ty = hir_to_cl(ty);
            builder.append_block_param(cl_blocks[i], cl_ty);
        }
    }

    // values[ValueId] → Cranelift Value
    let mut vmap: HashMap<ValueId, Value> = HashMap::new();
    // Reset the StrLit walk counter — used in lock-step with the pre-pass
    // that allocated GlobalValues so each Inst::StrLit hits its own entry.
    let mut strlit_walk_idx: usize = 0;

    // Bind entry block params (= function params) to their MIR ValueIds.
    if !mir.params.is_empty() {
        let entry = cl_blocks[0];
        let cl_params: Vec<Value> = builder.block_params(entry).to_vec();
        for ((vid, _ty), cl_val) in mir.params.iter().zip(cl_params.iter()) {
            vmap.insert(*vid, *cl_val);
        }
    }

    // Lower each block.
    for (i, bb) in mir.blocks.iter().enumerate() {
        let cl_blk = cl_blocks[i];
        builder.switch_to_block(cl_blk);

        // Bind block params (skip entry, already bound above)
        if i != 0 {
            let cl_params: Vec<Value> = builder.block_params(cl_blk).to_vec();
            for ((vid, _ty), cl_val) in bb.params.iter().zip(cl_params.iter()) {
                vmap.insert(*vid, *cl_val);
            }
        }

        for inst in &bb.insts {
            lower_inst(
                &mut builder,
                inst,
                &mut vmap,
                mir,
                &func_ref_cache,
                &strlit_cache,
                &mut strlit_walk_idx,
            )?;
        }
        lower_term(&mut builder, &bb.term, &vmap, &cl_blocks)?;
    }

    // Seal everything (dense list, all preds known after walk).
    for &cl_blk in &cl_blocks {
        builder.seal_block(cl_blk);
    }
    builder.finalize();

    // Pre-compila para extrair stack maps ANTES de define_function (que
    // limpa o ctx). Espelha o mesmo padrão do codegen AST autoritativo.
    {
        use cranelift_codegen::control::ControlPlane;
        let mut ctrl = ControlPlane::default();
        let gc_debug = std::env::var("RTS_GC_DEBUG").is_ok();
        match ctx.compile(module.isa(), &mut ctrl) {
            Ok(compiled) => {
                let raw_maps = compiled.buffer.user_stack_maps();
                if gc_debug {
                    eprintln!(
                        "[gc-mir] fn `{}` — {} raw stack map entries",
                        mir.name,
                        raw_maps.len()
                    );
                }
                let maps: Vec<(u32, Vec<u32>)> = raw_maps
                    .iter()
                    .filter_map(|(ret_offset, _, map)| {
                        let offsets: Vec<u32> =
                            map.entries().map(|(_, sp_off)| sp_off).collect();
                        if offsets.is_empty() {
                            None
                        } else {
                            Some((*ret_offset, offsets))
                        }
                    })
                    .collect();
                if !maps.is_empty() {
                    rts_runtime::namespaces::gc::stack_map_registry::push_pending(
                        func_id.as_u32(),
                        maps,
                    );
                }
            }
            Err(e) => {
                if gc_debug {
                    eprintln!(
                        "[gc-mir] fn `{}` — pre-compile failed: {}",
                        mir.name, e.inner
                    );
                }
            }
        }
    }

    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| anyhow!("define_function {}: {e}", mir.name))?;
    module.clear_context(&mut ctx);

    Ok(func_id)
}

// ---------------------------------------------------------------------------
// Signature
// ---------------------------------------------------------------------------

fn build_signature(module: &dyn Module, mir: &MirFunc) -> Signature {
    let conv = match mir.conv {
        CallConvHint::Tail => CallConv::Tail,
        CallConvHint::SystemV => CallConv::SystemV,
        CallConvHint::WindowsFastcall => CallConv::WindowsFastcall,
        CallConvHint::Cold => CallConv::SystemV, // Cranelift has no Cold conv; fallback
    };
    // For tests we use the module's default conv to avoid Tail+default mismatch
    // when the host doesn't enable preserve_frame_pointers. Fallback rule:
    // honor MIR's conv unless that's Tail and the ISA can't take it.
    let _ = module;
    let mut sig = Signature::new(conv);
    for (_, ty) in &mir.params {
        sig.params.push(AbiParam::new(hir_to_cl(ty)));
    }
    if !matches!(mir.ret, HirType::Void) {
        sig.returns.push(AbiParam::new(hir_to_cl(&mir.ret)));
    }
    sig
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

fn val(vmap: &HashMap<ValueId, Value>, vid: ValueId) -> Result<Value> {
    vmap.get(&vid)
        .copied()
        .ok_or_else(|| anyhow!("MIR value v{} referenced before being defined", vid))
}

fn lower_inst(
    builder: &mut FunctionBuilder,
    inst: &Inst,
    vmap: &mut HashMap<ValueId, Value>,
    mir: &MirFunc,
    func_refs: &HashMap<String, cranelift_codegen::ir::FuncRef>,
    strlit_cache: &HashMap<usize, (cranelift_codegen::ir::GlobalValue, usize)>,
    strlit_walk_idx: &mut usize,
) -> Result<()> {
    use Inst::*;
    macro_rules! bind {
        ($dst:expr, $v:expr) => {{
            vmap.insert(*$dst, $v);
        }};
    }

    match inst {
        IConst { dst, ty, val } => {
            let cl_ty = hir_to_cl(ty);
            let v = builder.ins().iconst(cl_ty, *val);
            bind!(dst, v);
        }
        F32Const { dst, val } => {
            let v = builder.ins().f32const(*val);
            bind!(dst, v);
        }
        F64Const { dst, val } => {
            let v = builder.ins().f64const(*val);
            bind!(dst, v);
        }

        // Integer arithmetic
        IAdd { dst, lhs, rhs } => {
            let v = builder.ins().iadd(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        IAddImm { dst, lhs, imm } => {
            let v = builder.ins().iadd_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        ISub { dst, lhs, rhs } => {
            let v = builder.ins().isub(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        IMul { dst, lhs, rhs } => {
            let v = builder.ins().imul(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        IMulImm { dst, lhs, imm } => {
            let v = builder.ins().imul_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        SDiv { dst, lhs, rhs } => {
            let v = builder.ins().sdiv(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        UDiv { dst, lhs, rhs } => {
            let v = builder.ins().udiv(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        SRem { dst, lhs, rhs } => {
            let v = builder.ins().srem(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        URem { dst, lhs, rhs } => {
            let v = builder.ins().urem(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        INeg { dst, src } => {
            let v = builder.ins().ineg(val(vmap, *src)?);
            bind!(dst, v);
        }

        // Bitwise
        BAnd { dst, lhs, rhs } => {
            let v = builder.ins().band(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        BAndImm { dst, lhs, imm } => {
            let v = builder.ins().band_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        BOr { dst, lhs, rhs } => {
            let v = builder.ins().bor(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        BOrImm { dst, lhs, imm } => {
            let v = builder.ins().bor_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        BXor { dst, lhs, rhs } => {
            let v = builder.ins().bxor(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        BXorImm { dst, lhs, imm } => {
            let v = builder.ins().bxor_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        BNot { dst, src } => {
            let v = builder.ins().bnot(val(vmap, *src)?);
            bind!(dst, v);
        }

        // Shifts
        IShl { dst, lhs, rhs } => {
            let v = builder.ins().ishl(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        IShlImm { dst, lhs, imm } => {
            let v = builder.ins().ishl_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        UShr { dst, lhs, rhs } => {
            let v = builder.ins().ushr(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        SShr { dst, lhs, rhs } => {
            let v = builder.ins().sshr(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        SShrImm { dst, lhs, imm } => {
            let v = builder.ins().sshr_imm(val(vmap, *lhs)?, *imm);
            bind!(dst, v);
        }
        Rotl { dst, lhs, rhs } => {
            let v = builder.ins().rotl(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        Rotr { dst, lhs, rhs } => {
            let v = builder.ins().rotr(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }

        // Bit counting
        Clz { dst, src } => {
            let v = builder.ins().clz(val(vmap, *src)?);
            bind!(dst, v);
        }
        Ctz { dst, src } => {
            let v = builder.ins().ctz(val(vmap, *src)?);
            bind!(dst, v);
        }
        Popcnt { dst, src } => {
            let v = builder.ins().popcnt(val(vmap, *src)?);
            bind!(dst, v);
        }
        Bswap { dst, src } => {
            let v = builder.ins().bswap(val(vmap, *src)?);
            bind!(dst, v);
        }

        // Float arithmetic
        FAdd { dst, lhs, rhs } => {
            let v = builder.ins().fadd(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        FSub { dst, lhs, rhs } => {
            let v = builder.ins().fsub(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        FMul { dst, lhs, rhs } => {
            let v = builder.ins().fmul(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        FDiv { dst, lhs, rhs } => {
            let v = builder.ins().fdiv(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        FNeg { dst, src } => {
            let v = builder.ins().fneg(val(vmap, *src)?);
            bind!(dst, v);
        }
        FAbs { dst, src } => {
            let v = builder.ins().fabs(val(vmap, *src)?);
            bind!(dst, v);
        }
        Sqrt { dst, src } => {
            let v = builder.ins().sqrt(val(vmap, *src)?);
            bind!(dst, v);
        }
        Fma { dst, a, b, c } => {
            let v = builder
                .ins()
                .fma(val(vmap, *a)?, val(vmap, *b)?, val(vmap, *c)?);
            bind!(dst, v);
        }
        FMin { dst, lhs, rhs } => {
            let v = builder.ins().fmin(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        FMax { dst, lhs, rhs } => {
            let v = builder.ins().fmax(val(vmap, *lhs)?, val(vmap, *rhs)?);
            bind!(dst, v);
        }
        Ceil { dst, src } => {
            let v = builder.ins().ceil(val(vmap, *src)?);
            bind!(dst, v);
        }
        Floor { dst, src } => {
            let v = builder.ins().floor(val(vmap, *src)?);
            bind!(dst, v);
        }
        Trunc { dst, src } => {
            let v = builder.ins().trunc(val(vmap, *src)?);
            bind!(dst, v);
        }

        // Conversions
        IReduce { dst, src, to } => {
            let v = builder.ins().ireduce(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        UExtend { dst, src, to } => {
            let v = builder.ins().uextend(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        SExtend { dst, src, to } => {
            let v = builder.ins().sextend(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        FPromote { dst, src } => {
            let v = builder.ins().fpromote(cl::F64, val(vmap, *src)?);
            bind!(dst, v);
        }
        FDemote { dst, src } => {
            let v = builder.ins().fdemote(cl::F32, val(vmap, *src)?);
            bind!(dst, v);
        }
        CvtFromSint { dst, src, to } => {
            let v = builder.ins().fcvt_from_sint(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        CvtFromUint { dst, src, to } => {
            let v = builder.ins().fcvt_from_uint(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        CvtToSintSat { dst, src, to } => {
            let v = builder
                .ins()
                .fcvt_to_sint_sat(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        CvtToUintSat { dst, src, to } => {
            let v = builder
                .ins()
                .fcvt_to_uint_sat(hir_to_cl(to), val(vmap, *src)?);
            bind!(dst, v);
        }
        Bitcast { dst, src, to } => {
            let v = builder
                .ins()
                .bitcast(hir_to_cl(to), MemFlags::new(), val(vmap, *src)?);
            bind!(dst, v);
        }

        // Comparisons — Cranelift icmp/fcmp returns I8; widen to dst type via uextend
        ICmp { dst, cond, lhs, rhs } => {
            let cc = int_cond_to_cl(cond);
            let cmp = builder.ins().icmp(cc, val(vmap, *lhs)?, val(vmap, *rhs)?);
            // dst type from MIR table
            let dst_ty = hir_to_cl(mir.type_of(*dst));
            let v = if dst_ty == cl::I8 {
                cmp
            } else {
                builder.ins().uextend(dst_ty, cmp)
            };
            bind!(dst, v);
        }
        FCmp { dst, cond, lhs, rhs } => {
            let cc = float_cond_to_cl(cond);
            let cmp = builder.ins().fcmp(cc, val(vmap, *lhs)?, val(vmap, *rhs)?);
            let dst_ty = hir_to_cl(mir.type_of(*dst));
            let v = if dst_ty == cl::I8 {
                cmp
            } else {
                builder.ins().uextend(dst_ty, cmp)
            };
            bind!(dst, v);
        }

        Select { dst, cond, on_true, on_false } => {
            let v = builder
                .ins()
                .select(val(vmap, *cond)?, val(vmap, *on_true)?, val(vmap, *on_false)?);
            bind!(dst, v);
        }

        // Memory: narrow loads/stores keep their semantics
        Load { dst, ty, ptr, offset, .. } => {
            let cl_ty = hir_to_cl(ty);
            let v = builder.ins().load(cl_ty, MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        Store { val: v, ptr, offset, .. } => {
            builder
                .ins()
                .store(MemFlags::trusted(), val(vmap, *v)?, val(vmap, *ptr)?, *offset);
        }
        ULoad8 { dst, ptr, offset } => {
            let v = builder.ins().uload8(cl::I64, MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        ULoad16 { dst, ptr, offset } => {
            let v = builder.ins().uload16(cl::I64, MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        ULoad32 { dst, ptr, offset } => {
            let v = builder.ins().uload32(MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        SLoad8 { dst, ptr, offset } => {
            let v = builder.ins().sload8(cl::I64, MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        SLoad16 { dst, ptr, offset } => {
            let v = builder.ins().sload16(cl::I64, MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        SLoad32 { dst, ptr, offset } => {
            let v = builder.ins().sload32(MemFlags::trusted(), val(vmap, *ptr)?, *offset);
            bind!(dst, v);
        }
        IStore8 { val: v, ptr, offset } => {
            builder.ins().istore8(MemFlags::trusted(), val(vmap, *v)?, val(vmap, *ptr)?, *offset);
        }
        IStore16 { val: v, ptr, offset } => {
            builder.ins().istore16(MemFlags::trusted(), val(vmap, *v)?, val(vmap, *ptr)?, *offset);
        }
        IStore32 { val: v, ptr, offset } => {
            builder.ins().istore32(MemFlags::trusted(), val(vmap, *v)?, val(vmap, *ptr)?, *offset);
        }

        // Stack alloc — needs a stack slot; opaque for now (returns 0).
        // Production lowering should declare a StackSlot via builder.create_sized_stack_slot.
        StackAlloc { dst, size: _, align: _ } => {
            let v = builder.ins().iconst(cl::I64, 0);
            bind!(dst, v);
        }

        // String literal materialization. The pre-pass allocated a data
        // segment per StrLit inst and stashed the GlobalValue in
        // `strlit_cache` keyed by the lock-step index `strlit_walk_idx`.
        StrLit { dst_ptr, dst_len, value: _ } => {
            let idx = *strlit_walk_idx;
            *strlit_walk_idx += 1;
            let (gv, len) = strlit_cache
                .get(&idx)
                .copied()
                .ok_or_else(|| anyhow!("mir_codegen: StrLit idx {} missing in cache", idx))?;
            let ptr_val = builder.ins().symbol_value(cl::I64, gv);
            let len_val = builder.ins().iconst(cl::I64, len as i64);
            vmap.insert(*dst_ptr, ptr_val);
            vmap.insert(*dst_len, len_val);
        }

        // User-fn call: resolved via the pre-built FuncRef cache.
        CallUser { dst, name, args, .. } => {
            let fref = func_refs.get(name).ok_or_else(|| {
                anyhow!("mir_codegen: CallUser '{}' has no FuncRef in cache", name)
            })?;
            let cl_args: Vec<Value> =
                args.iter().map(|a| val(vmap, *a)).collect::<Result<_>>()?;
            let call = builder.ins().call(*fref, &cl_args);
            if let Some(d) = dst {
                let results = builder.inst_results(call);
                if let Some(&first) = results.first() {
                    vmap.insert(*d, first);
                } else {
                    return Err(anyhow!(
                        "mir_codegen: CallUser '{}' has dst but callee returned void",
                        name
                    ));
                }
            }
        }

        // Extern call: same shape as CallUser but the FuncRef was imported
        // via `declare_function(Linkage::Import)` instead of a local FuncId.
        CallExtern { dst, sym, args, .. } => {
            let fref = func_refs.get(sym).ok_or_else(|| {
                anyhow!("mir_codegen: CallExtern '{}' has no FuncRef in cache", sym)
            })?;
            let cl_args: Vec<Value> =
                args.iter().map(|a| val(vmap, *a)).collect::<Result<_>>()?;
            let call = builder.ins().call(*fref, &cl_args);
            if let Some(d) = dst {
                let results = builder.inst_results(call);
                if let Some(&first) = results.first() {
                    vmap.insert(*d, first);
                } else {
                    return Err(anyhow!(
                        "mir_codegen: CallExtern '{}' has dst but callee returned void",
                        sym
                    ));
                }
            }
        }

        // GC stack map: marca o valor pra que Cranelift inclua seu slot
        // no stack map quando a fn for compilada. Coletado depois via
        // ctx.compiled_code() e registrado em stack_map_registry.
        DeclareGcValue { val } => {
            let v = vmap.get(val).copied().ok_or_else(|| {
                anyhow!("DeclareGcValue references unknown ValueId v{}", val)
            })?;
            builder.declare_value_needs_stack_map(v);
        }

        // Atomics: Cranelift 0.131 não expõe MemOrder no IR — todos
        // os atomics são SeqCst (mais conservativo). MemFlags precisa
        // de `notrap()` (atomics nunca trapam por alignment).
        AtomicLoad { dst, ptr, .. } => {
            let dst_ty = hir_to_cl(mir.type_of(*dst));
            let v = builder.ins().atomic_load(
                dst_ty,
                MemFlags::trusted(),
                val(vmap, *ptr)?,
            );
            bind!(dst, v);
        }
        AtomicStore { val: v, ptr, .. } => {
            builder.ins().atomic_store(
                MemFlags::trusted(),
                val(vmap, *v)?,
                val(vmap, *ptr)?,
            );
        }
        AtomicRmw { dst, op, ptr, val: v, .. } => {
            let dst_ty = hir_to_cl(mir.type_of(*dst));
            let cl_op = match op {
                rts_mir::ir::RmwOp::Add => cranelift_codegen::ir::AtomicRmwOp::Add,
                rts_mir::ir::RmwOp::Sub => cranelift_codegen::ir::AtomicRmwOp::Sub,
                rts_mir::ir::RmwOp::And => cranelift_codegen::ir::AtomicRmwOp::And,
                rts_mir::ir::RmwOp::Or => cranelift_codegen::ir::AtomicRmwOp::Or,
                rts_mir::ir::RmwOp::Xor => cranelift_codegen::ir::AtomicRmwOp::Xor,
                rts_mir::ir::RmwOp::Nand => cranelift_codegen::ir::AtomicRmwOp::Nand,
                rts_mir::ir::RmwOp::Xchg => cranelift_codegen::ir::AtomicRmwOp::Xchg,
                rts_mir::ir::RmwOp::Umin => cranelift_codegen::ir::AtomicRmwOp::Umin,
                rts_mir::ir::RmwOp::Umax => cranelift_codegen::ir::AtomicRmwOp::Umax,
                rts_mir::ir::RmwOp::Smin => cranelift_codegen::ir::AtomicRmwOp::Smin,
                rts_mir::ir::RmwOp::Smax => cranelift_codegen::ir::AtomicRmwOp::Smax,
            };
            let result = builder.ins().atomic_rmw(
                dst_ty,
                MemFlags::trusted(),
                cl_op,
                val(vmap, *ptr)?,
                val(vmap, *v)?,
            );
            bind!(dst, result);
        }
        AtomicCas { dst, ptr, expected, replacement, .. } => {
            let result = builder.ins().atomic_cas(
                MemFlags::trusted(),
                val(vmap, *ptr)?,
                val(vmap, *expected)?,
                val(vmap, *replacement)?,
            );
            bind!(dst, result);
        }
        Fence { .. } => {
            builder.ins().fence();
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Terminators
// ---------------------------------------------------------------------------

fn lower_term(
    builder: &mut FunctionBuilder,
    term: &Terminator,
    vmap: &HashMap<ValueId, Value>,
    cl_blocks: &[Block],
) -> Result<()> {
    match term {
        Terminator::Return(vs) => {
            let cl_vals: Vec<Value> = vs.iter().map(|v| val(vmap, *v)).collect::<Result<_>>()?;
            builder.ins().return_(&cl_vals);
        }
        Terminator::Jump { target, args } => {
            let cl_args: Vec<Value> = args.iter().map(|v| val(vmap, *v)).collect::<Result<_>>()?;
            let cl_args_owned: Vec<_> = cl_args.into_iter().map(|v| v.into()).collect();
            builder.ins().jump(cl_blocks[*target as usize], &cl_args_owned);
        }
        Terminator::Brif {
            cond,
            then_block,
            then_args,
            else_block,
            else_args,
        } => {
            let cond_v = val(vmap, *cond)?;
            let then_args_v: Vec<Value> = then_args
                .iter()
                .map(|v| val(vmap, *v))
                .collect::<Result<_>>()?;
            let else_args_v: Vec<Value> = else_args
                .iter()
                .map(|v| val(vmap, *v))
                .collect::<Result<_>>()?;
            let then_args_owned: Vec<_> = then_args_v.into_iter().map(|v| v.into()).collect();
            let else_args_owned: Vec<_> = else_args_v.into_iter().map(|v| v.into()).collect();
            builder.ins().brif(
                cond_v,
                cl_blocks[*then_block as usize],
                &then_args_owned,
                cl_blocks[*else_block as usize],
                &else_args_owned,
            );
        }
        Terminator::Switch { index, default, cases } => {
            let mut sw = cranelift_frontend::Switch::new();
            for (key, blk) in cases {
                sw.set_entry(*key as u128, cl_blocks[*blk as usize]);
            }
            sw.emit(builder, val(vmap, *index)?, cl_blocks[*default as usize]);
        }
        Terminator::TailCall { .. } | Terminator::TailCallIndirect { .. } => {
            return Err(anyhow!(
                "mir_codegen: tail call lowering not yet supported (needs FuncRef cache)"
            ));
        }
        Terminator::Trap { .. } => {
            builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
        }
    }
    Ok(())
}

fn int_cond_to_cl(c: &IntCond) -> IntCC {
    match c {
        IntCond::Eq => IntCC::Equal,
        IntCond::Ne => IntCC::NotEqual,
        IntCond::Slt => IntCC::SignedLessThan,
        IntCond::Sle => IntCC::SignedLessThanOrEqual,
        IntCond::Sgt => IntCC::SignedGreaterThan,
        IntCond::Sge => IntCC::SignedGreaterThanOrEqual,
        IntCond::Ult => IntCC::UnsignedLessThan,
        IntCond::Ule => IntCC::UnsignedLessThanOrEqual,
        IntCond::Ugt => IntCC::UnsignedGreaterThan,
        IntCond::Uge => IntCC::UnsignedGreaterThanOrEqual,
    }
}

fn float_cond_to_cl(c: &FloatCond) -> FloatCC {
    match c {
        FloatCond::Eq => FloatCC::Equal,
        FloatCond::Ne => FloatCC::NotEqual,
        FloatCond::OLt => FloatCC::LessThan,
        FloatCond::OLe => FloatCC::LessThanOrEqual,
        FloatCond::OGt => FloatCC::GreaterThan,
        FloatCond::OGe => FloatCC::GreaterThanOrEqual,
        FloatCond::ONe => FloatCC::OrderedNotEqual,
        FloatCond::ULt => FloatCC::UnorderedOrLessThan,
        FloatCond::ULe => FloatCC::UnorderedOrLessThanOrEqual,
        FloatCond::UGt => FloatCC::UnorderedOrGreaterThan,
        FloatCond::UGe => FloatCC::UnorderedOrGreaterThanOrEqual,
        FloatCond::UNe => FloatCC::NotEqual, // map both Ne to NotEqual
        FloatCond::Ordered => FloatCC::Ordered,
        FloatCond::Unordered => FloatCC::Unordered,
    }
}
