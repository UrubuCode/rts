//! Expression lowering to Cranelift IR.
//!
//! Entry point: `lower_expr` — recursively compiles a SWC expression into a
//! `TypedVal`. Handles literals, identifiers, binary ops, unary ops, and
//! namespace calls. String concatenation (`+` with a string operand) is
//! lowered to `__RTS_FN_NS_GC_STRING_CONCAT`.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{
    BinExpr, BinaryOp, CallExpr, Callee, Expr, Lit, MemberProp, Tpl, UnaryOp, UpdateOp,
};

use cranelift_module::Module;

use crate::abi::lookup;
use crate::abi::signature::lower_member;
use crate::abi::types::AbiType;

use super::ctx::{FnCtx, TypedVal, ValTy};

/// Compiles a SWC expression and returns a typed Cranelift value.
pub fn lower_expr(ctx: &mut FnCtx, expr: &Expr) -> Result<TypedVal> {
    match expr {
        // ── Literals ──────────────────────────────────────────────────────
        Expr::Lit(lit) => lower_lit(ctx, lit),

        // ── Identifiers ───────────────────────────────────────────────────
        Expr::Ident(id) => {
            let name = id.sym.as_str();
            // Locals and globals take priority. A user function with the
            // same name is shadowed — matches JS scoping.
            if let Some(tv) = ctx.read_local(name) {
                return Ok(tv);
            }
            // Fallback: bare reference to a user-defined function resolves
            // to its function pointer as an i64 value. Lets callers pass
            // functions around as first-class values and invoke them via
            // `call_indirect` (#97, fase 1).
            if ctx.user_fns.contains_key(name) {
                return emit_user_fn_addr(ctx, name);
            }
            Err(anyhow!("undefined variable `{name}`"))
        }

        // ── Parenthesised ─────────────────────────────────────────────────
        Expr::Paren(p) => lower_expr(ctx, &p.expr),

        // ── Unary ─────────────────────────────────────────────────────────
        Expr::Unary(u) => lower_unary(ctx, u),

        // ── Update (++, --) ───────────────────────────────────────────────
        Expr::Update(u) => {
            let name = ident_name(&u.arg)
                .ok_or_else(|| anyhow!("update target must be a simple identifier"))?;
            let cur = ctx
                .read_local(name)
                .ok_or_else(|| anyhow!("undefined variable `{name}`"))?;
            let one = match cur.ty {
                ValTy::I32 => TypedVal::new(ctx.builder.ins().iconst(cl::I32, 1), ValTy::I32),
                _ => TypedVal::new(ctx.builder.ins().iconst(cl::I64, 1), ValTy::I64),
            };
            let new_val = match u.op {
                UpdateOp::PlusPlus => lower_add(ctx, cur, one)?,
                UpdateOp::MinusMinus => lower_sub(ctx, cur, one)?,
            };
            ctx.write_local(name, new_val.val)?;
            // Prefix: return new value; postfix: return old value.
            if u.prefix { Ok(new_val) } else { Ok(cur) }
        }

        // ── Binary ────────────────────────────────────────────────────────
        Expr::Bin(bin) => lower_bin(ctx, bin),

        // ── Assignment ────────────────────────────────────────────────────
        Expr::Assign(a) => {
            use swc_ecma_ast::{AssignOp, AssignTarget};

            // Member assignment: obj.field = v / obj[key] = v
            // Suportado apenas para `=` simples (sem compound) no MVP.
            if let AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) = &a.left {
                if !matches!(a.op, AssignOp::Assign) {
                    return Err(anyhow!("compound assign em member access nao suportado"));
                }
                let rhs = lower_expr(ctx, &a.right)?;
                let rhs_i64 = ctx.coerce_to_i64(rhs).val;
                let obj_tv = lower_expr(ctx, &m.obj)?;
                let obj_h = ctx.coerce_to_i64(obj_tv).val;
                let set_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                    &[cl::I64, cl::I64, cl::I64, cl::I64],
                    None,
                )?;
                match &m.prop {
                    MemberProp::Ident(id) => {
                        let (kp, kl) = ctx.emit_str_literal(id.sym.as_bytes())?;
                        ctx.builder.ins().call(set_fn, &[obj_h, kp, kl, rhs_i64]);
                    }
                    MemberProp::Computed(c) => {
                        if let Expr::Lit(Lit::Str(s)) = c.expr.as_ref() {
                            let (kp, kl) = ctx.emit_str_literal(s.value.as_bytes())?;
                            ctx.builder.ins().call(set_fn, &[obj_h, kp, kl, rhs_i64]);
                        } else {
                            // indice numerico → vec_set
                            let idx_tv = lower_expr(ctx, &c.expr)?;
                            let idx = ctx.coerce_to_i64(idx_tv).val;
                            let vec_set = ctx.get_extern(
                                "__RTS_FN_NS_COLLECTIONS_VEC_SET",
                                &[cl::I64, cl::I64, cl::I64],
                                None,
                            )?;
                            ctx.builder.ins().call(vec_set, &[obj_h, idx, rhs_i64]);
                        }
                    }
                    MemberProp::PrivateName(_) => {
                        return Err(anyhow!("atribuicao a private name nao suportada"));
                    }
                }
                return Ok(TypedVal::new(rhs_i64, ValTy::I64));
            }

            let name = match &a.left {
                AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) => {
                    id.sym.as_str().to_string()
                }
                _ => return Err(anyhow!("only simple identifier assignment is supported")),
            };

            // Compound assignments (`x += y`, etc) — desugar para o
            // binop equivalente sobre `x` e a rhs, depois atribuir.
            // Logical-compound (`&&=`, `||=`, `??=`) ficam como
            // follow-up porque exigem short-circuit.
            let rhs_val = if matches!(a.op, AssignOp::Assign) {
                lower_expr(ctx, &a.right)?
            } else {
                let binop = match a.op {
                    AssignOp::AddAssign => BinaryOp::Add,
                    AssignOp::SubAssign => BinaryOp::Sub,
                    AssignOp::MulAssign => BinaryOp::Mul,
                    AssignOp::DivAssign => BinaryOp::Div,
                    AssignOp::ModAssign => BinaryOp::Mod,
                    AssignOp::LShiftAssign => BinaryOp::LShift,
                    AssignOp::RShiftAssign => BinaryOp::RShift,
                    AssignOp::ZeroFillRShiftAssign => BinaryOp::ZeroFillRShift,
                    AssignOp::BitOrAssign => BinaryOp::BitOr,
                    AssignOp::BitXorAssign => BinaryOp::BitXor,
                    AssignOp::BitAndAssign => BinaryOp::BitAnd,
                    AssignOp::ExpAssign => BinaryOp::Exp,
                    AssignOp::AndAssign
                    | AssignOp::OrAssign
                    | AssignOp::NullishAssign => {
                        return Err(anyhow!(
                            "logical compound assignment ({:?}) not supported yet",
                            a.op
                        ));
                    }
                    AssignOp::Assign => unreachable!(),
                };
                // Build synthetic `x op rhs` expression and lower via
                // the usual binary path. Keeps all the type promotion
                // and intrinsic logic intact.
                let synthetic_left = Expr::Ident(swc_ecma_ast::Ident {
                    span: a.span,
                    ctxt: Default::default(),
                    sym: name.as_str().into(),
                    optional: false,
                });
                let bin = BinExpr {
                    span: a.span,
                    op: binop,
                    left: Box::new(synthetic_left),
                    right: a.right.clone(),
                };
                lower_bin(ctx, &bin)?
            };

            // Coerce to the declared type of the local.
            let coerced = match ctx.var_ty(&name) {
                Some(ValTy::I32) => ctx.coerce_to_i32(rhs_val),
                Some(ValTy::I64) => ctx.coerce_to_i64(rhs_val),
                Some(ValTy::Handle) => ctx.coerce_to_handle(rhs_val)?,
                _ => rhs_val,
            };
            ctx.write_local(&name, coerced.val)?;
            Ok(coerced)
        }

        // ── Call ──────────────────────────────────────────────────────────
        Expr::Call(call) => lower_call(ctx, call),

        // ── Template literal ──────────────────────────────────────────────
        Expr::Tpl(tpl) => lower_tpl(ctx, tpl),

        // ── Ternary (a ? b : c) ───────────────────────────────────────────
        Expr::Cond(cond) => lower_cond(ctx, cond),

        // ── Array literal ─────────────────────────────────────────────────
        // `[a, b, c]` desugar em `vec_new` + `vec_push` por elemento.
        // Holes (sparse: `[1,,3]`) viram 0. Spread nao suportado no MVP.
        Expr::Array(arr) => lower_array_lit(ctx, arr),

        // ── Object literal ────────────────────────────────────────────────
        // `{k: v, x}` desugar em `map_new` + `map_set` por entrada. Apenas
        // chaves Ident/Str sao aceitas no MVP (sem computed/method/spread).
        Expr::Object(obj) => lower_object_lit(ctx, obj),

        // ── Member ────────────────────────────────────────────────────────
        // Tres casos:
        // 1. Namespace constant (math.PI): emit constant load.
        // 2. Computed access em runtime handle: obj[expr] → vec_get/map_get.
        // 3. Static access em runtime handle: obj.foo → map_get(obj, "foo").
        Expr::Member(m) => lower_member_expr(ctx, m),

        // ── Optional chain (a?.b, a?.()) ─────────────────────────────────
        // Member access (`a?.b`) precisa de objects (#53) — rejeita com
        // mensagem clara. Optional call (`fn?.()`) funciona porque
        // callee resolve a i64 funcptr; basta checar se e 0 e pular.
        Expr::OptChain(opt) => lower_opt_chain(ctx, opt),

        // ── this ─────────────────────────────────────────────────────────
        // `this` em metodo/constructor de classe resolve para o primeiro
        // parametro implicito (handle do receiver). Fora de classes,
        // erro.
        Expr::This(_) => {
            if ctx.current_class.is_none() {
                return Err(anyhow!("`this` so e valido dentro de metodo/constructor de classe"));
            }
            ctx.read_local("this")
                .ok_or_else(|| anyhow!("`this` nao disponivel no contexto atual"))
        }

        // ── new C(args) ──────────────────────────────────────────────────
        Expr::New(new_expr) => lower_new(ctx, new_expr),

        other => Err(anyhow!(
            "unsupported expression kind: {}",
            expr_kind_name(other)
        )),
    }
}

// ── Literals ──────────────────────────────────────────────────────────────

fn lower_lit(ctx: &mut FnCtx, lit: &Lit) -> Result<TypedVal> {
    match lit {
        Lit::Num(n) => {
            let v = n.value;
            // If the source written form carries a decimal point or exponent,
            // treat the literal as f64 even when the value happens to be
            // integral. Without this, `1.0` would silently become i32 and
            // poison divisions like `1.0 / 5.0`.
            let wrote_as_float = n
                .raw
                .as_ref()
                .map(|r| {
                    let s = r.as_bytes();
                    s.iter().any(|&b| b == b'.' || b == b'e' || b == b'E')
                })
                .unwrap_or(false);

            if wrote_as_float || !v.is_finite() || v.fract() != 0.0 {
                Ok(TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64))
            } else if v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                // Default to I32 for integer literals that fit; codegen
                // coerces when the context demands I64.
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I32, v as i64),
                    ValTy::I32,
                ))
            } else {
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I64, v as i64),
                    ValTy::I64,
                ))
            }
        }
        Lit::Bool(b) => Ok(TypedVal::new(
            ctx.builder
                .ins()
                .iconst(cl::I64, if b.value { 1 } else { 0 }),
            ValTy::Bool,
        )),
        Lit::Str(s) => {
            let tv = ctx.emit_str_handle(s.value.as_bytes())?;
            Ok(tv)
        }
        Lit::Null(_) => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        )),
        other => Err(anyhow!("unsupported literal: {other:?}")),
    }
}

// ── Unary ─────────────────────────────────────────────────────────────────

fn lower_unary(ctx: &mut FnCtx, u: &swc_ecma_ast::UnaryExpr) -> Result<TypedVal> {
    // `typeof`, `void`, `delete` trocam semantica do operando — nao
    // avaliamos antes pra evitar side effects em `typeof ident` e
    // controlar como `void` descarta.
    match u.op {
        UnaryOp::TypeOf => return lower_typeof(ctx, &u.arg),
        UnaryOp::Void => {
            // Avalia pelo side effect, descarta resultado, retorna
            // sentinel 0 no lane I64 (RTS nao tem `undefined` real).
            let _ = lower_expr(ctx, &u.arg)?;
            return Ok(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 0),
                ValTy::I64,
            ));
        }
        UnaryOp::Delete => {
            // Non-strict JS: `delete x` em variavel nao-property e
            // no-op, retorno true. Sem property access ainda; basta
            // emitir true.
            return Ok(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 1),
                ValTy::Bool,
            ));
        }
        _ => {}
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
                let as_i64 = ctx.coerce_to_i64(operand);
                Ok(TypedVal::new(
                    ctx.builder.ins().ineg(as_i64.val),
                    ValTy::I64,
                ))
            }
        },
        UnaryOp::Bang => {
            let as_i64 = ctx.coerce_to_i64(operand);
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let cmp = ctx.builder.ins().icmp(IntCC::Equal, as_i64.val, zero);
            let ext = ctx.builder.ins().uextend(cl::I64, cmp);
            Ok(TypedVal::new(ext, ValTy::Bool))
        }
        UnaryOp::Plus => {
            // Numeric identity — coerce to i64 if needed
            Ok(ctx.coerce_to_i64(operand))
        }
        UnaryOp::Tilde => {
            let as_i64 = ctx.coerce_to_i64(operand);
            Ok(TypedVal::new(
                ctx.builder.ins().bnot(as_i64.val),
                ValTy::I64,
            ))
        }
        op => Err(anyhow!("unsupported unary op: {op:?}")),
    }
}

/// Emite uma string literal equivalente a `typeof <expr>` em JS, usando
/// o `ValTy` que o codegen consegue inferir localmente. E static/estatico
/// ao contrario do JS runtime-dispatched — suficiente para o grosso dos
/// casos (branches baseadas em tipo de variavel declarada). Operandos
/// que nao resolvem tipo estatico caem em `"number"` por default, que e
/// o que `typeof` retornaria para o lane I64 usado pelo RTS em runtime.
fn lower_typeof(ctx: &mut FnCtx, operand: &Expr) -> Result<TypedVal> {
    // Evalua pelo side effect (membro call, etc), descarta o valor.
    let tv = lower_expr(ctx, operand)?;
    let ty_str: &str = match tv.ty {
        ValTy::Bool => "boolean",
        ValTy::Handle => "string",
        ValTy::F64 | ValTy::I32 | ValTy::I64 => "number",
    };
    ctx.emit_str_handle(ty_str.as_bytes())
}

// ── Template literals ─────────────────────────────────────────────────────

/// Desugars a template literal into a chain of `gc::string_concat` calls.
///
/// `` `a${x}b${y}c` `` becomes `concat(concat(concat(concat("a", x), "b"), y), "c")`.
/// Each quasi cooked value is uploaded as a static string handle; each
/// interpolated expression is coerced to a handle via `coerce_to_handle`.
fn lower_tpl(ctx: &mut FnCtx, tpl: &Tpl) -> Result<TypedVal> {
    let cook = |e: &swc_ecma_ast::TplElement| -> Vec<u8> {
        if let Some(c) = &e.cooked {
            if let Some(s) = c.as_str() {
                return s.as_bytes().to_vec();
            }
        }
        e.raw.as_bytes().to_vec()
    };

    // Start from the first quasi (there is always at least one).
    let first = tpl
        .quasis
        .first()
        .ok_or_else(|| anyhow!("template literal has no quasis"))?;
    let mut acc = ctx.emit_str_handle(&cook(first))?;

    let fref = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_CONCAT",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    for (i, expr) in tpl.exprs.iter().enumerate() {
        // Interpolated expression → handle
        let val = lower_expr(ctx, expr)?;
        let h = ctx.coerce_to_handle(val)?;
        let inst = ctx.builder.ins().call(fref, &[acc.val, h.val]);
        let v = ctx.builder.inst_results(inst)[0];
        acc = TypedVal::new(v, ValTy::Handle);

        // Trailing quasi after this expression
        let q = tpl
            .quasis
            .get(i + 1)
            .ok_or_else(|| anyhow!("malformed template: missing quasi after expression"))?;
        let bytes = cook(q);
        if !bytes.is_empty() {
            let qh = ctx.emit_str_handle(&bytes)?;
            let inst = ctx.builder.ins().call(fref, &[acc.val, qh.val]);
            let v = ctx.builder.inst_results(inst)[0];
            acc = TypedVal::new(v, ValTy::Handle);
        }
    }

    Ok(acc)
}

// ── Binary ────────────────────────────────────────────────────────────────

/// Extracts a small integer literal (`i64` fits) from an expression,
/// including `-n` unary on a numeric literal. Returns `None` for anything
/// the imm form can't represent.
fn as_int_literal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Lit(Lit::Num(n)) => {
            let v = n.value;
            // Must be exact integer, finite, fits in i64.
            if v.fract() != 0.0 || !v.is_finite() {
                return None;
            }
            // Literals written with a decimal point are f64 — not an imm candidate.
            if let Some(raw) = n.raw.as_ref() {
                if raw.as_bytes().iter().any(|&b| b == b'.' || b == b'e' || b == b'E') {
                    return None;
                }
            }
            if v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                Some(v as i64)
            } else {
                None
            }
        }
        Expr::Unary(u) if matches!(u.op, UnaryOp::Minus) => {
            // Recurse: `-5` is a literal we can represent as imm.
            as_int_literal(&u.arg).and_then(|v| v.checked_neg())
        }
        _ => None,
    }
}

/// Attempts to lower `lhs OP literal` using Cranelift imm forms. Returns
/// `Ok(None)` if the op isn't supported by an imm variant or if the
/// operand shape doesn't match. Keeps the result ValTy aligned with the
/// lhs type so callers downstream see the same tag they would from the
/// materialised path.
fn try_bin_imm(ctx: &mut FnCtx, bin: &BinExpr) -> Result<Option<TypedVal>> {
    // Pick out a literal on either side; for commutative ops we still
    // evaluate lhs first to preserve TS evaluation order.
    let (const_side, var_side, imm_is_rhs) = match as_int_literal(&bin.right) {
        Some(v) => (v, bin.left.as_ref(), true),
        None => match as_int_literal(&bin.left) {
            // Non-commutative ops (sub/div/mod/shifts) cannot swap operands.
            Some(v)
                if matches!(
                    bin.op,
                    BinaryOp::Add | BinaryOp::Mul | BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
                ) =>
            {
                (v, bin.right.as_ref(), false)
            }
            _ => return Ok(None),
        },
    };

    let lhs = lower_expr(ctx, var_side)?;
    // Only operate on integer-ish lanes; floats would need fadd_imm which
    // Cranelift doesn't expose.
    if lhs.ty == ValTy::F64 {
        // Restore the original path by re-entering the generic code: caller
        // will repeat lower_expr on var_side which is cheap (constant fold-
        // friendly) but we'd rather avoid double-eval side effects. Since
        // `as_int_literal` only fires on pure literals, the side-effect of
        // re-lowering the literal side is nil. Signal "no imm" instead.
        return Ok(None);
    }
    let lhs_i64 = ctx.coerce_to_i64(lhs).val;

    let val = match bin.op {
        BinaryOp::Add => ctx.builder.ins().iadd_imm(lhs_i64, const_side),
        BinaryOp::Sub if imm_is_rhs => ctx.builder.ins().iadd_imm(lhs_i64, -const_side),
        BinaryOp::Mul => ctx.builder.ins().imul_imm(lhs_i64, const_side),
        BinaryOp::BitAnd => ctx.builder.ins().band_imm(lhs_i64, const_side),
        BinaryOp::BitOr => ctx.builder.ins().bor_imm(lhs_i64, const_side),
        BinaryOp::BitXor => ctx.builder.ins().bxor_imm(lhs_i64, const_side),
        BinaryOp::LShift if imm_is_rhs => ctx.builder.ins().ishl_imm(lhs_i64, const_side),
        BinaryOp::RShift if imm_is_rhs => ctx.builder.ins().sshr_imm(lhs_i64, const_side),
        BinaryOp::ZeroFillRShift if imm_is_rhs => ctx.builder.ins().ushr_imm(lhs_i64, const_side),
        _ => return Ok(None),
    };
    Ok(Some(TypedVal::new(val, ValTy::I64)))
}

fn lower_bin(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    // Short-circuit logical ops
    if matches!(
        bin.op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
    ) {
        return lower_logical(ctx, bin);
    }

    // Immediate-form peephole: when RHS is a small integer literal and the
    // op has an `_imm` variant, skip the `iconst` and call the imm form.
    // Limits to i64-backed integer ops; float / string paths fall through.
    if let Some(result) = try_bin_imm(ctx, bin)? {
        return Ok(result);
    }

    let lhs = lower_expr(ctx, &bin.left)?;
    let rhs = lower_expr(ctx, &bin.right)?;

    // String concat: if either side is a Handle, use string concat
    if matches!(bin.op, BinaryOp::Add) && (lhs.ty == ValTy::Handle || rhs.ty == ValTy::Handle) {
        let lh = ctx.coerce_to_handle(lhs)?;
        let rh = ctx.coerce_to_handle(rhs)?;
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[lh.val, rh.val]);
        let val = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(val, ValTy::Handle));
    }

    // String equality (#130): quando ambos os operandos sao Handle,
    // comparar por conteudo via __RTS_FN_NS_GC_STRING_EQ. Sem este
    // desvio, `==` compara handles u64 (sempre distintos para
    // interneds diferentes).
    if matches!(
        bin.op,
        BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
    ) && lhs.ty == ValTy::Handle
        && rhs.ty == ValTy::Handle
    {
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_EQ",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[lhs.val, rhs.val]);
        let eq = ctx.builder.inst_results(inst)[0];
        let result = if matches!(bin.op, BinaryOp::NotEq | BinaryOp::NotEqEq) {
            // Inverte: 1 -> 0, 0 -> 1.
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            ctx.builder.ins().bxor(eq, one)
        } else {
            eq
        };
        return Ok(TypedVal::new(result, ValTy::Bool));
    }

    // Numeric: promote to common type
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs);

    match bin.op {
        BinaryOp::Add => lower_add(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Sub => lower_sub(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Mul => lower_mul(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Div => lower_div(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),
        BinaryOp::Mod => lower_mod(ctx, TypedVal::new(lv, ty), TypedVal::new(rv, ty)),

        BinaryOp::EqEq | BinaryOp::EqEqEq => Ok(lower_icmp(
            ctx,
            IntCC::Equal,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::NotEq | BinaryOp::NotEqEq => Ok(lower_icmp(
            ctx,
            IntCC::NotEqual,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::Lt => Ok(lower_icmp(
            ctx,
            if ty == ValTy::F64 {
                IntCC::SignedLessThan
            } else {
                IntCC::SignedLessThan
            },
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::LtEq => Ok(lower_icmp(
            ctx,
            IntCC::SignedLessThanOrEqual,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::Gt => Ok(lower_icmp(
            ctx,
            IntCC::SignedGreaterThan,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),
        BinaryOp::GtEq => Ok(lower_icmp(
            ctx,
            IntCC::SignedGreaterThanOrEqual,
            TypedVal::new(lv, ty),
            TypedVal::new(rv, ty),
        )),

        // Bitwise — always operate on i64. JS spec truncates to i32 but the
        // rest of the codebase works in i64; matching existing conventions.
        BinaryOp::BitAnd => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().band(li, ri), ValTy::I64))
        }
        BinaryOp::BitOr => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().bor(li, ri), ValTy::I64))
        }
        BinaryOp::BitXor => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().bxor(li, ri), ValTy::I64))
        }
        BinaryOp::LShift => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().ishl(li, ri), ValTy::I64))
        }
        BinaryOp::RShift => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().sshr(li, ri), ValTy::I64))
        }
        BinaryOp::ZeroFillRShift => {
            let (li, ri) = (coerce_bits_i64(ctx, lv, ty), coerce_bits_i64(ctx, rv, ty));
            Ok(TypedVal::new(ctx.builder.ins().ushr(li, ri), ValTy::I64))
        }

        // `a ** b` — delega para `pow` de libc. Mesma instrucao que
        // Math.pow em JS. Se ambos operandos forem inteiros pequenos
        // o resultado ainda e f64 aqui; caller pode truncar via
        // anotacao `: i32` se quiser o valor inteiro.
        BinaryOp::Exp => {
            let lf = to_f64(ctx, TypedVal::new(lv, ty));
            let rf = to_f64(ctx, TypedVal::new(rv, ty));
            let fref = ctx.get_extern("pow", &[cl::F64, cl::F64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(fref, &[lf, rf]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(TypedVal::new(v, ValTy::F64))
        }

        op => Err(anyhow!("unsupported binary op: {op:?}")),
    }
}

/// Lowers `a?.b` / `a?.()`. Sem suporte a property access (depende de
/// #53 objects), so tratamos optional call — `callee?.()` resolvendo
/// callee a i64 funcptr: se for 0 (null), resultado e 0; senao
/// call_indirect normal.
fn lower_opt_chain(
    ctx: &mut FnCtx,
    opt: &swc_ecma_ast::OptChainExpr,
) -> Result<TypedVal> {
    use swc_ecma_ast::OptChainBase;
    match opt.base.as_ref() {
        OptChainBase::Call(call) => {
            let callee_tv = lower_expr(ctx, &call.callee)?;
            let callee_i64 = ctx.coerce_to_i64(callee_tv).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, callee_i64, zero);

            let null_block = ctx.builder.create_block();
            let call_block = ctx.builder.create_block();
            let merge_block = ctx.builder.create_block();
            let result_var = ctx.builder.declare_var(cl::I64);

            ctx.builder
                .ins()
                .brif(is_null, null_block, &[], call_block, &[]);

            // null path: resultado 0
            ctx.builder.switch_to_block(null_block);
            ctx.builder.seal_block(null_block);
            ctx.builder.def_var(result_var, zero);
            ctx.builder.ins().jump(merge_block, &[]);

            // call path: reconstroi CallExpr sintetico e indireta
            ctx.builder.switch_to_block(call_block);
            ctx.builder.seal_block(call_block);
            let synthetic_call = CallExpr {
                span: call.span,
                ctxt: call.ctxt,
                callee: Callee::Expr(call.callee.clone()),
                args: call.args.clone(),
                type_args: call.type_args.clone(),
            };
            let result = lower_indirect_call(ctx, &call.callee, &synthetic_call)?;
            let result_i64 = ctx.coerce_to_i64(result).val;
            ctx.builder.def_var(result_var, result_i64);
            ctx.builder.ins().jump(merge_block, &[]);

            ctx.builder.switch_to_block(merge_block);
            ctx.builder.seal_block(merge_block);
            let v = ctx.builder.use_var(result_var);
            Ok(TypedVal::new(v, ValTy::I64))
        }
        OptChainBase::Member(_) => Err(anyhow!(
            "optional member access (a?.b) requires object literals (#53) — not supported yet"
        )),
    }
}

fn lower_cond(ctx: &mut FnCtx, cond: &swc_ecma_ast::CondExpr) -> Result<TypedVal> {
    // Evaluate test, branch into cons/alt, merge in i64 slot.
    let test = lower_expr(ctx, &cond.test)?;
    let test_i64 = ctx.coerce_to_i64(test);
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_truthy = ctx.builder.ins().icmp(IntCC::NotEqual, test_i64.val, zero);

    let cons_block = ctx.builder.create_block();
    let alt_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();
    let result_var = ctx.builder.declare_var(cl::I64);

    ctx.builder
        .ins()
        .brif(is_truthy, cons_block, &[], alt_block, &[]);

    ctx.builder.switch_to_block(cons_block);
    ctx.builder.seal_block(cons_block);
    let cons = lower_expr(ctx, &cond.cons)?;
    let cons_ty = cons.ty;
    let cons_i64 = ctx.coerce_to_i64(cons);
    ctx.builder.def_var(result_var, cons_i64.val);
    ctx.builder.ins().jump(merge_block, &[]);

    ctx.builder.switch_to_block(alt_block);
    ctx.builder.seal_block(alt_block);
    let alt = lower_expr(ctx, &cond.alt)?;
    let alt_ty = alt.ty;
    let alt_i64 = ctx.coerce_to_i64(alt);
    ctx.builder.def_var(result_var, alt_i64.val);
    ctx.builder.ins().jump(merge_block, &[]);

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    let result = ctx.builder.use_var(result_var);
    // Slot is i64; if both branches report Handle/Bool (also i64-backed in
    // Cranelift) we keep the tag so downstream code skips redundant work.
    let ty = match (cons_ty, alt_ty) {
        (ValTy::Handle, ValTy::Handle) => ValTy::Handle,
        (ValTy::Bool, ValTy::Bool) => ValTy::Bool,
        _ => ValTy::I64,
    };
    Ok(TypedVal::new(result, ty))
}

fn lower_logical(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    // &&: evaluate lhs; if falsy, result = lhs (0); else result = rhs
    // ||: evaluate lhs; if truthy, result = lhs; else result = rhs
    let result_var = ctx.builder.declare_var(cl::I64);

    let lhs = lower_expr(ctx, &bin.left)?;
    let lhs_i64 = ctx.coerce_to_i64(lhs);

    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_truthy = ctx.builder.ins().icmp(IntCC::NotEqual, lhs_i64.val, zero);

    let true_block = ctx.builder.create_block();
    let false_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();

    ctx.builder
        .ins()
        .brif(is_truthy, true_block, &[], false_block, &[]);

    match bin.op {
        BinaryOp::LogicalAnd => {
            // true branch: evaluate rhs
            ctx.builder.switch_to_block(true_block);
            ctx.builder.seal_block(true_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs);
            ctx.builder.def_var(result_var, rhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);

            // false branch: short-circuit with 0
            ctx.builder.switch_to_block(false_block);
            ctx.builder.seal_block(false_block);
            ctx.builder.def_var(result_var, zero);
            ctx.builder.ins().jump(merge_block, &[]);
        }
        BinaryOp::LogicalOr => {
            // true branch: short-circuit with lhs value
            ctx.builder.switch_to_block(true_block);
            ctx.builder.seal_block(true_block);
            ctx.builder.def_var(result_var, lhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);

            // false branch: evaluate rhs
            ctx.builder.switch_to_block(false_block);
            ctx.builder.seal_block(false_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs);
            ctx.builder.def_var(result_var, rhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);
        }
        BinaryOp::NullishCoalescing => {
            // `lhs ?? rhs` — JS spec: se lhs e null/undefined, retorna
            // rhs; caso contrario retorna lhs. RTS representa null
            // como valor sentinel 0 (no lane I64 ou como Handle 0).
            // Para numeros bem tipados a semantica reduz a "se lhs
            // == 0 use rhs senao use lhs" — igual ao || do
            // ramo truthy. Ja funciona porque is_truthy == (x != 0).
            // true branch: non-zero, mantem lhs
            ctx.builder.switch_to_block(true_block);
            ctx.builder.seal_block(true_block);
            ctx.builder.def_var(result_var, lhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);

            // false branch: zero/null/undefined, usa rhs
            ctx.builder.switch_to_block(false_block);
            ctx.builder.seal_block(false_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs);
            ctx.builder.def_var(result_var, rhs_i64.val);
            ctx.builder.ins().jump(merge_block, &[]);
        }
        _ => unreachable!(),
    }

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    let val = ctx.builder.use_var(result_var);
    Ok(TypedVal::new(val, ValTy::Bool))
}

fn promote_numeric(
    ctx: &mut FnCtx,
    lhs: TypedVal,
    rhs: TypedVal,
) -> (
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    ValTy,
) {
    // If either side is f64, promote both
    if lhs.ty == ValTy::F64 || rhs.ty == ValTy::F64 {
        let lv = to_f64(ctx, lhs);
        let rv = to_f64(ctx, rhs);
        return (lv, rv, ValTy::F64);
    }

    // If either side is I64/Handle/Bool, widen both
    if lhs.ty == ValTy::I64
        || lhs.ty == ValTy::Handle
        || lhs.ty == ValTy::Bool
        || rhs.ty == ValTy::I64
        || rhs.ty == ValTy::Handle
        || rhs.ty == ValTy::Bool
    {
        let lv = ctx.coerce_to_i64(lhs).val;
        let rv = ctx.coerce_to_i64(rhs).val;
        return (lv, rv, ValTy::I64);
    }

    // Both I32: evaluate in I64 to avoid premature overflow in mixed
    // arithmetic chains like `(a * b + c) % m`. We truncate only when the
    // value is assigned/stored into an I32-typed slot.
    let lv = ctx.coerce_to_i64(lhs).val;
    let rv = ctx.coerce_to_i64(rhs).val;
    (lv, rv, ValTy::I64)
}

/// Coerces a raw Cranelift value (of type `ty` as seen by promote_numeric)
/// into an i64 suitable for bitwise ops. F64 is reinterpreted by converting
/// to signed integer (JS spec would ToInt32; we use i64 for consistency).
fn coerce_bits_i64(
    ctx: &mut FnCtx,
    val: cranelift_codegen::ir::Value,
    ty: ValTy,
) -> cranelift_codegen::ir::Value {
    if ty == ValTy::F64 {
        ctx.builder.ins().fcvt_to_sint_sat(cl::I64, val)
    } else {
        val
    }
}

fn to_f64(ctx: &mut FnCtx, tv: TypedVal) -> cranelift_codegen::ir::Value {
    match tv.ty {
        ValTy::F64 => tv.val,
        ValTy::I32 => ctx.builder.ins().fcvt_from_sint(cl::F64, tv.val),
        _ => {
            let as_i64 = ctx.coerce_to_i64(tv);
            ctx.builder.ins().fcvt_from_sint(cl::F64, as_i64.val)
        }
    }
}

fn lower_add(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fadd(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().iadd(lhs.val, rhs.val),
        _ => ctx.builder.ins().iadd(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_sub(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fsub(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().isub(lhs.val, rhs.val),
        _ => ctx.builder.ins().isub(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_mul(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fmul(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().imul(lhs.val, rhs.val),
        _ => ctx.builder.ins().imul(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_div(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => ctx.builder.ins().fdiv(lhs.val, rhs.val),
        ValTy::I32 => ctx.builder.ins().sdiv(lhs.val, rhs.val),
        _ => ctx.builder.ins().sdiv(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_mod(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let val = match lhs.ty {
        ValTy::F64 => {
            // Cranelift has no native f64 remainder. Delegate to libc `fmod`,
            // which is available on every supported target (msvcrt on
            // Windows, libc elsewhere). Matches JS `%` semantics on finite
            // operands and keeps the fractional part.
            let fref = ctx.get_extern("fmod", &[cl::F64, cl::F64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(fref, &[lhs.val, rhs.val]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(TypedVal::new(v, ValTy::F64));
        }
        ValTy::I32 => ctx.builder.ins().srem(lhs.val, rhs.val),
        _ => ctx.builder.ins().srem(lhs.val, rhs.val),
    };
    Ok(TypedVal::new(val, lhs.ty))
}

fn lower_icmp(ctx: &mut FnCtx, cc: IntCC, lhs: TypedVal, rhs: TypedVal) -> TypedVal {
    let cmp = if lhs.ty == ValTy::F64 {
        use cranelift_codegen::ir::condcodes::FloatCC;
        let fcc = match cc {
            IntCC::Equal => FloatCC::Equal,
            IntCC::NotEqual => FloatCC::NotEqual,
            IntCC::SignedLessThan => FloatCC::LessThan,
            IntCC::SignedLessThanOrEqual => FloatCC::LessThanOrEqual,
            IntCC::SignedGreaterThan => FloatCC::GreaterThan,
            IntCC::SignedGreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
            _ => FloatCC::Equal,
        };
        ctx.builder.ins().fcmp(fcc, lhs.val, rhs.val)
    } else {
        ctx.builder.ins().icmp(cc, lhs.val, rhs.val)
    };
    let ext = ctx.builder.ins().uextend(cl::I64, cmp);
    TypedVal::new(ext, ValTy::Bool)
}

// ── Calls ─────────────────────────────────────────────────────────────────

fn lower_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    // super(args) — chamada ao constructor da classe pai.
    if matches!(&call.callee, Callee::Super(_)) {
        return lower_super_call(ctx, call);
    }
    // Namespace call: `ns.fn(...)` — apenas quando `ns` resolve a
    // namespace do ABI. Quando `ns` e um local com classe estatica
    // conhecida, despacha para `__class_<C>_<method>`.
    if let Callee::Expr(callee) = &call.callee {
        if let Expr::Member(m) = callee.as_ref() {
            // super.method(args)
            if matches!(m.obj.as_ref(), Expr::SuperProp(_)) {
                return Err(anyhow!("super.method via SuperProp ainda nao suportado — use super(args) no constructor"));
            }
            if let Expr::SuperProp(sp) = m.obj.as_ref() {
                let _ = sp;
            }
            // Detecta `super.method(args)` (SWC representa como Member
            // com obj = Expr::Super... na verdade sao callees diferentes)
            if let Some(qualified) = qualified_member_name(callee) {
                if lookup(&qualified).is_some() {
                    return lower_ns_call(ctx, &qualified, call);
                }
            }
            // obj.method(args) com obj = ident local de classe conhecida
            if let Expr::Ident(obj_id) = m.obj.as_ref() {
                let obj_name = obj_id.sym.as_str();
                if let Some(class_name) = ctx.local_class_ty.get(obj_name).cloned() {
                    if let MemberProp::Ident(method_id) = &m.prop {
                        let method_name = method_id.sym.as_str();
                        return lower_class_method_call(
                            ctx,
                            &class_name,
                            method_name,
                            obj_name,
                            call,
                        );
                    }
                }
            }
            // this.method(args)
            if matches!(m.obj.as_ref(), Expr::This(_)) {
                if let MemberProp::Ident(method_id) = &m.prop {
                    let method_name = method_id.sym.as_str();
                    let class_name = ctx
                        .current_class
                        .clone()
                        .ok_or_else(|| anyhow!("`this.method()` fora de classe"))?;
                    return lower_class_method_call(ctx, &class_name, method_name, "this", call);
                }
            }
        }
        if let Some(qualified) = qualified_member_name(callee) {
            return lower_ns_call(ctx, &qualified, call);
        }
        // Ident callee: prefer direct user-fn call; fall back to indirect
        // when the name resolves to a local/parameter holding a funcptr
        // (e.g. `function apply(fn, x) { return fn(x); }`).
        if let Expr::Ident(id) = callee.as_ref() {
            let name = id.sym.as_str();
            if ctx.user_fns.contains_key(name) && ctx.var_ty(name).is_none() {
                return lower_user_call(ctx, name, call);
            }
            if ctx.var_ty(name).is_some() {
                return lower_indirect_call(ctx, callee, call);
            }
            // Unknown name — let lower_user_call produce a clear error.
            return lower_user_call(ctx, name, call);
        }
    }
    Err(anyhow!("unsupported call expression form"))
}

// ── Class operations ─────────────────────────────────────────────────────

/// Resolve o nome do metodo `m` na classe `c`, descendo pela hierarquia
/// de heranca quando filho nao define o proprio. Retorna o nome da
/// classe que efetivamente provê o metodo, ou None se nao encontrar.
fn resolve_method_owner<'a>(
    ctx: &'a FnCtx,
    class: &str,
    method: &str,
) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if meta.methods.iter().any(|m| m == method) {
            return Some(cur);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

/// Sobe a hierarquia ate achar uma classe que define constructor proprio.
/// Quando nenhuma classe da cadeia define, retorna None — chamada de
/// `__init` e elidida.
fn resolve_init_owner(ctx: &FnCtx, class: &str) -> Option<String> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if meta.has_constructor {
            return Some(cur);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

fn lower_new(ctx: &mut FnCtx, new_expr: &swc_ecma_ast::NewExpr) -> Result<TypedVal> {
    let class_name = match new_expr.callee.as_ref() {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        _ => return Err(anyhow!("`new` so suporta callee identifier (sem `new (expr)()`)")),
    };
    let _meta = ctx
        .classes
        .get(&class_name)
        .ok_or_else(|| anyhow!("classe `{class_name}` nao declarada"))?
        .clone();

    // Aloca map handle para a instancia.
    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];

    // Chama o constructor (se a classe ou algum ancestral definir).
    if let Some(init_owner) = resolve_init_owner(ctx, &class_name) {
        let init_fn_name = format!("__class_{init_owner}__init");
        let abi = ctx
            .user_fns
            .get(&init_fn_name)
            .ok_or_else(|| anyhow!("init de classe `{init_owner}` nao registrado"))?
            .clone();
        let mangled: &'static str =
            Box::leak(format!("__user_{init_fn_name}").into_boxed_str());
        let fn_id = *ctx
            .extern_cache
            .get(mangled)
            .ok_or_else(|| anyhow!("init mangled `{mangled}` faltando"))?;
        let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

        // abi.params[0] e o `this` (i64); pulamos no zip com call.args.
        let user_args: &[swc_ecma_ast::ExprOrSpread] = new_expr
            .args
            .as_ref()
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let expected = abi.params.len().saturating_sub(1);
        if user_args.len() != expected {
            return Err(anyhow!(
                "constructor de `{class_name}` espera {} argumento(s), recebeu {}",
                expected,
                user_args.len()
            ));
        }
        let mut args = vec![handle];
        for (a, expected_ty) in user_args.iter().zip(abi.params.iter().skip(1).copied()) {
            if a.spread.is_some() {
                return Err(anyhow!("spread em `new` nao suportado"));
            }
            let tv = lower_expr(ctx, &a.expr)?;
            let value = match expected_ty {
                ValTy::I32 => ctx.coerce_to_i32(tv).val,
                ValTy::F64 => to_f64(ctx, tv),
                _ => ctx.coerce_to_i64(tv).val,
            };
            args.push(value);
        }
        ctx.builder.ins().call(fref, &args);
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

fn lower_super_call(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    let class_name = ctx
        .current_class
        .clone()
        .ok_or_else(|| anyhow!("`super(...)` fora de metodo de classe"))?;
    let parent = ctx
        .classes
        .get(&class_name)
        .and_then(|m| m.super_class.clone())
        .ok_or_else(|| anyhow!("`super(...)` em classe sem extends"))?;
    let init_owner = resolve_init_owner(ctx, &parent).unwrap_or(parent);

    let init_fn_name = format!("__class_{init_owner}__init");
    let abi = ctx
        .user_fns
        .get(&init_fn_name)
        .ok_or_else(|| anyhow!("super init de `{init_owner}` nao registrado"))?
        .clone();
    let mangled: &'static str = Box::leak(format!("__user_{init_fn_name}").into_boxed_str());
    let fn_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("super init mangled `{mangled}` faltando"))?;
    let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

    let this_val = ctx
        .read_local("this")
        .ok_or_else(|| anyhow!("`this` indisponivel em super(...)"))?;
    let mut args = vec![this_val.val];
    let expected = abi.params.len().saturating_sub(1);
    if call.args.len() != expected {
        return Err(anyhow!(
            "super(...) espera {} argumento(s), recebeu {}",
            expected,
            call.args.len()
        ));
    }
    for (a, expected_ty) in call.args.iter().zip(abi.params.iter().skip(1).copied()) {
        if a.spread.is_some() {
            return Err(anyhow!("spread em super(...) nao suportado"));
        }
        let tv = lower_expr(ctx, &a.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
            _ => ctx.coerce_to_i64(tv).val,
        };
        args.push(value);
    }
    ctx.builder.ins().call(fref, &args);
    Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64))
}

fn lower_class_method_call(
    ctx: &mut FnCtx,
    class_name: &str,
    method_name: &str,
    receiver_local: &str,
    call: &CallExpr,
) -> Result<TypedVal> {
    let owner = resolve_method_owner(ctx, class_name, method_name).ok_or_else(|| {
        anyhow!("metodo `{method_name}` nao encontrado em `{class_name}` ou ancestrais")
    })?;

    let fn_name = format!("__class_{owner}_{method_name}");
    let abi = ctx
        .user_fns
        .get(&fn_name)
        .ok_or_else(|| anyhow!("metodo `{owner}.{method_name}` nao registrado"))?
        .clone();
    let mangled: &'static str = Box::leak(format!("__user_{fn_name}").into_boxed_str());
    let fn_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("metodo mangled `{mangled}` faltando"))?;
    let fref = ctx.module.declare_func_in_func(fn_id, ctx.builder.func);

    let recv = ctx
        .read_local(receiver_local)
        .ok_or_else(|| anyhow!("receiver `{receiver_local}` indisponivel"))?;
    let mut args = vec![ctx.coerce_to_i64(recv).val];
    let expected = abi.params.len().saturating_sub(1);
    if call.args.len() != expected {
        return Err(anyhow!(
            "metodo `{owner}.{method_name}` espera {} argumento(s), recebeu {}",
            expected,
            call.args.len()
        ));
    }
    for (a, expected_ty) in call.args.iter().zip(abi.params.iter().skip(1).copied()) {
        if a.spread.is_some() {
            return Err(anyhow!("spread em chamada de metodo nao suportado"));
        }
        let tv = lower_expr(ctx, &a.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
            _ => ctx.coerce_to_i64(tv).val,
        };
        args.push(value);
    }
    let inst = ctx.builder.ins().call(fref, &args);
    let results = ctx.builder.inst_results(inst);
    if let Some(&v) = results.first() {
        let ret_ty = abi.ret.unwrap_or(ValTy::I64);
        Ok(TypedVal::new(v, ret_ty))
    } else {
        Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64))
    }
}

/// Materialises the address of a user-defined function as an i64 value.
/// Produced when a bare `Ident` referring to a user fn appears in a value
/// position — lets callers pass functions as first-class values (#97).
fn emit_user_fn_addr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    let mangled: &'static str = Box::leak(format!("__user_{name}").into_boxed_str());
    let func_id = *ctx
        .extern_cache
        .get(mangled)
        .ok_or_else(|| anyhow!("user function `{name}` has no cached id"))?;
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);
    let ptr_ty = ctx.module.isa().pointer_type();
    let addr = ctx.builder.ins().func_addr(ptr_ty, fref);
    // Function pointers live in the I64 lane — same as any other handle-
    // shaped value. Downstream consumers just need to be able to
    // call_indirect with a compatible signature.
    Ok(TypedVal::new(addr, ValTy::I64))
}

/// Indirect call through a value holding a function pointer.
/// Provisional signature: one i64 param, i64 return. Covers the canonical
/// `function apply(fn, x) { return fn(x); }` pattern used to test first-
/// class functions. Richer signatures require closure-aware typing (fase
/// 2 of #97).
fn lower_indirect_call(
    ctx: &mut FnCtx,
    callee_expr: &Expr,
    call: &CallExpr,
) -> Result<TypedVal> {
    use cranelift_codegen::ir::{AbiParam, Signature};
    use cranelift_codegen::isa::CallConv;

    let callee = lower_expr(ctx, callee_expr)?;
    let callee_val = ctx.coerce_to_i64(callee).val;

    // Build provisional signature matching how user fns are declared
    // (Tail conv, all i64). One parameter per call arg; single i64 return.
    let mut sig = Signature::new(CallConv::Tail);
    for _ in &call.args {
        sig.params.push(AbiParam::new(cl::I64));
    }
    sig.returns.push(AbiParam::new(cl::I64));
    let sig_ref = ctx.builder.import_signature(sig);

    // Lower argument expressions, coercing each to i64.
    let mut args: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in indirect call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        args.push(ctx.coerce_to_i64(tv).val);
    }

    let inst = ctx.builder.ins().call_indirect(sig_ref, callee_val, &args);
    let results = ctx.builder.inst_results(inst);
    let v = results
        .first()
        .copied()
        .unwrap_or_else(|| ctx.builder.ins().iconst(cl::I64, 0));
    Ok(TypedVal::new(v, ValTy::I64))
}

/// Emits a zero-arg call to a constant's accessor symbol (e.g. `math.PI`).
///
/// Constants are backed by thin `extern "C"` functions declared via the ABI
/// so callers can read `math.PI` as an expression; LLVM/Cranelift is free
/// to inline the returned literal through normal import rules.
fn emit_constant_load(
    ctx: &mut FnCtx,
    member: &crate::abi::NamespaceMember,
) -> Result<TypedVal> {
    use cranelift_codegen::ir::{AbiParam, Signature};
    use cranelift_module::Linkage;

    let lowered = lower_member(member);
    let ret_cl = lowered
        .ret
        .ok_or_else(|| anyhow!("constant `{}` has no return type", member.name))?;

    // Declare import (idempotent via cache).
    let func_id = if let Some(id) = ctx.extern_cache.get(member.symbol).copied() {
        id
    } else {
        let mut sig = Signature::new(ctx.module.isa().default_call_conv());
        sig.returns.push(AbiParam::new(ret_cl));
        let id = ctx
            .module
            .declare_function(member.symbol, Linkage::Import, &sig)
            .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
        ctx.extern_cache.insert(member.symbol, id);
        id
    };
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);
    let inst = ctx.builder.ins().call(fref, &[]);
    let val = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(val, ValTy::from_abi(member.returns)))
}

/// Emits Cranelift IR inline for an intrinsic. Returns `Ok(None)` when the
/// intrinsic is not handled here (e.g. still pending implementation) so the
/// caller falls back to a regular extern call.
fn lower_intrinsic(
    ctx: &mut FnCtx,
    kind: crate::abi::Intrinsic,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    use crate::abi::Intrinsic;
    use cranelift_codegen::ir::condcodes::IntCC;

    // Helper: evaluate each argument and coerce to the requested scalar.
    fn arg_f64(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(to_f64(ctx, tv))
    }
    fn arg_i64(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg {idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in intrinsic call"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_i64(tv).val)
    }

    match kind {
        Intrinsic::Sqrt => {
            let x = arg_f64(ctx, call, 0)?;
            let v = ctx.builder.ins().sqrt(x);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::AbsF64 => {
            let x = arg_f64(ctx, call, 0)?;
            let v = ctx.builder.ins().fabs(x);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::MinF64 => {
            let a = arg_f64(ctx, call, 0)?;
            let b = arg_f64(ctx, call, 1)?;
            let v = ctx.builder.ins().fmin(a, b);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::MaxF64 => {
            let a = arg_f64(ctx, call, 0)?;
            let b = arg_f64(ctx, call, 1)?;
            let v = ctx.builder.ins().fmax(a, b);
            Ok(Some(TypedVal::new(v, ValTy::F64)))
        }
        Intrinsic::AbsI64 => {
            // `abs(x) = x >= 0 ? x : -x` via select; matches wrapping_abs for i64::MIN.
            let x = arg_i64(ctx, call, 0)?;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_neg = ctx.builder.ins().icmp(IntCC::SignedLessThan, x, zero);
            let neg = ctx.builder.ins().ineg(x);
            let v = ctx.builder.ins().select(is_neg, neg, x);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        Intrinsic::MinI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let less = ctx.builder.ins().icmp(IntCC::SignedLessThan, a, b);
            let v = ctx.builder.ins().select(less, a, b);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        Intrinsic::MaxI64 => {
            let a = arg_i64(ctx, call, 0)?;
            let b = arg_i64(ctx, call, 1)?;
            let greater = ctx.builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
            let v = ctx.builder.ins().select(greater, a, b);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        Intrinsic::RandomF64 => {
            // Inline xorshift64:
            //   x = *state
            //   x ^= x << 13; x ^= x >> 7; x ^= x << 17
            //   *state = x
            //   f = ((x >> 11) as f64) / 2^53
            use cranelift_codegen::ir::MemFlags;
            use cranelift_module::{DataDescription, Linkage};

            const STATE_SYMBOL: &str = "__RTS_DATA_NS_MATH_RNG_STATE";
            // Declare the data as an import (idempotent). We never define
            // it here — the runtime staticlib provides the actual storage.
            let data_id = ctx
                .module
                .declare_data(STATE_SYMBOL, Linkage::Import, true, false)
                .map_err(|e| anyhow!("failed to declare {STATE_SYMBOL}: {e}"))?;
            // declare_data for an import is fine even if called multiple
            // times; Cranelift dedupes.
            let _ = DataDescription::new(); // keep type in scope, no-op

            let gv = ctx.module.declare_data_in_func(data_id, ctx.builder.func);
            let ptr_ty = ctx.module.isa().pointer_type();
            let ptr = ctx.builder.ins().global_value(ptr_ty, gv);

            // RNG state is a static u64: naturally aligned and always
            // valid storage. `trusted()` unlocks the tightest load/store.
            let x0 = ctx.builder.ins().load(cl::I64, MemFlags::trusted(), ptr, 0);
            let s13 = ctx.builder.ins().ishl_imm(x0, 13);
            let x1 = ctx.builder.ins().bxor(x0, s13);
            let s7 = ctx.builder.ins().ushr_imm(x1, 7);
            let x2 = ctx.builder.ins().bxor(x1, s7);
            let s17 = ctx.builder.ins().ishl_imm(x2, 17);
            let x3 = ctx.builder.ins().bxor(x2, s17);
            ctx.builder.ins().store(MemFlags::trusted(), x3, ptr, 0);

            // Take top 53 bits and divide by 2^53 as f64.
            let bits = ctx.builder.ins().ushr_imm(x3, 11);
            let as_f = ctx.builder.ins().fcvt_from_uint(cl::F64, bits);
            let scale = ctx
                .builder
                .ins()
                .f64const(1.0f64 / ((1u64 << 53) as f64));
            let result = ctx.builder.ins().fmul(as_f, scale);
            Ok(Some(TypedVal::new(result, ValTy::F64)))
        }
    }
}

fn lower_ns_call(ctx: &mut FnCtx, qualified: &str, call: &CallExpr) -> Result<TypedVal> {
    let (_spec, member) =
        lookup(qualified).ok_or_else(|| anyhow!("unknown namespace member `{qualified}`"))?;

    // If the member has an intrinsic, emit IR inline. Falls through to the
    // extern call only when the intrinsic is not recognised (keeps the
    // symbol alive so reflection/FFI consumers see the exported impl).
    if let Some(kind) = member.intrinsic {
        if let Some(result) = lower_intrinsic(ctx, kind, call)? {
            return Ok(result);
        }
    }

    let lowered = lower_member(member);

    // Declare the extern (idempotent via cache)
    let func_id = {
        if !ctx.extern_cache.contains_key(member.symbol) {
            use cranelift_codegen::ir::{AbiParam, Signature};
            use cranelift_module::Linkage;
            let mut sig = Signature::new(ctx.module.isa().default_call_conv());
            for &p in &lowered.params {
                sig.params.push(AbiParam::new(p));
            }
            if let Some(r) = lowered.ret {
                sig.returns.push(AbiParam::new(r));
            }
            let id = ctx
                .module
                .declare_function(member.symbol, Linkage::Import, &sig)
                .map_err(|e| anyhow!("failed to declare {}: {e}", member.symbol))?;
            ctx.extern_cache.insert(member.symbol, id);
        }
        *ctx.extern_cache.get(member.symbol).unwrap()
    };
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);

    // Build argument values
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
                let tv = lower_expr(ctx, &arg.expr)?;
                match tv.ty {
                    ValTy::Handle => {
                        // Extract ptr+len from the handle
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
                    _ => {
                        // Literal string: get (ptr, len) from rodata
                        return Err(anyhow!("StrPtr argument must be a string value"));
                    }
                }
            }
            AbiType::I32 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i32(tv).val);
            }
            AbiType::I64 | AbiType::U64 | AbiType::Handle => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val);
            }
            AbiType::F64 => {
                let tv = lower_expr(ctx, &arg.expr)?;
                let fv = to_f64(ctx, tv);
                values.push(fv);
            }
            AbiType::Bool => {
                let tv = lower_expr(ctx, &arg.expr)?;
                values.push(ctx.coerce_to_i64(tv).val);
            }
            AbiType::Void => {}
        }
    }

    let inst = ctx.builder.ins().call(fref, &values);
    let ret_val = if let Some(_ret_cl) = lowered.ret {
        let v = ctx.builder.inst_results(inst)[0];
        let ret_ty = ValTy::from_abi(member.returns);
        TypedVal::new(v, ret_ty)
    } else {
        TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64)
    };
    Ok(ret_val)
}

fn lower_user_call(ctx: &mut FnCtx, name: &str, call: &CallExpr) -> Result<TypedVal> {
    let abi = ctx
        .user_fns
        .get(name)
        .ok_or_else(|| anyhow!("call to undeclared user function `{name}`"))?
        .clone();

    let mangled: &'static str = Box::leak(format!("__user_{name}").into_boxed_str());
    if !ctx.extern_cache.contains_key(mangled) {
        return Err(anyhow!("call to undeclared user function `{name}`"));
    }
    let func_id = *ctx.extern_cache.get(mangled).unwrap();
    let fref = ctx.module.declare_func_in_func(func_id, ctx.builder.func);

    if call.args.len() != abi.params.len() {
        return Err(anyhow!(
            "function `{name}` expects {} argument(s), got {}",
            abi.params.len(),
            call.args.len()
        ));
    }

    let mut values = Vec::new();
    for (arg, expected_ty) in call.args.iter().zip(abi.params.iter().copied()) {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let value = match expected_ty {
            ValTy::I32 => ctx.coerce_to_i32(tv).val,
            ValTy::I64 | ValTy::Bool | ValTy::Handle => ctx.coerce_to_i64(tv).val,
            ValTy::F64 => to_f64(ctx, tv),
        };
        values.push(value);
    }

    // Tail-call optimisation (#93). Only safe when:
    //   1. The caller itself uses the Tail calling convention
    //      (otherwise the ABI does not support tail transfer).
    //   2. The callsite is in tail position (set by `Stmt::Return`).
    //   3. The callee's return type matches the caller's — return_call
    //      cannot convert values between types; this is true here because
    //      every user fn we lower returns via the same ABI lane.
    if ctx.is_tail_conv && ctx.in_tail_position {
        ctx.builder.ins().return_call(fref, &values);
        // `return_call` is a terminator. Switch to a fresh sealed block
        // with no predecessors — subsequent IR emitted by the caller
        // (typically `Stmt::Return` placing a final `return_`) lands in
        // dead code that Cranelift DCEs. The block needs a terminator
        // for the verifier though, which `Stmt::Return` provides.
        let cont = ctx.builder.create_block();
        ctx.builder.switch_to_block(cont);
        ctx.builder.seal_block(cont);
        let ty = abi.ret.unwrap_or(ValTy::I64);
        // Placeholder value with the correct Cranelift type so the
        // caller's coerce_* code produces a well-typed `return_` arg.
        let placeholder = match ty {
            ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
            ValTy::F64 => ctx.builder.ins().f64const(0.0),
            _ => ctx.builder.ins().iconst(cl::I64, 0),
        };
        return Ok(TypedVal::new(placeholder, ty));
    }

    let inst = ctx.builder.ins().call(fref, &values);
    let results = ctx.builder.inst_results(inst);
    if let Some(ret_ty) = abi.ret {
        if let Some(&value) = results.first() {
            Ok(TypedVal::new(value, ret_ty))
        } else {
            Ok(TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ret_ty))
        }
    } else {
        Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ))
    }
}

// ── Array / Object literals ───────────────────────────────────────────────

fn lower_array_lit(ctx: &mut FnCtx, arr: &swc_ecma_ast::ArrayLit) -> Result<TypedVal> {
    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];

    let push_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
        &[cl::I64, cl::I64],
        None,
    )?;

    for elem in &arr.elems {
        let value = match elem {
            Some(e) => {
                if e.spread.is_some() {
                    return Err(anyhow!("spread em array literal nao suportado (MVP)"));
                }
                let tv = lower_expr(ctx, &e.expr)?;
                ctx.coerce_to_i64(tv).val
            }
            None => ctx.builder.ins().iconst(cl::I64, 0),
        };
        ctx.builder.ins().call(push_fn, &[handle, value]);
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

fn lower_object_lit(ctx: &mut FnCtx, obj: &swc_ecma_ast::ObjectLit) -> Result<TypedVal> {
    use swc_ecma_ast::{Prop, PropName, PropOrSpread};

    let new_fn = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_NEW", &[], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(new_fn, &[]);
    let handle = ctx.builder.inst_results(inst)[0];

    let set_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_SET",
        &[cl::I64, cl::I64, cl::I64, cl::I64],
        None,
    )?;

    for prop in &obj.props {
        let p = match prop {
            PropOrSpread::Prop(p) => p,
            PropOrSpread::Spread(_) => {
                return Err(anyhow!("spread em object literal nao suportado (MVP)"));
            }
        };

        let (key_str, value_expr): (String, Box<Expr>) = match p.as_ref() {
            Prop::KeyValue(kv) => {
                let k = match &kv.key {
                    PropName::Ident(id) => id.sym.as_str().to_string(),
                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                    PropName::Num(n) => n.value.to_string(),
                    PropName::Computed(_) | PropName::BigInt(_) => {
                        return Err(anyhow!("computed/bigint key em object literal nao suportado (MVP)"));
                    }
                };
                (k, kv.value.clone())
            }
            Prop::Shorthand(id) => {
                let name = id.sym.as_str().to_string();
                let synthetic = Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: id.span,
                    ctxt: Default::default(),
                    sym: name.as_str().into(),
                    optional: false,
                }));
                (name, synthetic)
            }
            Prop::Method(_) | Prop::Getter(_) | Prop::Setter(_) | Prop::Assign(_) => {
                return Err(anyhow!("method/get/set/assign em object literal nao suportado (MVP)"));
            }
        };

        let value_tv = lower_expr(ctx, &value_expr)?;
        let value_i64 = ctx.coerce_to_i64(value_tv).val;
        let (kptr, klen) = ctx.emit_str_literal(key_str.as_bytes())?;
        ctx.builder.ins().call(set_fn, &[handle, kptr, klen, value_i64]);
    }

    Ok(TypedVal::new(handle, ValTy::Handle))
}

fn lower_member_expr(ctx: &mut FnCtx, m: &swc_ecma_ast::MemberExpr) -> Result<TypedVal> {
    // Caso 1: namespace constant (math.PI). qualified_member_name so
    // resolve quando obj e ident e prop e ident.
    if let Some(qualified) = qualified_member_name(&Expr::Member(m.clone())) {
        if let Some((_spec, member)) = lookup(&qualified) {
            if !matches!(member.kind, crate::abi::MemberKind::Constant) {
                return Err(anyhow!(
                    "`{qualified}` is a function, not a constant — use `{qualified}(...)`"
                ));
            }
            return emit_constant_load(ctx, member);
        }
    }

    // Resolve a classe estatica do receiver (this ou local tipado), se
    // houver. Permite tipar o resultado de map_get conforme o field
    // declarado da classe (ex: `: string` retorna Handle).
    let receiver_class: Option<String> = match m.obj.as_ref() {
        Expr::This(_) => ctx.current_class.clone(),
        Expr::Ident(id) => ctx.local_class_ty.get(id.sym.as_str()).cloned(),
        _ => None,
    };

    // Caso 2/3: runtime member access. obj precisa virar handle.
    let obj_tv = lower_expr(ctx, &m.obj)?;
    let obj_handle = ctx.coerce_to_i64(obj_tv).val;

    match &m.prop {
        // obj.foo → map_get(obj, "foo")
        MemberProp::Ident(id) => {
            let key = id.sym.as_str();
            // Se conhecemos a classe + tipo do field, retornamos com o
            // tipo declarado em vez do default I64.
            let field_ty = receiver_class
                .as_deref()
                .and_then(|c| field_type_in_hierarchy(ctx, c, key));
            map_get_static_typed(ctx, obj_handle, key.as_bytes(), field_ty)
        }
        // obj["foo"] ou obj[i]
        MemberProp::Computed(c) => {
            // Detecta string literal estatica para usar map_get direto.
            if let Expr::Lit(Lit::Str(s)) = c.expr.as_ref() {
                return map_get_static(ctx, obj_handle, s.value.as_bytes());
            }
            // Indexa por valor: avalia e decide pela natureza do valor.
            let idx_tv = lower_expr(ctx, &c.expr)?;
            match idx_tv.ty {
                ValTy::Handle => {
                    // String dinamica como chave de map.
                    let ptr_fref = ctx
                        .get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                    let len_fref = ctx
                        .get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                    let pi = ctx.builder.ins().call(ptr_fref, &[idx_tv.val]);
                    let kptr = ctx.builder.inst_results(pi)[0];
                    let li = ctx.builder.ins().call(len_fref, &[idx_tv.val]);
                    let klen = ctx.builder.inst_results(li)[0];
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                        &[cl::I64, cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, kptr, klen]);
                    let v = ctx.builder.inst_results(inst)[0];
                    Ok(TypedVal::new(v, ValTy::I64))
                }
                _ => {
                    // Indice numerico: vec_get.
                    let idx = ctx.coerce_to_i64(idx_tv).val;
                    let get_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
                        &[cl::I64, cl::I64],
                        Some(cl::I64),
                    )?;
                    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, idx]);
                    let v = ctx.builder.inst_results(inst)[0];
                    Ok(TypedVal::new(v, ValTy::I64))
                }
            }
        }
        MemberProp::PrivateName(_) => {
            Err(anyhow!("private name access nao suportado"))
        }
    }
}

fn map_get_static(ctx: &mut FnCtx, obj_handle: cranelift_codegen::ir::Value, key: &[u8]) -> Result<TypedVal> {
    map_get_static_typed(ctx, obj_handle, key, None)
}

fn map_get_static_typed(
    ctx: &mut FnCtx,
    obj_handle: cranelift_codegen::ir::Value,
    key: &[u8],
    declared_ty: Option<ValTy>,
) -> Result<TypedVal> {
    let (kptr, klen) = ctx.emit_str_literal(key)?;
    let get_fn = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        &[cl::I64, cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(get_fn, &[obj_handle, kptr, klen]);
    let v = ctx.builder.inst_results(inst)[0];
    // map_get devolve sempre i64 (lane fisica). Coerce conforme o tipo
    // declarado do field (Handle, I32, F64, ...). Default I64 quando
    // nao sabemos.
    match declared_ty {
        Some(ValTy::I32) => {
            let narrowed = ctx.builder.ins().ireduce(cl::I32, v);
            Ok(TypedVal::new(narrowed, ValTy::I32))
        }
        Some(ValTy::Handle) => Ok(TypedVal::new(v, ValTy::Handle)),
        Some(ValTy::Bool) => Ok(TypedVal::new(v, ValTy::Bool)),
        // F64 nao suportado como field no MVP — coerce_to_i64 perde
        // precisao numerica. Usuario que precise pode usar `number` (i64).
        _ => Ok(TypedVal::new(v, ValTy::I64)),
    }
}

/// Procura o tipo declarado de um field na cadeia de heranca da classe.
fn field_type_in_hierarchy(ctx: &FnCtx, class: &str, field: &str) -> Option<ValTy> {
    let mut cur = class.to_string();
    loop {
        let meta = ctx.classes.get(&cur)?;
        if let Some(ty) = meta.field_types.get(field).copied() {
            return Some(ty);
        }
        match &meta.super_class {
            Some(parent) => cur = parent.clone(),
            None => return None,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn qualified_member_name(expr: &Expr) -> Option<String> {
    let Expr::Member(m) = expr else { return None };
    let Expr::Ident(ns) = m.obj.as_ref() else {
        return None;
    };
    let fn_name = match &m.prop {
        MemberProp::Ident(id) => id.sym.as_str().to_string(),
        _ => return None,
    };
    Some(format!("{}.{}", ns.sym.as_str(), fn_name))
}

fn ident_name(expr: &Expr) -> Option<&str> {
    if let Expr::Ident(id) = expr {
        Some(id.sym.as_str())
    } else {
        None
    }
}

fn expr_kind_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Array(_) => "array",
        Expr::Arrow(_) => "arrow",
        Expr::Await(_) => "await",
        Expr::Bin(_) => "binary",
        Expr::Call(_) => "call",
        Expr::Class(_) => "class",
        Expr::Cond(_) => "ternary",
        Expr::Fn(_) => "function-expr",
        Expr::Ident(_) => "ident",
        Expr::Lit(_) => "literal",
        Expr::Member(_) => "member",
        Expr::MetaProp(_) => "meta-prop",
        Expr::New(_) => "new",
        Expr::Object(_) => "object",
        Expr::Paren(_) => "paren",
        Expr::Seq(_) => "sequence",
        Expr::TaggedTpl(_) => "tagged-template",
        Expr::This(_) => "this",
        Expr::Tpl(_) => "template",
        Expr::Unary(_) => "unary",
        Expr::Update(_) => "update",
        Expr::Yield(_) => "yield",
        _ => "unknown",
    }
}
