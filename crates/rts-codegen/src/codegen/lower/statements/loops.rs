use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{Decl, ForHead, Pat, Stmt, VarDeclOrExpr};

use super::super::ctx::{FnCtx, ValTy};
use super::super::expressions::lower_expr;
use super::decls::ts_type_to_val_ty;
use super::lower_stmt;

pub(super) fn lower_while_stmt(ctx: &mut FnCtx, wh: &swc_ecma_ast::WhileStmt) -> Result<bool> {
    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    let cond = lower_expr(ctx, &wh.test)?;
    let is_true = ctx.to_branch_cond(cond);
    ctx.builder.ins().brif(is_true, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    ctx.loop_stack
        .push((exit, header, ctx.pending_label.take(), ctx.finally_stack.len()));
    lower_stmt(ctx, &wh.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(header, &[]);
    }
    ctx.builder.seal_block(body);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_do_while_stmt(ctx: &mut FnCtx, dw: &swc_ecma_ast::DoWhileStmt) -> Result<bool> {
    let body = ctx.builder.create_block();
    let cond_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(body, &[]);
    ctx.builder.switch_to_block(body);

    ctx.loop_stack
        .push((exit, cond_block, ctx.pending_label.take(), ctx.finally_stack.len()));
    lower_stmt(ctx, &dw.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(cond_block, &[]);
    }

    ctx.builder.switch_to_block(cond_block);
    let cond = lower_expr(ctx, &dw.test)?;
    let is_true = ctx.to_branch_cond(cond);
    ctx.builder.ins().brif(is_true, body, &[], exit, &[]);
    ctx.builder.seal_block(body);
    ctx.builder.seal_block(cond_block);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_for_stmt(ctx: &mut FnCtx, for_stmt: &swc_ecma_ast::ForStmt) -> Result<bool> {
    if let Some(init) = &for_stmt.init {
        match init {
            VarDeclOrExpr::VarDecl(vd) => {
                lower_stmt(ctx, &Stmt::Decl(Decl::Var(vd.clone())))?;
            }
            VarDeclOrExpr::Expr(e) => {
                lower_expr(ctx, e)?;
            }
        }
    }

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    if let Some(test) = &for_stmt.test {
        let cond = lower_expr(ctx, test)?;
        let is_true = ctx.to_branch_cond(cond);
        ctx.builder.ins().brif(is_true, body, &[], exit, &[]);
    } else {
        ctx.builder.ins().jump(body, &[]);
    }

    ctx.builder.switch_to_block(body);
    ctx.loop_stack
        .push((exit, update_block, ctx.pending_label.take(), ctx.finally_stack.len()));
    // `body_terminated`: o corpo terminou com return/throw incondicional.
    // `is_unreachable()` mede ALCANCABILIDADE, nao terminacao â€” apos um `return`
    // no fim do corpo o bloco continua "alcancavel" mas ja' esta' fechado, e o
    // `jump(update_block)` viraria instrucao APOS o terminador (verifier error:
    // "terminator before end of block"). Usa o bool de lower_stmt.
    let body_terminated = lower_stmt(ctx, &for_stmt.body)?;
    ctx.loop_stack.pop();
    if !body_terminated && !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(update_block, &[]);
    }
    ctx.builder.seal_block(body);

    ctx.builder.switch_to_block(update_block);
    // Se o corpo sempre termina e nao ha' `continue`, update_block fica sem
    // predecessores (morto). Nao emite update/jump num bloco morto â€” Cranelift
    // o elimina; emitir deixaria instrucoes sem terminador valido.
    if !ctx.builder.is_unreachable() {
        if let Some(update) = &for_stmt.update {
            lower_expr(ctx, update)?;
        }
        if !ctx.builder.is_unreachable() {
            ctx.builder.ins().jump(header, &[]);
        }
    }
    ctx.builder.seal_block(update_block);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_for_of(ctx: &mut FnCtx, for_of: &swc_ecma_ast::ForOfStmt) -> Result<bool> {
    // (cross-runtime #109/#379/#392) `for await` sobre um async generator eager:
    // os valores ja' estao num Vec materializado (o body de `async function*`
    // produz `__gen_buf`), entao iteramos como for-of normal â€” await de um
    // non-Promise eh no-op. Casos lazy/Promise-yielding ficam pra #207.

    // (#210) for-of suporta tanto `for (const x of arr)` quanto
    // `for (const [k, v] of pairs)`. No segundo caso, geramos bind temp
    // `__forof_pair_N` e capturamos os nomes pra extrair via vec_get
    // dentro do body.
    // object_destructure_keys: Some(vec[(var_name, key)]) quando o bind eh
    // object pattern (`for (const {a, b} of items)`). Extraido via MAP_GET por
    // chave no body (analogo ao array pattern via vec_get por indice).
    let mut object_destructure_keys: Option<Vec<(String, String)>> = None;
    let (bind_name, bind_ty, array_destructure_names) = match &for_of.left {
        ForHead::VarDecl(vd) => {
            if vd.decls.len() != 1 {
                return Err(anyhow!("for-of bind deve declarar uma variavel"));
            }
            match &vd.decls[0].name {
                Pat::Ident(id) => {
                    let ty = id
                        .type_ann
                        .as_ref()
                        .and_then(|t| ts_type_to_val_ty(&t.type_ann))
                        .unwrap_or(ValTy::I64);
                    (id.sym.as_str().to_string(), ty, None)
                }
                // (cross-runtime) `for (const {a, b} of items)` â€” object pattern.
                // Coleta (var_name, key) de cada prop. Suporta shorthand `{a}` e
                // rename `{a: b}`. Extrai via MAP_GET no body.
                Pat::Object(obj_pat) => {
                    use swc_ecma_ast::ObjectPatProp;
                    let mut keys: Vec<(String, String)> = Vec::new();
                    for prop in &obj_pat.props {
                        match prop {
                            ObjectPatProp::Assign(a) => {
                                // `{a}` ou `{a = default}` â€” var e key sao iguais.
                                keys.push((a.key.id.sym.to_string(), a.key.id.sym.to_string()));
                            }
                            ObjectPatProp::KeyValue(kv) => {
                                let key = match &kv.key {
                                    swc_ecma_ast::PropName::Ident(i) => i.sym.to_string(),
                                    swc_ecma_ast::PropName::Str(s) => {
                                        s.value.to_string_lossy().to_string()
                                    }
                                    _ => return Err(anyhow!(
                                        "for-of object destructuring: chave nao suportada"
                                    )),
                                };
                                if let Pat::Ident(id) = kv.value.as_ref() {
                                    keys.push((id.id.sym.to_string(), key));
                                } else {
                                    return Err(anyhow!(
                                        "for-of object destructuring: valor deve ser ident"
                                    ));
                                }
                            }
                            ObjectPatProp::Rest(_) => return Err(anyhow!(
                                "for-of object destructuring: rest nao suportado"
                            )),
                        }
                    }
                    object_destructure_keys = Some(keys);
                    let tmp = format!("__forof_objpat_{:p}", &for_of.span);
                    (tmp, ValTy::Handle, None)
                }
                Pat::Array(arr_pat) => {
                    // Coleta (nome, tipo) dos elementos. Aceita Ident
                    // (com type ann opcional) e None (elision).
                    let mut names: Vec<Option<(String, ValTy)>> =
                        Vec::with_capacity(arr_pat.elems.len());
                    for elem in &arr_pat.elems {
                        match elem {
                            None => names.push(None),
                            Some(Pat::Ident(id)) => {
                                let ty = id
                                    .type_ann
                                    .as_ref()
                                    .and_then(|t| ts_type_to_val_ty(&t.type_ann))
                                    .unwrap_or(ValTy::I64);
                                names.push(Some((id.sym.as_str().to_string(), ty)))
                            }
                            // (cross-runtime) `for (const [a = 0, b = 0] of ...)`
                            // â€” default no pattern. Extrai o ident interno; o
                            // slot ausente ja' vira sentinel (0 p/ number), que
                            // cobre o caso comum `= 0`. Default nao-trivial eh
                            // refinamento. Sem isto, dava erro e nao iterava.
                            Some(Pat::Assign(ap)) => {
                                if let Pat::Ident(id) = ap.left.as_ref() {
                                    let ty = id
                                        .type_ann
                                        .as_ref()
                                        .and_then(|t| ts_type_to_val_ty(&t.type_ann))
                                        .unwrap_or(ValTy::I64);
                                    names.push(Some((id.sym.as_str().to_string(), ty)))
                                } else {
                                    return Err(anyhow!(
                                        "for-of destructuring: default so' em ident simples"
                                    ));
                                }
                            }
                            _ => {
                                return Err(anyhow!(
                                    "for-of destructuring suporta apenas idents simples ou elision"
                                ));
                            }
                        }
                    }
                    let tmp = format!("__forof_pair_{:p}", &for_of.span);
                    (tmp, ValTy::Handle, Some(names))
                }
                _ => return Err(anyhow!("for-of bind deve ser ident ou array pattern")),
            }
        }
        ForHead::Pat(p) => match p.as_ref() {
            Pat::Ident(id) => (id.sym.as_str().to_string(), ValTy::I64, None),
            _ => return Err(anyhow!("for-of bind deve ser ident simples")),
        },
        ForHead::UsingDecl(_) => return Err(anyhow!("`using` em for-of nao suportado")),
    };

    let iter_tv = lower_expr(ctx, &for_of.right)?;
    let mut handle = ctx.coerce_to_i64(iter_tv).val;

    // (#222) Map/Set iteracao via for-of:
    // - Map: converte para Vec de [k,v] entries preservando insertion order
    // - Set: usa MAP_VALUES (elementos reconvertidos)
    // Detecta classe via local_class_ty[var_name] OU `new Map/Set(...)` inline.
    let iter_class: Option<String> = match for_of.right.as_ref() {
        swc_ecma_ast::Expr::Ident(id) => ctx.local_class_ty.get(id.sym.as_str()).cloned(),
        // (cross-runtime) `for (const x of new Set([...]))` inline (sem var).
        swc_ecma_ast::Expr::New(ne) => {
            if let swc_ecma_ast::Expr::Ident(cid) = ne.callee.as_ref() {
                match cid.sym.as_str() {
                    "Set" => Some("Set".to_string()),
                    "Map" => Some("Map".to_string()),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    };
    if matches!(iter_class.as_deref(), Some("Map")) {
        let entries_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(entries_fn, &[handle]);
        handle = ctx.builder.inst_results(inst)[0];
    } else if matches!(iter_class.as_deref(), Some("Set")) {
        // Set internamente eh Map<key, 1> â€” os ELEMENTOS sao as keys.
        // (cross-runtime) Usa MAP_VALUES (nao MAP_KEYS): p/ Set, MAP_VALUES
        // reconverte cada key string de volta ao valor (parse int -> number,
        // senao handle string). MAP_KEYS retornava as keys string CRUAS, e
        // `for (const x of setNum) sum += x` somava o handle (lixo). Mesma
        // conversao usada no spread de Set (#1229).
        let vals_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(vals_fn, &[handle]);
        handle = ctx.builder.inst_results(inst)[0];
    } else if for_of_iterates_string(ctx, &for_of.right) {
        // (cross-runtime) `for (const c of "abc")` / string var: itera os
        // CHARS (codepoints). Converte a string num Vec de char-handles via
        // VEC_NEW + SPREAD_INTO_VEC (mesmo helper do spread `[..."abc"]`).
        // Sem isso, o for-of tratava a string como Vec (VEC_LEN=-1) e nao
        // iterava.
        let vec_new = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[], Some(cl::I64))?;
        let inst_v = ctx.builder.ins().call(vec_new, &[]);
        let char_vec = ctx.builder.inst_results(inst_v)[0];
        let spread_fn = ctx.get_extern(
            "__RTS_FN_RT_SPREAD_INTO_VEC",
            &[cl::I64, cl::I64],
            None,
        )?;
        ctx.builder.ins().call(spread_fn, &[char_vec, handle]);
        handle = char_vec;
    } else if iter_class.is_none() {
        // (#394) Classe estatica desconhecida (ex: bind de outro for-of, como
        // `for (const s of setOfSets) for (const n of s)`). Normaliza em
        // runtime: Set->elementos, Map->entries, resto inalterado. So' altera
        // handles que seriam mal-iterados (Set/Map sem classe conhecida); Vec
        // passa direto.
        let norm_fn = ctx.get_extern(
            "__RTS_FN_RT_FOR_OF_NORMALIZE",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let inst = ctx.builder.ins().call(norm_fn, &[handle]);
        handle = ctx.builder.inst_results(inst)[0];
    }

    let len_fref = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_LEN", &[cl::I64], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(len_fref, &[handle]);
    let len = ctx.builder.inst_results(inst)[0];

    let get_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    if matches!(&for_of.left, ForHead::VarDecl(_)) {
        ctx.declare_local(&bind_name, bind_ty, zero);
    }

    if let swc_ecma_ast::Expr::Ident(id) = for_of.right.as_ref() {
        let arr_name = id.sym.as_str();
        if let Some(elem_cls) = ctx.local_array_class_ty.get(arr_name).cloned() {
            ctx.local_class_ty.insert(bind_name.clone(), elem_cls);
        }
        // (#592) Array de object literals: propaga field types pra bind.
        if let Some(types) = ctx.local_array_obj_field_types.get(arr_name).cloned() {
            ctx.local_obj_field_types.insert(bind_name.clone(), types);
        }
    }

    // (bug #2) for-of over a class-field array (`for (const it of this.items)`
    // where `items: CartItem[]`): infer the element class from the field's
    // declared type and give the bind that class. Without it the bind is
    // untyped, so `it.method()` mis-dispatches on a corrupted receiver â†’ trap
    // (ud2). Only the Ident (local var) case was handled above.
    if let swc_ecma_ast::Expr::Member(m) = for_of.right.as_ref() {
        if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
            let recv_class: Option<String> = match m.obj.as_ref() {
                swc_ecma_ast::Expr::This(_) => ctx.current_class.clone(),
                swc_ecma_ast::Expr::Ident(oid) => ctx
                    .local_class_ty
                    .get(oid.sym.as_str())
                    .cloned()
                    .or_else(|| ctx.global_class_ty.get(oid.sym.as_str()).cloned()),
                _ => None,
            };
            if let Some(rc) = recv_class {
                if let Some(meta) = ctx.classes.get(&rc) {
                    if let Some(ann) = meta.field_class_names.get(prop.sym.as_str()) {
                        let elem = ann.trim().strip_suffix("[]").unwrap_or(ann.trim());
                        if ctx.classes.contains_key(elem) {
                            ctx.local_class_ty.insert(bind_name.clone(), elem.to_string());
                        }
                    }
                }
            }
        }
    }

    let counter_name = format!("__rts_for_of_i_{:p}", &for_of.span);
    ctx.declare_local(&counter_name, ValTy::I64, zero);

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let is_in_range = ctx
        .builder
        .ins()
        .icmp(IntCC::SignedLessThan, i_now.val, len);
    ctx.builder.ins().brif(is_in_range, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let inst = ctx.builder.ins().call(get_fref, &[handle, i_now.val]);
    let mut elem = ctx.builder.inst_results(inst)[0];
    // (cross-runtime #392) `for await (const v of arr)`: aguarda CADA elemento.
    // AWAIT_VALUE faz passthrough de non-Promises (valores ja' resolvidos de um
    // async gen drain) e wait+valor de Promises (array de Promises). JS spec:
    // for-await aguarda cada valor produzido pelo iterador.
    if for_of.is_await {
        let await_fn = ctx.get_extern(
            "__RTS_FN_NS_PROMISE_AWAIT_VALUE",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let ai = ctx.builder.ins().call(await_fn, &[elem]);
        elem = ctx.builder.inst_results(ai)[0];
    }
    ctx.write_local(&bind_name, elem)?;

    // (cross-runtime #222) Bind simples de for-of sem tipo Handle estatico:
    // o slot pode carregar um handle (string/obj) OU i64 puro. Caso
    // `for (const k of map.keys())` / `map.values()` onde os elementos sao
    // strings. Marca o elem como vec-slot ambiguo pra que template
    // (`ks + k`) use coercao runtime (detecta string) em vez de imprimir o
    // handle cru. So' quando NAO ha destructuring (esse ja' marca por slot).
    if array_destructure_names.is_none()
        && !matches!(bind_ty, ValTy::Handle | ValTy::F64 | ValTy::I32)
    {
        ctx.var_member_call_values.insert(elem);
        ctx.var_vec_slot_values.insert(elem);
    }

    // (#210) for-of array destructuring: cada nome vira local extraindo
    // posicao correspondente do vec via vec_get. Fora-do-range retorna 0.
    if let Some(names) = &array_destructure_names {
        // (#210) Inferencia de tipo dos slots do destructuring.
        // SWC nao expoe tuple types em Pat::Array de forma utilizavel,
        // entao usamos heuristica: se o iteravel veio de Object.entries
        // (o caso mais comum de `for (const [k, v] of obj)`), o slot 0
        // e' string Handle. Para outros casos (`[number, number]`), o
        // user pode anotar o ident: `for (const [k, v]: any of pairs)`
        // nao funciona via parser mas a heuristica cobre 90% do uso.
        // Detecta se o iteravel veio direto de `Object.entries(...)`
        // OU de `map.entries()` (NAO `arr.entries()` â€” array.entries
        // tem slot 0 = i64 indice, nao Handle). (#208 + #494)
        fn is_object_entries_expr(e: &swc_ecma_ast::Expr, ctx: &FnCtx) -> bool {
            match e {
                swc_ecma_ast::Expr::Call(c) => match &c.callee {
                    swc_ecma_ast::Callee::Expr(callee) => match callee.as_ref() {
                        swc_ecma_ast::Expr::Member(m) => {
                            // Confirma prop.entries.
                            let is_entries = matches!(
                                &m.prop,
                                swc_ecma_ast::MemberProp::Ident(p) if p.sym.as_str() == "entries"
                            );
                            if !is_entries {
                                return false;
                            }
                            // Object.entries(obj) â€” slot 0 e' string Handle.
                            // map.entries() â€” slot 0 e' string Handle.
                            // arr.entries() â€” slot 0 e' i64 indice â†’ falso.
                            match m.obj.as_ref() {
                                // Object.entries â€” explicito.
                                swc_ecma_ast::Expr::Ident(id) if id.sym.as_str() == "Object" => true,
                                // var ident: se for marcado como array, falso.
                                swc_ecma_ast::Expr::Ident(id) => {
                                    !ctx.local_array_vars.contains(id.sym.as_str())
                                }
                                _ => true, // conservador
                            }
                        }
                        _ => false,
                    },
                    _ => false,
                },
                swc_ecma_ast::Expr::Paren(p) => is_object_entries_expr(&p.expr, ctx),
                _ => false,
            }
        }
        // (#222) Map iter via for-of: slot 0 eh Handle de key.
        let is_map_iter = if let swc_ecma_ast::Expr::Ident(id) = for_of.right.as_ref() {
            matches!(ctx.local_class_ty.get(id.sym.as_str()).map(|s| s.as_str()), Some("Map"))
        } else {
            false
        };
        let is_object_entries =
            is_map_iter || is_object_entries_expr(for_of.right.as_ref(), ctx);
        for (idx, name_opt) in names.iter().enumerate() {
            let Some((name, ann_ty)) = name_opt else { continue };
            let idx_val = ctx.builder.ins().iconst(cl::I64, idx as i64);
            let inst = ctx.builder.ins().call(get_fref, &[elem, idx_val]);
            let val = ctx.builder.inst_results(inst)[0];
            // Heuristica: se anotacao explicita, usa. Senao, slot 0 de
            // Object.entries() e' Handle (key string), demais I64.
            let resolved_ty = if is_object_entries && idx == 0 {
                ValTy::Handle
            } else {
                *ann_ty
            };
            ctx.declare_local(name, resolved_ty, val);
            // (cross-runtime #208/#494) Sem tipo estatico Handle, o slot pode
            // carregar um handle (string/obj) OU i64 puro â€” caso `arr.entries()`
            // onde o slot 1 e' o elemento do array (string quando arr eh de
            // strings). Marca o valor como vec-slot ambiguo pra que template
            // (`i + ":" + v`) use TPL_COERCE_VEC_SLOT/INSPECT e detecte string
            // em runtime, em vez de imprimir o handle cru. Sem isso,
            // `for (const [i,v] of arr.entries())` imprimia o numero do handle.
            if !matches!(resolved_ty, ValTy::Handle) {
                ctx.var_member_call_values.insert(val);
                ctx.var_vec_slot_values.insert(val);
            }
        }
    }

    // (cross-runtime) for-of object destructuring: cada (var, key) extrai
    // `elem[key]` via MAP_GET. elem eh o objeto iterado (handle Map). Valor
    // marcado ambiguo (campo pode ser string/number/handle) p/ template/uso
    // despachar coercao runtime.
    if let Some(keys) = &object_destructure_keys {
        let get_fn = ctx.get_extern(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET",
            &[cl::I64, cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        for (var_name, key) in keys {
            let (kp, kl) = ctx.emit_str_literal(key.as_bytes())?;
            let g = ctx.builder.ins().call(get_fn, &[elem, kp, kl]);
            let val = ctx.builder.inst_results(g)[0];
            ctx.declare_local(var_name, ValTy::I64, val);
            ctx.var_member_call_values.insert(val);
            ctx.var_vec_slot_values.insert(val);
        }
    }

    ctx.loop_stack
        .push((exit, update_block, ctx.pending_label.take(), ctx.finally_stack.len()));
    lower_stmt(ctx, &for_of.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(update_block, &[]);
    }
    ctx.builder.seal_block(body);

    ctx.builder.switch_to_block(update_block);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let one = ctx.builder.ins().iconst(cl::I64, 1);
    let i_next = ctx.builder.ins().iadd(i_now.val, one);
    ctx.write_local(&counter_name, i_next)?;
    ctx.builder.ins().jump(header, &[]);
    ctx.builder.seal_block(update_block);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_for_in(ctx: &mut FnCtx, for_in: &swc_ecma_ast::ForInStmt) -> Result<bool> {
    let bind_name = match &for_in.left {
        ForHead::VarDecl(vd) => {
            if vd.decls.len() != 1 {
                return Err(anyhow!("for-in bind deve declarar uma variavel"));
            }
            match &vd.decls[0].name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => return Err(anyhow!("for-in bind deve ser ident simples")),
            }
        }
        ForHead::Pat(p) => match p.as_ref() {
            Pat::Ident(id) => id.sym.as_str().to_string(),
            _ => return Err(anyhow!("for-in bind deve ser ident simples")),
        },
        ForHead::UsingDecl(_) => return Err(anyhow!("`using` em for-in nao suportado")),
    };

    let iter_tv = lower_expr(ctx, &for_in.right)?;
    let raw_handle = ctx.coerce_to_i64(iter_tv).val;

    // (#94) for-in funciona em Map E Array. MAP_LEN/MAP_KEY_AT so'
    // funciona em Map (retorna -1 em Vec). Materializa keys via
    // FOR_IN_KEYS (#1097) que estende OBJECT_KEYS_AUTO com walk
    // da prototype chain (Object.create / class extends), filtra
    // __rts_class e Map/Set JS conforme JS spec.
    let keys_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_FOR_IN_KEYS",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let keys_inst = ctx.builder.ins().call(keys_fref, &[raw_handle]);
    let handle = ctx.builder.inst_results(keys_inst)[0];

    let len_fref = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_LEN", &[cl::I64], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(len_fref, &[handle]);
    let len = ctx.builder.inst_results(inst)[0];

    let key_at_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    if matches!(&for_in.left, ForHead::VarDecl(_)) {
        ctx.declare_local(&bind_name, ValTy::Handle, zero);
    }

    let counter_name = format!("__rts_for_in_i_{:p}", &for_in.span);
    ctx.declare_local(&counter_name, ValTy::I64, zero);

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-in counter sumiu"))?;
    let is_in_range = ctx
        .builder
        .ins()
        .icmp(IntCC::SignedLessThan, i_now.val, len);
    ctx.builder.ins().brif(is_in_range, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-in counter sumiu"))?;
    let inst = ctx.builder.ins().call(key_at_fref, &[handle, i_now.val]);
    let key_handle = ctx.builder.inst_results(inst)[0];
    ctx.write_local(&bind_name, key_handle)?;

    ctx.loop_stack
        .push((exit, update_block, ctx.pending_label.take(), ctx.finally_stack.len()));
    lower_stmt(ctx, &for_in.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(update_block, &[]);
    }
    ctx.builder.seal_block(body);

    ctx.builder.switch_to_block(update_block);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-in counter sumiu"))?;
    let one = ctx.builder.ins().iconst(cl::I64, 1);
    let i_next = ctx.builder.ins().iadd(i_now.val, one);
    ctx.write_local(&counter_name, i_next)?;
    ctx.builder.ins().jump(header, &[]);
    ctx.builder.seal_block(update_block);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

/// (cross-runtime) `for (const c of <expr>)` itera os chars quando `<expr>`
/// eh string: literal `"..."`, template, ou var rastreada como string.
/// Conservador â€” so' dispara com sinal claro de string (nao afeta
/// arrays/Map/Set).
fn for_of_iterates_string(ctx: &FnCtx, e: &swc_ecma_ast::Expr) -> bool {
    use swc_ecma_ast::Expr;
    match e {
        Expr::Lit(swc_ecma_ast::Lit::Str(_)) => true,
        Expr::Tpl(_) => true,
        Expr::Paren(p) => for_of_iterates_string(ctx, &p.expr),
        Expr::Ident(id) => ctx.local_string_vars.contains(id.sym.as_str()),
        // (cross-runtime) `for (const ch of a + b)` concat -> string quando
        // algum lado eh string. So' `+` (outros ops nao produzem string).
        Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) => {
            for_of_iterates_string(ctx, &b.left) || for_of_iterates_string(ctx, &b.right)
        }
        // (cross-runtime) `for (const ch of f())` / `obj.m()` onde a fn/metodo
        // retorna string (FNS_RET_STRING populado no array_methods_pass).
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                match callee.as_ref() {
                    Expr::Ident(fid) => crate::codegen::lower::passes::parallelism::FNS_RET_STRING
                        .with(|s| s.borrow().contains(fid.sym.as_str())),
                    Expr::Member(m) => matches!(&m.prop,
                        swc_ecma_ast::MemberProp::Ident(p)
                            if crate::codegen::lower::passes::parallelism::FNS_RET_STRING
                                .with(|s| s.borrow().contains(p.sym.as_str()))),
                    _ => false,
                }
            } else {
                false
            }
        }
        // (cross-runtime) `for (const ch of h.text)` / `this.content` onde o
        // field eh `: string`. Resolve a classe do receiver (this/ident) e
        // checa field_class_names == "string" na hierarquia.
        Expr::Member(m) => {
            if let swc_ecma_ast::MemberProp::Ident(prop) = &m.prop {
                if let Some(cls) = receiver_class_for_string(ctx, &m.obj) {
                    return field_is_string(ctx, &cls, prop.sym.as_str());
                }
            }
            false
        }
        _ => false,
    }
}

/// (cross-runtime) Resolve a classe do receiver de um member access, apenas
/// para os casos comuns: `this` (classe atual) e ident de var tipada classe.
fn receiver_class_for_string(ctx: &FnCtx, e: &swc_ecma_ast::Expr) -> Option<String> {
    use swc_ecma_ast::Expr;
    match e {
        Expr::This(_) => ctx.current_class.clone(),
        Expr::Ident(id) => ctx
            .local_class_ty
            .get(id.sym.as_str())
            .cloned()
            .or_else(|| ctx.global_class_ty.get(id.sym.as_str()).cloned()),
        Expr::Paren(p) => receiver_class_for_string(ctx, &p.expr),
        _ => None,
    }
}

/// (cross-runtime) O field `prop` da classe `cls` (ou de um ancestral) eh
/// declarado `: string`? Consulta field_class_names subindo a hierarquia.
fn field_is_string(ctx: &FnCtx, cls: &str, prop: &str) -> bool {
    let mut current = Some(cls.to_string());
    let mut depth = 0u32;
    while let Some(name) = current {
        if depth > 16 {
            break;
        }
        let Some(meta) = ctx.classes.get(&name) else {
            break;
        };
        if let Some(ty) = meta.field_class_names.get(prop) {
            return ty.trim() == "string";
        }
        current = meta.super_class.clone();
        depth += 1;
    }
    false
}
