use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{Expr, Lit, Tpl, UnaryOp};

use super::lower_expr;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};

pub(super) fn lower_lit(ctx: &mut FnCtx, lit: &Lit) -> Result<TypedVal> {
    match lit {
        Lit::Num(n) => {
            let v = n.value;
            // Se o source escreveu \`1.0\` ou \`1e3\` (com . ou expoente),
            // mantemos F64 mesmo que matematicamente seja inteiro. Isso
            // evita que codigo \`x <= 1.0\` em loop quente faca
            // iconst.i32 + fcvt_from_sint.f64 toda iter — basta
            // f64const direto. Cranelift egraph nao hoist esse fcvt
            // mesmo quando trivialmente loop-invariant.
            let wrote_as_float = n
                .raw
                .as_ref()
                .map(|r| {
                    let s = r.as_bytes();
                    s.iter().any(|&b| b == b'.' || b == b'e' || b == b'E')
                })
                .unwrap_or(false);
            // Cache key: (bits, is_float, block) — per-block para evitar
            // usar Values de blocos não-dominadores (mesmo bug do str_handle_cache).
            let cur_block = ctx.builder.current_block().unwrap_or_else(|| {
                cranelift_codegen::ir::Block::with_number(0).unwrap()
            });
            let cache_key = (v.to_bits(), wrote_as_float as u8, cur_block);
            if let Some(&cached) = ctx.num_val_cache.get(&cache_key) {
                let ty = if !wrote_as_float && v.fract() == 0.0 && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                    ValTy::I32
                } else if !wrote_as_float && v.fract() == 0.0 && v.is_finite()
                    && v >= i64::MIN as f64 && v <= i64::MAX as f64
                {
                    ValTy::I64
                } else {
                    ValTy::F64
                };
                return Ok(TypedVal::new(cached, ty));
            }
            // (cross-runtime) Literal inteiro so' vira iconst se CABE no range
            // do tipo. `1e21` (`v.fract()==0` e finito) NAO cabe em i64 — sem
            // o range check, `v as i64` saturava em i64::MAX (9223372036854775807)
            // em vez de manter o valor como f64 (JS: number sempre f64).
            let tv = if !wrote_as_float
                && v.fract() == 0.0
                && v >= i32::MIN as f64
                && v <= i32::MAX as f64
            {
                TypedVal::new(ctx.builder.ins().iconst(cl::I32, v as i64), ValTy::I32)
            } else if !wrote_as_float && v.fract() == 0.0 && v.is_finite()
                && v >= i64::MIN as f64 && v <= i64::MAX as f64
            {
                TypedVal::new(ctx.builder.ins().iconst(cl::I64, v as i64), ValTy::I64)
            } else {
                TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64)
            };
            ctx.num_val_cache.insert(cache_key, tv.val);
            Ok(tv)
        }
        Lit::Bool(b) => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, i64::from(b.value)),
            ValTy::Bool,
        )),
        Lit::Null(_) => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::Handle,
        )),
        Lit::Str(s) => {
            let tv = ctx.emit_str_handle(s.value.as_bytes())?;
            Ok(TypedVal::new(tv.val, ValTy::Handle))
        }
        // (cross-runtime #751) BigInt literals (1234n) — RTS nao tem BigInt
        // real, mas tratamos como i64 quando o valor cabe (perde precisao
        // acima de 2^63, mas suficiente pra fixtures com valores pequenos
        // como `1_234_567_890n`).
        Lit::BigInt(b) => {
            let s = b.value.to_string();
            // Parse decimal string como i64 (saturate em overflow).
            let v: i64 = s.parse().unwrap_or(i64::MAX);
            Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, v), ValTy::I64))
        }
        Lit::Regex(r) => {
            // /pattern/flags  →  regex.compile(pattern, flags)
            let pat_bytes = r.exp.as_bytes();
            let flag_bytes = r.flags.as_bytes();
            let (pp, pl) = ctx.emit_str_literal(pat_bytes)?;
            let (fp, fl) = ctx.emit_str_literal(flag_bytes)?;
            let compile = ctx.get_extern(
                "__RTS_FN_NS_REGEX_COMPILE",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(compile, &[pp, pl, fp, fl]);
            Ok(TypedVal::new(
                ctx.builder.inst_results(inst)[0],
                ValTy::Handle,
            ))
        }
        other => Err(anyhow!("unsupported literal: {other:?}")),
    }
}

pub(super) fn lower_unary(ctx: &mut FnCtx, u: &swc_ecma_ast::UnaryExpr) -> Result<TypedVal> {
    if matches!(u.op, UnaryOp::TypeOf) {
        return lower_typeof(ctx, &u.arg);
    }
    if matches!(u.op, UnaryOp::Void) {
        // `void X` avalia X (side effects) e retorna undefined. Usamos
        // sentinel i64::MIN+2 para coincidir com a representacao undefined
        // em slots I64 (Array/Vec/Map) — assim `void 0 === undefined` da
        // true via fast path do binary op.
        let _ = lower_expr(ctx, &u.arg)?;
        use cranelift_codegen::ir::InstBuilder;
        let v = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
        return Ok(TypedVal::new(v, ValTy::I64));
    }
    // `delete obj.prop` / `delete obj["k"]` / `delete arr[i]`.
    if matches!(u.op, UnaryOp::Delete) {
        if let Expr::Member(m) = u.arg.as_ref() {
            let obj_tv = lower_expr(ctx, &m.obj)?;
            let obj_h = ctx.coerce_to_i64(obj_tv).val;
            // Caso 1: Member::Ident (obj.prop) — chave estatica.
            if let swc_ecma_ast::MemberProp::Ident(id) = &m.prop {
                let key_h = ctx.emit_str_handle(id.sym.as_bytes())?.val;
                let f = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_DELETE_AUTO",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, key_h]);
                let v = ctx.builder.inst_results(inst)[0];
                let v8 = ctx.builder.ins().ireduce(cl::I8, v);
                return Ok(TypedVal::new(v8, ValTy::Bool));
            }
            // Caso 2: Member::Computed (arr[i] ou obj[k]) — chave dinamica.
            if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                let idx_tv = lower_expr(ctx, &c.expr)?;
                let f = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_INDEX_DELETE_AUTO",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let idx_i = ctx.coerce_to_i64(idx_tv).val;
                let inst = ctx.builder.ins().call(f, &[obj_h, idx_i]);
                let v = ctx.builder.inst_results(inst)[0];
                let v8 = ctx.builder.ins().ireduce(cl::I8, v);
                return Ok(TypedVal::new(v8, ValTy::Bool));
            }
        }
        // Fallback: avalia e retorna true (sem efeito).
        let _ = lower_expr(ctx, &u.arg)?;
        return Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I8, 1),
            ValTy::Bool,
        ));
    }

    // (#872) JS `-0` precisa preservar o sinal — IEEE 754 distingue +0 e -0,
    // mas `ineg(iconst 0) = 0` perde isso. Quando o operando eh literal
    // `Num(0)` que viraria I32/I64, emite f64const(-0.0) direto.
    if matches!(u.op, UnaryOp::Minus) {
        if let Expr::Lit(Lit::Num(n)) = u.arg.as_ref() {
            if n.value == 0.0 {
                let v = ctx.builder.ins().f64const(-0.0_f64);
                return Ok(TypedVal::new(v, ValTy::F64));
            }
        }
    }

    let operand = lower_expr(ctx, &u.arg)?;
    match u.op {
        UnaryOp::Minus => match operand.ty {
            ValTy::F64 => Ok(TypedVal::new(
                ctx.builder.ins().fneg(operand.val),
                ValTy::F64,
            )),
            ValTy::I32 => Ok(TypedVal::new(
                ctx.builder.ins().ineg(operand.val),
                ValTy::I32,
            )),
            _ => {
                let operand_i64 = ctx.coerce_to_i64(operand).val;
                Ok(TypedVal::new(
                    ctx.builder.ins().ineg(operand_i64),
                    ValTy::I64,
                ))
            }
        },
        UnaryOp::Plus => {
            // (cross-runtime #1069) JS spec: unary `+` faz ToNumber.
            // Sentinels: true=1, false=0, null=0, undefined=NaN.
            // Handle string: parse numerico via NUM_COERCE.
            // I64/F64/I32: passthrough (ja sao numbers).
            match operand.ty {
                ValTy::Handle => {
                    let coerce_fn = ctx.get_extern(
                        "__RTS_FN_RT_TO_NUMBER",
                        &[cl::I64],
                        Some(cl::F64),
                    )?;
                    let inst = ctx.builder.ins().call(coerce_fn, &[operand.val]);
                    let v = ctx.builder.inst_results(inst)[0];
                    Ok(TypedVal::new(v, ValTy::F64))
                }
                ValTy::Bool => {
                    // Bool i64 0/1 ja eh numero.
                    Ok(TypedVal::new(operand.val, ValTy::I64))
                }
                ValTy::I64 | ValTy::U64 => {
                    // Pode ser sentinel (null/undefined). Routine via TO_NUMBER.
                    let coerce_fn = ctx.get_extern(
                        "__RTS_FN_RT_TO_NUMBER",
                        &[cl::I64],
                        Some(cl::F64),
                    )?;
                    let inst = ctx.builder.ins().call(coerce_fn, &[operand.val]);
                    let v = ctx.builder.inst_results(inst)[0];
                    Ok(TypedVal::new(v, ValTy::F64))
                }
                _ => Ok(operand),
            }
        }
        UnaryOp::Bang => {
            use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
            // (#550) Handle: !handle === true quando handle == 0 OU handle
            // aponta para Entry::String vazia. Caso contrario falsy.
            // Esquema:
            //   handle == 0      -> falsy   -> !x = true
            //   string_len > 0   -> truthy  -> !x = false
            //   string_len == 0  -> falsy   -> !x = true
            //   string_len < 0 (nao-string handle) -> truthy -> !x = false
            if matches!(operand.ty, ValTy::Handle) {
                let str_len = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                let is_zero_h = ctx.builder.ins().icmp(IntCC::Equal, operand.val, zero);
                let inst = ctx.builder.ins().call(str_len, &[operand.val]);
                let len = ctx.builder.inst_results(inst)[0];
                let is_empty_str = ctx.builder.ins().icmp(IntCC::Equal, len, zero);
                let falsy = ctx.builder.ins().bor(is_zero_h, is_empty_str);
                return Ok(TypedVal::new(
                    ctx.builder.ins().uextend(cl::I64, falsy),
                    ValTy::Bool,
                ));
            }
            // F64: NaN tambem e' falsy. !x = (x == 0 OR x eh NaN).
            // Usa fcmp Equal(x, 0) | Equal(x, x)==false. Como Equal eh
            // ordered (false p/ NaN), nao usamos NotEqual nem
            // OrderedNotEqual (panic em aarch64 Cranelift 0.131).
            if matches!(operand.ty, ValTy::F64) {
                let zero = ctx.builder.ins().f64const(0.0);
                // is_zero = x == 0
                let is_zero = ctx.builder.ins().fcmp(FloatCC::Equal, operand.val, zero);
                // is_self_eq = x == x (false sse NaN)
                let is_self_eq = ctx.builder.ins().fcmp(FloatCC::Equal, operand.val, operand.val);
                let is_nan_i = {
                    let i = ctx.builder.ins().uextend(cl::I64, is_self_eq);
                    let one = ctx.builder.ins().iconst(cl::I64, 1);
                    ctx.builder.ins().bxor(i, one)
                };
                let is_zero_i = ctx.builder.ins().uextend(cl::I64, is_zero);
                let falsy = ctx.builder.ins().bor(is_zero_i, is_nan_i);
                return Ok(TypedVal::new(falsy, ValTy::Bool));
            }
            let value = ctx.coerce_to_i64(operand).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_zero =
                ctx.builder
                    .ins()
                    .icmp(IntCC::Equal, value, zero);
            // (cross-runtime #368) Sentinels falsy: bool false (i64::MIN) e
            // undefined (i64::MIN+2). `!falseField` deve dar true quando o
            // campo bool foi lido como i64 ambiguo (sem field type). Espelha
            // to_branch_cond. BOOL_TRUE (i64::MIN+1) segue truthy.
            let bool_false = ctx.builder.ins().iconst(cl::I64, i64::MIN);
            let is_false_sentinel =
                ctx.builder.ins().icmp(IntCC::Equal, value, bool_false);
            let undef = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
            let is_undef =
                ctx.builder.ins().icmp(IntCC::Equal, value, undef);
            let falsy = ctx.builder.ins().bor(is_zero, is_false_sentinel);
            let falsy = ctx.builder.ins().bor(falsy, is_undef);
            Ok(TypedVal::new(
                ctx.builder.ins().uextend(cl::I64, falsy),
                ValTy::Bool,
            ))
        }
        UnaryOp::Tilde => {
            let operand_i64 = ctx.coerce_to_i64(operand).val;
            Ok(TypedVal::new(
                ctx.builder.ins().bnot(operand_i64),
                ValTy::I64,
            ))
        }
        UnaryOp::Delete => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 1),
            ValTy::Bool,
        )),
        UnaryOp::Void | UnaryOp::TypeOf => unreachable!(),
    }
}

fn lower_typeof(ctx: &mut FnCtx, operand: &Expr) -> Result<TypedVal> {
    // Resolucao AST-level cobre os casos JS antes de tentar lowering:
    // - undeclaredVar -> "undefined" (sem ReferenceError)
    // - null literal  -> "object" (quirk JS)
    // - object/array literal -> "object"
    // - fn/arrow expr ou ident de user fn -> "function"
    // - Symbol(...) call -> "symbol"
    if let Expr::Ident(id) = operand {
        let name = id.sym.as_str();
        if ctx.user_fns.contains_key(name) {
            return ctx.emit_str_handle(b"function");
        }
        // Classes (user e globais) sao "function" em JS (typeof Class === 'function').
        if ctx.classes.contains_key(name)
            || crate::abi::global_class_lookup(name).is_some()
        {
            return ctx.emit_str_handle(b"function");
        }
        let is_js_global = matches!(name, "NaN" | "Infinity" | "undefined");
        // globalThis e' "object" em JS.
        if name == "globalThis" {
            return ctx.emit_str_handle(b"object");
        }
        // (cross-runtime #1079) Builtins sem GlobalClassSpec dedicado mas
        // referenciaveis: Array/Object/Map/Set/Proxy/... -> "function";
        // Math/JSON/Reflect/Atomics/Intl -> "object".
        if matches!(name, "Array" | "Object" | "Map" | "Set" | "Proxy") {
            return ctx.emit_str_handle(b"function");
        }
        if matches!(name, "Math" | "JSON" | "Reflect" | "Atomics" | "Intl") {
            return ctx.emit_str_handle(b"object");
        }
        if !is_js_global && ctx.read_local(name).is_none() {
            return ctx.emit_str_handle(b"undefined");
        }
        if name == "undefined" {
            return ctx.emit_str_handle(b"undefined");
        }
    }
    if let Expr::Lit(Lit::Null(_)) = operand {
        return ctx.emit_str_handle(b"object");
    }
    // `typeof void X` -> "undefined" (X eh avaliado por side effects).
    if let Expr::Unary(u) = operand {
        if matches!(u.op, UnaryOp::Void) {
            let _ = lower_expr(ctx, &u.arg)?;
            return ctx.emit_str_handle(b"undefined");
        }
    }
    if matches!(operand, Expr::Object(_) | Expr::Array(_)) {
        return ctx.emit_str_handle(b"object");
    }
    if matches!(operand, Expr::Fn(_) | Expr::Arrow(_)) {
        return ctx.emit_str_handle(b"function");
    }
    if let Expr::Call(c) = operand {
        if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
            if let Expr::Ident(id) = callee.as_ref() {
                if id.sym.as_ref() == "Symbol" {
                    return ctx.emit_str_handle(b"symbol");
                }
            }
        }
    }
    // (cross-runtime #1079) `typeof globalThis.X` — classifica X como se fosse
    // ident solo. Cobre globalThis.Math/JSON/Promise/Array/parseInt/isNaN/etc.
    if let Expr::Member(m) = operand {
        if let Expr::Ident(obj_id) = m.obj.as_ref() {
            if obj_id.sym.as_str() == "globalThis" {
                if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
                    let n = prop.sym.as_str();
                    // Classes globais sem GlobalClassSpec dedicado -> "function"
                    if matches!(n, "Array" | "Object" | "Map" | "Set" | "Proxy") {
                        return ctx.emit_str_handle(b"function");
                    }
                    // Classes globais (Promise/Error/Symbol/WeakMap/etc) via spec -> "function"
                    if crate::abi::global_class_lookup(n).is_some() {
                        return ctx.emit_str_handle(b"function");
                    }
                    // Namespaces objeto -> "object"
                    if matches!(n,
                        "Math" | "JSON" | "Reflect" | "console" | "performance"
                        | "globalThis" | "Atomics" | "Intl"
                    ) {
                        return ctx.emit_str_handle(b"object");
                    }
                    // Funcoes globais soltas
                    if matches!(n,
                        "parseInt" | "parseFloat" | "isNaN" | "isFinite"
                        | "encodeURIComponent" | "decodeURIComponent"
                        | "encodeURI" | "decodeURI"
                        | "atob" | "btoa" | "structuredClone"
                        | "fetch" | "setTimeout" | "setInterval"
                        | "clearTimeout" | "clearInterval" | "queueMicrotask"
                    ) {
                        return ctx.emit_str_handle(b"function");
                    }
                    // Valores especiais
                    if matches!(n, "NaN" | "Infinity") {
                        return ctx.emit_str_handle(b"number");
                    }
                    if n == "undefined" {
                        return ctx.emit_str_handle(b"undefined");
                    }
                }
            }
        }
        if let Some(qualified) = crate::codegen::lower::expressions::members::qualified_member_name(operand) {
            let target: String = if let Some(prop) = qualified.strip_prefix("Math.") {
                format!("math.{prop}")
            } else {
                qualified.clone()
            };
            if let Some((_, member)) = crate::abi::lookup(&target) {
                return match member.kind {
                    crate::abi::MemberKind::Constant => ctx.emit_str_handle(b"number"),
                    _ => ctx.emit_str_handle(b"function"),
                };
            }
        }
        // (#685/308) `typeof obj.method` quando obj eh instancia de
        // classe global (ex: WeakRef/FinalizationRegistry/RegExp/Date)
        // e method existe como InstanceMethod -> "function".
        if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
            let key = prop.sym.as_str();
            if let Expr::Ident(obj_id) = m.obj.as_ref() {
                let obj_name = obj_id.sym.as_str();
                if let Some(cls) = ctx.local_class_ty.get(obj_name) {
                    if let Some(spec) = crate::abi::global_class_lookup(cls) {
                        if spec.instance_method(key).is_some() {
                            return ctx.emit_str_handle(b"function");
                        }
                    }
                }
            }
        }
    }
    let tv = lower_expr(ctx, operand)?;
    // Handle pode ser string OU symbol/function/object/etc. Em vez de
    // assumir "string" estaticamente, despacha pra runtime helper que
    // inspeciona Entry e retorna o tipo JS correto.
    // (cross-runtime #110) I64/U64 ambiguo (var_member_call_values —
    // arr[i], map.get(k), descs.b.get, etc.) tambem despacha runtime
    // pois pode conter handle Function/Symbol/etc.
    let is_ambig_handle = matches!(tv.ty, ValTy::I64 | ValTy::U64)
        && ctx.var_member_call_values.contains(&tv.val);
    // (cross-runtime #293) ValTy::U64 vem de fns que retornam handle opaco
    // (JSON.parse, etc.) — sempre dispatch runtime pra detectar tipo do
    // Entry (Map → "object", String → "string", Vec → "object", etc.).
    let is_u64_handle = matches!(tv.ty, ValTy::U64);
    if matches!(tv.ty, ValTy::Handle) || is_ambig_handle || is_u64_handle {
        use cranelift_codegen::ir::InstBuilder;
        let typeof_fn = ctx.get_extern(
            "__RTS_FN_RT_TYPEOF_HANDLE",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let h = ctx.coerce_to_i64(tv).val;
        let inst = ctx.builder.ins().call(typeof_fn, &[h]);
        let r = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(r, ValTy::Handle));
    }
    let ty_str = match tv.ty {
        ValTy::Bool => "boolean",
        ValTy::Handle => "string", // unreachable but exhaustive
        ValTy::F64 | ValTy::I32 | ValTy::I64 | ValTy::U64
        | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => "number",
    };
    ctx.emit_str_handle(ty_str.as_bytes())
}

pub(super) fn lower_tpl(ctx: &mut FnCtx, tpl: &Tpl) -> Result<TypedVal> {
    fn cook(q: &swc_ecma_ast::TplElement) -> Vec<u8> {
        q.cooked
            .as_ref()
            .map(|v| v.to_string_lossy().into_owned().into_bytes())
            .unwrap_or_default()
    }

    let first = tpl
        .quasis
        .first()
        .ok_or_else(|| anyhow!("template literal sem quasi inicial"))?;
    let mut acc = ctx.emit_str_handle(&cook(first))?;

    for (expr, quasi) in tpl.exprs.iter().zip(tpl.quasis.iter().skip(1)) {
        let val = lower_expr(ctx, expr)?;
        let concat = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        // (#432) Se o val veio de optional chain, decide em runtime:
        // val == 0 → handle de "undefined"; senao coerce normal.
        let is_opt_chain = ctx.optional_chain_values.contains(&val.val);
        // (#proto-method) Se veio de var_member_call, use coerce auto que
        // detecta string handle em runtime.
        let is_var_member_call = ctx.var_member_call_values.contains(&val.val);
        let val_ty = val.ty;
        let rhs = if is_opt_chain {
            let undef_h = ctx.emit_str_handle(b"undefined")?.val;
            let val_i64 = ctx.coerce_to_i64(val).val;
            let normal_h = ctx.coerce_to_handle(val)?.val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, val_i64, zero);
            ctx.builder.ins().select(is_null, undef_h, normal_h)
        } else if ctx.var_vec_slot_values.contains(&val.val) {
            // (#45/#94) Slot Vec: value=0 eh literal `0`, nao null.
            let val_i64 = ctx.coerce_to_i64(val).val;
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_VEC_SLOT",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[val_i64]);
            ctx.builder.inst_results(inst)[0]
        } else if is_var_member_call || matches!(val_ty, ValTy::Handle | ValTy::U64) {
            // (#573) Handle ambiguo (string/numero embutido, ou U64 que pode
            // ser handle valido OU i64 raw como JSON.parse('42')) usa
            // COERCE_AUTO que decide em runtime.
            let val_i64 = ctx.coerce_to_i64(val).val;
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_AUTO",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[val_i64]);
            ctx.builder.inst_results(inst)[0]
        } else {
            ctx.coerce_to_handle(val)?.val
        };
        let inst = ctx.builder.ins().call(concat, &[acc.val, rhs]);
        let r = ctx.builder.inst_results(inst)[0];
        ctx.register_temp_handle(r);
        acc = TypedVal::new(r, ValTy::Handle);

        let bytes = cook(quasi);
        if !bytes.is_empty() {
            let qh = ctx.emit_str_handle(&bytes)?;
            let inst = ctx.builder.ins().call(concat, &[acc.val, qh.val]);
            let r = ctx.builder.inst_results(inst)[0];
            ctx.register_temp_handle(r);
            acc = TypedVal::new(r, ValTy::Handle);
        }
    }

    Ok(acc)
}
