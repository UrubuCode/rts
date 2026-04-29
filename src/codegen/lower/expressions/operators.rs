use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{
    InstBuilder,
    condcodes::{FloatCC, IntCC},
    types as cl,
};
use swc_ecma_ast::{BinExpr, BinaryOp, CallExpr, Expr, Lit, UpdateOp};

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
    // \`x / 2^k\` vira \`x >> k\` (sshr arithmetic) quando POT positivo.
    // Sshr e' aritmetico — preserva sinal. \`-8 / 4 = -2\` continua
    // valido com sshr. Cranelift egraph nao faz pra signed.
    if lhs_is_int && matches!(bin.op, BinaryOp::Div) {
        if let Some(opt) = div_imm_peephole(ctx, &lhs, imm) {
            return Ok(Some(opt));
        }
    }
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
///     adj  = (x >> (BITS-1)) & (n-1)    // -1 todos bits se x < 0, else 0
///     r    = (x + adj) & (n-1)          // r positivo
///     r    = r - adj                    // ajusta sinal
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

/// `x / imm` peephole. POT positivo vira sshr (arithmetic right shift,
/// preserva sinal: -8 >> 2 = -2 = -8 / 4). Para imm < 0 ou nao-POT,
/// caminho default (sdiv).
fn div_imm_peephole(
    ctx: &mut FnCtx,
    lhs: &TypedVal,
    imm: i64,
) -> Option<TypedVal> {
    if imm == 1 {
        return Some(*lhs);
    }
    if imm > 1 && (imm as u64).is_power_of_two() {
        let k = imm.trailing_zeros() as i64;
        let max_k = if matches!(lhs.ty, ValTy::I32) { 30 } else { 62 };
        if k <= max_k {
            let v = match lhs.ty {
                ValTy::I32 => ctx.builder.ins().sshr_imm(lhs.val, k),
                _ => {
                    let lv = ctx.coerce_to_i64(*lhs).val;
                    ctx.builder.ins().sshr_imm(lv, k)
                }
            };
            let ty = if matches!(lhs.ty, ValTy::I32) { ValTy::I32 } else { ValTy::I64 };
            return Some(TypedVal::new(v, ty));
        }
    }
    None
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

    // === / !== com tipos diferentes em compile-time → const false/true.
    // (#306) JS strict equality nao coerce; `0 === false` deve ser false.
    // Bool e' detectavel separado de I64/F64 em ValTy mesmo backed por
    // mesmo cl_type, e Handle (string) e' distinto de numericos.
    if matches!(bin.op, BinaryOp::EqEqEq | BinaryOp::NotEqEq) {
        if !same_strict_kind(lhs.ty, rhs.ty) {
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
            Ok(TypedVal::new(ctx.builder.ins().ushr(lv, rv), ty))
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
            super::members::lower_member_expr(ctx, member)
        }
        swc_ecma_ast::OptChainBase::Call(call) => {
            // `callee?.(args)`: se callee for 0 (null), retorna 0 sem chamar.
            // Caso contrario, faz call_indirect via i64 funcptr.
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
            Ok(TypedVal::new(result, ValTy::I64))
        }
    }
}

pub(super) fn lower_cond(ctx: &mut FnCtx, cond: &swc_ecma_ast::CondExpr) -> Result<TypedVal> {
    let test = lower_expr(ctx, &cond.test)?;
    let test_i64 = ctx.coerce_to_i64(test).val;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, test_i64, zero);

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
    let cons_val = coerce_result(ctx, cons, result_ty)?;
    ctx.builder.ins().jump(merge_block, &[cons_val.into()]);

    ctx.builder.switch_to_block(else_block);
    ctx.builder.seal_block(else_block);
    let alt = lower_expr(ctx, &cond.alt)?;
    let alt_val = coerce_result(ctx, alt, result_ty)?;
    ctx.builder.ins().jump(merge_block, &[alt_val.into()]);

    ctx.builder.switch_to_block(merge_block);
    ctx.builder.seal_block(merge_block);
    Ok(TypedVal::new(result_param, result_ty))
}

fn lower_logical(ctx: &mut FnCtx, bin: &BinExpr) -> Result<TypedVal> {
    let lhs = lower_expr(ctx, &bin.left)?;
    let lhs_i64 = ctx.coerce_to_i64(lhs).val;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let merge = ctx.builder.create_block();
    let result = ctx.builder.append_block_param(merge, cl::I64);

    match bin.op {
        BinaryOp::LogicalAnd => {
            let rhs_block = ctx.builder.create_block();
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, lhs_i64, zero);
            ctx.builder
                .ins()
                .brif(is_true, rhs_block, &[], merge, &[lhs_i64.into()]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        BinaryOp::LogicalOr => {
            let rhs_block = ctx.builder.create_block();
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, lhs_i64, zero);
            ctx.builder
                .ins()
                .brif(is_true, merge, &[lhs_i64.into()], rhs_block, &[]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        BinaryOp::NullishCoalescing => {
            let rhs_block = ctx.builder.create_block();
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, lhs_i64, zero);
            ctx.builder
                .ins()
                .brif(is_null, rhs_block, &[], merge, &[lhs_i64.into()]);
            ctx.builder.switch_to_block(rhs_block);
            ctx.builder.seal_block(rhs_block);
            let rhs = lower_expr(ctx, &bin.right)?;
            let rhs_i64 = ctx.coerce_to_i64(rhs).val;
            ctx.builder.ins().jump(merge, &[rhs_i64.into()]);
        }
        _ => unreachable!(),
    }

    ctx.builder.switch_to_block(merge);
    ctx.builder.seal_block(merge);
    Ok(TypedVal::new(result, ValTy::I64))
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
        let concat = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let lhs_h = ctx.coerce_to_handle(lhs)?.val;
        let rhs_h = ctx.coerce_to_handle(rhs)?.val;
        let inst = ctx.builder.ins().call(concat, &[lhs_h, rhs_h]);
        return Ok(TypedVal::new(
            ctx.builder.inst_results(inst)[0],
            ValTy::Handle,
        ));
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
    // (#296) sdiv crasha em divisor 0. Solucao: int/int onde o divisor e'
    // literal nao-zero mantem sdiv (caminho rapido); caso contrario emite
    // guard inline que retorna i64. Em divisor 0, retorna 0 como sentinel
    // — nao e' Infinity exato mas evita trap. Float div ja' e' IEEE-754,
    // f.div_zero retorna Inf/-Inf/NaN naturalmente.
    let (lv, rv, ty) = promote_numeric(ctx, lhs, rhs)?;
    if matches!(ty, ValTy::F64) {
        return Ok(TypedVal::new(ctx.builder.ins().fdiv(lv, rv), ty));
    }
    let val = lower_idiv_safe(ctx, lv, rv, ty);
    Ok(TypedVal::new(val, ty))
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
/// em divisor 0, divide por 1 (mascara is_zero=1 entra no rv). Depois
/// AND com !is_zero zera o resultado quando original era 0.
/// Sem branches, sem select — IR mais previsivel pra Cranelift.
/// Emite sdiv com guard branchless pra divisor 0. (#296) Em divisor 0
/// retorna 0 (sentinel). Estrategia: `safe_rv = rv | is_zero` evita o
/// trap, depois `result & (is_zero - 1)` mascara para 0 no caso original 0.
fn lower_idiv_safe(
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
    let q = ctx.builder.ins().sdiv(lv, safe_rv);
    let one = ctx.builder.ins().iconst(cl_ty, 1);
    let mask = ctx.builder.ins().isub(is_zero, one);
    ctx.builder.ins().band(q, mask)
}

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
            ValTy::I32 | ValTy::I64 | ValTy::F64 | ValTy::U64 => 2,
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
    if !ctx.classes.contains_key(&class_name) {
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
