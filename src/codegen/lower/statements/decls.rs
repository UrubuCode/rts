use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use std::collections::HashMap;
use swc_ecma_ast::{Pat, VarDecl, VarDeclKind};

use super::super::ctx::{FnCtx, TypedVal, ValTy};
use super::super::expressions::lower_expr;

pub(super) fn lower_var_decl(ctx: &mut FnCtx, var_decl: &VarDecl) -> Result<bool> {
    for decl in &var_decl.decls {
        let name = match &decl.name {
            Pat::Ident(id) => id.sym.as_str().to_string(),
            Pat::Array(_) | Pat::Object(_) => return Err(anyhow!("destructuring not supported")),
            other => return Err(anyhow!("unsupported binding pattern: {other:?}")),
        };

        let ann_ty = match &decl.name {
            Pat::Ident(id) => id
                .type_ann
                .as_ref()
                .and_then(|t| ts_type_to_val_ty(&t.type_ann)),
            _ => None,
        };

        if let Pat::Ident(id) = &decl.name {
            if let Some(ann) = id.type_ann.as_ref() {
                if let Some(cn) = class_name_from_annotation(&ann.type_ann) {
                    if ctx.classes.contains_key(&cn)
                        || crate::abi::global_class_lookup(&cn).is_some()
                    {
                        ctx.local_class_ty.insert(name.clone(), cn);
                    }
                }
                if let swc_ecma_ast::TsType::TsArrayType(arr) = ann.type_ann.as_ref() {
                    if let Some(cn) = class_name_from_annotation(&arr.elem_type) {
                        if ctx.classes.contains_key(&cn) {
                            ctx.local_array_class_ty.insert(name.clone(), cn);
                        }
                    }
                    // (#208) Marca a var como array independente do tipo de elemento,
                    // pra que `lower_var_member_call` prefira `lower_array_builtin`.
                    ctx.local_array_vars.insert(name.clone());
                }
                // Também `Array<T>` / `ReadonlyArray<T>` via TsTypeRef.
                if let swc_ecma_ast::TsType::TsTypeRef(tref) = ann.type_ann.as_ref() {
                    if let swc_ecma_ast::TsEntityName::Ident(id) = &tref.type_name {
                        let n = id.sym.as_str();
                        if matches!(n, "Array" | "ReadonlyArray") {
                            ctx.local_array_vars.insert(name.clone());
                        }
                    }
                }
            }
        }

        // Capture field types for object literals (used by enum string).
        if let Some(init) = decl.init.as_ref() {
            // (#274) Peel TS-only wrappers (as/satisfies/as const/!) para
            // que `const cfg = { port: 3000 } satisfies T` ainda registre
            // os tipos de campo. Sem isso `cfg.port` cai em fallback de
            // GLOBAL_CLASS_SPECS (URL.port) e retorna lixo.
            fn peel_ts_init(e: &swc_ecma_ast::Expr) -> &swc_ecma_ast::Expr {
                match e {
                    swc_ecma_ast::Expr::TsAs(a) => peel_ts_init(&a.expr),
                    swc_ecma_ast::Expr::TsSatisfies(a) => peel_ts_init(&a.expr),
                    swc_ecma_ast::Expr::TsConstAssertion(a) => peel_ts_init(&a.expr),
                    swc_ecma_ast::Expr::TsNonNull(n) => peel_ts_init(&n.expr),
                    swc_ecma_ast::Expr::TsTypeAssertion(a) => peel_ts_init(&a.expr),
                    swc_ecma_ast::Expr::Paren(p) => peel_ts_init(&p.expr),
                    _ => e,
                }
            }
            let init_peeled = peel_ts_init(init.as_ref());
            // (#208) `const a = [1,2,3]` — sem anotacao mas init e' literal
            // array. Marca pra preferir lower_array_builtin.
            if matches!(init_peeled, swc_ecma_ast::Expr::Array(_)) {
                ctx.local_array_vars.insert(name.clone());
            }
            if let swc_ecma_ast::Expr::Object(obj) = init_peeled {
                let mut field_types: std::collections::HashMap<String, ValTy> =
                    std::collections::HashMap::new();
                for prop in &obj.props {
                    if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                            let key = match &kv.key {
                                swc_ecma_ast::PropName::Ident(id) => id.sym.as_str().to_string(),
                                swc_ecma_ast::PropName::Str(s) => {
                                    s.value.to_string_lossy().to_string()
                                }
                                _ => continue,
                            };
                            // Strings literais armazenam Handle.
                            // Numeros literais armazenam I64 (suficiente
                            // pra distinguir Map/Set/Array vs object com
                            // campo `size`/`length` no #222 lookup).
                            match kv.value.as_ref() {
                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(_)) => {
                                    field_types.insert(key.clone(), ValTy::Handle);
                                }
                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)) => {
                                    field_types.insert(key.clone(), ValTy::I64);
                                }
                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => {
                                    field_types.insert(key.clone(), ValTy::Bool);
                                }
                                // (#210) Sub-object literal: registra tipos
                                // dos campos do nested para nested
                                // destructuring conseguir inferir.
                                swc_ecma_ast::Expr::Object(sub_obj) => {
                                    let mut sub_types: std::collections::HashMap<
                                        String,
                                        ValTy,
                                    > = std::collections::HashMap::new();
                                    for sub_prop in &sub_obj.props {
                                        if let swc_ecma_ast::PropOrSpread::Prop(sp) = sub_prop {
                                            if let swc_ecma_ast::Prop::KeyValue(skv) = sp.as_ref() {
                                                let sk = match &skv.key {
                                                    swc_ecma_ast::PropName::Ident(id) => {
                                                        id.sym.as_str().to_string()
                                                    }
                                                    swc_ecma_ast::PropName::Str(s) => {
                                                        s.value.to_string_lossy().to_string()
                                                    }
                                                    _ => continue,
                                                };
                                                let sty = match skv.value.as_ref() {
                                                    swc_ecma_ast::Expr::Lit(
                                                        swc_ecma_ast::Lit::Str(_),
                                                    ) => Some(ValTy::Handle),
                                                    swc_ecma_ast::Expr::Lit(
                                                        swc_ecma_ast::Lit::Num(_),
                                                    ) => Some(ValTy::I64),
                                                    swc_ecma_ast::Expr::Lit(
                                                        swc_ecma_ast::Lit::Bool(_),
                                                    ) => Some(ValTy::Bool),
                                                    _ => None,
                                                };
                                                if let Some(t) = sty {
                                                    sub_types.insert(sk, t);
                                                }
                                            }
                                        }
                                    }
                                    field_types.insert(key.clone(), ValTy::Handle);
                                    if !sub_types.is_empty() {
                                        ctx.local_nested_obj_field_types.insert(
                                            (name.clone(), key.clone()),
                                            sub_types,
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if !field_types.is_empty() {
                    ctx.local_obj_field_types.insert(name.clone(), field_types);
                }
            }
            // #210 destructuring — `const __destruct_N = obj` ou
            // `const x = obj` com obj sendo var local que tem tipos
            // de campo registrados: propaga os tipos. Sem isso a leitura
            // subsequente de \`__destruct_N.field\` retorna I64 e strings
            // saem como handles brutos.
            if let swc_ecma_ast::Expr::Ident(src_id) = init.as_ref() {
                let src_name = src_id.sym.as_str();
                if let Some(types) = ctx.local_obj_field_types.get(src_name).cloned() {
                    ctx.local_obj_field_types.insert(name.clone(), types);
                }
                // (#210) Aliasing: propaga nested types tambem, prefixados
                // pelo novo nome — assim `const __d0 = cfg` herda
                // os mesmos pares (cfg, key) sob (__d0, key).
                let nested_clone: Vec<((String, String), HashMap<String, ValTy>)> = ctx
                    .local_nested_obj_field_types
                    .iter()
                    .filter(|((on, _), _)| on == src_name)
                    .map(|((_, k), v)| ((name.clone(), k.clone()), v.clone()))
                    .collect();
                for (k, v) in nested_clone {
                    ctx.local_nested_obj_field_types.insert(k, v);
                }
                // Aliasing: const b = a — propaga local_class_ty.
                if let Some(cn) = ctx.local_class_ty.get(src_name).cloned() {
                    ctx.local_class_ty.insert(name.clone(), cn);
                }
            }
            // (#210) Nested destructuring — `const x = cfg.db` onde cfg
            // tem nested object literal registrado. Recursivamente
            // propaga tipos dos sub-campos. Sem isso, `const { db: { host } } = cfg`
            // (depois de expand_destructuring vira `const __d1 = cfg.db; const { host } = __d1;`)
            // perde o tipo de `host` e mostra handle bruto em template literal.
            if let swc_ecma_ast::Expr::Member(m) = init.as_ref() {
                if let (swc_ecma_ast::Expr::Ident(obj_id), swc_ecma_ast::MemberProp::Ident(prop)) =
                    (m.obj.as_ref(), &m.prop)
                {
                    let obj_name = obj_id.sym.as_str();
                    let key = prop.sym.as_str();
                    // Procura tipos nested para `obj.key` registrados via
                    // local_nested_obj_field_types. Sem essa estrutura,
                    // tentamos heuristica: se o campo `key` em obj_name e'
                    // Handle e o init original era objeto literal, podemos
                    // propagar — mas isso exige tracking de literals nested.
                    // Por hora, herda tipos default (Handle pra strings)
                    // se obj_name aponta pra registro de literais nested.
                    if let Some(nested) = ctx
                        .local_nested_obj_field_types
                        .get(&(obj_name.to_string(), key.to_string()))
                        .cloned()
                    {
                        ctx.local_obj_field_types.insert(name.clone(), nested);
                    }
                }
            }
        }

        if !ctx.local_class_ty.contains_key(&name) {
            if let Some(init) = decl.init.as_ref() {
                if let swc_ecma_ast::Expr::New(ne) = init.as_ref() {
                    if let swc_ecma_ast::Expr::Ident(cid) = ne.callee.as_ref() {
                        let cn = cid.sym.as_str().to_string();
                        // (#214) Error builtin classes: registra field
                        // types pra que `e.message`/`e.name` retorne
                        // Handle em vez de I64 anonimo.
                        let is_error_class = matches!(
                            cn.as_str(),
                            "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
                        );
                        if ctx.classes.contains_key(&cn)
                            || crate::abi::global_class_lookup(&cn).is_some()
                        {
                            ctx.local_class_ty.insert(name.clone(), cn.clone());
                        }
                        if is_error_class {
                            let mut ft: std::collections::HashMap<String, ValTy> =
                                std::collections::HashMap::new();
                            ft.insert("message".into(), ValTy::Handle);
                            ft.insert("name".into(), ValTy::Handle);
                            ctx.local_obj_field_types.insert(name.clone(), ft);
                        }
                    }
                }
                // Peel await: `await fetch(...)` → `fetch(...)`
                fn peel_await(e: &swc_ecma_ast::Expr) -> &swc_ecma_ast::Expr {
                    match e {
                        swc_ecma_ast::Expr::Await(a) => peel_await(&a.arg),
                        swc_ecma_ast::Expr::Paren(p) => peel_await(&p.expr),
                        _ => e,
                    }
                }
                let init_inner = peel_await(init.as_ref());
                if let swc_ecma_ast::Expr::Call(call) = init_inner {
                    if let swc_ecma_ast::Callee::Expr(cb) = &call.callee {
                        if let swc_ecma_ast::Expr::Ident(fid) = cb.as_ref() {
                            let fname = fid.sym.as_str();
                            // fetch() → Response
                            if fname == "fetch" {
                                ctx.local_class_ty.insert(name.clone(), "Response".to_string());
                            } else if let Some(cn) = ctx.fn_class_returns.get(fname) {
                                ctx.local_class_ty.insert(name.clone(), cn.clone());
                            }
                        }
                        // Function global (#359): `.bind(...)` retorna Function.
                        // Propaga local_class_ty pro var receptor.
                        if let swc_ecma_ast::Expr::Member(m) = cb.as_ref() {
                            if let swc_ecma_ast::MemberProp::Ident(mid) = &m.prop {
                                if mid.sym.as_str() == "bind" {
                                    ctx.local_class_ty.insert(name.clone(), "Function".to_string());
                                }
                            }
                        }
                    }
                }
                let asserted_class = match init.as_ref() {
                    swc_ecma_ast::Expr::TsAs(a) => class_name_from_annotation(&a.type_ann),
                    swc_ecma_ast::Expr::TsTypeAssertion(a) => {
                        class_name_from_annotation(&a.type_ann)
                    }
                    _ => None,
                };
                if let Some(cn) = asserted_class {
                    if ctx.classes.contains_key(&cn) {
                        ctx.local_class_ty.insert(name.clone(), cn);
                    }
                }
            }
        }

        let (init_val, inferred_ty) = if let Some(init) = &decl.init {
            let tv = lower_expr(ctx, init)?;
            (tv.val, tv.ty)
        } else {
            let ty = ann_ty.unwrap_or(ValTy::I64);
            (zero_for_ty(ctx, ty), ty)
        };

        let ty = if ctx.module_scope && ctx.has_global(&name) {
            ctx.var_ty(&name).unwrap_or(ann_ty.unwrap_or(inferred_ty))
        } else {
            ann_ty.unwrap_or(inferred_ty)
        };
        let init_coerced = match ty {
            ValTy::I32 => ctx.coerce_to_i32(TypedVal::new(init_val, inferred_ty)).val,
            ValTy::I64 => ctx.coerce_to_i64(TypedVal::new(init_val, inferred_ty)).val,
            ValTy::F64 => ctx.coerce_to_f64(TypedVal::new(init_val, inferred_ty)).val,
            _ => init_val,
        };

        if ctx.module_scope && ctx.has_global(&name) {
            ctx.write_local(&name, init_coerced)?;
        } else {
            let is_const = matches!(var_decl.kind, VarDeclKind::Const);
            let function_scope = matches!(var_decl.kind, VarDeclKind::Var);
            ctx.declare_local_kind(&name, ty, init_coerced, is_const, function_scope);
        }
    }
    Ok(false)
}

pub(super) fn ts_type_to_val_ty(ty: &swc_ecma_ast::TsType) -> Option<ValTy> {
    use swc_ecma_ast::{TsKeywordTypeKind, TsLit, TsLitType, TsType, TsUnionOrIntersectionType};
    if let TsType::TsKeywordType(kw) = ty {
        return Some(match kw.kind {
            TsKeywordTypeKind::TsNumberKeyword => ValTy::F64,
            TsKeywordTypeKind::TsBooleanKeyword => ValTy::Bool,
            TsKeywordTypeKind::TsStringKeyword => ValTy::Handle,
            TsKeywordTypeKind::TsVoidKeyword => ValTy::I64,
            _ => return None,
        });
    }
    if let TsType::TsLitType(TsLitType { lit, .. }) = ty {
        return Some(match lit {
            TsLit::Str(_) | TsLit::Tpl(_) => ValTy::Handle,
            TsLit::Number(_) => ValTy::I64,
            TsLit::Bool(_) => ValTy::Bool,
            TsLit::BigInt(_) => ValTy::I64,
        });
    }
    if let TsType::TsTypeRef(r) = ty {
        let name = match &r.type_name {
            swc_ecma_ast::TsEntityName::Ident(id) => id.sym.as_str(),
            _ => return None,
        };
        return Some(ValTy::from_annotation(name));
    }
    if let TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(u)) = ty {
        // Union: se todos os ramos resolvem para o mesmo ValTy, usa ele.
        // Ramos null/undefined sao ignorados (covers `T | null`).
        let mut acc: Option<ValTy> = None;
        for member in &u.types {
            // Skip null/undefined branches.
            if let TsType::TsKeywordType(k) = member.as_ref() {
                if matches!(
                    k.kind,
                    TsKeywordTypeKind::TsNullKeyword
                        | TsKeywordTypeKind::TsUndefinedKeyword
                ) {
                    continue;
                }
            }
            let mt = ts_type_to_val_ty(member)?;
            match acc {
                None => acc = Some(mt),
                Some(prev) if prev == mt => {}
                _ => return None, // tipos misturados — codegen trata como I64.
            }
        }
        return acc;
    }
    if let TsType::TsParenthesizedType(p) = ty {
        return ts_type_to_val_ty(&p.type_ann);
    }
    // `T[]` e `Array<T>` sao handles GC (Vec<i64>).
    if let TsType::TsArrayType(_) = ty {
        return Some(ValTy::Handle);
    }
    None
}

pub(super) fn class_name_from_annotation(ty: &swc_ecma_ast::TsType) -> Option<String> {
    if let swc_ecma_ast::TsType::TsTypeRef(r) = ty {
        if let swc_ecma_ast::TsEntityName::Ident(id) = &r.type_name {
            return Some(id.sym.as_str().to_string());
        }
    }
    None
}

fn zero_for_ty(ctx: &mut FnCtx, ty: ValTy) -> cranelift_codegen::ir::Value {
    match ty {
        ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
        ValTy::F64 => ctx.builder.ins().f64const(0.0),
        _ => ctx.builder.ins().iconst(cl::I64, 0),
    }
}
