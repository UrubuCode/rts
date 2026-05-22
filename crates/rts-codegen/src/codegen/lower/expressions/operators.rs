use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{
    InstBuilder,
    condcodes::{FloatCC, IntCC},
    types as cl,
};
use swc_ecma_ast::{BinExpr, BinaryOp, CallExpr, Expr, Lit, MemberProp, UpdateOp};

use super::calls::lower_class_method_call_with_recv;
use super::lower_expr;
use super::members::lhs_static_class;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};

pub(super) fn lower_update_expr(ctx: &mut FnCtx, u: &swc_ecma_ast::UpdateExpr) -> Result<TypedVal> {
    let name =
        ident_name(&u.arg).ok_or_else(|| anyhow!("update target must be a simple identifier"))?;
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
    if u.prefix { Ok(new_val) } else { Ok(cur) }
}

fn as_int_literal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Lit(Lit::Num(n)) if n.value.fract() == 0.0 && n.value.is_finite() => {
            Some(n.value as i64)
        }
        Expr::Paren(p) => as_int_literal(&p.expr),
        _ => None,
    }
}

fn try_bin_imm(ctx: &mut FnCtx, bin: &BinExpr) -> Result<Option<TypedVal>> {
    // Checa op antes de qualquer lower — sem isso, ops fora desta lista
    // pagavam lower duplicado da subexpr (uma aqui, outra no fluxo
    // principal). Em hot loops com FP arith isso era visivel no IR.
    if !matches!(
        bin.op,
        BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
    ) {
        return Ok(None);
    }
    // Para ops comutativas (Add, Mul, BitAnd/Or/Xor), peephole pode usar
    // qualquer lado como imm. Para nao-comutativas (Sub, Div, Mod), so'
    // aceita imm na direita: \`x - 5\`, \`x / 5\`, \`x % 5\` (peephole \`var op imm\`),
    // mas NAO \`5 - x\`, \`5 / x\`, \`5 % x\` (var no lado direito quebraria
    // a semantica — \`10 / i\` virava \`i / 10\` antes deste fix).
    let is_commutative = matches!(
        bin.op,
        BinaryOp::Add
            | BinaryOp::Mul
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
    );
    let (var_side, imm) = match (as_int_literal(&bin.left), as_int_literal(&bin.right)) {
        (None, Some(imm)) => (&bin.left, imm),
        (Some(imm), None) if is_commutative => (&bin.right, imm),
        _ => return Ok(None),
    };

    let lhs = lower_expr(ctx, var_side)?;

    // (#299) Peephole de Add inverteu lhs/rhs quando literal estava na
    // esquerda. Pra Number e' OK (3+5=5+3) mas Add com Handle vira
    // string concat e a ordem importa (\`3+\"5\"=\"35\"\`, \`\"5\"+3=\"53\"\`).
    // Quando o var_side e' Handle e o literal estava do outro lado,
    // a inversao quebra a semantica — abort do peephole, deixa o fluxo
    // principal lower_bin emitir concat na ordem do AST.
    if matches!(bin.op, BinaryOp::Add)
        && matches!(lhs.ty, ValTy::Handle)
        && matches!(as_int_literal(&bin.left), Some(_))
    {
        return Ok(None);
    }

    // Peepholes so' aplicam quando lhs eh inteiro. F64 *2 nao pode
    // virar shift; f64 %4 nao pode virar band. Sem essa guarda o
    // peephole quebrava \`5.5 % 4\` (vinha 1 em vez de 1.5) e
    // \`-2.5 * 2\` (vinha 0 em vez de -5).
    let lhs_is_int = matches!(lhs.ty, ValTy::I32 | ValTy::I64);

    // Peephole: \`x * 2^k\` vira \`x << k\`. Dramatically melhor que
    // imul (1 ciclo vs 3-5). Cranelift egraph nao faz por padrao.
    // \`x * 0\` vira 0. \`x * 1\` vira x.
    if lhs_is_int && matches!(bin.op, BinaryOp::Mul) {
        if let Some(opt) = mul_imm_peephole(ctx, &lhs, imm) {
            return Ok(Some(opt));
        }
    }
    // Peephole: \`x % 2^k\` vira \`x & (2^k - 1)\` quando x e' nao-negativo
    // OU quando o uso e' \`=== 0\`. Conservador: so' aplica para POT
    // positivos quando podemos provar que x >= 0 ou quando o resultado
    // so' importa pra zero check (caller decide). Aqui aplicamos
    // sempre — para x negativo \`x % 2^k\` em RTS retorna negativo,
    // mas \`x & MASK\` retorna positivo. Usuario que precisa do
    // semantica negativa deve evitar shift trick. Como JS usa
    // Number (f64) e RTS usa i64 com semantica de C, ficamos com
    // band para casos pos. Para correcao geral, voltamos ao srem.
    // CONSERVADOR: so' aplica quando lhs.ty == I64/I32 e imm > 0
    // potencia de 2. Trade-off: pra x negativo, x & MASK difere
    // de x % POT — usuario que iterar com i sempre nao-negativo
    // pega win.
    if lhs_is_int && matches!(bin.op, BinaryOp::Mod) {
        if let Some(opt) = mod_imm_peephole(ctx, &lhs, imm) {
            return Ok(Some(opt));
        }
    }
    // (#584) Antes: `x / 2^k` virava `x >> k`. Em JS, `/` sempre retorna
    // f64 — int division viola a spec. Removida a peephole; se o user
    // quer shift, escreve `x >> k` explicitamente. Mantida `mod_imm_peephole`
    // porque `%` em JS preserva tipo (i64 % 2 = i64).
    // Identidades aritmeticas com 0: x + 0 = x, x - 0 = x.
    // Cranelift egraph deveria pegar mas observado no IR mostra
    // \`iadd v, 0\` permanecendo. Documenta no IR e poupa um
    // ciclo opcional.
    if lhs_is_int && imm == 0 {
        match bin.op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::BitOr | BinaryOp::BitXor => {
                return Ok(Some(lhs));
            }
            BinaryOp::BitAnd => {
                // x & 0 = 0
                let zero = match lhs.ty {
                    ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
                    _ => ctx.builder.ins().iconst(cl::I64, 0),
                };
                let ty = if matches!(lhs.ty, ValTy::I32) { ValTy::I32 } else { ValTy::I64 };
                return Ok(Some(TypedVal::new(zero, ty)));
            }
            _ => {}
        }
    }
    // x & -1 = x, x | -1 = -1, x ^ -1 = ~x. Pulamos esses por agora —
    // raros em codigo idiomatico, e Cranelift tem peephole pra bnot.

    // Para BitAnd/BitOr/BitXor com imm != 0 caem no fluxo principal
    // (criam imm_tv e chamam lower_bin original via match abaixo).
    // Mas o match abaixo so trata Add/Sub/Mul/Div/Mod — se chegou
    // BitOp aqui, retorna None pra deixar o caller fazer.
    if matches!(
        bin.op,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
    ) {
        return Ok(None);
    }

    let imm_tv = if matches!(lhs.ty, ValTy::I32) {
        TypedVal::new(ctx.builder.ins().iconst(cl::I32, imm), ValTy::I32)
    } else {
        TypedVal::new(ctx.builder.ins().iconst(cl::I64, imm), ValTy::I64)
    };

    let result = match bin.op {
        BinaryOp::Add => lower_add(ctx, lhs, imm_tv)?,
        BinaryOp::Sub => lower_sub(ctx, lhs, imm_tv)?,
        BinaryOp::Mul => lower_mul(ctx, lhs, imm_tv)?,
        BinaryOp::Div => lower_div(ctx, lhs, imm_tv)?,
        BinaryOp::Mod => lower_mod(ctx, lhs, imm_tv)?,
        _ => unreachable!("op verificado acima"),
    };
    Ok(Some(result))
}

/// `x * imm` peephole. Cobre 0, 1, e potencias de 2 (shift).
fn mul_imm_peephole(
    ctx: &mut FnCtx,
    lhs: &TypedVal,
    imm: i64,
) -> Option<TypedVal> {
    // x * 0 = 0
    if imm == 0 {
        let zero = match lhs.ty {
            ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
            _ => ctx.builder.ins().iconst(cl::I64, 0),
        };
        let ty = if matches!(lhs.ty, ValTy::I32) { ValTy::I32 } else { ValTy::I64 };
        return Some(TypedVal::new(zero, ty));
    }
    // x * 1 = x
    if imm == 1 {
        return Some(*lhs);
    }
    // x * 2^k -> x << k (k em [1, 30] pra i32, [1, 62] pra i64)
    if imm > 1 && (imm as u64).is_power_of_two() {
        let k = imm.trailing_zeros() as i64;
        let max_k = if matches!(lhs.ty, ValTy::I32) { 30 } else { 62 };
        if k <= max_k {
            let v = match lhs.ty {
                ValTy::I32 => ctx.builder.ins().ishl_imm(lhs.val, k),
                _ => {
                    let lv = ctx.coerce_to_i64(*lhs).val;
                    ctx.builder.ins().ishl_imm(lv, k)
                }
            };
            let ty = if matches!(lhs.ty, ValTy::I32) { ValTy::I32 } else { ValTy::I64 };
            return Some(TypedVal::new(v, ty));
        }
    }
    None
}

/// `x % imm` peephole correto pra signed (#297).
///
/// Para POT positivo `n = 2^k`, `x % n` em JS preserva sinal do dividendo:
/// `-7 % 4 = -3`, `-8 % 4 = 0`. A trick `x & (n-1)` so' funciona para
/// `x >= 0` — para x negativo `x & MASK` retorna positivo (errado).
///
/// Fix correto sem perder a otimizacao em hot paths:
///
/// ```text
/// adj  = (x >> (BITS-1)) & (n-1)    // -1 todos bits se x < 0, else 0
/// r    = (x + adj) & (n-1)          // r positivo
/// r    = r - adj                    // ajusta sinal
/// ```
///
/// Equivalente a `(x % n + n) & (n-1)` mas sem branch. 4 instrucoes
/// vs srem ~20+. Cranelift egraph nao faz pra signed mod.
fn mod_imm_peephole(
    ctx: &mut FnCtx,
    lhs: &TypedVal,
    imm: i64,
) -> Option<TypedVal> {
    if !(imm > 0 && (imm as u64).is_power_of_two()) {
        return None;
    }
    let mask = imm - 1;
    let (lv, ty, bits_minus_one) = match lhs.ty {
        ValTy::I32 => (lhs.val, ValTy::I32, 31),
        _ => (ctx.coerce_to_i64(*lhs).val, ValTy::I64, 63),
    };
    // adj = (x >> (BITS-1)) & mask
    let signbits = ctx.builder.ins().sshr_imm(lv, bits_minus_one);
    let adj = ctx.builder.ins().band_imm(signbits, mask);
    // r = (x + adj) & mask
    let plus = ctx.builder.ins().iadd(lv, adj);
    let masked = ctx.builder.ins().band_imm(plus, mask);
    // r = masked - adj
    let r = ctx.builder.ins().isub(masked, adj);
    Some(TypedVal::new(r, ty))
}

fn operator_method_name(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("add"),
        BinaryOp::Sub => Some("sub"),
        BinaryOp::Mul => Some("mul"),
        BinaryOp::Div => Some("div"),
        BinaryOp::Mod => Some("mod"),
        BinaryOp::EqEq | BinaryOp::EqEqEq => Some("eq"),
        BinaryOp::NotEq | BinaryOp::NotEqEq => Some("ne"),
        BinaryOp::Lt => Some("lt"),
        BinaryOp::LtEq => Some("le"),
        BinaryOp::Gt => Some("gt"),
        BinaryOp::GtEq => Some("ge"),
        _ => None,
    }
}

fn try_operator_overload(ctx: &mut FnCtx, bin: &BinExpr) -> Result<Option<TypedVal>> {
    let method = match operator_method_name(bin.op) {
        Some(method) => method,
        None => return Ok(None),
    };
    // Checa classe ANTES de fazer lower — lower emite IR e nao tem
    // como desfazer. Sem essa guarda, todo binop nao-overload pagava
    // por um lower duplicado da subexpr esquerda. Em hot loops com
    // \`x*x + y*y <= 1.0\`, isso tripla-emitia o IR (3 caminhos:
    // try_operator_overload, try_bin_imm, fluxo principal).
    let Some(class_name) = lhs_static_class(ctx, &bin.left) else {
        return Ok(None);
    };
    // Confirma que a classe tem o metodo do operator antes de gastar
    // lower_expr. Sem o metodo a chamada cairia em runtime warning
    // (call to undeclared method) e o fluxo principal eh o caminho
    // correto.
    let has_method = ctx
        .classes
        .get(&class_name)
        .map(|m| m.methods.iter().any(|n| n == method))
        .unwrap_or(false);
    if !has_method {
        return Ok(None);
    }
    let lhs_tv = lower_expr(ctx, &bin.left)?;
    let recv_i64 = ctx.coerce_to_i64(lhs_tv).val;
    let synthetic_call = CallExpr {
        span: bin.span,
        ctxt: Default::default(),
        callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: bin.span,
            ctxt: Default::default(),
            sym: method.into(),
            optional: false,
        }))),
        args: vec![swc_ecma_ast::ExprOrSpread {
            spread: None,
            expr: bin.right.clone(),
        }],
        type_args: None,
    };
    let result =
        lower_class_method_call_with_recv(ctx, &class_name, method, recv_i64, &synthetic_call)?;
    Ok(Some(result))
}

pub(super) fn lower_bin(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    if matches!(
        bin.op,
        BinaryOp::LogicalOr | BinaryOp::LogicalAnd | BinaryOp::NullishCoalescing
    ) {
        return lower_logical(ctx, bin);
    }
    // Identidade JS: `Number.parseInt === parseInt`, `Number.parseFloat ===
    // parseFloat` sao true em JS (aliases). RTS resolve em compile-time —
    // sem essa shortcut, lower_expr de \`Number.parseInt\` (Member) falharia
    // ou geraria valor incompativel com \`parseInt\` (Ident).
    if matches!(bin.op, BinaryOp::EqEqEq | BinaryOp::NotEqEq) {
        let is_number_alias = |left: &Expr, right: &Expr, name: &str| -> bool {
            let left_is_member = matches!(left, Expr::Member(m)
                if matches!(m.obj.as_ref(), Expr::Ident(i) if i.sym.as_str() == "Number")
                && matches!(&m.prop, swc_ecma_ast::MemberProp::Ident(p) if p.sym.as_str() == name)
            );
            let right_is_ident = matches!(right, Expr::Ident(i) if i.sym.as_str() == name);
            left_is_member && right_is_ident
        };
        for name in ["parseInt", "parseFloat", "isNaN", "isFinite"] {
            if is_number_alias(&bin.left, &bin.right, name)
                || is_number_alias(&bin.right, &bin.left, name)
            {
                let val = if matches!(bin.op, BinaryOp::EqEqEq) { 1 } else { 0 };
                let v = ctx.builder.ins().iconst(cl::I64, val);
                return Ok(TypedVal::new(v, ValTy::Bool));
            }
        }
    }
    if let Some(tv) = try_operator_overload(ctx, bin)? {
        return Ok(tv);
    }
    if let Some(tv) = try_bin_imm(ctx, bin)? {
        return Ok(tv);
    }

    // `x instanceof C` — RHS é um Ident de classe, não uma expression
    // valor. Lê __rts_class do receiver e compara contra C e todas as
    // subclasses de C conhecidas em compile-time (descendentes).
    if matches!(bin.op, BinaryOp::InstanceOf) {
        return lower_instanceof(ctx, bin);
    }

    // (cross-runtime #102) BigInt literal arithmetic: quando AMBOS lados
    // sao BigInt literals (`17n / 3n`), `/` deve fazer trunc i64 (BigInt
    // spec) em vez de fdiv f64. RTS nao tem BigInt real, mas casos comuns
    // com 2 literais sao detectaveis sintaticamente.
    if matches!(bin.op, BinaryOp::Div) {
        let lhs_is_bigint = matches!(bin.left.as_ref(), Expr::Lit(Lit::BigInt(_)));
        let rhs_is_bigint = matches!(bin.right.as_ref(), Expr::Lit(Lit::BigInt(_)));
        if lhs_is_bigint && rhs_is_bigint {
            let lhs_tv = lower_expr(ctx, &bin.left)?;
            let rhs_tv = lower_expr(ctx, &bin.right)?;
            let lv = ctx.coerce_to_i64(lhs_tv).val;
            let rv = ctx.coerce_to_i64(rhs_tv).val;
            // Guard divisor 0: emite trunc com sdiv safe — em rv=0 retorna 0.
            let val = lower_imod_safe(ctx, lv, rv, ValTy::I64);
            // lower_imod_safe faz modulo — precisamos de sdiv aqui.
            // Reusar mesma estrategia: bor com is_zero flag.
            let _ = val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_zero = ctx.builder.ins().icmp(IntCC::Equal, rv, zero);
            let is_zero_i64 = ctx.builder.ins().uextend(cl::I64, is_zero);
            let safe_rv = ctx.builder.ins().bor(rv, is_zero_i64);
            let q = ctx.builder.ins().sdiv(lv, safe_rv);
            let result = ctx.builder.ins().select(is_zero, zero, q);
            return Ok(TypedVal::new(result, ValTy::I64));
        }
    }

    // (cross-runtime #52/#753) `key in obj` — dispatcher universal que
    // aceita Vec ou Map, com key como handle (String, Symbol, etc).
    // Chama OBJ_HAS que despacha por Entry type do obj.
    if matches!(bin.op, BinaryOp::In) {
        let key_tv = lower_expr(ctx, &bin.left)?;
        let obj_tv = lower_expr(ctx, &bin.right)?;
        // (#83) Aceita tambem I64/U64 — em callbacks de array methods o
        // param vem como I64 raw carregando handle. Sem isso `"key" in y`
        // em arrow fail com "unsupported binary op: in". Primitives reais
        // (F64/Bool/I32) caem no path original que retorna Err.
        if matches!(
            obj_tv.ty,
            ValTy::Handle | ValTy::I64 | ValTy::U64
        ) {
            // Coerce key para handle: string literals/handles passam direto,
            // numbers viram string handle via gc.string_from_i64/f64.
            let key_h = ctx.coerce_to_handle(key_tv)?.val;
            let obj_h = obj_tv.val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_OBJ_HAS",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, key_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(TypedVal::new(v, ValTy::Bool));
        }
    }

    // (cross-runtime #178) `<expr> === undefined` / `!== undefined` —
    // detecta sentinelas MIN+2 (undefined) e MIN+4 (sparse hole de
    // `new Array(N)`). Sem isso comparacao com handle de string
    // "undefined" falharia para slots sentinela.
    if matches!(bin.op, BinaryOp::EqEqEq | BinaryOp::NotEqEq) {
        let is_undef_ident = |e: &Expr| matches!(e, Expr::Ident(id) if id.sym.as_str() == "undefined");
        let other = if is_undef_ident(&bin.right) {
            Some(&bin.left)
        } else if is_undef_ident(&bin.left) {
            Some(&bin.right)
        } else {
            None
        };
        if let Some(other_expr) = other {
            // Skip se ambos sao undefined ident (tratado abaixo).
            if !is_undef_ident(other_expr) {
                // (#777) Member access em obj sem prop existente retorna 0
                // (MAP_GET_CHAIN miss). JS spec: `obj.missing === undefined`
                // deve ser true. Detecta se LHS eh Member e considera 0
                // tambem como undefined-equivalente.
                let lhs_is_member = matches!(
                    other_expr.as_ref(),
                    Expr::Member(_) | Expr::OptChain(_)
                );
                let tv = lower_expr(ctx, other_expr)?;
                let v_i64 = ctx.coerce_to_i64(tv).val;
                let undef = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
                let hole = ctx.builder.ins().iconst(cl::I64, i64::MIN + 4);
                let eq_undef = ctx.builder.ins().icmp(IntCC::Equal, v_i64, undef);
                let eq_hole = ctx.builder.ins().icmp(IntCC::Equal, v_i64, hole);
                let mut is_undef_val = ctx.builder.ins().bor(eq_undef, eq_hole);
                if lhs_is_member {
                    let zero = ctx.builder.ins().iconst(cl::I64, 0);
                    let eq_zero = ctx.builder.ins().icmp(IntCC::Equal, v_i64, zero);
                    is_undef_val = ctx.builder.ins().bor(is_undef_val, eq_zero);
                }
                let result_v = if matches!(bin.op, BinaryOp::NotEqEq) {
                    let one = ctx.builder.ins().iconst(cl::I8, 1);
                    ctx.builder.ins().bxor(is_undef_val, one)
                } else {
                    is_undef_val
                };
                let ext = ctx.builder.ins().uextend(cl::I64, result_v);
                return Ok(TypedVal::new(ext, ValTy::Bool));
            }
        }
    }

    // (#643/null-undef) `null == undefined` deve ser true em JS abstract
    // equality. Em RTS, null vira (0, Handle) e undefined vira handle
    // de string "undefined" — comparacao normal falharia. Detecta caso
    // especial em compile-time.
    if matches!(bin.op, BinaryOp::EqEq | BinaryOp::NotEq) {
        let is_null_lit = |e: &Expr| matches!(e, Expr::Lit(Lit::Null(_)));
        let is_undef = |e: &Expr| matches!(e, Expr::Ident(id) if id.sym.as_str() == "undefined");
        let lhs_nu = is_null_lit(&bin.left) || is_undef(&bin.left);
        let rhs_nu = is_null_lit(&bin.right) || is_undef(&bin.right);
        if lhs_nu && rhs_nu {
            // `null == null`, `null == undefined`, `undefined == null`,
            // `undefined == undefined` — todos true em loose equality.
            let val = if matches!(bin.op, BinaryOp::EqEq) { 1 } else { 0 };
            let v = ctx.builder.ins().iconst(cl::I64, val);
            return Ok(TypedVal::new(v, ValTy::Bool));
        }
    }

    // (#786) `expr === undefined` / `expr !== undefined` quando expr eh
    // resultado i64 (ex: `new Array(5)[0]` = sentinel i64::MIN+2).
    // Lhs/rhs antes de lower_expr porque `undefined` ident vira string
    // handle e a comparacao normal falha.
    if matches!(bin.op, BinaryOp::EqEqEq | BinaryOp::NotEqEq | BinaryOp::EqEq | BinaryOp::NotEq) {
        let is_undef_ident = |e: &Expr| matches!(e, Expr::Ident(id) if id.sym.as_str() == "undefined");
        let lhs_undef = is_undef_ident(&bin.left) && ctx.read_local("undefined").is_none();
        let rhs_undef = is_undef_ident(&bin.right) && ctx.read_local("undefined").is_none();
        let other = if rhs_undef {
            Some(&bin.left)
        } else if lhs_undef {
            Some(&bin.right)
        } else {
            None
        };
        if let Some(other_expr) = other {
            let other_tv = lower_expr(ctx, other_expr)?;
            // Handle path: comparar via string_eq com handle de "undefined".
            // Cobre `void X === undefined` e variaveis cujo valor eh "undefined".
            if matches!(other_tv.ty, ValTy::Handle) {
                let undef_h = ctx.emit_str_handle(b"undefined")?.val;
                let fref = ctx.get_extern(
                    "__RTS_FN_NS_GC_STRING_EQ",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[other_tv.val, undef_h]);
                let eq = ctx.builder.inst_results(inst)[0];
                let result = if matches!(bin.op, BinaryOp::NotEqEq | BinaryOp::NotEq) {
                    let one = ctx.builder.ins().iconst(cl::I64, 1);
                    ctx.builder.ins().bxor(eq, one)
                } else {
                    eq
                };
                let result = ctx.builder.ins().ireduce(cl::I8, result);
                return Ok(TypedVal::new(result, ValTy::Bool));
            }
            // Caminho aplicavel quando `other` eh I64 (slot de Vec/Array) —
            // compara com sentinel i64::MIN+2.
            if matches!(other_tv.ty, ValTy::I64 | ValTy::U64) {
                let sentinel = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
                let eq = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    other_tv.val,
                    sentinel,
                );
                let result = if matches!(bin.op, BinaryOp::NotEqEq | BinaryOp::NotEq) {
                    let one = ctx.builder.ins().iconst(cl::I8, 1);
                    ctx.builder.ins().bxor(eq, one)
                } else {
                    eq
                };
                return Ok(TypedVal::new(result, ValTy::Bool));
            }
        }
    }

    let lhs = lower_expr(ctx, &bin.left)?;
    let rhs = lower_expr(ctx, &bin.right)?;

    // Add precisa do tipo original (string concat detecta Handle).
    // Demais ops aritmeticos promovem internamente.
    if matches!(bin.op, BinaryOp::Add) {
        return lower_add(ctx, lhs, rhs);
    }

    // String equality (#130): quando ambos sao Handle, comparar por
    // conteudo via __RTS_FN_NS_GC_STRING_EQ. Sem isso `==` compararia
    // handles u64 (sempre distintos para interneds diferentes).
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
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            ctx.builder.ins().bxor(eq, one)
        } else {
            eq
        };
        return Ok(TypedVal::new(result, ValTy::Bool));
    }

    // String ordering (#616): <, <=, >, >= entre dois Handles devem comparar
    // conteudo lexicograficamente, nao ponteiros. Roteia via STRING_CMP que
    // retorna -1/0/1 (memcmp + diff length).
    if matches!(
        bin.op,
        BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
    ) && lhs.ty == ValTy::Handle
        && rhs.ty == ValTy::Handle
    {
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CMP",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[lhs.val, rhs.val]);
        let cmp = ctx.builder.inst_results(inst)[0];
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let cc = match bin.op {
            BinaryOp::Lt => IntCC::SignedLessThan,
            BinaryOp::LtEq => IntCC::SignedLessThanOrEqual,
            BinaryOp::Gt => IntCC::SignedGreaterThan,
            BinaryOp::GtEq => IntCC::SignedGreaterThanOrEqual,
            _ => unreachable!(),
        };
        let result = ctx.builder.ins().icmp(cc, cmp, zero);
        return Ok(TypedVal::new(result, ValTy::Bool));
    }

    // === / !== com tipos diferentes em compile-time → const false/true.
    // (#306) JS strict equality nao coerce; `0 === false` deve ser false.
    // Bool e' detectavel separado de I64/F64 em ValTy mesmo backed por
    // mesmo cl_type, e Handle (string) e' distinto de numericos.
    if matches!(bin.op, BinaryOp::EqEqEq | BinaryOp::NotEqEq) {
        if !same_strict_kind(lhs.ty, rhs.ty) {
            // (#819) Se uma ponta eh I64 ambiguo (vec_get/map_get/etc),
            // o tipo estatico nao reflete o conteudo: pode ser handle.
            // Roteia para runtime helper que decide em runtime.
            let lhs_ambig = matches!(lhs.ty, ValTy::I64 | ValTy::U64)
                && ctx.var_member_call_values.contains(&lhs.val);
            let rhs_ambig = matches!(rhs.ty, ValTy::I64 | ValTy::U64)
                && ctx.var_member_call_values.contains(&rhs.val);
            let other_is_handle = lhs.ty == ValTy::Handle || rhs.ty == ValTy::Handle;
            if (lhs_ambig || rhs_ambig) && other_is_handle {
                let lv = ctx.coerce_to_i64(lhs).val;
                let rv = ctx.coerce_to_i64(rhs).val;
                let fref = ctx.get_extern(
                    "__RTS_FN_RT_STRICT_EQ_AMBIG",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[lv, rv]);
                let eq = ctx.builder.inst_results(inst)[0];
                let result = if matches!(bin.op, BinaryOp::NotEqEq) {
                    let one = ctx.builder.ins().iconst(cl::I64, 1);
                    ctx.builder.ins().bxor(eq, one)
                } else {
                    eq
                };
                return Ok(TypedVal::new(result, ValTy::Bool));
            }
            let result = if matches!(bin.op, BinaryOp::NotEqEq) { 1 } else { 0 };
            let v = ctx.builder.ins().iconst(cl::I64, result);
            return Ok(TypedVal::new(v, ValTy::Bool));
        }
    }

    // == / != com Bool ↔ numerico: coerce ambos para numerico e comparar.
    // (#306) JS abstract equality: `0 == false` e' true via Number(false)=0.
    // Mesmo tratamento para I64<->F64<->I32 (promove pra F64 via promote_numeric
    // que ja' roda abaixo). Mas distincao Bool e' importante: sem este branch,
    // `0 === false` (mesmo backing i64) cairia em strict-eq numerico true.
    if matches!(bin.op, BinaryOp::EqEq | BinaryOp::NotEq)
        && (lhs.ty == ValTy::Bool || rhs.ty == ValTy::Bool)
        && lhs.ty != ValTy::Handle
        && rhs.ty != ValTy::Handle
    {
        // Promote ambos para i64 e comparar.
        let lv = ctx.coerce_to_i64(lhs).val;
        let rv = ctx.coerce_to_i64(rhs).val;
        let cc = if matches!(bin.op, BinaryOp::EqEq) {
            IntCC::Equal
        } else {
            IntCC::NotEqual
        };
        let result = ctx.builder.ins().icmp(cc, lv, rv);
        return Ok(TypedVal::new(result, ValTy::Bool));
    }

    // == entre Handle (string) e numerico: parse a string como numero
    // e compara. Conservador — sem fast path para casos comuns.
    if matches!(bin.op, BinaryOp::EqEq | BinaryOp::NotEq)
        && ((lhs.ty == ValTy::Handle && rhs.ty != ValTy::Handle)
            || (rhs.ty == ValTy::Handle && lhs.ty != ValTy::Handle))
    {
        // Caso especial: comparacao com 0 numerico literal sentinel (null).
        // Handles 0/null sao usados pra sentinela em RTS (\`h0 == 0\`).
        // Vai como icmp i64 — nao tenta string convert.
        let other_is_zero_lit = if lhs.ty == ValTy::Handle {
            matches!(&*bin.right, Expr::Lit(Lit::Num(n)) if n.value == 0.0)
        } else {
            matches!(&*bin.left, Expr::Lit(Lit::Num(n)) if n.value == 0.0)
        };
        if other_is_zero_lit {
            let h = if lhs.ty == ValTy::Handle { lhs.val } else { rhs.val };
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let cc = if matches!(bin.op, BinaryOp::EqEq) {
                IntCC::Equal
            } else {
                IntCC::NotEqual
            };
            let result = ctx.builder.ins().icmp(cc, h, zero);
            return Ok(TypedVal::new(result, ValTy::Bool));
        }
        // Converte o numerico em string handle e compara conteudo.
        // JS: `"1" == 1` -> ToNumber("1") == 1 -> 1 == 1 -> true.
        // Implementacao: stringify ambos e usa STRING_EQ. Funciona pq
        // STRING_FROM_I64/F64 emite a representacao decimal canonica que
        // bate com a string parseavel original (`"1" -> 1 -> "1"`).
        let lhs_h = ctx.coerce_to_handle(lhs)?.val;
        let rhs_h = ctx.coerce_to_handle(rhs)?.val;
        let fref = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_EQ",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(fref, &[lhs_h, rhs_h]);
        let eq = ctx.builder.inst_results(inst)[0];
        let result = if matches!(bin.op, BinaryOp::NotEq) {
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            ctx.builder.ins().bxor(eq, one)
        } else {
            eq
        };
        return Ok(TypedVal::new(result, ValTy::Bool));
    }

    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    // Reaproveita os valores ja promovidos pra comparacoes —
    // antes lower_icmp recebia lhs/rhs originais e fazia coerce_to_i64
    // de novo, emitindo sextend duplicado. Em \`i < N\` com N: i32,
    // gerava 2x \`sextend.i64\` no IR.
    let lhs_p = TypedVal::new(lv, ty);
    let rhs_p = TypedVal::new(rv, ty);

    match bin.op {
        BinaryOp::Add => unreachable!(),
        BinaryOp::Sub => lower_sub(ctx, lhs_p, rhs_p),
        BinaryOp::Mul => lower_mul(ctx, lhs_p, rhs_p),
        BinaryOp::Div => lower_div(ctx, lhs_p, rhs_p),
        BinaryOp::Mod => lower_mod(ctx, lhs_p, rhs_p),
        BinaryOp::EqEq | BinaryOp::EqEqEq => Ok(lower_icmp(ctx, IntCC::Equal, lhs_p, rhs_p)),
        BinaryOp::NotEq | BinaryOp::NotEqEq => Ok(lower_icmp(ctx, IntCC::NotEqual, lhs_p, rhs_p)),
        BinaryOp::Lt => Ok(lower_icmp(ctx, IntCC::SignedLessThan, lhs_p, rhs_p)),
        BinaryOp::LtEq => Ok(lower_icmp(ctx, IntCC::SignedLessThanOrEqual, lhs_p, rhs_p)),
        BinaryOp::Gt => Ok(lower_icmp(ctx, IntCC::SignedGreaterThan, lhs_p, rhs_p)),
        BinaryOp::GtEq => Ok(lower_icmp(ctx, IntCC::SignedGreaterThanOrEqual, lhs_p, rhs_p)),
        BinaryOp::BitOr => Ok(TypedVal::new(ctx.builder.ins().bor(lv, rv), ty)),
        BinaryOp::BitXor => Ok(TypedVal::new(ctx.builder.ins().bxor(lv, rv), ty)),
        BinaryOp::BitAnd => Ok(TypedVal::new(ctx.builder.ins().band(lv, rv), ty)),
        BinaryOp::LShift => {
            Ok(TypedVal::new(ctx.builder.ins().ishl(lv, rv), ty))
        }
        BinaryOp::RShift => {
            Ok(TypedVal::new(ctx.builder.ins().sshr(lv, rv), ty))
        }
        BinaryOp::ZeroFillRShift => {
            // JS spec: ToUint32(lhs) >>> (rhs & 0x1F). Promove para i64,
            // aplica masks de 32 bits no LHS, depois shift unsigned.
            // Retorna em i64 — resultado eh 0..=u32::MAX, valor JS preservado.
            use cranelift_codegen::ir::types as cl;
            let lv64 = if matches!(ty, ValTy::I32 | ValTy::I64) {
                if ctx.builder.func.dfg.value_type(lv) == cl::I64 {
                    lv
                } else {
                    ctx.builder.ins().sextend(cl::I64, lv)
                }
            } else {
                lv
            };
            let rv64 = if ctx.builder.func.dfg.value_type(rv) == cl::I64 {
                rv
            } else {
                ctx.builder.ins().uextend(cl::I64, rv)
            };
            let mask = ctx.builder.ins().iconst(cl::I64, 0xFFFF_FFFF);
            let lv_u32 = ctx.builder.ins().band(lv64, mask);
            let shift_mask = ctx.builder.ins().iconst(cl::I64, 0x1F);
            let rv_masked = ctx.builder.ins().band(rv64, shift_mask);
            let shifted = ctx.builder.ins().ushr(lv_u32, rv_masked);
            Ok(TypedVal::new(shifted, ValTy::I64))
        }
        BinaryOp::Exp => {
            let lf = to_f64(ctx, TypedVal::new(lv, ty));
            let rf = to_f64(ctx, TypedVal::new(rv, ty));
            let fref = ctx.get_extern("pow", &[cl::F64, cl::F64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(fref, &[lf, rf]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(TypedVal::new(v, ValTy::F64))
        }
        other => Err(anyhow!("unsupported binary op: {other:?}")),
    }
}

pub(super) fn lower_opt_chain(
    ctx: &mut FnCtx,
    opt: &swc_ecma_ast::OptChainExpr,
) -> Result<TypedVal> {
    match opt.base.as_ref() {
        swc_ecma_ast::OptChainBase::Member(member) => {
            // (#271) \`obj?.prop\`: brif em obj==0; bloco null retorna 0;
            // bloco non-null faz member_expr normal. Resultado merged
            // preservando o tipo do access (Handle, F64, I32, etc).
            //
            // (#592 follow-up) Dupla avaliacao do obj quando m.obj e' OptChain
            // ou expressao com side effects. Materializa obj em var temp e
            // reescreve o member sintetico para usar a temp — assim
            // lower_member_expr nao re-evalua a chain.
            let obj_tv = lower_expr(ctx, &member.obj)?;
            let obj_i64 = ctx.coerce_to_i64(obj_tv).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, obj_i64, zero);

            // (#592 follow-up) Materializa obj em var temp ANTES do brif
            // para evitar re-avaliacao em lower_member_expr quando member.obj
            // e' OptChain/Call/etc. Para Ident simples nao precisa.
            let synthetic_member = if matches!(member.obj.as_ref(), Expr::Ident(_)) {
                member.clone()
            } else {
                let tmp_name = format!("__opt_recv_{}", ctx.next_opt_chain_temp_id());
                ctx.declare_local(&tmp_name, ValTy::Handle, obj_i64);
                // (#opt-chain-nested) Marca o temp como object literal
                // anonimo. Propaga campos: se member.obj eh chain do tipo
                // `root.field1.field2` onde root tem `field1` registrado
                // em local_nested_obj_field_types[(root, field2)], copia
                // esses tipos para local_obj_field_types[temp]. Cobre
                // `cfg?.server?.host` onde temp recebe nested(server).
                let inner_types = chain_inner_field_types(ctx, member.obj.as_ref());
                ctx.local_obj_field_types
                    .insert(tmp_name.clone(), inner_types);
                swc_ecma_ast::MemberExpr {
                    span: member.span,
                    obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: tmp_name.into(),
                        optional: false,
                    })),
                    prop: member.prop.clone(),
                }
            };

            let null_block = ctx.builder.create_block();
            let access_block = ctx.builder.create_block();
            let merge = ctx.builder.create_block();
            ctx.builder.ins().brif(is_null, null_block, &[], access_block, &[]);

            ctx.builder.switch_to_block(access_block);
            ctx.builder.seal_block(access_block);
            let access_tv = super::members::lower_member_expr(ctx, &synthetic_member)?;
            let access_ty = access_tv.ty;
            // (#795) Se o member access foi marcado como ambiguo
            // (var_member_call_values), propaga para o block param `result`
            // do merge — caso contrario o template literal renderiza
            // o sentinel bool/undefined como i64 raw.
            let access_was_ambig = ctx.var_member_call_values.contains(&access_tv.val);
            let access_val = match access_ty {
                ValTy::F64 | ValTy::I32 => access_tv.val,
                _ => ctx.coerce_to_i64(access_tv).val,
            };
            let merge_cl_ty = match access_ty {
                ValTy::F64 => cl::F64,
                ValTy::I32 => cl::I32,
                _ => cl::I64,
            };
            let result = ctx.builder.append_block_param(merge, merge_cl_ty);
            ctx.builder.ins().jump(merge, &[access_val.into()]);

            ctx.builder.switch_to_block(null_block);
            ctx.builder.seal_block(null_block);
            let z = match access_ty {
                ValTy::F64 => ctx.builder.ins().f64const(0.0),
                ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
                _ => ctx.builder.ins().iconst(cl::I64, 0),
            };
            ctx.builder.ins().jump(merge, &[z.into()]);

            ctx.builder.switch_to_block(merge);
            ctx.builder.seal_block(merge);
            // (#432) Marca o result para que lower_tpl emita "undefined"
            // em vez de "0" quando este valor cair em template literal.
            ctx.optional_chain_values.insert(result);
            // (#795) Propaga ambiguidade do access para o block param do merge
            // se o lower_member_expr marcou — assim TPL_COERCE_AUTO eh chamado
            // no template literal e detecta sentinelas (bool true/undefined).
            if access_was_ambig {
                ctx.var_member_call_values.insert(result);
            }
            Ok(TypedVal::new(result, access_ty))
        }
        swc_ecma_ast::OptChainBase::Call(call) => {
            // (#333) Caso `obj?.method(args)`: o swc representa como
            // `OptChainExpr<Call>` cujo callee e' outro `OptChainExpr<Member>`
            // (`obj?.method`). O lower antigo recursava em ambos OptChain
            // criando 2 niveis de blocks que nao conectavam corretamente —
            // Verifier "invalid block reference". Achatamos pra um unico
            // null-guard sobre `obj`: se `obj` e' null retorna 0 sem
            // chamar, senao faz a chamada normal `obj.method(args)`.
            if let swc_ecma_ast::Expr::OptChain(inner_opt) = call.callee.as_ref() {
                if let swc_ecma_ast::OptChainBase::Member(inner_member) =
                    inner_opt.base.as_ref()
                {
                    // Avalia `obj` UMA VEZ. Quando `inner_member.obj` é
                    // expressao complexa (OptChain aninhada, Call, etc),
                    // materializa em local temp e usa Ident sintetico —
                    // assim `lower_call` ve `Member(obj=Ident, prop=Ident)`,
                    // forma coberta. Sem isso, lower_call retorna
                    // "unsupported call expression form" pra `Member.obj=OptChain`,
                    // o `?` propaga e os blocks ja criados ficam orfaos
                    // (Verifier: "invalid block reference"). #481
                    let obj_tv = lower_expr(ctx, &inner_member.obj)?;
                    let obj_i64 = ctx.coerce_to_i64(obj_tv).val;
                    let zero = ctx.builder.ins().iconst(cl::I64, 0);
                    let is_null = ctx.builder.ins().icmp(IntCC::Equal, obj_i64, zero);

                    let null_block = ctx.builder.create_block();
                    let call_block = ctx.builder.create_block();
                    let merge = ctx.builder.create_block();
                    // Block param sempre i64 — o tipo logico (Handle vs F64
                    // bits) eh decidido depois pelo TypedVal retornado.
                    // Para metodos F64, fazemos bitcast do resultado pra
                    // i64 antes de jump pro merge.
                    let result = ctx.builder.append_block_param(merge, cl::I64);

                    ctx.builder.ins().brif(is_null, null_block, &[], call_block, &[]);

                    ctx.builder.switch_to_block(null_block);
                    ctx.builder.seal_block(null_block);
                    let z = ctx.builder.ins().iconst(cl::I64, 0);
                    ctx.builder.ins().jump(merge, &[z.into()]);

                    ctx.builder.switch_to_block(call_block);
                    ctx.builder.seal_block(call_block);

                    // Decide se reusa o ident original (obj simples) ou
                    // materializa em temp. Reuso evita declare_local extra.
                    let obj_for_synth: Box<Expr> = if matches!(
                        inner_member.obj.as_ref(),
                        Expr::Ident(_)
                    ) {
                        inner_member.obj.clone()
                    } else {
                        let tmp_name = format!(
                            "__opt_recv_{}",
                            ctx.next_opt_chain_temp_id()
                        );
                        ctx.declare_local(&tmp_name, ValTy::I64, obj_i64);
                        Box::new(Expr::Ident(swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: tmp_name.into(),
                            optional: false,
                        }))
                    };

                    // Constroi `obj.method(args)` sintetico (sem `?`).
                    let synthetic_member = swc_ecma_ast::MemberExpr {
                        span: inner_member.span,
                        obj: obj_for_synth,
                        prop: inner_member.prop.clone(),
                    };
                    let synthetic_callee =
                        Box::new(swc_ecma_ast::Expr::Member(synthetic_member));
                    let synthetic = CallExpr {
                        span: call.span,
                        ctxt: call.ctxt,
                        callee: swc_ecma_ast::Callee::Expr(synthetic_callee),
                        args: call.args.clone(),
                        type_args: call.type_args.clone(),
                    };
                    let call_tv = super::calls::lower_call(ctx, &synthetic)?;
                    let result_ty = call_tv.ty;
                    // Empacota result em i64 — F64 vai como bits.
                    let call_packed = if matches!(result_ty, ValTy::F64) {
                        ctx.builder.ins().bitcast(
                            cl::I64,
                            cranelift_codegen::ir::MemFlags::new(),
                            call_tv.val,
                        )
                    } else {
                        ctx.coerce_to_i64(call_tv).val
                    };
                    ctx.builder.ins().jump(merge, &[call_packed.into()]);

                    ctx.builder.switch_to_block(merge);
                    ctx.builder.seal_block(merge);
                    ctx.optional_chain_values.insert(result);
                    // Para F64 retornado, desempacota bits via bitcast.
                    let final_val = if matches!(result_ty, ValTy::F64) {
                        ctx.builder.ins().bitcast(
                            cl::F64,
                            cranelift_codegen::ir::MemFlags::new(),
                            result,
                        )
                    } else {
                        result
                    };
                    return Ok(TypedVal::new(final_val, result_ty));
                }
            }

            // Caso geral: `callee?.(args)` onde callee e' uma expressao
            // qualquer (var, member nao-optional, etc). Avalia callee, se
            // e' null retorna 0 sem chamar.
            let callee_tv = lower_expr(ctx, &call.callee)?;
            let callee_i64 = ctx.coerce_to_i64(callee_tv).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, callee_i64, zero);

            let null_block = ctx.builder.create_block();
            let call_block = ctx.builder.create_block();
            let merge = ctx.builder.create_block();
            let result = ctx.builder.append_block_param(merge, cl::I64);

            ctx.builder
                .ins()
                .brif(is_null, null_block, &[], call_block, &[]);

            ctx.builder.switch_to_block(null_block);
            ctx.builder.seal_block(null_block);
            let z = ctx.builder.ins().iconst(cl::I64, 0);
            ctx.builder.ins().jump(merge, &[z.into()]);

            ctx.builder.switch_to_block(call_block);
            ctx.builder.seal_block(call_block);
            let synthetic = CallExpr {
                span: call.span,
                ctxt: call.ctxt,
                callee: swc_ecma_ast::Callee::Expr(call.callee.clone()),
                args: call.args.clone(),
                type_args: call.type_args.clone(),
            };
            let call_tv = super::calls::lower_call(ctx, &synthetic)?;
            let call_i64 = ctx.coerce_to_i64(call_tv).val;
            ctx.builder.ins().jump(merge, &[call_i64.into()]);

            ctx.builder.switch_to_block(merge);
            ctx.builder.seal_block(merge);
            ctx.optional_chain_values.insert(result);
            Ok(TypedVal::new(result, ValTy::I64))
        }
    }
}

pub(super) fn lower_cond(ctx: &mut FnCtx, cond: &swc_ecma_ast::CondExpr) -> Result<TypedVal> {
    let test = lower_expr(ctx, &cond.test)?;
    let test_ty = test.ty;
    let test_i64 = ctx.coerce_to_i64(test).val;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    // (#or-empty-str) Mesma logica de truthy do logical: string vazia
    // (handle != 0) precisa ser tratada como falsy.
    let test_needs_truthy = matches!(test_ty, ValTy::Handle | ValTy::U64)
        || (matches!(test_ty, ValTy::I64)
            && ctx.var_member_call_values.contains(&test_i64));
    let truthy_val = if test_needs_truthy {
        let truthy_fn = ctx.get_extern(
            "__RTS_FN_RT_TRUTHY",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(truthy_fn, &[test_i64]);
        ctx.builder.inst_results(inst)[0]
    } else {
        test_i64
    };
    let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, truthy_val, zero);

    let then_block = ctx.builder.create_block();
    let else_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();

    let result_ty = promote_result_ty(ctx, &cond.cons, &cond.alt)?;
    let result_param = ctx
        .builder
        .append_block_param(merge_block, result_ty.cl_type());

    ctx.builder
        .ins()
        .brif(is_true, then_block, &[], else_block, &[]);

    ctx.builder.switch_to_block(then_block);
    ctx.builder.seal_block(then_block);
    let cons = lower_expr(ctx, &cond.cons)?;
    let cons_ambig = ctx.var_member_call_values.contains(&cons.val);
    let cons_val = coerce_result(ctx, cons, result_ty)?;
    ctx.builder.ins().jump(merge_block, &[cons_val.into()]);

    ctx.builder.switch_to_block(else_block);
    ctx.builder.seal_block(else_block);
    let alt = lower_expr(ctx, &cond.alt)?;
    let alt_ambig = ctx.var_member_call_values.contains(&alt.val);
    let alt_val = coerce_result(ctx, alt, result_ty)?;
    ctx.builder.ins().jump(merge_block, &[alt_val.into()]);

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    // (#83) Propaga ambiguity para o block param do merge — se algum
    // branch retornou I64 ambiguo (handle de string ou numero), o
    // resultado tambem eh ambiguo. Sem isso, `cond ? a : b` em concat
    // formatava handle como numero raw.
    if cons_ambig || alt_ambig {
        ctx.var_member_call_values.insert(result_param);
    }
    Ok(TypedVal::new(result_param, result_ty))
}

fn lower_logical(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    let lhs = lower_expr(ctx, &bin.left)?;
    let lhs_ty = lhs.ty;
    let lhs_i64 = ctx.coerce_to_i64(lhs).val;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let merge = ctx.builder.create_block();
    let result = ctx.builder.append_block_param(merge, cl::I64);

    // (#or-empty-str) Quando lhs eh Handle/U64, usa __RTS_FN_RT_TRUTHY
    // que reconhece string vazia como falsy (JS spec). Sem isso,
    // `'' || 'fb'` retornava '' porque handle de "" eh != 0.
    let lhs_needs_truthy = matches!(lhs_ty, ValTy::Handle | ValTy::U64)
        || (matches!(lhs_ty, ValTy::I64)
            && ctx.var_member_call_values.contains(&lhs_i64));
    let truthy_val = if lhs_needs_truthy {
        let truthy_fn = ctx.get_extern(
            "__RTS_FN_RT_TRUTHY",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(truthy_fn, &[lhs_i64]);
        ctx.builder.inst_results(inst)[0]
    } else {
        lhs_i64
    };

    let rhs_ty: ValTy;
    match bin.op {
        BinaryOp::LogicalAnd => {
            let rhs_block = ctx.builder.create_block();
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, truthy_val, zero);
            ctx.builder
                .ins()
                .brif(is_true, rhs_block, &[], merge, &[lhs_i64.into()]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            rhs_ty = rhs.ty;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        BinaryOp::LogicalOr => {
            let rhs_block = ctx.builder.create_block();
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, truthy_val, zero);
            ctx.builder
                .ins()
                .brif(is_true, merge, &[lhs_i64.into()], rhs_block, &[]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            rhs_ty = rhs.ty;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        BinaryOp::NullishCoalescing => {
            let rhs_block = ctx.builder.create_block();
            // (cross-runtime #902) Nullish em RTS: 0 (null classico),
            // i64::MIN+2 (undefined), i64::MIN+3 (null sentinela),
            // i64::MIN+4 (sparse hole — comporta como undefined).
            let is_zero = ctx.builder.ins().icmp(IntCC::Equal, lhs_i64, zero);
            let undef_s = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
            let null_s = ctx.builder.ins().iconst(cl::I64, i64::MIN + 3);
            let hole_s = ctx.builder.ins().iconst(cl::I64, i64::MIN + 4);
            let is_undef = ctx.builder.ins().icmp(IntCC::Equal, lhs_i64, undef_s);
            let is_null_s = ctx.builder.ins().icmp(IntCC::Equal, lhs_i64, null_s);
            let is_hole = ctx.builder.ins().icmp(IntCC::Equal, lhs_i64, hole_s);
            let a = ctx.builder.ins().bor(is_zero, is_undef);
            let b = ctx.builder.ins().bor(is_null_s, is_hole);
            let is_null = ctx.builder.ins().bor(a, b);
            ctx.builder
                .ins()
                .brif(is_null, rhs_block, &[], merge, &[lhs_i64.into()]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            rhs_ty = rhs.ty;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        _ => unreachable!(),
    }

    ctx.builder.switch_to_block(merge);
    ctx.builder.seal_block(merge);
    // Tipo de resultado: Handle se qualquer lado eh Handle; Bool se
    // ambos sao Bool (preserva semantica de \`a && b\` retornar bool em
    // JS quando ambos sao bool); senao I64.
    let out_ty = if matches!(lhs_ty, ValTy::Handle) || matches!(rhs_ty, ValTy::Handle) {
        ValTy::Handle
    } else if matches!(lhs_ty, ValTy::Bool) && matches!(rhs_ty, ValTy::Bool) {
        ValTy::Bool
    } else {
        ValTy::I64
    };
    Ok(TypedVal::new(result, out_ty))
}

fn promote_numeric(
    ctx: &mut FnCtx,
    lhs: TypedVal,
    rhs: TypedVal,
) -> Result<(
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    ValTy,
)> {
    if matches!(lhs.ty, ValTy::F64) || matches!(rhs.ty, ValTy::F64) {
        return Ok((to_f64(ctx, lhs), to_f64(ctx, rhs), ValTy::F64));
    }
    if matches!(lhs.ty, ValTy::I32) && matches!(rhs.ty, ValTy::I32) {
        return Ok((lhs.val, rhs.val, ValTy::I32));
    }
    let result_ty = if matches!(lhs.ty, ValTy::U64) || matches!(rhs.ty, ValTy::U64) {
        ValTy::U64
    } else {
        ValTy::I64
    };
    Ok((
        ctx.coerce_to_i64(lhs).val,
        ctx.coerce_to_i64(rhs).val,
        result_ty,
    ))
}

fn promote_result_ty(ctx: &FnCtx, cons: &Expr, alt: &Expr) -> Result<ValTy> {
    let guess = |expr: &Expr| match expr {
        Expr::Lit(Lit::Num(n))
            if n.value.fract() == 0.0
                && n.value >= i32::MIN as f64
                && n.value <= i32::MAX as f64 =>
        {
            Some(ValTy::I32)
        }
        Expr::Lit(Lit::Num(_)) => Some(ValTy::F64),
        Expr::Lit(Lit::Str(_)) => Some(ValTy::Handle),
        Expr::Lit(Lit::Bool(_)) => Some(ValTy::Bool),
        Expr::Ident(id) => ctx.var_ty(id.sym.as_str()),
        _ => None,
    };
    Ok(match (guess(cons), guess(alt)) {
        (Some(ValTy::F64), _) | (_, Some(ValTy::F64)) => ValTy::F64,
        (Some(ValTy::Handle), _) | (_, Some(ValTy::Handle)) => ValTy::Handle,
        (Some(ValTy::I32), Some(ValTy::I32)) => ValTy::I32,
        _ => ValTy::I64,
    })
}

fn coerce_result(
    ctx: &mut FnCtx,
    value: TypedVal,
    target: ValTy,
) -> Result<cranelift_codegen::ir::Value> {
    Ok(match target {
        ValTy::I32 => ctx.coerce_to_i32(value).val,
        ValTy::F64 => to_f64(ctx, value),
        ValTy::Handle => ctx.coerce_to_handle(value)?.val,
        _ => ctx.coerce_to_i64(value).val,
    })
}


pub(super) fn to_f64(ctx: &mut FnCtx, tv: TypedVal) -> cranelift_codegen::ir::Value {
    match tv.ty {
        ValTy::F64 => tv.val,
        ValTy::I32 => ctx.builder.ins().fcvt_from_sint(cl::F64, tv.val),
        _ => {
            let value = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().fcvt_from_sint(cl::F64, value)
        }
    }
}

pub(super) fn lower_add(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    if matches!(lhs.ty, ValTy::Handle) || matches!(rhs.ty, ValTy::Handle) {
        // Determine liveness of operands before coerce modifies types.
        // A Value is a "fresh" allocation when it was created by emit_str_handle
        // or coerce_to_handle and has not been assigned to a named variable yet
        // (tracked via FnCtx::fresh_handle_set). Fresh handles are safe to free
        // after concat — they have no other references.
        let lhs_is_fresh = ctx.fresh_handle_set.contains(&lhs.val);
        let rhs_is_fresh = ctx.fresh_handle_set.contains(&rhs.val);

        let concat = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        // (#627) Operando i64 ambiguo (resultado de obj.x sem tipo declarado,
        // member call em var, etc) usa TPL_COERCE_AUTO que detecta em runtime
        // se eh handle de string ou i64 puro. Sem isso `"X: " + obj.x` com
        // x: string vinda de destructuring de param emite STRING_FROM_I64
        // (formata handle bruto como numero).
        // I64 ao lado de Handle: ou esta em var_member_call_values (ja' era
        // ambiguo antes), ou o outro operando e' Handle e o I64 nao eh constante
        // (iconst) — indica contexto de string concat com param/var de tipo
        // desconhecido que pode ser handle (ex: callback de replace).
        // Constantes (iconst 0, iconst 42) sao numeros puros; nao usar TPL_COERCE_AUTO.
        let lhs_vec_slot = matches!(lhs.ty, ValTy::I64 | ValTy::U64)
            && ctx.var_vec_slot_values.contains(&lhs.val);
        let rhs_vec_slot = matches!(rhs.ty, ValTy::I64 | ValTy::U64)
            && ctx.var_vec_slot_values.contains(&rhs.val);
        let lhs_ambig = matches!(lhs.ty, ValTy::I64 | ValTy::U64)
            && ctx.var_member_call_values.contains(&lhs.val);
        let rhs_ambig = matches!(rhs.ty, ValTy::I64 | ValTy::U64)
            && ctx.var_member_call_values.contains(&rhs.val);
        let lhs_h = if lhs_vec_slot {
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_VEC_SLOT",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[lhs.val]);
            ctx.builder.inst_results(inst)[0]
        } else if lhs_ambig {
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_AUTO",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[lhs.val]);
            ctx.builder.inst_results(inst)[0]
        } else {
            ctx.coerce_to_handle(lhs)?.val
        };
        let rhs_h = if rhs_vec_slot {
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_VEC_SLOT",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[rhs.val]);
            ctx.builder.inst_results(inst)[0]
        } else if rhs_ambig {
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_AUTO",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[rhs.val]);
            ctx.builder.inst_results(inst)[0]
        } else {
            ctx.coerce_to_handle(rhs)?.val
        };

        // coerce_to_handle may have created new fresh handles for numeric operands.
        let lhs_free = lhs_is_fresh || (!matches!(lhs.ty, ValTy::Handle) && lhs_h != lhs.val);
        let rhs_free = rhs_is_fresh || (!matches!(rhs.ty, ValTy::Handle) && rhs_h != rhs.val);

        let inst = ctx.builder.ins().call(concat, &[lhs_h, rhs_h]);
        let result = ctx.builder.inst_results(inst)[0];

        // Register result for GC stack map tracking and legacy scope auto-free.
        ctx.declare_gc_handle(result);
        ctx.fresh_handle_set.insert(result);
        ctx.register_temp_handle(result);

        // Free fresh operand handles — they are not referenced anywhere else.
        // BUG FIX (#chat-tab): nao liberar handle que esta no str_handle_cache,
        // pois pode ser reusado por outro concat na mesma expressao
        // (ex: `"x" + "\t" + "y" + "\t" + "z"` — segundo "\t" busca cache,
        // recebe handle ja' liberado, concat le lixo). Cache eh keyed por
        // (bytes, block) -> Value; checar pelo Value.
        let lhs_cached = ctx.str_handle_cache.values().any(|&v| v == lhs.val);
        let rhs_cached = ctx.str_handle_cache.values().any(|&v| v == rhs.val);
        if let Ok(free_fn) = ctx.get_extern("__RTS_FN_NS_GC_STRING_FREE", &[cl::I64], Some(cl::I64)) {
            if lhs_free && !lhs_cached {
                ctx.fresh_handle_set.remove(&lhs.val);
                ctx.builder.ins().call(free_fn, &[lhs_h]);
            }
            if rhs_free && !rhs_cached {
                ctx.fresh_handle_set.remove(&rhs.val);
                ctx.builder.ins().call(free_fn, &[rhs_h]);
            }
        }

        return Ok(TypedVal::new(result, ValTy::Handle));
    }
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fadd(lv, rv),
        ValTy::I32 => ctx.builder.ins().iadd(lv, rv),
        _ => ctx.builder.ins().iadd(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

pub(super) fn lower_sub(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fsub(lv, rv),
        _ => ctx.builder.ins().isub(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

fn lower_mul(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    let val = match ty {
        ValTy::F64 => ctx.builder.ins().fmul(lv, rv),
        _ => ctx.builder.ins().imul(lv, rv),
    };
    Ok(TypedVal::new(val, ty))
}

fn lower_div(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    // (#584) JS spec: `/` sempre retorna f64. Mesmo `1 / 3` produz
    // 0.3333... em JS — int division e' truncamento e nao existe pra `/`.
    // Promovemos ambos os lados a f64 e fazemos fdiv. IEEE-754 cobre
    // divisor 0 naturalmente (Inf/-Inf/NaN), entao nao precisamos do
    // guard de #296 aqui.
    let lf = to_f64(ctx, lhs);
    let rf = to_f64(ctx, rhs);
    Ok(TypedVal::new(ctx.builder.ins().fdiv(lf, rf), ValTy::F64))
}

fn lower_mod(ctx: &mut FnCtx, lhs: TypedVal, rhs: TypedVal) -> Result<TypedVal> {
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    if matches!(ty, ValTy::F64) {
        let div = ctx.builder.ins().fdiv(lv, rv);
        let trunc = ctx.builder.ins().trunc(div);
        let mul = ctx.builder.ins().fmul(trunc, rv);
        return Ok(TypedVal::new(ctx.builder.ins().fsub(lv, mul), ty));
    }
    let val = lower_imod_safe(ctx, lv, rv, ty);
    Ok(TypedVal::new(val, ty))
}

/// Emite sdiv com guard pra divisor 0. (#296) Em divisor 0 retorna 0.
/// Estrategia branchless: bor(rv, is_zero_flag) garante divisor != 0;
fn lower_imod_safe(
    ctx: &mut FnCtx,
    lv: cranelift_codegen::ir::Value,
    rv: cranelift_codegen::ir::Value,
    ty: ValTy,
) -> cranelift_codegen::ir::Value {
    let cl_ty = if matches!(ty, ValTy::I32) { cl::I32 } else { cl::I64 };
    let zero = ctx.builder.ins().iconst(cl_ty, 0);
    let is_zero_b = ctx.builder.ins().icmp(IntCC::Equal, rv, zero);
    let bool_ty = ctx.builder.func.dfg.value_type(is_zero_b);
    let is_zero = if bool_ty == cl_ty {
        is_zero_b
    } else {
        ctx.builder.ins().uextend(cl_ty, is_zero_b)
    };
    let safe_rv = ctx.builder.ins().bor(rv, is_zero);
    let r = ctx.builder.ins().srem(lv, safe_rv);
    let one = ctx.builder.ins().iconst(cl_ty, 1);
    let mask = ctx.builder.ins().isub(is_zero, one);
    ctx.builder.ins().band(r, mask)
}

fn lower_icmp(ctx: &mut FnCtx, cc: IntCC, lhs: TypedVal, rhs: TypedVal) -> TypedVal {
    let cmp = if matches!(lhs.ty, ValTy::F64) || matches!(rhs.ty, ValTy::F64) {
        let lhs = to_f64(ctx, lhs);
        let rhs = to_f64(ctx, rhs);
        let fcc = match cc {
            IntCC::Equal => FloatCC::Equal,
            IntCC::NotEqual => FloatCC::NotEqual,
            IntCC::SignedLessThan => FloatCC::LessThan,
            IntCC::SignedLessThanOrEqual => FloatCC::LessThanOrEqual,
            IntCC::SignedGreaterThan => FloatCC::GreaterThan,
            IntCC::SignedGreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
            _ => FloatCC::Equal,
        };
        ctx.builder.ins().fcmp(fcc, lhs, rhs)
    } else {
        let lhs = ctx.coerce_to_i64(lhs).val;
        let rhs = ctx.coerce_to_i64(rhs).val;
        ctx.builder.ins().icmp(cc, lhs, rhs)
    };
    // Mantem cmp como i8 (Bool nativo Cranelift). Quando precisar i64
    // (ex: \`const flag = a < b\`), coerce_to_i64(Bool) faz uextend
    // explicito. Em brif (loop/if), to_branch_cond passa direto sem
    // re-extender — elimina \`uextend + iconst 0 + icmp ne\` que era
    // emitido em todos os hot loops.
    TypedVal::new(cmp, ValTy::Bool)
}

fn ident_name(expr: &Expr) -> Option<&str> {
    if let Expr::Ident(id) = expr {
        Some(id.sym.as_str())
    } else {
        None
    }
}

/// Strict equality (===) considera tipos como JS: Bool, Number (I32/I64/F64
/// unificados), String (Handle). U64 trata como numerico.
fn same_strict_kind(a: ValTy, b: ValTy) -> bool {
    fn kind(t: ValTy) -> u8 {
        match t {
            ValTy::Bool => 0,
            ValTy::Handle => 1,
            ValTy::I32 | ValTy::I64 | ValTy::F64 | ValTy::U64
            | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => 2,
        }
    }
    kind(a) == kind(b)
}

/// `lhs instanceof RhsClass`. RHS deve ser um Ident referenciando uma
/// classe registrada em `ctx.classes`. Lê o tag `__rts_class` do
/// receiver (handle de string com o nome da classe runtime) e compara
/// com cada classe `C` em `{RhsClass} ∪ descendants(RhsClass)`.
/// Resultado é `bool` (i64 0/1), retornando true se algum match.
fn lower_instanceof(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    use super::members::emit_class_tag_read;

    let class_name = match bin.right.as_ref() {
        Expr::Ident(id) => id.sym.as_str().to_string(),
        _ => return Err(anyhow!("instanceof RHS must be a class identifier")),
    };

    // Global classes: dispatch para runtime check via Entry tipo do handle.
    // Array → Entry::Vec; Object → Map ou qualquer; Date → DateMs;
    // RegExp → Regex; Map → Map; Set → Map (set usa Map storage); Error*
    // → Map com __rts_class. String/Number/Boolean → primitives sao falsy.
    if !ctx.classes.contains_key(&class_name) {
        let known_global = crate::abi::global_class_lookup(&class_name).is_some()
            || matches!(
                class_name.as_str(),
                "Array" | "Object" | "Map" | "Set" | "Boolean" | "Function"
            );
        if known_global {
            return lower_global_instanceof(ctx, &class_name, &bin.left);
        }
        return Err(anyhow!("instanceof RHS `{class_name}` is not a known class"));
    }

    let lhs = lower_expr(ctx, &bin.left)?;
    let recv = ctx.coerce_to_i64(lhs).val;

    // Coleta nomes de todas as classes que são RhsClass ou herdam dela.
    let mut matches: Vec<String> = Vec::new();
    for (name, meta) in ctx.classes.iter() {
        let mut cur = name.clone();
        loop {
            if cur == class_name {
                matches.push(name.clone());
                break;
            }
            match ctx.classes.get(&cur).and_then(|m| m.super_class.clone()) {
                Some(p) => cur = p,
                None => break,
            }
        }
        let _ = meta;
    }

    let tag = emit_class_tag_read(ctx, recv, &class_name)?;

    // OR de string-equal contra cada nome.
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let mut acc = zero;
    let str_eq = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_EQ",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    for name in &matches {
        let (kp, kl) = ctx.emit_str_literal(name.as_bytes())?;
        // emit_str_literal retorna (ptr, len) — precisamos de string handle.
        // GC_STRING_EQ compara dois handles. Em vez disso usamos
        // gc.string_from_static(ptr, len) para criar handle e comparar.
        let mk_static = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(mk_static, &[kp, kl]);
        let lit_handle = ctx.builder.inst_results(inst)[0];

        let inst = ctx.builder.ins().call(str_eq, &[tag, lit_handle]);
        let eq = ctx.builder.inst_results(inst)[0];
        acc = ctx.builder.ins().bor(acc, eq);
    }

    Ok(TypedVal::new(acc, ValTy::Bool))
}

/// `lhs instanceof <GlobalClass>` — dispatcha para runtime fn que detecta
/// Entry type. Array → Vec; Date → DateMs; RegExp → Regex; Map/Set → Map;
/// Error* → Map com tag `__rts_class` correspondente; Object → qualquer
/// handle nao primitivo; primitives nunca sao instance.
fn lower_global_instanceof(
    ctx: &mut FnCtx,
    class_name: &str,
    lhs_expr: &Expr,
) -> Result<TypedVal> {
    let lhs = lower_expr(ctx, lhs_expr)?;
    // F64/I32/Bool sao primitives puros — nunca passam instanceof <class>.
    // I64/U64 podem carregar handles (ex: callback param sem tipo onde
    // o array element eh handle, OU var_member_call ambiguo). Caem no
    // runtime check via IS_* que retorna 0 para handles invalidos — sem
    // regressao em primitivos. (cross-runtime #278)
    if matches!(lhs.ty, ValTy::F64 | ValTy::I32 | ValTy::Bool) {
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        return Ok(TypedVal::new(zero, ValTy::Bool));
    }
    let recv = ctx.coerce_to_i64(lhs).val;

    // Error: qualquer Entry::ErrorObj passa.
    if class_name == "Error" {
        let f = ctx.get_extern("__RTS_FN_GL_IS_ERROR", &[cl::I64], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(f, &[recv]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(v, ValTy::Bool));
    }
    // TypeError/RangeError/etc.: checa name field exato.
    if matches!(
        class_name,
        "TypeError" | "RangeError" | "SyntaxError" | "ReferenceError"
    ) {
        let (np, nl) = ctx.emit_str_literal(class_name.as_bytes())?;
        let f = ctx.get_extern(
            "__RTS_FN_GL_IS_ERROR_NAMED",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(f, &[recv, np, nl]);
        let v = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(v, ValTy::Bool));
    }

    // Object: qualquer handle nao-primitivo eh "object" em JS — Map e Vec
    // ambos passam. Verificacao via 2 calls com OR.
    if class_name == "Object" {
        let is_map = ctx.get_extern("__RTS_FN_NS_GC_IS_MAP_LIKE", &[cl::I64], Some(cl::I64))?;
        let is_vec = ctx.get_extern("__RTS_FN_NS_GC_IS_VEC", &[cl::I64], Some(cl::I64))?;
        let inst1 = ctx.builder.ins().call(is_map, &[recv]);
        let m = ctx.builder.inst_results(inst1)[0];
        let inst2 = ctx.builder.ins().call(is_vec, &[recv]);
        let v = ctx.builder.inst_results(inst2)[0];
        let or = ctx.builder.ins().bor(m, v);
        return Ok(TypedVal::new(or, ValTy::Bool));
    }
    // Outros: chama runtime fn por tipo.
    let sym = match class_name {
        "Array" => "__RTS_FN_NS_GC_IS_VEC",
        "Date" => "__RTS_FN_NS_GC_IS_DATE",
        "RegExp" => "__RTS_FN_NS_GC_IS_REGEX",
        "Map" | "Set" | "WeakMap" | "WeakSet" | "Function" => "__RTS_FN_NS_GC_IS_MAP_LIKE",
        "Promise" => "__RTS_FN_NS_GC_IS_PROMISE",
        // String/Number/Boolean: instances primitivas, sempre false em handle nao-string.
        // Permite "x" instanceof String === false (string primitive).
        "String" | "Number" | "Boolean" | "Symbol" => {
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            return Ok(TypedVal::new(zero, ValTy::Bool));
        }
        _ => {
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            return Ok(TypedVal::new(zero, ValTy::Bool));
        }
    };
    let f = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(f, &[recv]);
    let v = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(v, ValTy::Bool))
}

/// Para `obj.A.B` ou `obj?.A?.B`, retorna o map `local_nested_obj_field_types`
/// associado a (root_name, B) — i.e., os tipos de campos do sub-objeto que
/// resulta dessa chain. Usado em `lower_opt_chain` para que o temp interno
/// herde os field_types e o member access subsequente resolva corretamente.
fn chain_inner_field_types(
    ctx: &crate::codegen::lower::ctx::FnCtx,
    e: &Expr,
) -> std::collections::HashMap<String, ValTy> {
    fn extract_path(e: &Expr) -> Option<(String, Vec<String>)> {
        match e {
            Expr::Ident(id) => Some((id.sym.to_string(), Vec::new())),
            Expr::Member(mi) => {
                if let MemberProp::Ident(p) = &mi.prop {
                    let (root, mut path) = extract_path(&mi.obj)?;
                    path.push(p.sym.to_string());
                    Some((root, path))
                } else {
                    None
                }
            }
            Expr::OptChain(o) => {
                if let swc_ecma_ast::OptChainBase::Member(mi) = o.base.as_ref() {
                    if let MemberProp::Ident(p) = &mi.prop {
                        let (root, mut path) = extract_path(&mi.obj)?;
                        path.push(p.sym.to_string());
                        Some((root, path))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    if let Some((root, path)) = extract_path(e) {
        if let Some(last) = path.last() {
            let key = (root, last.clone());
            if let Some(types) = ctx.local_nested_obj_field_types.get(&key) {
                return types.clone();
            }
            if let Some(types) = ctx.global_nested_obj_field_types.get(&key) {
                return types.clone();
            }
        }
    }
    std::collections::HashMap::new()
}
