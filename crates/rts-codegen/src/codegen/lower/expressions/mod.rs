//! Expression lowering to Cranelift IR.

mod basics;
mod calls;
pub(crate) mod members;
mod operators;

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::InstBuilder;
use swc_ecma_ast::{BinExpr, BinaryOp, Expr, Lit, MemberProp};

use super::ctx::{FnCtx, TypedVal, ValTy};
use super::compile::class::class_setter_name;

use self::calls::{
    AccessorKind, emit_virtual_accessor_dispatch, lower_call, lower_new,
    lower_super_prop_assign, lower_super_prop_read, resolve_setter_owner,
};
// (#1281) re-export p/ lower_return_stmt reificar arrow liftada que escapa.
pub(crate) use self::calls::emit_lifted_arrow_handle_with_captures;
// (cross-runtime closures) re-export p/ main_fn registrar ABI das user fns.
pub(crate) use self::calls::emit_user_fn_addr;
pub(crate) use self::members::val_ty_to_kind;
use self::members::{
    assign_target_field_is_f64, class_field_uses_flat, emit_flat_field_write,
    field_is_readonly_in_hierarchy, lhs_static_class, lower_array_lit, lower_member_expr,
    lower_object_lit, validate_private_scope, validate_visibility,
};
use self::operators::{lower_bin, lower_cond, lower_opt_chain, lower_update_expr, to_f64};

/// Compiles a SWC expression and returns a typed Cranelift value.
pub fn lower_expr(ctx: &mut FnCtx, expr: &Expr) -> Result<TypedVal> {
    match expr {
        Expr::Lit(lit) => basics::lower_lit(ctx, lit),
        Expr::Ident(id) => lower_ident_expr(ctx, id.sym.as_str()),
        Expr::Paren(p) => lower_expr(ctx, &p.expr),
        Expr::Unary(u) => basics::lower_unary(ctx, u),
        Expr::Update(u) => lower_update_expr(ctx, u),
        Expr::Bin(bin) => lower_bin(ctx, bin),
        Expr::Assign(assign) => lower_assign_expr(ctx, assign),
        Expr::Call(call) => lower_call(ctx, call),
        Expr::Tpl(tpl) => basics::lower_tpl(ctx, tpl),
        Expr::TaggedTpl(tt) => lower_tagged_tpl(ctx, tt),
        Expr::Cond(cond) => lower_cond(ctx, cond),
        Expr::Array(arr) => lower_array_lit(ctx, arr),
        Expr::Object(obj) => lower_object_lit(ctx, obj),
        Expr::Member(member) => lower_member_expr(ctx, member),
        Expr::OptChain(opt) => lower_opt_chain(ctx, opt),
        Expr::SuperProp(sp) => lower_super_prop_read(ctx, sp),
        Expr::New(new_expr) => lower_new(ctx, new_expr),
        Expr::This(_) => {
            // Em metodo de classe (compilado com `this` como param real),
            // ler o local. Em fn plain nao-arrow, ler do thread-local
            // `this` slot (populado por Function.call/.apply em runtime).
            // Em arrow plain top-level, slot retorna 0 (=undefined).
            if let Some(v) = ctx.read_local("this") {
                Ok(v)
            } else {
                let fref =
                    ctx.get_extern("__RTS_FN_RT_THIS_GET", &[], Some(cranelift_codegen::ir::types::I64))?;
                let inst = ctx.builder.ins().call(fref, &[]);
                let v = ctx.builder.inst_results(inst)[0];
                Ok(TypedVal::new(v, ValTy::I64))
            }
        }
        Expr::TsAs(a) => lower_expr(ctx, &a.expr),
        Expr::TsTypeAssertion(a) => lower_expr(ctx, &a.expr),
        Expr::TsConstAssertion(a) => lower_expr(ctx, &a.expr),
        Expr::TsSatisfies(a) => lower_expr(ctx, &a.expr),
        Expr::TsNonNull(n) => lower_expr(ctx, &n.expr),
        Expr::Await(a) => lower_expr(ctx, &a.arg),
        Expr::Seq(s) => {
            // Comma operator: avalia tudo pelo side-effect, retorna o ultimo.
            let mut last: Option<TypedVal> = None;
            for e in &s.exprs {
                last = Some(lower_expr(ctx, e)?);
            }
            last.ok_or_else(|| anyhow!("empty sequence expression"))
        }
        Expr::MetaProp(mp) => {
            // `import.meta` retorna Map vazio (objeto). Suficiente para
            // `typeof import.meta === "object"`. `import.meta.url` cai no
            // path Member -> lower esse Map -> map_get("url") retorna
            // sentinela undefined; codegen de Member sobre import.meta
            // pode emitir url via path especial (handled em members.rs).
            // `new.target` retorna undefined (i64::MIN+2) — sem suporte real.
            use swc_ecma_ast::MetaPropKind;
            match mp.kind {
                MetaPropKind::ImportMeta => {
                    let new_fn = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
                        &[],
                        Some(cranelift_codegen::ir::types::I64),
                    )?;
                    let inst = ctx.builder.ins().call(new_fn, &[]);
                    let h = ctx.builder.inst_results(inst)[0];
                    Ok(TypedVal::new(h, ValTy::Handle))
                }
                MetaPropKind::NewTarget => {
                    let v = ctx.builder.ins().iconst(
                        cranelift_codegen::ir::types::I64,
                        i64::MIN + 2,
                    );
                    // Marca como ambiguo para que TRUTHY detecte sentinela
                    // undefined em ternary/`||` (sem isso `new.target ? a : b`
                    // sempre cai em `a` porque i64::MIN+2 != 0).
                    ctx.var_member_call_values.insert(v);
                    Ok(TypedVal::new(v, ValTy::I64))
                }
            }
        }
        other => Err(anyhow!("unsupported expression: {}", expr_kind_name(other))),
    }
}

/// (#376/#195) Resolve `__captured_this` para o valor de `this` a capturar por
/// valor: le o local `this` (metodo de classe, this e' param real) ou — quando
/// nao ha local (metodo de OBJETO, onde `this` vive no slot thread-local) —
/// emite `THIS_GET()`. Assim uma arrow `() => this.v` retornada de um metodo de
/// objeto captura o `this` corrente (o objeto), nao o this no momento da chamada.
fn resolve_captured_this(ctx: &mut FnCtx) -> Result<TypedVal> {
    use cranelift_codegen::ir::InstBuilder;
    use crate::codegen::lower::ctx::ValTy;
    if let Some(v) = ctx.read_local("this") {
        return Ok(v);
    }
    let fref =
        ctx.get_extern("__RTS_FN_RT_THIS_GET", &[], Some(cranelift_codegen::ir::types::I64))?;
    let inst = ctx.builder.ins().call(fref, &[]);
    let v = ctx.builder.inst_results(inst)[0];
    Ok(TypedVal::new(v, ValTy::I64))
}

fn lower_ident_expr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    // Alias de import (`import { add as plus } from "./lib"` -> plus->add):
    // se o ident e' um alias local, troca pelo nome original do source antes
    // de qualquer lookup. Apenas quando NAO ha local sombreando o alias,
    // pra preservar a ordem natural de scope (locals > imports).
    if ctx.read_local(name).is_none() {
        if let Some(orig) = ctx.local_alias_map.get(name).cloned() {
            return lower_ident_expr(ctx, &orig);
        }
    }
    if let Some(tv) = ctx.read_local(name) {
        // (#627) Propaga ambiguidade para o Value carregado: var declarada
        // com init de obj.x sem tipo conhecido marca cada read como
        // var_member_call_values para que `+` use TPL_COERCE_AUTO.
        if ctx.local_ambiguous_vars.contains(name) {
            ctx.var_member_call_values.insert(tv.val);
        }
        return Ok(tv);
    }
    if ctx.user_fns.contains_key(name) {
        // Arrows hoisted via hoist_fn_expressions têm prefixo __hoisted_arrow_.
        // Quando aparecem em posição de valor (não call), reificamos como
        // Function handle com is_arrow=1 para que INVOKE_AUTO não faça
        // THIS_PUSH — arrow não tem own this.
        // (#195) Lifted fn-EXPR (`function(){...}` retornado, p/ closures
        // mutaveis via celula) que captura free vars: reifica com bound_args.
        // SEM captura cai no caminho normal (raw fn addr) — NAO no fallback de
        // `this` dos arrows (que reificava com bound_this errado, quebrando
        // metodos de objeto liftados como next() de iteradores custom).
        if name.starts_with("__hoisted_fn_") {
            if let Some(captures) =
                crate::codegen::lower::passes::this_arrow::lifted_arrow_captures(name)
            {
                let mut cap_vals: Vec<crate::codegen::lower::ctx::TypedVal> =
                    Vec::with_capacity(captures.len());
                for c in &captures {
                    let tv = if c == "__captured_this" {
                        Some(resolve_captured_this(ctx)?)
                    } else {
                        ctx.read_local(c)
                    };
                    match tv {
                        Some(v) => cap_vals.push(v),
                        None => break,
                    }
                }
                if cap_vals.len() == captures.len() {
                    return calls::emit_lifted_arrow_handle_with_captures(ctx, name, &cap_vals);
                }
            }
            // (cross-runtime closures) fn-EXPR VARIADIC (`function(...rest){...}`)
            // retornada/armazenada como valor: precisa ser handle (nao raw addr)
            // p/ gravar rest_param_idx no FunctionData — so' assim o invoke via
            // handle empacota o tail num array. Sem isto `(...args)=>...` em
            // forma de fn-expr (trampoline/curry) recebia os args soltos: o
            // primeiro virava o array-rest e os demais sumiam (mul(2,3,4)->0).
            if crate::codegen::lower::passes::args::rest_args::fn_rest_idx(name).is_some() {
                return calls::emit_lifted_arrow_handle_with_captures(ctx, name, &[]);
            }
            return emit_user_fn_addr(ctx, name);
        }
        if name.starts_with("__hoisted_arrow_")
            || name.starts_with("__lifted_arrow_")
        {
            // (#195) Arrow que captura variaveis livres por valor: reifica via
            // REIFY_CAPTURED passando os valores das capturas como bound_args.
            // Isso da captura-por-ativacao correta (curry/recursao), ao
            // contrario do promote-to-global compartilhado.
            if let Some(captures) =
                crate::codegen::lower::passes::this_arrow::lifted_arrow_captures(name)
            {
                let mut cap_vals: Vec<crate::codegen::lower::ctx::TypedVal> =
                    Vec::with_capacity(captures.len());
                for c in &captures {
                    // (#376 camada 3) `__captured_this` resolve para o `this`
                    // atual — a arrow capturou `this` por valor (bound_arg). Em
                    // metodo de OBJETO o this vive no slot thread-local, entao
                    // resolve_captured_this emite THIS_GET() (nunca None).
                    let tv = if c == "__captured_this" {
                        Some(resolve_captured_this(ctx)?)
                    } else {
                        ctx.read_local(c)
                    };
                    match tv {
                        Some(v) => cap_vals.push(v),
                        None => break,
                    }
                }
                // So' usa o caminho de captura quando TODAS resolvem como
                // local em escopo (senao cai no fallback antigo).
                if cap_vals.len() == captures.len() {
                    return calls::emit_lifted_arrow_handle_with_captures(ctx, name, &cap_vals);
                }
            }
            // (#195) Arrow liftada VARIADIC (`...rest`), mesmo sem captura
            // resolvida: precisa ser reificada como handle (nao raw fn-addr) pra
            // gravar `rest_param_idx` no FunctionData — so' assim o invoke via
            // handle empacota o tail num array. Sem isto, `(...fns) => ...`
            // reificava como endereco cru e o rest chegava como args soltos.
            if crate::codegen::lower::passes::args::rest_args::fn_rest_idx(name).is_some() {
                return calls::emit_lifted_arrow_handle_with_captures(ctx, name, &[]);
            }
            // Se `this` é local (escopo de método de classe), captura o valor
            // atual no handle via REIFY_BOUND. Arrows armazenadas/retornadas
            // leem `this` correto mesmo após o método retornar.
            // Se `this` NÃO está em escopo (callbacks de describe/test etc.),
            // retorna raw fn addr — a infraestrutura espera ponteiro direto.
            if let Some(tv) = ctx.read_local("this") {
                let bound_this = ctx.coerce_to_i64(tv).val;
                return calls::emit_hoisted_arrow_handle(ctx, name, Some(bound_this));
            }
            // (A+C — #1281) Arrow com param/ret number/bool (kind!=0): raw fn
            // addr `(f64)->f64` nao pode ser invocado via INVOKE_AUTO (ABI i64).
            // Reifica como handle TYPED p/ invoke_typed fazer from_bits.
            // Arrow todo-i64 mantem raw fn addr (byte-identico ao de hoje).
            let has_nonzero_kind = ctx
                .user_fns
                .get(name)
                .map(|f| {
                    f.params
                        .iter()
                        .any(|p| members::val_ty_to_kind(*p) != 0)
                        || f.ret.map(members::val_ty_to_kind).unwrap_or(0) != 0
                })
                .unwrap_or(false);
            if has_nonzero_kind {
                return calls::emit_hoisted_arrow_handle(ctx, name, None);
            }
            // Fora de escopo de classe — raw fn addr como antes.
            return emit_user_fn_addr(ctx, name);
        }
        return emit_user_fn_addr(ctx, name);
    }
    // (#298) Globais JS NaN/Infinity/undefined. NaN e Infinity sao
    // f64 IEEE; \`undefined\` em RTS nao tem representacao distinta de
    // 0/null entao mapeamos para 0 (caller que comparar com === detecta
    // tipo via context). Cobre uso comum em template/aritmetica.
    use cranelift_codegen::ir::InstBuilder;
    use crate::codegen::lower::ctx::ValTy;
    match name {
        "NaN" => {
            let v = ctx.builder.ins().f64const(f64::NAN);
            return Ok(TypedVal::new(v, ValTy::F64));
        }
        "Infinity" => {
            let v = ctx.builder.ins().f64const(f64::INFINITY);
            return Ok(TypedVal::new(v, ValTy::F64));
        }
        "undefined" => {
            // Sentinel i64::MIN+2 para undefined. TPL_COERCE_AUTO e
            // STRING_FROM_I64 convertem para "undefined" em string contexts;
            // NullishCoalescing e is_undefined checks reconhecem este sentinel.
            let sentinel = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I64, i64::MIN + 2);
            return Ok(TypedVal::new(sentinel, ValTy::I64));
        }
        _ => {}
    }
    // (#360) `globalThis` em posicao de VALOR -> handle do Map singleton. Assim
    // `const g = globalThis; g.x = v` ou um param ligado a globalThis
    // (`(function(_root){ _root._lib = ... })(globalThis)`) gravam/leem no mesmo
    // Map que `globalThis.x`. (Leituras `globalThis.X` e `globalThis[k]` sao
    // interceptadas antes, entao identidade de builtins nao eh afetada.)
    if name == "globalThis" {
        use cranelift_codegen::ir::InstBuilder;
        let gt_fn = ctx.get_extern(
            "__RTS_FN_RT_GLOBAL_THIS_MAP",
            &[],
            Some(cranelift_codegen::ir::types::I64),
        )?;
        let inst = ctx.builder.ins().call(gt_fn, &[]);
        let h = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(h, ValTy::Handle));
    }
    // (cross-runtime #304/#310) Global JS classes/namespaces (Promise, Date,
    // Error, console, etc.) referenciadas como valor — retorna handle
    // sentinel de string com o nome. Suficiente para `Promise.try.call(...)`,
    // `const g = console.group`, `typeof console === "object"` etc.
    let is_global = crate::abi::global_class_lookup(name).is_some()
        || crate::abi::SPECS.iter().any(|s| s.name == name);
    if is_global {
        return ctx.emit_str_handle(name.as_bytes());
    }
    // (cross-runtime #1079) JS builtins sem GlobalClassSpec/Namespace dedicado
    // — referenciados como valor (ex: `globalThis.Array === Array`, `typeof Map`,
    // pattern `if (Array in globalThis)`). Retorna handle sentinel de string com
    // o nome. Identidade via `===` funciona porque ambos os lados produzem o
    // mesmo handle de string interna.
    if matches!(name,
        "Array" | "Object" | "Map" | "Set" | "Proxy" | "Reflect"
        | "Math" | "JSON" | "Atomics" | "Intl"
        | "ArrayBuffer" | "SharedArrayBuffer"
    ) {
        return ctx.emit_str_handle(name.as_bytes());
    }
    // (cross-runtime #1072) Classes de usuario em posicao de valor —
    // retorna handle sentinel "[class <Name>]". Suporta Reflect.construct,
    // identidade, typeof (que classifica via classes.contains_key → "function").
    if ctx.classes.contains_key(name) {
        let label = format!("[class {name}]");
        return ctx.emit_str_handle(label.as_bytes());
    }
    Err(anyhow!("undefined variable `{name}`"))
}

/// (#1051) Detecta se uma expressao eh literal float nao-inteiro (ex:
/// \`3.14\`, \`-0.5\`). Usado para decidir se preserva precisao via
/// bitcast em vez de fcvt_to_sint_sat (que trunca). Peel TS wrappers
/// e unary minus em num literal.
pub(super) fn rhs_is_non_integer_float_lit(e: &Expr) -> bool {
    let mut cur = e;
    loop {
        match cur {
            Expr::Paren(p) => cur = &p.expr,
            Expr::TsAs(a) => cur = &a.expr,
            Expr::TsConstAssertion(a) => cur = &a.expr,
            Expr::TsTypeAssertion(a) => cur = &a.expr,
            Expr::TsSatisfies(a) => cur = &a.expr,
            Expr::TsNonNull(n) => cur = &n.expr,
            Expr::Lit(Lit::Num(n)) => return n.value.fract() != 0.0,
            Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Minus) => {
                cur = &u.arg;
            }
            // (cross-runtime #1144) `c + 273.15` — binary com pelo menos
            // um operando float lit non-integer eh garantidamente F64.
            Expr::Bin(b) if matches!(
                b.op,
                swc_ecma_ast::BinaryOp::Add | swc_ecma_ast::BinaryOp::Sub
                    | swc_ecma_ast::BinaryOp::Mul | swc_ecma_ast::BinaryOp::Div
            ) => {
                return rhs_is_non_integer_float_lit(&b.left)
                    || rhs_is_non_integer_float_lit(&b.right);
            }
            _ => return false,
        }
    }
}

fn lower_assign_expr(ctx: &mut FnCtx, a: &swc_ecma_ast::AssignExpr) -> Result<TypedVal> {
    use swc_ecma_ast::{AssignOp, AssignTarget};

    if let AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::SuperProp(sp)) = &a.left {
        return lower_super_prop_assign(ctx, sp, a);
    }

    // (#211/#222) `ia = x[Symbol.iterator]()` — marca `ia` como generator_var
    // pra que `ia.next()` roteie a GENERATOR_NEXT (despacho Vec/GenState em
    // runtime). Cobre o caso in-SM onde o desugar emite o bind como ASSIGN
    // (decls.rs so' alcanca a forma `const ia = ...`). Sem isto, `ia.next()`
    // num generator de zip/iterator-protocol cai no dispatch generico -> SIGILL
    // ou loop infinito (ra.done nunca true).
    if let AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) = &a.left {
        if matches!(a.op, AssignOp::Assign) {
            if let Expr::Call(c) = a.right.as_ref() {
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    if let Expr::Member(m) = callee.as_ref() {
                        if let MemberProp::Computed(cp) = &m.prop {
                            if let Expr::Member(sm) = cp.expr.as_ref() {
                                if let (Expr::Ident(o), MemberProp::Ident(p)) =
                                    (sm.obj.as_ref(), &sm.prop)
                                {
                                    if o.sym.as_str() == "Symbol"
                                        && p.sym.as_str() == "iterator"
                                    {
                                        ctx.generator_vars.insert(id.id.sym.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) = &a.left {
        // (cross-runtime #1125) `globalThis[key] = v` — Map global singleton.
        if matches!(a.op, AssignOp::Assign) {
            // Peel TsAs/Paren para detectar `(globalThis as any)["k"]`.
            fn peel<'a>(e: &'a Expr) -> &'a Expr {
                match e {
                    Expr::TsAs(a) => peel(&a.expr),
                    Expr::TsTypeAssertion(a) => peel(&a.expr),
                    Expr::TsConstAssertion(a) => peel(&a.expr),
                    Expr::TsNonNull(a) => peel(&a.expr),
                    Expr::Paren(p) => peel(&p.expr),
                    _ => e,
                }
            }
            // (#310/#311/#312) `(console as any).<method> = fn` — grava
            // override runtime no side-table CONSOLE_OVERRIDES. O call site
            // de console.<method> checa o override antes do builtin nativo.
            if let Expr::Ident(obj_id) = peel(m.obj.as_ref()) {
                if obj_id.sym.as_str() == "console" {
                    if let MemberProp::Ident(prop) = &m.prop {
                        use cranelift_codegen::ir::InstBuilder;
                        use cranelift_codegen::ir::types as cl;
                        let method = prop.sym.as_str();
                        let (mp, ml) = ctx.emit_str_literal(method.as_bytes())?;
                        // (#310) Detecta se o callback eh variadic (`...args`):
                        // apos hoist_fn, a.right vira Ident da lifted fn.
                        let is_variadic = match peel(a.right.as_ref()) {
                            Expr::Ident(rid) => {
                                crate::codegen::lower::passes::hoist_fn::is_lifted_variadic(
                                    rid.sym.as_str(),
                                )
                            }
                            _ => false,
                        };
                        let val_tv = lower_expr(ctx, &a.right)?;
                        // fn handle (arrow/Function) ou 0 quando restaura o
                        // original (capturado em `const g = console.group`,
                        // que devolve 0 — sem handle nativo reificado).
                        let fn_h = ctx.coerce_to_i64(val_tv).val;
                        let var_flag = ctx.builder.ins().iconst(cl::I64, is_variadic as i64);
                        let set_fn = ctx.get_extern(
                            "__RTS_FN_RT_CONSOLE_SET_OVERRIDE",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(set_fn, &[mp, ml, fn_h, var_flag]);
                        return Ok(TypedVal::new(fn_h, ValTy::I64));
                    }
                }
            }
            if let Expr::Ident(obj_id) = peel(m.obj.as_ref()) {
                if obj_id.sym.as_str() == "globalThis" {
                    use cranelift_codegen::ir::InstBuilder;
                    use cranelift_codegen::ir::types as cl;
                    // Chave: computed (`globalThis[k]`) ou ident (`globalThis.x`).
                    // (#360) O ident-case faltava — `globalThis.foo = v` nao
                    // persistia (read checa o Map singleton mas write nao
                    // gravava nele) -> read devolvia 0.
                    let key: Option<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> =
                        match &m.prop {
                            MemberProp::Computed(c) => {
                                let key_tv = lower_expr(ctx, &c.expr)?;
                                let key_h = ctx.coerce_to_handle(key_tv)?.val;
                                let str_ptr = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                                let str_len = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                                let p_inst = ctx.builder.ins().call(str_ptr, &[key_h]);
                                let kp = ctx.builder.inst_results(p_inst)[0];
                                let l_inst = ctx.builder.ins().call(str_len, &[key_h]);
                                let kl = ctx.builder.inst_results(l_inst)[0];
                                Some((kp, kl))
                            }
                            MemberProp::Ident(prop) => {
                                Some(ctx.emit_str_literal(prop.sym.as_bytes())?)
                            }
                            _ => None,
                        };
                    if let Some((kp, kl)) = key {
                        let val_tv = lower_expr(ctx, &a.right)?;
                        let val = ctx.coerce_to_i64(val_tv).val;
                        let gt_fn = ctx.get_extern("__RTS_FN_RT_GLOBAL_THIS_MAP", &[], Some(cl::I64))?;
                        let gt_inst = ctx.builder.ins().call(gt_fn, &[]);
                        let gt = ctx.builder.inst_results(gt_inst)[0];
                        let set_fn = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(set_fn, &[gt, kp, kl, val]);
                        return Ok(TypedVal::new(val, ValTy::I64));
                    }
                }
            }
        }
        // `arr.length = N` — JS spec: trunca/extende o array.
        // Detecta via prop name; runtime decide se eh Vec mesmo.
        if matches!(a.op, AssignOp::Assign) {
            if let MemberProp::Ident(prop) = &m.prop {
                if prop.sym.as_str() == "length" {
                    use cranelift_codegen::ir::types as cl;
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    let val_tv = lower_expr(ctx, &a.right)?;
                    let n = ctx.coerce_to_i64(val_tv).val;
                    let f = ctx.get_extern(
                        "__RTS_FN_NS_COLLECTIONS_VEC_SET_LENGTH",
                        &[cl::I64, cl::I64],
                        None,
                    )?;
                    ctx.builder.ins().call(f, &[obj_h, n]);
                    // JS: assignment retorna o RHS.
                    return Ok(TypedVal::new(n, ValTy::I64));
                }
                // (#782) `re.lastIndex = N` — setter direto no Entry::Regex.
                // Runtime ignora handles nao-Regex.
                if prop.sym.as_str() == "lastIndex" {
                    use cranelift_codegen::ir::types as cl;
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    if matches!(obj_tv.ty, ValTy::Handle) {
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let val_tv = lower_expr(ctx, &a.right)?;
                        let n = ctx.coerce_to_i64(val_tv).val;
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_REGEXP_LAST_INDEX_SET",
                            &[cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(f, &[obj_h, n]);
                        return Ok(TypedVal::new(n, ValTy::I64));
                    }
                }
                // (cross-runtime #67) `url.pathname = "..."` — setter para
                // URL Entry::Env. Runtime ignora handles nao-URL.
                if prop.sym.as_str() == "pathname" {
                    use cranelift_codegen::ir::types as cl;
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    if matches!(obj_tv.ty, ValTy::Handle) {
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let val_tv = lower_expr(ctx, &a.right)?;
                        let val_h = ctx.coerce_to_handle(val_tv)?.val;
                        let str_ptr = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
                        let str_len = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                        let p_inst = ctx.builder.ins().call(str_ptr, &[val_h]);
                        let vp = ctx.builder.inst_results(p_inst)[0];
                        let l_inst = ctx.builder.ins().call(str_len, &[val_h]);
                        let vl = ctx.builder.inst_results(l_inst)[0];
                        let f = ctx.get_extern(
                            "__RTS_FN_GL_URL_SET_PATHNAME",
                            &[cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(f, &[obj_h, vp, vl]);
                        return Ok(TypedVal::new(val_h, ValTy::Handle));
                    }
                }
            }
        }
        // (#object-setter) Intercepta `obj.X = value` quando obj tem
        // setter `__set_X`. Chama o setter via INVOKE_AUTO em vez do
        // MAP_SET direto. Detecta em runtime via map_get(__set_X).
        if matches!(a.op, AssignOp::Assign) {
            if let MemberProp::Ident(prop) = &m.prop {
                let prop_name = prop.sym.as_str();
                if !prop_name.starts_with("__set_")
                    && !prop_name.starts_with("__get_")
                    && prop_name != "prototype"
                {
                    // Skipar se obj eh instance de classe — class setter
                    // tem dispatch proprio via class_setter_name. Verifica
                    // se local_class_ty[obj] aponta para classe registrada.
                    // Tambem skipa para `this.X = ...` em metodo de classe
                    // (this eh sempre instance — caminho normal preserva
                    // tipos via class field metadata).
                    let is_class_instance = if let Expr::Ident(id) = m.obj.as_ref() {
                        let n = id.sym.as_str();
                        ctx.local_class_ty.get(n)
                            .map(|cls| ctx.classes.contains_key(cls))
                            .unwrap_or(false)
                    } else if matches!(m.obj.as_ref(), Expr::This(_)) {
                        ctx.current_class.is_some()
                    } else {
                        false
                    };
                    // (cross-runtime #1071) Mesmo para class instance, se a
                    // classe NAO tem setter sintatic (`set x() {}`) para a
                    // prop, tenta dynamic `__set_<key>` via walk __proto__
                    // — cobre `Object.defineProperty(Widget.prototype, ...,
                    // {set})`. Sem isso o assign caia em MAP_SET direto
                    // sem invocar o setter dinamico.
                    let has_static_setter = if is_class_instance {
                        if let Expr::Ident(id) = m.obj.as_ref() {
                            ctx.local_class_ty.get(id.sym.as_str())
                                .and_then(|cls| {
                                    ctx.classes.get(cls).map(|meta| {
                                        meta.setters.iter().any(|s| s == prop_name)
                                    })
                                })
                                .unwrap_or(false)
                        } else if matches!(m.obj.as_ref(), Expr::This(_)) {
                            ctx.current_class.as_deref()
                                .and_then(|cls| ctx.classes.get(cls))
                                .map(|meta| meta.setters.iter().any(|s| s == prop_name))
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    use cranelift_codegen::ir::types as cl;
                    if is_class_instance && has_static_setter {
                        // Setter sintatic existe — deixa fluxo normal abaixo despachar.
                    } else {
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    if matches!(obj_tv.ty, ValTy::Handle) {
                        let obj_h = ctx.coerce_to_i64(obj_tv).val;
                        let setter_key: Vec<u8> = {
                            let mut v = b"__set_".to_vec();
                            v.extend_from_slice(prop_name.as_bytes());
                            v
                        };
                        let (kptr, klen) = ctx.emit_str_literal(&setter_key)?;
                        // (#218) MAP_GET_DIRECT nao aciona trap Proxy.
                        let plain_get = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_GET_DIRECT",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        let inst_g = ctx.builder.ins().call(plain_get, &[obj_h, kptr, klen]);
                        let setter_h = ctx.builder.inst_results(inst_g)[0];
                        let zero = ctx.builder.ins().iconst(cl::I64, 0);
                        let has_setter = ctx.builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            setter_h,
                            zero,
                        );
                        let setter_block = ctx.builder.create_block();
                        let plain_block = ctx.builder.create_block();
                        let merge = ctx.builder.create_block();
                        ctx.builder.ins().brif(has_setter, setter_block, &[], plain_block, &[]);

                        // Avalia value uma vez antes do branch.
                        ctx.builder.switch_to_block(setter_block);
                        ctx.builder.seal_block(setter_block);
                        let value_tv = lower_expr(ctx, &a.right)?;
                        let value_i64 = ctx.coerce_to_i64(value_tv).val;
                        let vec_new = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                            &[],
                            Some(cl::I64),
                        )?;
                        let inst_v = ctx.builder.ins().call(vec_new, &[]);
                        let args_h = ctx.builder.inst_results(inst_v)[0];
                        let vec_push = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                            &[cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(vec_push, &[args_h, value_i64]);
                        let invoke = ctx.get_extern(
                            "__RTS_FN_RT_INVOKE_AUTO",
                            &[cl::I64, cl::I64, cl::I64],
                            Some(cl::I64),
                        )?;
                        ctx.builder.ins().call(invoke, &[setter_h, obj_h, args_h]);
                        ctx.builder.ins().jump(merge, &[]);

                        ctx.builder.switch_to_block(plain_block);
                        ctx.builder.seal_block(plain_block);
                        // Plain MAP_SET — re-aplica logica de assign normal
                        // (sem chamar lower_assign_expr de novo, faz inline).
                        let value_tv = lower_expr(ctx, &a.right)?;
                        // (372) Campo `number` (F64): representacao uniforme =
                        // BITS do f64, simetrica com a leitura tipada. Converte
                        // o RHS pra f64 e bitcasta (inclusive init/assign
                        // inteiro). Cobre o caso `inst.campoF64 = <int|metodo>`
                        // que cai neste plain_block do dispatch de setter.
                        let value_i64 = if assign_target_field_is_f64(ctx, m) {
                            let f = to_f64(ctx, value_tv);
                            ctx.builder.ins().bitcast(
                                cl::I64,
                                cranelift_codegen::ir::MemFlags::new(),
                                f,
                            )
                        } else if matches!(value_tv.ty, ValTy::F64)
                            && rhs_is_non_integer_float_lit(&a.right)
                        {
                            // (#1051 part 2) Preserva precisao f64 quando RHS eh
                            // literal float nao-inteiro — bitcast em vez de
                            // fcvt_to_sint_sat que trunca.
                            ctx.builder.ins().bitcast(
                                cl::I64,
                                cranelift_codegen::ir::MemFlags::new(),
                                value_tv.val,
                            )
                        } else {
                            ctx.coerce_to_i64(value_tv).val
                        };
                        let (kptr2, klen2) = ctx.emit_str_literal(prop_name.as_bytes())?;
                        let map_set = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                            &[cl::I64, cl::I64, cl::I64, cl::I64],
                            None,
                        )?;
                        ctx.builder.ins().call(map_set, &[obj_h, kptr2, klen2, value_i64]);
                        ctx.builder.ins().jump(merge, &[]);

                        ctx.builder.switch_to_block(merge);
                        ctx.builder.seal_block(merge);
                        return Ok(TypedVal::new(zero, ValTy::I64));
                    }
                    } // close is_class_instance else
                }
            }
        }
        // (#264 PR4+) Intercepta `<UserFn>.prototype = X` para chamar
        // FUNCTION_PROTOTYPE_SET que atualiza o registry global. Sem isso,
        // \`Dog.prototype = Object.create(...)\` faz MAP_SET em handle Function
        // (nao Map) e o registry continua com o Map antigo.
        if matches!(a.op, AssignOp::Assign) {
            if let MemberProp::Ident(prop) = &m.prop {
                if prop.sym.as_str() == "prototype" {
                    // Peel TsAs/Paren no obj.
                    let mut obj_e: &Expr = m.obj.as_ref();
                    loop {
                        match obj_e {
                            Expr::TsAs(a) => obj_e = &a.expr,
                            Expr::TsTypeAssertion(a) => obj_e = &a.expr,
                            Expr::TsConstAssertion(a) => obj_e = &a.expr,
                            Expr::TsSatisfies(a) => obj_e = &a.expr,
                            Expr::TsNonNull(a) => obj_e = &a.expr,
                            Expr::Paren(p) => obj_e = &p.expr,
                            _ => break,
                        }
                    }
                    if let Expr::Ident(id) = obj_e {
                        let name = id.sym.as_str();
                        if ctx.user_fns.contains_key(name) && ctx.var_ty(name).is_none() {
                            // Reify Function handle.
                            let fn_handle_tv = self::calls::emit_user_fn_addr(ctx, name)?;
                            let fn_addr = fn_handle_tv.val;
                            let arity = ctx
                                .user_fns
                                .get(name)
                                .map(|f| f.params.len() as i64)
                                .unwrap_or(0);
                            let arity_v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I64, arity);
                            let name_tv = ctx.emit_str_handle(name.as_bytes())?;
                            let name_h = ctx.coerce_to_i64(name_tv).val;
                            let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cranelift_codegen::ir::types::I64], Some(cranelift_codegen::ir::types::I64))?;
                            let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cranelift_codegen::ir::types::I64], Some(cranelift_codegen::ir::types::I64))?;
                            let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
                            let n_ptr = ctx.builder.inst_results(inst_p)[0];
                            let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
                            let n_len = ctx.builder.inst_results(inst_l)[0];
                            let is_arrow_v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I32, 0);
                            let has_this_v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I32, 0);
                            let reify_fn = ctx.get_extern(
                                "__RTS_FN_GL_FUNCTION_REIFY",
                                &[cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I32, cranelift_codegen::ir::types::I32],
                                Some(cranelift_codegen::ir::types::I64),
                            )?;
                            let inst_r = ctx
                                .builder
                                .ins()
                                .call(reify_fn, &[fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v]);
                            let fn_handle = ctx.builder.inst_results(inst_r)[0];
                            // Lower RHS.
                            let rhs_tv = lower_expr(ctx, &a.right)?;
                            let rhs_h = ctx.coerce_to_i64(rhs_tv).val;
                            let set_proto = ctx.get_extern(
                                "__RTS_FN_GL_FUNCTION_PROTOTYPE_SET",
                                &[cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64],
                                None,
                            )?;
                            ctx.builder.ins().call(set_proto, &[fn_handle, rhs_h]);
                            return Ok(TypedVal::new(rhs_h, ValTy::Handle));
                        }
                    }
                }
            }
        }
        let final_rhs_expr: Box<Expr> = if matches!(a.op, AssignOp::Assign) {
            a.right.clone()
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
                AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign => {
                    // `obj.x ||= y` → recursa como `obj.x = obj.x || y`.
                    // O lower_logical em lower_bin emite curto-circuito;
                    // depois cai no caminho normal de Member assign abaixo.
                    let logical_op = match a.op {
                        AssignOp::AndAssign => BinaryOp::LogicalAnd,
                        AssignOp::OrAssign => BinaryOp::LogicalOr,
                        AssignOp::NullishAssign => BinaryOp::NullishCoalescing,
                        _ => unreachable!(),
                    };
                    let read_lhs = Expr::Member(swc_ecma_ast::MemberExpr {
                        span: a.span,
                        obj: m.obj.clone(),
                        prop: m.prop.clone(),
                    });
                    let synthetic_right = Box::new(Expr::Bin(BinExpr {
                        span: a.span,
                        op: logical_op,
                        left: Box::new(read_lhs),
                        right: a.right.clone(),
                    }));
                    let synthetic_assign = swc_ecma_ast::AssignExpr {
                        span: a.span,
                        op: AssignOp::Assign,
                        left: a.left.clone(),
                        right: synthetic_right,
                    };
                    return lower_assign_expr(ctx, &synthetic_assign);
                }
                AssignOp::Assign => unreachable!(),
            };
            let read_lhs = Expr::Member(swc_ecma_ast::MemberExpr {
                span: a.span,
                obj: m.obj.clone(),
                prop: m.prop.clone(),
            });
            Box::new(Expr::Bin(BinExpr {
                span: a.span,
                op: binop,
                left: Box::new(read_lhs),
                right: a.right.clone(),
            }))
        };

        if let MemberProp::Ident(id) = &m.prop {
            if let Some(cls) = lhs_static_class(ctx, &m.obj) {
                let prop_name = id.sym.as_str();
                validate_visibility(ctx, &cls, prop_name)?;
                if field_is_readonly_in_hierarchy(ctx, &cls, prop_name)
                    && (!ctx.current_is_ctor || ctx.current_class.as_deref() != Some(&cls))
                {
                    return Err(anyhow!(
                        "readonly `{cls}.{prop_name}` so pode ser atribuido dentro do constructor de `{cls}`"
                    ));
                }
            }
        }

        // (#264 PR5+) Quando o RHS eh ident de user fn E o LHS eh
        // \`<UserFn>.prototype.X\` (member chain de prototype), reify
        // o ident em handle Function antes do MAP_SET. Isso permite
        // que call sites futuros (\`instance.X(...)\`) usem
        // FUNCTION_CALL com return_kind correto.
        let reify_rhs = matches!(a.op, AssignOp::Assign)
            && {
                // Detecta lhs: <UserFn>.prototype.X
                if let MemberProp::Ident(_) = &m.prop {
                    if let Expr::Member(inner) = m.obj.as_ref() {
                        if let MemberProp::Ident(p) = &inner.prop {
                            if p.sym.as_str() == "prototype" {
                                let mut o: &Expr = inner.obj.as_ref();
                                loop {
                                    match o {
                                        Expr::TsAs(a) => o = &a.expr,
                                        Expr::Paren(p) => o = &p.expr,
                                        _ => break,
                                    }
                                }
                                if let Expr::Ident(id) = o {
                                    let n = id.sym.as_str();
                                    ctx.user_fns.contains_key(n) && ctx.var_ty(n).is_none()
                                } else { false }
                            } else { false }
                        } else { false }
                    } else { false }
                } else { false }
            }
            && {
                // RHS eh ident de user fn? Peel TsAs.
                let mut e: &Expr = final_rhs_expr.as_ref();
                loop {
                    match e {
                        Expr::TsAs(a) => e = &a.expr,
                        Expr::Paren(p) => e = &p.expr,
                        _ => break,
                    }
                }
                if let Expr::Ident(id) = e {
                    let n = id.sym.as_str();
                    ctx.user_fns.contains_key(n) && ctx.var_ty(n).is_none()
                } else { false }
            };

        let rhs = if reify_rhs {
            // Extrai o nome da user fn (peel TsAs).
            let mut e: &Expr = final_rhs_expr.as_ref();
            loop {
                match e {
                    Expr::TsAs(a) => e = &a.expr,
                    Expr::Paren(p) => e = &p.expr,
                    _ => break,
                }
            }
            let fn_name = if let Expr::Ident(id) = e {
                id.sym.as_str().to_string()
            } else {
                unreachable!()
            };
            // Reify em handle Function.
            let fn_addr = self::calls::emit_user_fn_addr(ctx, &fn_name)?.val;
            let arity = ctx
                .user_fns
                .get(&fn_name)
                .map(|f| f.params.len() as i64)
                .unwrap_or(0);
            let arity_v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I64, arity);
            let name_tv = ctx.emit_str_handle(fn_name.as_bytes())?;
            let name_h = ctx.coerce_to_i64(name_tv).val;
            let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cranelift_codegen::ir::types::I64], Some(cranelift_codegen::ir::types::I64))?;
            let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cranelift_codegen::ir::types::I64], Some(cranelift_codegen::ir::types::I64))?;
            let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
            let n_ptr = ctx.builder.inst_results(inst_p)[0];
            let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
            let n_len = ctx.builder.inst_results(inst_l)[0];
            let is_arrow_v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I32, 0);
            let has_this_v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I32, 0);
            let reify_fn = ctx.get_extern(
                "__RTS_FN_GL_FUNCTION_REIFY",
                &[cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I32, cranelift_codegen::ir::types::I32],
                Some(cranelift_codegen::ir::types::I64),
            )?;
            let inst_r = ctx
                .builder
                .ins()
                .call(reify_fn, &[fn_addr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v]);
            let v = ctx.builder.inst_results(inst_r)[0];
            TypedVal::new(v, ValTy::Handle)
        } else {
            lower_expr(ctx, &final_rhs_expr)?
        };

        // Dual-path #147 passo 7: escrita tipada em campo flat. Preserva
        // o tipo do RHS para coercao no slot exato (i32/f64/i64/handle).
        if let MemberProp::Ident(id) = &m.prop {
            if let Some(cls) = lhs_static_class(ctx, &m.obj) {
                let prop_name = id.sym.as_str();
                if class_field_uses_flat(ctx, &cls, prop_name) {
                    // Setters dinamicos ja descartam o flat path em
                    // `class_field_uses_flat`, entao chegando aqui e seguro
                    // emitir store direto.
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    emit_flat_field_write(ctx, obj_h, &cls, prop_name, rhs)?;
                    return Ok(rhs);
                }
            }
        }

        // (372) Campo `number` (F64): representacao uniforme = BITS do f64.
        // A leitura tipada (map_get_static_typed) reinterpreta via bitcast.
        // Converte o RHS pra f64 (i64/i32 -> fcvt_from_sint) e bitcasta, pra
        // que store e load sejam simetricos — inclusive quando o init eh
        // inteiro (`a: number = 100`) ou vem de chamada de metodo
        // (`this.#k = this.#toKelvin(c)`).
        let dest_field_is_f64 = assign_target_field_is_f64(ctx, m);
        // True quando `rhs_i64` carrega os BITS de um f64 (via bitcast), nao
        // um inteiro logico. Usado no dispatch de setter virtual para escolher
        // bitcast vs fcvt ao coagir para um param F64 (cross-runtime 38 vs
        // super_field_setter, que trata `number` como i64 logico).
        let rhs_i64_is_f64_bits = dest_field_is_f64
            || (matches!(rhs.ty, ValTy::F64)
                && rhs_is_non_integer_float_lit(final_rhs_expr.as_ref()));
        let rhs_i64 = if dest_field_is_f64 {
            let f = to_f64(ctx, rhs);
            ctx.builder.ins().bitcast(
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::MemFlags::new(),
                f,
            )
        } else if matches!(rhs.ty, ValTy::F64)
            && rhs_is_non_integer_float_lit(final_rhs_expr.as_ref())
        {
            // (#1051) RHS literal float nao-inteiro em campo sem F64 conhecido
            // (objeto generico). INSPECT/TPL_COERCE_AUTO detectam |v|>2^53.
            ctx.builder.ins().bitcast(
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::MemFlags::new(),
                rhs.val,
            )
        } else {
            ctx.coerce_to_i64(rhs).val
        };

        if let MemberProp::Ident(id) = &m.prop {
            if let Some(cls) = lhs_static_class(ctx, &m.obj) {
                let prop_name = id.sym.as_str();
                if let Some(setter_owner) = resolve_setter_owner(ctx, &cls, prop_name) {
                    let obj_tv = lower_expr(ctx, &m.obj)?;
                    let obj_h = ctx.coerce_to_i64(obj_tv).val;
                    let setter_fn_name = class_setter_name(&setter_owner, prop_name);
                    let setter_abi = ctx
                        .user_fns
                        .get(&setter_fn_name)
                        .ok_or_else(|| anyhow!("setter `{setter_fn_name}` nao registrada"))?
                        .clone();
                    let param_ty = setter_abi.params.get(1).copied().unwrap_or(ValTy::I64);
                    let coerced = match param_ty {
                        ValTy::I32 => ctx.coerce_to_i32(TypedVal::new(rhs_i64, ValTy::I64)).val,
                        // Setter espera F64. Se o RHS ja' era f64 (ou destino
                        // f64), `rhs_i64` carrega os BITS do f64 (bitcast acima)
                        // — reinterpretar via bitcast, nao fcvt_from_sint (que
                        // converteria os bits como inteiro, gerando lixo —
                        // cross-runtime 38_classes_deep). Senao (RHS inteiro
                        // real), converter via fcvt.
                        ValTy::F64 => {
                            if rhs_i64_is_f64_bits {
                                ctx.builder.ins().bitcast(
                                    cranelift_codegen::ir::types::F64,
                                    cranelift_codegen::ir::MemFlags::new(),
                                    rhs_i64,
                                )
                            } else {
                                to_f64(ctx, TypedVal::new(rhs_i64, ValTy::I64))
                            }
                        }
                        _ => rhs_i64,
                    };
                    let cls_owned = cls.clone();
                    let prop_owned = prop_name.to_string();
                    emit_virtual_accessor_dispatch(
                        ctx,
                        &cls_owned,
                        &setter_owner,
                        AccessorKind::Setter,
                        &prop_owned,
                        obj_h,
                        &[coerced],
                    )?;
                    return Ok(TypedVal::new(rhs_i64, ValTy::I64));
                }
            }
        }

        // (#811/205) `view[i] = v` onde view eh TypedArray-view sobre buffer:
        // escreve `elem_bytes` bytes little-endian via TA_SET_ELEM no buffer
        // (compartilhado entre views). Para float, rhs_i64 carrega os bits f64.
        if let (Expr::Ident(obj_id), MemberProp::Computed(c)) = (m.obj.as_ref(), &m.prop) {
            if let Some(&(eb, _sg, fl)) = ctx.local_ta_view.get(obj_id.sym.as_str()) {
                use cranelift_codegen::ir::types as cl;
                let obj_h = {
                    let tv = lower_expr(ctx, &m.obj)?;
                    ctx.coerce_to_i64(tv).val
                };
                let idx_tv = lower_expr(ctx, &c.expr)?;
                let idx = ctx.coerce_to_i64(idx_tv).val;
                // Para float, garante que rhs_i64 sao bits de f64.
                let val = if fl != 0 {
                    let f = to_f64(ctx, TypedVal::new(rhs_i64, ValTy::I64));
                    ctx.builder.ins().bitcast(cl::I64, cranelift_codegen::ir::MemFlags::new(), f)
                } else {
                    rhs_i64
                };
                let eb_v = ctx.builder.ins().iconst(cl::I64, eb);
                let fl_v = ctx.builder.ins().iconst(cl::I64, fl);
                let set_elem = ctx.get_extern(
                    "__RTS_FN_GL_TA_SET_ELEM",
                    &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I64],
                    None,
                )?;
                ctx.builder.ins().call(set_elem, &[obj_h, idx, eb_v, fl_v, val]);
                return Ok(TypedVal::new(rhs_i64, ValTy::I64));
            }
        }

        let obj_tv = lower_expr(ctx, &m.obj)?;
        let obj_h = ctx.coerce_to_i64(obj_tv).val;
        let set_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
            &[
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::types::I64,
                cranelift_codegen::ir::types::I64,
            ],
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
                    let key_tv = lower_expr(ctx, &c.expr)?;
                    // (cross-runtime #340) Hot path: key claramente numerica
                    // (literal Num) -> VEC_SET direto. Caso ambiguo (param,
                    // member call result, I64 marcado) cai em OBJ_SET que
                    // detecta tipo (Vec usa numeric, Map usa key handle).
                    let is_clear_num = matches!(c.expr.as_ref(), Expr::Lit(Lit::Num(_)))
                        || matches!(key_tv.ty, ValTy::I32);
                    if is_clear_num {
                        let idx = ctx.coerce_to_i64(key_tv).val;
                        let vec_set = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_VEC_SET",
                            &[
                                cranelift_codegen::ir::types::I64,
                                cranelift_codegen::ir::types::I64,
                                cranelift_codegen::ir::types::I64,
                            ],
                            None,
                        )?;
                        ctx.builder.ins().call(vec_set, &[obj_h, idx, rhs_i64]);
                    } else {
                        // (#753) Symbol/handle key -> OBJ_SET dispatcher
                        // (Vec/Map em runtime + repr canonica de Symbol).
                        let key_h = ctx.coerce_to_handle(key_tv)?.val;
                        let obj_set = ctx.get_extern(
                            "__RTS_FN_NS_COLLECTIONS_OBJ_SET",
                            &[
                                cranelift_codegen::ir::types::I64,
                                cranelift_codegen::ir::types::I64,
                                cranelift_codegen::ir::types::I64,
                            ],
                            None,
                        )?;
                        ctx.builder.ins().call(obj_set, &[obj_h, key_h, rhs_i64]);
                    }
                }
            }
            MemberProp::PrivateName(pn) => {
                let raw_name = pn.name.as_ref();
                let raw_key = format!("#{}", raw_name);
                validate_private_scope(ctx, &raw_key)?;
                // (cross-runtime #267) Mangle por current_class.
                let key = if let Some(cur) = ctx.current_class.as_deref() {
                    format!("#{}_{}", cur, raw_name)
                } else {
                    raw_key.clone()
                };
                let (kp, kl) = ctx.emit_str_literal(key.as_bytes())?;
                ctx.builder.ins().call(set_fn, &[obj_h, kp, kl, rhs_i64]);
            }
        }
        return Ok(TypedVal::new(rhs_i64, ValTy::I64));
    }

    // (#1083) Array destructuring assignment: `[a, b] = [b, a]`,
    // `[arr[i], arr[j]] = [arr[j], arr[i]]`. Lowera para:
    //   const __tmp_0 = rhs[0]; const __tmp_1 = rhs[1];
    //   a = __tmp_0; b = __tmp_1;
    // Cada slot pode ser ident OU member (arr[i] / obj.x).
    if let AssignTarget::Pat(pat) = &a.left {
        if matches!(a.op, AssignOp::Assign) {
            if let swc_ecma_ast::AssignTargetPat::Array(arr_pat) = pat {
                use swc_ecma_ast::Expr as SwcExpr;
                let rhs_tv = lower_expr(ctx, &a.right)?;
                let rhs_h = ctx.coerce_to_i64(rhs_tv).val;
                let vec_get = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_GET",
                    &[cranelift_codegen::ir::types::I64, cranelift_codegen::ir::types::I64],
                    Some(cranelift_codegen::ir::types::I64),
                )?;
                // Materializa primeiro todos os slots em SSA values pra
                // suportar swap (avalia RHS antes de mutar LHS).
                let mut values: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(arr_pat.elems.len());
                for (i, _) in arr_pat.elems.iter().enumerate() {
                    let idx_v = ctx.builder.ins().iconst(
                        cranelift_codegen::ir::types::I64,
                        i as i64,
                    );
                    let inst = ctx.builder.ins().call(vec_get, &[rhs_h, idx_v]);
                    values.push(ctx.builder.inst_results(inst)[0]);
                }
                // Escreve cada slot.
                for (elem, v) in arr_pat.elems.iter().zip(values.iter()) {
                    let Some(e) = elem else { continue }; // hole — skip
                    // Construct synthetic AssignExpr (elem = literal_v) via shortcut:
                    // chamamos write_assign_target_simple sintetico.
                    // Para simplicidade, suportamos so' Ident e Member computed.
                    match e {
                        swc_ecma_ast::Pat::Ident(id) => {
                            let nm = id.id.sym.as_str();
                            // VEC_GET retorna i64 raw. Se a var local eh F64,
                            // converte via fcvt; o normalize do write_local
                            // espera tipos compativeis.
                            let val_to_write = if let Some(ty) = ctx.var_ty(nm) {
                                match ty {
                                    ValTy::F64 => ctx.builder.ins().fcvt_from_sint(
                                        cranelift_codegen::ir::types::F64,
                                        *v,
                                    ),
                                    _ => *v,
                                }
                            } else { *v };
                            ctx.write_local(nm, val_to_write)?;
                        }
                        swc_ecma_ast::Pat::Expr(expr) => {
                            if let SwcExpr::Member(m) = expr.as_ref() {
                                let obj_tv = lower_expr(ctx, &m.obj)?;
                                let obj_h = ctx.coerce_to_i64(obj_tv).val;
                                match &m.prop {
                                    MemberProp::Computed(c) => {
                                        let idx_tv = lower_expr(ctx, &c.expr)?;
                                        // Vec slot via VEC_SET (key numerica).
                                        if matches!(idx_tv.ty, ValTy::I64 | ValTy::I32 | ValTy::U64 | ValTy::F64) {
                                            let idx = ctx.coerce_to_i64(idx_tv).val;
                                            let vec_set = ctx.get_extern(
                                                "__RTS_FN_NS_COLLECTIONS_VEC_SET",
                                                &[
                                                    cranelift_codegen::ir::types::I64,
                                                    cranelift_codegen::ir::types::I64,
                                                    cranelift_codegen::ir::types::I64,
                                                ],
                                                None,
                                            )?;
                                            ctx.builder.ins().call(vec_set, &[obj_h, idx, *v]);
                                        } else {
                                            // chave string handle -> MAP_SET_KH
                                            let key_h = ctx.coerce_to_i64(idx_tv).val;
                                            let map_set_kh = ctx.get_extern(
                                                "__RTS_FN_NS_COLLECTIONS_MAP_SET_KH",
                                                &[
                                                    cranelift_codegen::ir::types::I64,
                                                    cranelift_codegen::ir::types::I64,
                                                    cranelift_codegen::ir::types::I64,
                                                ],
                                                None,
                                            )?;
                                            ctx.builder.ins().call(map_set_kh, &[obj_h, key_h, *v]);
                                        }
                                    }
                                    MemberProp::Ident(id) => {
                                        let (kp, kl) = ctx.emit_str_literal(id.sym.as_bytes())?;
                                        let map_set = ctx.get_extern(
                                            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                                            &[
                                                cranelift_codegen::ir::types::I64,
                                                cranelift_codegen::ir::types::I64,
                                                cranelift_codegen::ir::types::I64,
                                                cranelift_codegen::ir::types::I64,
                                            ],
                                            None,
                                        )?;
                                        ctx.builder.ins().call(map_set, &[obj_h, kp, kl, *v]);
                                    }
                                    _ => return Err(anyhow!("destructure assign: unsupported member prop kind")),
                                }
                            } else {
                                return Err(anyhow!("destructure assign target: unsupported expr kind"));
                            }
                        }
                        _ => return Err(anyhow!("destructure assign: unsupported pattern element")),
                    }
                }
                return Ok(TypedVal::new(rhs_h, ValTy::I64));
            }
        }
    }

    let name = match &a.left {
        AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) => {
            id.sym.as_str().to_string()
        }
        _ => return Err(anyhow!("only simple identifier assignment is supported")),
    };

    // Logical compound assignment: `x ||= y`, `x &&= y`, `x ??= y` —
    // semantica curto-circuito. Translado para `x = x op y` via Bin
    // logical, que ja avalia y so quando necessario.
    if matches!(a.op, AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign) {
        let logical_op = match a.op {
            AssignOp::AndAssign => BinaryOp::LogicalAnd,
            AssignOp::OrAssign => BinaryOp::LogicalOr,
            AssignOp::NullishAssign => BinaryOp::NullishCoalescing,
            _ => unreachable!(),
        };
        let synthetic_left = Expr::Ident(swc_ecma_ast::Ident {
            span: a.span,
            ctxt: Default::default(),
            sym: name.as_str().into(),
            optional: false,
        });
        let bin = BinExpr {
            span: a.span,
            op: logical_op,
            left: Box::new(synthetic_left),
            right: a.right.clone(),
        };
        let rhs_val = lower_bin(ctx, &bin)?;
        let coerced = match ctx.var_ty(&name) {
            Some(ValTy::I32) => ctx.coerce_to_i32(rhs_val),
            Some(ValTy::I64) => ctx.coerce_to_i64(rhs_val),
            Some(ValTy::F64) => ctx.coerce_to_f64(rhs_val),
            Some(ValTy::Handle) => ctx.coerce_to_handle(rhs_val)?,
            _ => rhs_val,
        };
        ctx.write_local(&name, coerced.val)?;
        return Ok(coerced);
    }

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
            AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign => {
                unreachable!("logical compound handled above")
            }
            AssignOp::Assign => unreachable!(),
        };
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

    let coerced = match ctx.var_ty(&name) {
        Some(ValTy::I32) => ctx.coerce_to_i32(rhs_val),
        Some(ValTy::I64) => ctx.coerce_to_i64(rhs_val),
        Some(ValTy::Handle) => {
            // (#err-extends/627) Quando rhs eh I64 ambiguo (resultado de
            // obj.x sem tipo) e var declarada como string (Handle), usa
            // TPL_COERCE_AUTO em vez de STRING_FROM_I64 (que formata
            // handle bruto como numero).
            if matches!(rhs_val.ty, ValTy::I64 | ValTy::U64)
                && ctx.var_member_call_values.contains(&rhs_val.val)
            {
                use cranelift_codegen::ir::types as cl;
                let coerce_fn = ctx.get_extern(
                    "__RTS_FN_RT_TPL_COERCE_AUTO",
                    &[cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(coerce_fn, &[rhs_val.val]);
                let v = ctx.builder.inst_results(inst)[0];
                TypedVal::new(v, ValTy::Handle)
            } else {
                ctx.coerce_to_handle(rhs_val)?
            }
        }
        _ => rhs_val,
    };
    ctx.write_local(&name, coerced.val)?;
    Ok(coerced)
}

/// Lower de tagged template literal (#269): `tag\`a${x}b${y}c\`` →
/// `tag([\"a\", \"b\", \"c\"], x, y)`.
///
/// JS spec define o primeiro arg como TemplateStringsArray (objeto com
/// propriedade `.raw`); aqui passamos um array simples — caller
/// recebe `strings[0]`, `strings[1]`, etc via index access. `.raw` nao
/// e' implementado nesta fase (raw strings preservam escape sequences;
/// cooked aplica). Documentado como limitacao no commit.
fn lower_tagged_tpl(
    ctx: &mut FnCtx,
    tt: &swc_ecma_ast::TaggedTpl,
) -> Result<TypedVal> {
    use swc_ecma_ast::{ArrayLit, CallExpr, Callee, ExprOrSpread};

    // (cross-runtime #803) `String.raw\`...\`` — concatena os segmentos
    // raw (sem escape interpretation) intercalando os exprs interpolados
    // como template literal normal. Reescreve para um `Tpl` com as
    // mesmas exprs mas quasis usando `raw` em vez de `cooked`.
    if let Expr::Member(m) = tt.tag.as_ref() {
        if let Expr::Ident(obj) = m.obj.as_ref() {
            if obj.sym.as_str() == "String" {
                if let MemberProp::Ident(prop) = &m.prop {
                    if prop.sym.as_str() == "raw" {
                        let raw_quasis: Vec<swc_ecma_ast::TplElement> = tt
                            .tpl
                            .quasis
                            .iter()
                            .map(|q| {
                                let raw_s = q.raw.as_str().to_string();
                                swc_ecma_ast::TplElement {
                                    span: q.span,
                                    tail: q.tail,
                                    cooked: Some(raw_s.as_str().into()),
                                    raw: q.raw.clone(),
                                }
                            })
                            .collect();
                        let new_tpl = swc_ecma_ast::Tpl {
                            span: tt.tpl.span,
                            exprs: tt.tpl.exprs.clone(),
                            quasis: raw_quasis,
                        };
                        return basics::lower_tpl(ctx, &new_tpl);
                    }
                }
            }
        }
    }

    // Constroi array literal das string parts (cooked).
    let elems: Vec<Option<ExprOrSpread>> = tt
        .tpl
        .quasis
        .iter()
        .map(|q| {
            let cooked: String = q
                .cooked
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| q.raw.as_str().to_string());
            Some(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(swc_ecma_ast::Str {
                    span: Default::default(),
                    value: cooked.as_str().into(),
                    raw: None,
                }))),
            })
        })
        .collect();
    let strings_array = Expr::Array(ArrayLit {
        span: Default::default(),
        elems,
    });

    // (cross-runtime #744) Constroi array das raw strings. Registra
    // cooked_handle -> raw_handle em side-table apos lower (via codegen
    // explicito no final).
    let raw_elems: Vec<Option<ExprOrSpread>> = tt
        .tpl
        .quasis
        .iter()
        .map(|q| {
            let raw_s = q.raw.as_str().to_string();
            Some(ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(swc_ecma_ast::Str {
                    span: Default::default(),
                    value: raw_s.as_str().into(),
                    raw: None,
                }))),
            })
        })
        .collect();
    let raw_array_expr = Expr::Array(ArrayLit {
        span: Default::default(),
        elems: raw_elems,
    });

    // (cross-runtime #744) Pre-lower o cooked array, registra raw no
    // side-table thread-local (gc::tagged_raw), e armazena o cooked
    // handle em uma var sintetica que o synthetic_call referencia via
    // Ident. Isso permite que `strings.raw` em runtime resolva o handle
    // raw via TAGGED_RAW_GET (member.raw eh interceptado abaixo no
    // codegen via fallback de Handle universal — fix futuro).
    use cranelift_codegen::ir::InstBuilder;
    use cranelift_codegen::ir::types as cl;
    use crate::codegen::lower::ctx::ValTy;
    let cooked_tv = lower_expr(ctx, &strings_array)?;
    let cooked_h = ctx.coerce_to_i64(cooked_tv).val;
    let raw_tv = lower_expr(ctx, &raw_array_expr)?;
    let raw_h = ctx.coerce_to_i64(raw_tv).val;
    let reg_fn = ctx.get_extern(
        "__RTS_FN_NS_GC_TAGGED_RAW_REGISTER",
        &[cl::I64, cl::I64],
        None,
    )?;
    ctx.builder.ins().call(reg_fn, &[cooked_h, raw_h]);
    let tmp_name = format!("__rts_tpl_cooked_{}", tt.span.lo.0);
    ctx.declare_local(&tmp_name, ValTy::Handle, cooked_h);

    let cooked_ident = Expr::Ident(swc_ecma_ast::Ident {
        span: Default::default(),
        ctxt: Default::default(),
        sym: tmp_name.as_str().into(),
        optional: false,
    });

    // Args: [cooked_ident (var sintetica), ...interpolated_exprs]
    let mut args: Vec<ExprOrSpread> = Vec::with_capacity(1 + tt.tpl.exprs.len());
    args.push(ExprOrSpread {
        spread: None,
        expr: Box::new(cooked_ident),
    });
    for e in &tt.tpl.exprs {
        args.push(ExprOrSpread {
            spread: None,
            expr: e.clone(),
        });
    }

    let mut synthetic_call = CallExpr {
        span: tt.span,
        ctxt: tt.ctxt,
        callee: Callee::Expr(tt.tag.clone()),
        args,
        type_args: tt.type_params.clone(),
    };

    // (cross-runtime #744) Empacota rest params se o tag eh fn user com
    // `...rest`. O pass `expand_rest_args` so processa Expr::Call no AST
    // original; tagged templates passam por aqui depois.
    if let Callee::Expr(callee_expr) = &synthetic_call.callee {
        if let Expr::Ident(id) = callee_expr.as_ref() {
            if let Some(abi) = ctx.user_fns.get(id.sym.as_str()) {
                let n_params = abi.params.len();
                // Heuristica: se temos mais args que params e o ultimo param
                // eh Handle (signature tipica de rest array), empacotamos
                // args extras em array literal no slot do rest.
                let last_is_handle = abi
                    .params
                    .last()
                    .map(|t| matches!(t, crate::codegen::lower::ctx::ValTy::Handle | crate::codegen::lower::ctx::ValTy::I64))
                    .unwrap_or(false);
                // (cross-runtime #345) So' empacota quando o slot de rest eh
                // >= 1 (n_params >= 2). O slot 0 eh sempre `strings`
                // (TemplateStringsArray, agora Handle) — NUNCA o rest. Sem
                // este guard, `tag(strings)` (1 param) com TemplateStringsArray
                // Handle disparava o empacotamento e metia [cookedArray, val]
                // no proprio `strings`, corrompendo strings[0]/strings[1].
                // Tag de 1 param ignora os valores interpolados (JS: args
                // extras sao descartados), entao basta nao empacotar.
                // (cross-runtime #345) `>= n_params - 1`: o slot rest sempre
                // recebe um array, mesmo vazio. `tag\`ab\`` (0 interp) ->
                // values=[] (args.len()==1==n_params-1, drain de range vazio);
                // `tag\`a${1}b\`` (1 interp) -> values=[1]; etc. Sem isto, com
                // 0/1 valores o `values` ficava sem array (length -1).
                let rest_idx = n_params - 1;
                if n_params >= 2 && synthetic_call.args.len() >= rest_idx && last_is_handle {
                    let extra: Vec<Option<ExprOrSpread>> = synthetic_call
                        .args
                        .drain(rest_idx..)
                        .map(Some)
                        .collect();
                    let arr = Expr::Array(ArrayLit {
                        span: Default::default(),
                        elems: extra,
                    });
                    synthetic_call.args.push(ExprOrSpread {
                        spread: None,
                        expr: Box::new(arr),
                    });
                }
            }
        }
    }

    lower_call(ctx, &synthetic_call)
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
        Expr::TaggedTpl(_) => "tagged-template-fallback",
        Expr::This(_) => "this",
        Expr::Tpl(_) => "template",
        Expr::Unary(_) => "unary",
        Expr::Update(_) => "update",
        Expr::Yield(_) => "yield",
        _ => "unknown",
    }
}
