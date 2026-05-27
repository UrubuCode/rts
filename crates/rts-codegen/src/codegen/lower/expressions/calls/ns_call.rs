//! Chamadas de namespace ABI: `io.print`, `math.sqrt`, etc.
//!
//! `lower_ns_call` encontra o `NamespaceMember` em `abi::SPECS` e
//! emite a chamada Cranelift correspondente — pode ser:
//! - `lower_intrinsic` quando o membro tem `Intrinsic` (sqrt/abs/min/max
//!   inlined em IR direto);
//! - `emit_constant_load` quando eh um `MemberKind::Constant`;
//! - `extern_call` declarado via SPECS para o resto.
//!
//! `lower_node_ns_call` cobre redirecionamento de Node imports
//! (`fs.readFile` → namespace `fs`).
//! `lower_global_instance_call` cobre `console.log`, `Date.now`, etc.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types as cl};
use cranelift_module::{Linkage, Module};
use swc_ecma_ast::{CallExpr, Expr};

use crate::abi::lookup;
use crate::abi::signature::lower_member;
use crate::abi::types::AbiType;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::super::operators::to_f64;

pub(super) fn emit_constant_load(ctx: &mut FnCtx, member: &crate::abi::NamespaceMember) -> Result<TypedVal> {
    use crate::abi::signature::scalar_to_cl;
    let lowered = lower_member(member);
    let ret_abi = lowered
        .ret
        .ok_or_else(|| anyhow!("constant `{}` has no return type", member.name))?;
    let ret_cl = scalar_to_cl(ret_abi);

    let func_id = if let Some(id) = ctx.extern_cache.get(member.symbol).copied() {
        id
    } else {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        sig.returns.push(AbiParam::new(ret_cl));
        let id = ctx
            .module
            .declare_function(member.symbol, Linkage::Import, &sig)
            .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
        ctx.extern_cache.insert(member.symbol.to_string(), id);
        id
    };
    let fref = ctx.fref_for_id(func_id);
    let inst = ctx.builder.ins().call(fref, &[]);
    let val = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(val, ValTy::from_abi(member.returns)))
}

pub(super) fn lower_intrinsic(
    ctx: &mut FnCtx,
    kind: crate::abi::Intrinsic,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::abi::Intrinsic;
    use cranelift_codegen::ir::condcodes::IntCC;

    fn arg_f64(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<cranelift_codegen::ir::Value> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(to_f64(ctx, tv))
    }

    fn arg_i64(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<cranelift_codegen::ir::Value> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_i64(tv).val)
    }

    match kind {
        Intrinsic::Sqrt => {
            let x = arg_f64(ctx, call, 0)?;
            Ok(Some(TypedVal::new(ctx.builder.ins().sqrt(x), ValTy::F64)))
        }
        Intrinsic::AbsF64 => {
            let x = arg_f64(ctx, call, 0)?;
            Ok(Some(TypedVal::new(ctx.builder.ins().fabs(x), ValTy::F64)))
        }
        Intrinsic::MinF64 => Ok(Some(TypedVal::new(
            {
                let a = arg_f64(ctx, call, 0)?;
                let b = arg_f64(ctx, call, 1)?;
                ctx.builder.ins().fmin(a, b)
            },
            ValTy::F64,
        ))),
        Intrinsic::MaxF64 => Ok(Some(TypedVal::new(
            {
                let a = arg_f64(ctx, call, 0)?;
                let b = arg_f64(ctx, call, 1)?;
                ctx.builder.ins().fmax(a, b)
            },
            ValTy::F64,
        ))),
        Intrinsic::AbsI64 => {
            let x = arg_i64(ctx, call, 0)?;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_neg = ctx.builder.ins().icmp(IntCC::SignedLessThan, x, zero);
            let neg = ctx.builder.ins().ineg(x);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().select(is_neg, neg, x),
                ValTy::I64,
            )))
        }
        Intrinsic::MinI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let less = ctx.builder.ins().icmp(IntCC::SignedLessThan, a, b);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().select(less, a, b),
                ValTy::I64,
            )))
        }
        Intrinsic::MaxI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let greater = ctx.builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().select(greater, a, b),
                ValTy::I64,
            )))
        }
    }
}

pub(super) fn lower_ns_call_member(
    ctx: &mut FnCtx,
    member: &'static crate::abi::member::NamespaceMember,
    call: &CallExpr,
) -> Result<TypedVal> {
    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }
    lower_ns_call_body(ctx, member, call)
}

pub(super) fn lower_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    let (_spec, member) =
        lookup(qualified).ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;

    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }

    lower_ns_call_body(ctx, member, call)
}

pub(super) fn lower_ns_call_body(
    ctx: &mut FnCtx,
    member: &'static crate::abi::member::NamespaceMember,
    call: &CallExpr,
) -> Result<TypedVal> {
    use crate::abi::signature::scalar_to_cl;
    let qualified = member.symbol;
    let lowered = lower_member(member);

    let func_id = if !ctx.extern_cache.contains_key(member.symbol) {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        for &p in &lowered.params {
            sig.params.push(AbiParam::new(scalar_to_cl(p)));
        }
        if let Some(r) = lowered.ret {
            sig.returns.push(AbiParam::new(scalar_to_cl(r)));
        }
        let id = ctx
            .module
            .declare_function(member.symbol, Linkage::Import, &sig)
            .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
        ctx.extern_cache.insert(member.symbol.to_string(), id);
        id
    } else {
        *ctx.extern_cache.get(member.symbol).unwrap()
    };
    let fref = ctx.fref_for_id(func_id);

    let mut values = Vec::new();
    let mut arg_iter = call.args.iter();
    for &abi_ty in member.args {
        let arg = arg_iter
            .next()
            .ok_or_else(|| anyhow!("too few arguments for `{qualified}`"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in namespace calls"));
        }
        match abi_ty {
            AbiType::StrPtr => {
                // Fast path: arg eh literal string. Emite (ptr, len)
                // estaticos direto, sem string_from_static + string_ptr
                // + string_len. Em \`io.print(\"hello\")\` reduz 4 calls
                // pra 1.
                fn unwrap_paren(e: &Expr) -> &Expr {
                    match e {
                        Expr::Paren(p) => unwrap_paren(&p.expr),
                        _ => e,
                    }
                }
                let lit_bytes: Option<Vec<u8>> = match unwrap_paren(&arg.expr) {
                    Expr::Lit(swc_ecma_ast::Lit::Str(s)) => {
                        Some(s.value.as_bytes().to_vec())
                    }
                    _ => None,
                };
                if let Some(bytes) = lit_bytes {
                    let (ptr, len) = ctx.emit_str_literal(&bytes)?;
                    values.push(ptr);
                    values.push(len);
                    continue;
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                match tv.ty {
                    ValTy::Handle => {
                        let ptr_fref =
                            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                        let len_fref =
                            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                        let pi = ctx.builder.ins().call(ptr_fref, &[tv.val]);
                        let ptr = ctx.builder.inst_results(pi)[0];
                        let li = ctx.builder.ins().call(len_fref, &[tv.val]);
                        let len = ctx.builder.inst_results(li)[0];
                        values.push(ptr);
                        values.push(len);
                    }
                    _ => return Err(anyhow!("StrPtr argument must be a string value")),
                }
            }
            AbiType::I32 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i32(tv).val)
            }
            AbiType::U64 => {
                // U64 e tipo opaco — handle de runtime ou ponteiro.
                // Quando o input e f64 (variavel `number` carregando um
                // handle), converte numericamente via fcvt_to_uint_sat:
                // o valor f64 ja' foi produzido por uma conversao i64→f64,
                // entao fcvt_to_uint_sat recupera o inteiro original.
                // Bitcast daria o bit-pattern IEEE 754, que e' um valor
                // completamente diferente — causaria handles invalidos.
                //
                // Excecao (#435): `thread.spawn*(fp, arg)` — quando o
                // worker pede `arg: number` (f64), o lifter cria
                // trampolim com param `__rts_spawn_arg_f64` que faz
                // bitcast i64→f64 ao receber. Nesse caso o caller deve
                // passar os BITS do f64, nao a conversao numerica.
                // Detectado pelo simbolo do membro (todas variantes
                // de spawn que aceitam fn_ptr+arg).
                let is_spawn_arg = matches!(
                    member.symbol,
                    "__RTS_FN_NS_THREAD_SPAWN"
                    | "__RTS_FN_NS_THREAD_SPAWN_ASYNC"
                    | "__RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN"
                    | "__RTS_FN_NS_THREAD_SPAWN_DETACHED"
                    | "__RTS_FN_NS_THREAD_SPAWN_WITH_UD"
                ) && values.len() == 1; // segundo arg = `arg` (apos fn_ptr)
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = match tv.ty {
                    crate::codegen::lower::ctx::ValTy::F64 if is_spawn_arg => {
                        ctx.builder.ins().bitcast(
                            cl::I64,
                            cranelift_codegen::ir::MemFlags::new(),
                            tv.val,
                        )
                    }
                    crate::codegen::lower::ctx::ValTy::F64 => {
                        ctx.builder.ins().fcvt_to_uint_sat(cl::I64, tv.val)
                    }
                    _ => ctx.coerce_to_i64(tv).val,
                };
                values.push(v)
            }
            AbiType::I64 | AbiType::Handle | AbiType::Bool => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val)
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(to_f64(ctx, tv))
            }
            AbiType::Void => {}
        }
    }

    let inst = ctx.builder.ins().call(fref, &values);
    if lowered.ret.is_some() {
        let v = ctx.builder.inst_results(inst)[0];
        // `parallel.find/reduce*` retornam I64 mas o slot pode ser handle de
        // string/objeto. Marca como ambiguo para que template literal/console
        // use TPL_COERCE_AUTO. Necessario para reduce de strings (#254).
        if matches!(
            member.symbol,
            "__RTS_FN_NS_PARALLEL_FIND"
            | "__RTS_FN_NS_PARALLEL_REDUCE"
            | "__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT"
            | "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT"
            | "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT_NO_INIT"
            // (#92) promise.wait retorna i64 ambiguo: handle de string OU
            // valor inteiro. Marca pra TPL_COERCE_AUTO resolver em runtime.
            | "__RTS_FN_NS_PROMISE_WAIT"
            // (PR #1207) JSON.parse retorna handle ambiguo — pode ser
            // Map, Vec, String, scalar i64, ou sentinel JS (true/false/null).
            // Marca pra `.flags`/`.source`/`.port` em sub-obj NAO colidir
            // com GLOBAL_CLASS_SPECS getters (RegExp.flags, URL.port, etc).
            | "__RTS_FN_NS_JSON_PARSE"
            | "__RTS_FN_NS_JSON_PARSE_REVIVER"
            | "__RTS_FN_NS_JSON_PARSE5"
        ) {
            ctx.var_member_call_values.insert(v);
        }
        Ok(TypedVal::new(v, ValTy::from_abi(member.returns)))
    } else {
        // (cross-runtime #248) `parallel.for_each` retorna void mas JS
        // \`arr.forEach(...)\` retorna undefined. Emite sentinela MIN+2
        // marcada ambigua para que template/console use TPL_COERCE_AUTO.
        let is_foreach = member.symbol == "__RTS_FN_NS_PARALLEL_FOR_EACH";
        let val = if is_foreach { i64::MIN + 2 } else { 0 };
        let v = ctx.builder.ins().iconst(cl::I64, val);
        if is_foreach {
            ctx.var_member_call_values.insert(v);
        }
        Ok(TypedVal::new(v, ValTy::I64))
    }
}

/// Lowers a call to a `node:*` member via nodespace lookup.
///
/// `qualified` is the codegen-internal name like `"node_fs.readFileSync"`.
/// The nodespace member maps directly to an existing RTS ABI symbol, so
/// this function builds the same extern call as `lower_ns_call` but sources
/// the metadata from `crate::nodespace::node_lookup` instead of `abi::lookup`.
pub(super) fn lower_node_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    use crate::abi::signature::{lower_params, lower_return, scalar_to_cl};

    let member = crate::nodespace::node_lookup(qualified)
        .ok_or_else(|| anyhow!("unknown node namespace member `{qualified}`"))?;

    let lowered_params = lower_params(member.args);
    let lowered_ret = lower_return(member.returns);

    let func_id = if !ctx.extern_cache.contains_key(member.symbol) {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        for &p in &lowered_params {
            sig.params.push(AbiParam::new(scalar_to_cl(p)));
        }
        if let Some(r) = lowered_ret {
            sig.returns.push(AbiParam::new(scalar_to_cl(r)));
        }
        let id = ctx
            .module
            .declare_function(member.symbol, Linkage::Import, &sig)
            .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
        ctx.extern_cache.insert(member.symbol.to_string(), id);
        id
    } else {
        *ctx.extern_cache.get(member.symbol).unwrap()
    };
    let fref = ctx.fref_for_id(func_id);

    let mut values = Vec::new();
    let mut arg_iter = call.args.iter();
    for &abi_ty in member.args {
        let arg = arg_iter
            .next()
            .ok_or_else(|| anyhow!("too few arguments for node `{qualified}`"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in node namespace calls"));
        }
        match abi_ty {
            AbiType::StrPtr => {
                fn unwrap_paren(e: &Expr) -> &Expr {
                    match e {
                        Expr::Paren(p) => unwrap_paren(&p.expr),
                        _ => e,
                    }
                }
                let lit_bytes: Option<Vec<u8>> = match unwrap_paren(&arg.expr) {
                    Expr::Lit(swc_ecma_ast::Lit::Str(s)) => Some(s.value.as_bytes().to_vec()),
                    _ => None,
                };
                if let Some(bytes) = lit_bytes {
                    let (ptr, len) = ctx.emit_str_literal(&bytes)?;
                    values.push(ptr);
                    values.push(len);
                    continue;
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                match tv.ty {
                    ValTy::Handle => {
                        let ptr_fref = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_PTR",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let len_fref = ctx.get_extern(
                            "__RTS_FN_NS_GC_STRING_LEN",
                            &[cl::I64],
                            Some(cl::I64),
                        )?;
                        let pi = ctx.builder.ins().call(ptr_fref, &[tv.val]);
                        let ptr = ctx.builder.inst_results(pi)[0];
                        let li = ctx.builder.ins().call(len_fref, &[tv.val]);
                        let len = ctx.builder.inst_results(li)[0];
                        values.push(ptr);
                        values.push(len);
                    }
                    _ => return Err(anyhow!("StrPtr argument must be a string value")),
                }
            }
            AbiType::I32 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i32(tv).val)
            }
            AbiType::U64 | AbiType::I64 | AbiType::Handle | AbiType::Bool => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val)
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(to_f64(ctx, tv))
            }
            AbiType::Void => {}
        }
    }

    let inst = ctx.builder.ins().call(fref, &values);
    if lowered_ret.is_some() {
        let v = ctx.builder.inst_results(inst)[0];
        let tv = TypedVal::new(v, ValTy::from_abi(member.returns));
        // (cross-runtime #248) parallel.for_each = arr.forEach: JS spec
        // retorna undefined. Marca como I64 ambiguo para que template
        // literal use TPL_COERCE_AUTO e exiba "undefined".
        // (Nao alteramos a sentinela ainda — caller que importa.)
        Ok(tv)
    } else {
        // (cross-runtime #248) Void calls -- algumas correspondem a
        // forEach (JS spec: returns undefined). Emite sentinela
        // MIN+2 (undefined) que TPL_COERCE_AUTO traduz quando
        // usado em concat.
        let is_foreach_like = matches!(
            (member.symbol, "__RTS_FN_NS_PARALLEL_FOR_EACH"),
            (a, b) if a == b
        );
        let val = if is_foreach_like { i64::MIN + 2 } else { 0 };
        let v = ctx.builder.ins().iconst(cl::I64, val);
        let tv = TypedVal::new(v, ValTy::I64);
        if is_foreach_like {
            ctx.var_member_call_values.insert(v);
        }
        Ok(tv)
    }
}

/// Emits a call to a global class instance method (e.g. `d.getFullYear()`).
/// `recv` is the already-lowered Handle value. The InstanceMethod ABI has the
/// Handle as its first arg (slot 0 of member.args), so we prepend it and pass
/// the remaining TS args in order.
pub(super) fn lower_global_instance_call(
    ctx: &mut FnCtx,
    member: &'static crate::abi::member::NamespaceMember,
    recv: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<TypedVal> {
    use crate::abi::signature::lower_member;

    let sig = lower_member(member);
    let fn_ref = ctx.get_extern_abi(member.symbol, &sig.params, sig.ret)?;

    // slot 0 = Handle receiver; slots 1.. = TS call args
    let mut values = vec![recv];
    let abi_args = &member.args[1..]; // skip Handle slot
    let mut arg_iter = call.args.iter();
    for &abi_ty in abi_args {
        let arg = arg_iter
            .next()
            .ok_or_else(|| anyhow!("too few arguments for `{}`", member.name))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in global class method call"));
        }
        match abi_ty {
            AbiType::StrPtr => {
                let tv = lower_expr(ctx, &arg.expr)?;
                let h = ctx.coerce_to_i64(tv).val;
                let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let pi = ctx.builder.ins().call(ptr_fn, &[h]);
                values.push(ctx.builder.inst_results(pi)[0]);
                let li = ctx.builder.ins().call(len_fn, &[h]);
                values.push(ctx.builder.inst_results(li)[0]);
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(to_f64(ctx, tv));
            }
            AbiType::I32 => {
                // Arg I32 (ex: flag `littleEndian`): coerce explicito para i32
                // — passar i64 cru causa type mismatch no verifier Cranelift.
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i32(tv).val);
            }
            _ => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val);
            }
        }
    }

    let inst = ctx.builder.ins().call(fn_ref, &values);
    if sig.ret.is_some() {
        let v = ctx.builder.inst_results(inst)[0];
        Ok(TypedVal::new(v, ValTy::from_abi(member.returns)))
    } else {
        Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64))
    }
}


pub(crate) fn emit_namespace_constant(
    ctx: &mut FnCtx,
    qualified: &str,
) -> Result<Option<TypedVal>> {
    let Some((_spec, member)) = lookup(qualified) else {
        return Ok(None);
    };
    if !matches!(member.kind, crate::abi::MemberKind::Constant) {
        return Err(anyhow!(
            "`{qualified}` is a function, not a constant — use `{qualified}(...)`"
        ));
    }
    Ok(Some(emit_constant_load(ctx, member)?))
}
