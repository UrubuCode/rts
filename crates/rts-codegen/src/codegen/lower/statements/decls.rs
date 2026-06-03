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
            // (generators) `const it = g()` onde `g` eh generator fn -> marca
            // `it` como generator_var pra que `it.next()` use GENERATOR_NEXT.
            if let Some(init) = &decl.init {
                if let swc_ecma_ast::Expr::Call(c) = init.as_ref() {
                    if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                        if let swc_ecma_ast::Expr::Ident(fid) = callee.as_ref() {
                            if crate::codegen::lower::compile::program::is_generator_fn(
                                fid.sym.as_str(),
                            ) {
                                ctx.generator_vars.insert(name.clone());
                            }
                        }
                    }
                }
            }
            // (A — #1281) `const g = (i:number)=>i+100` — init eh ident de
            // arrow liftada/hoistada cujo UserFnAbi.ret e' F64. Marca
            // local_fn_ret_f64 p/ que `g(5)` use INVOKE_AUTO_AS_F64 + bitcast
            // (senao o retorno f64 e' lido como int via fcvt_from_sint).
            if let Some(init) = &decl.init {
                if let swc_ecma_ast::Expr::Ident(fid) = init.as_ref() {
                    let fname = fid.sym.as_str();
                    if (fname.starts_with("__lifted_arrow_")
                        || fname.starts_with("__hoisted_arrow_"))
                        && ctx
                            .user_fns
                            .get(fname)
                            .and_then(|f| f.ret)
                            .map(|r| matches!(r, ValTy::F64))
                            .unwrap_or(false)
                    {
                        ctx.local_fn_ret_f64.insert(name.clone());
                    }
                }
            }
            if let Some(ann) = id.type_ann.as_ref() {
                // (372) `f: () => number` — fn que retorna number. Marca pra
                // que `f()` reinterprete o i64-bits do invoke como f64.
                if let swc_ecma_ast::TsType::TsFnOrConstructorType(
                    swc_ecma_ast::TsFnOrConstructorType::TsFnType(fnty),
                ) = ann.type_ann.as_ref()
                {
                    if matches!(
                        fnty.type_ann.type_ann.as_ref(),
                        swc_ecma_ast::TsType::TsKeywordType(k)
                            if matches!(k.kind, swc_ecma_ast::TsKeywordTypeKind::TsNumberKeyword)
                    ) {
                        ctx.local_fn_ret_f64.insert(name.clone());
                    }
                }
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
                        // (cross-runtime) anotacao `: Map/Set/WeakMap/WeakSet`
                        // marca a var p/ `.size` rotear ao UNIVERSAL_LENGTH.
                        if matches!(n, "Map" | "Set" | "WeakMap" | "WeakSet" | "ReadonlyMap" | "ReadonlySet") {
                            ctx.local_map_vars.insert(name.clone());
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
            if let swc_ecma_ast::Expr::Array(arr_lit) = init_peeled {
                ctx.local_array_vars.insert(name.clone());
                // (#341/elo-f64) `const ps = [new P(...), new P(...)]` sem
                // anotacao `: P[]`. Infere a classe do elemento a partir dos
                // `new ClassName()` do literal, para que o bind de for-of
                // (`for (const p of ps)`) herde a classe e `p.campoFloat`
                // leia f64 (sem isto, `p.age >= 18` comparava bits-i64 e
                // contava floats truncados). So' quando TODOS os elementos sao
                // `new C()` da MESMA classe user conhecida.
                let mut elem_cls: Option<String> = None;
                let mut all_same = true;
                let mut saw_any = false;
                for el in arr_lit.elems.iter().flatten() {
                    if el.spread.is_some() { all_same = false; break; }
                    if let swc_ecma_ast::Expr::New(n) = el.expr.as_ref() {
                        if let swc_ecma_ast::Expr::Ident(cid) = n.callee.as_ref() {
                            let cn = cid.sym.as_str();
                            if ctx.classes.contains_key(cn) {
                                saw_any = true;
                                match &elem_cls {
                                    None => elem_cls = Some(cn.to_string()),
                                    Some(prev) if prev == cn => {}
                                    Some(_) => { all_same = false; break; }
                                }
                                continue;
                            }
                        }
                    }
                    all_same = false;
                    break;
                }
                if all_same && saw_any {
                    if let Some(cn) = elem_cls {
                        ctx.local_array_class_ty.insert(name.clone(), cn);
                    }
                }
            }
            // (cross-runtime) `const m = new Map/Set(...)` ou `const m = f()`
            // onde f retorna Map/Set — marca p/ `.size` rotear corretamente.
            match init_peeled {
                swc_ecma_ast::Expr::New(ne) => {
                    if let swc_ecma_ast::Expr::Ident(cid) = ne.callee.as_ref() {
                        if matches!(cid.sym.as_str(), "Map" | "Set" | "WeakMap" | "WeakSet") {
                            ctx.local_map_vars.insert(name.clone());
                        }
                    }
                }
                swc_ecma_ast::Expr::Call(ce) => {
                    if let swc_ecma_ast::Callee::Expr(callee) = &ce.callee {
                        if let swc_ecma_ast::Expr::Ident(fid) = callee.as_ref() {
                            if fn_returns_map_set(fid.sym.as_str()) {
                                ctx.local_map_vars.insert(name.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
            // (cross-runtime) `const s = "..."` / template — marca como string
            // pra que `for (const c of s)` itere os chars. Tambem `: string`.
            let ann_is_string = if let Pat::Ident(id) = &decl.name {
                id.type_ann.as_ref().map(|t| matches!(
                    t.type_ann.as_ref(),
                    swc_ecma_ast::TsType::TsKeywordType(k)
                        if matches!(k.kind, swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword)
                )).unwrap_or(false)
            } else { false };
            if ann_is_string
                || matches!(
                    init_peeled,
                    swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(_)) | swc_ecma_ast::Expr::Tpl(_)
                )
            {
                ctx.local_string_vars.insert(name.clone());
            }
            // Alias direto de array: `const es = arr` onde `arr` ja' eh
            // array conhecido. Sem isso, `es.map(arrow)` nao reconhece o
            // receiver como Vec e cai no caminho string/map_get (trap).
            if let swc_ecma_ast::Expr::Ident(id) = init_peeled {
                if ctx.local_array_vars.contains(id.sym.as_str()) {
                    ctx.local_array_vars.insert(name.clone());
                }
            }
            // (#798) `const x = Object.keys/values/getOwnPropertyNames/getOwnPropertySymbols(...)`
            // retorna Vec — marca pra que `x.includes(sym)` use VEC_INCLUDES e nao
            // o string builtin (que ignora needle e retorna 0).
            // (cross-runtime #808) Tambem array methods que retornam array:
            // `.map/.filter/.slice/.concat/.flat/.flatMap/.reduceRight (com acc Vec)/...`.
            if let swc_ecma_ast::Expr::Call(call) = init_peeled {
                if let swc_ecma_ast::Callee::Expr(callee) = &call.callee {
                    if let swc_ecma_ast::Expr::Member(m) = callee.as_ref() {
                        let is_object_recv = matches!(
                            m.obj.as_ref(),
                            swc_ecma_ast::Expr::Ident(id) if id.sym.as_str() == "Object"
                        );
                        let is_array_static = matches!(
                            m.obj.as_ref(),
                            swc_ecma_ast::Expr::Ident(id) if id.sym.as_str() == "Array"
                        );
                        let prop_name: Option<&str> = match &m.prop {
                            swc_ecma_ast::MemberProp::Ident(id) => Some(id.sym.as_str()),
                            _ => None,
                        };
                        if is_object_recv {
                            if let Some(p) = prop_name {
                                if matches!(
                                    p,
                                    "keys" | "values" | "entries"
                                    | "getOwnPropertyNames" | "getOwnPropertySymbols"
                                ) {
                                    ctx.local_array_vars.insert(name.clone());
                                }
                            }
                        }
                        if is_array_static {
                            if let Some(p) = prop_name {
                                if matches!(p, "from" | "of") {
                                    ctx.local_array_vars.insert(name.clone());
                                }
                            }
                        }
                        // (cross-runtime #808) Array instance methods que retornam
                        // Vec — propaga local_array_vars para o var receptor.
                        // Verifica que receiver eh array (literal direto OU ident
                        // ja' marcado como local_array_var).
                        let recv_is_array = match m.obj.as_ref() {
                            swc_ecma_ast::Expr::Array(_) => true,
                            swc_ecma_ast::Expr::Ident(id) => {
                                ctx.local_array_vars.contains(id.sym.as_str())
                            }
                            _ => false,
                        };
                        if recv_is_array {
                            if let Some(p) = prop_name {
                                if matches!(
                                    p,
                                    "map" | "filter" | "slice" | "concat" | "flat" | "flatMap"
                                    | "splice" | "toReversed" | "toSorted" | "toSpliced" | "with"
                                ) {
                                    ctx.local_array_vars.insert(name.clone());
                                }
                                if matches!(p, "reduce" | "reduceRight")
                                    && call.args.len() == 2
                                {
                                    // init eh 2o arg; se for Array literal, retorno
                                    // eh array.
                                    if matches!(
                                        call.args[1].expr.as_ref(),
                                        swc_ecma_ast::Expr::Array(_)
                                    ) {
                                        ctx.local_array_vars.insert(name.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // (#592) Array de object literals — extrai field types do
            // primeiro elemento (heuristica: arrays homogeneos).
            if let swc_ecma_ast::Expr::Array(arr) = init_peeled {
                if let Some(Some(first)) = arr.elems.first() {
                    let inner = peel_ts_init(&first.expr);
                    if let swc_ecma_ast::Expr::Object(obj_lit) = inner {
                        let mut elem_field_types: std::collections::HashMap<String, ValTy> =
                            std::collections::HashMap::new();
                        for prop in &obj_lit.props {
                            if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                                if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                                    let key = match &kv.key {
                                        swc_ecma_ast::PropName::Ident(id) => id.sym.as_str().to_string(),
                                        swc_ecma_ast::PropName::Str(s) => s.value.to_string_lossy().to_string(),
                                        _ => continue,
                                    };
                                    let ty = match kv.value.as_ref() {
                                        swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(_)) => ValTy::Handle,
                                        swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)) => ValTy::I64,
                                        swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => ValTy::Bool,
                                        swc_ecma_ast::Expr::Tpl(_) => ValTy::Handle,
                                        _ => ValTy::I64,
                                    };
                                    elem_field_types.insert(key, ty);
                                }
                            }
                        }
                        if !elem_field_types.is_empty() {
                            ctx.local_array_obj_field_types
                                .insert(name.clone(), elem_field_types);
                        }
                    }
                }
            }
            if let swc_ecma_ast::Expr::Object(obj) = init_peeled {
                let mut field_types: std::collections::HashMap<String, ValTy> =
                    std::collections::HashMap::new();
                for prop in &obj.props {
                    // (cross-runtime) Spread `{ ...base }`: herda os tipos de
                    // campo do objeto fonte. Sem isso, `ext.name` [campo vindo
                    // do spread] cai em I64 default e `ext.name + ext.val`
                    // [ambos string] vira SOMA NUMERICA dos handles em vez de
                    // concat. Props explicitas depois do spread sobrescrevem.
                    if let swc_ecma_ast::PropOrSpread::Spread(sp) = prop {
                        if let swc_ecma_ast::Expr::Ident(src_id) = sp.expr.as_ref() {
                            if let Some(src_types) =
                                ctx.local_obj_field_types.get(src_id.sym.as_str()).cloned()
                            {
                                for (k, v) in src_types {
                                    field_types.insert(k, v);
                                }
                            }
                        }
                        continue;
                    }
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
                                // (#480) Method shorthand: \`{ add(n) {...} }\`
                                // apos hoist_fn_expressions vira KeyValue com
                                // value=Fn/Arrow/Ident("__hoisted_fn_N").
                                // Registra como I64 fn ptr para gating em
                                // lower_var_member_call (skip Set.add hijack).
                                swc_ecma_ast::Expr::Fn(_) | swc_ecma_ast::Expr::Arrow(_) => {
                                    field_types.insert(key.clone(), ValTy::I64);
                                }
                                swc_ecma_ast::Expr::Ident(id)
                                    if id.sym.as_str().starts_with("__hoisted_fn_") =>
                                {
                                    field_types.insert(key.clone(), ValTy::I64);
                                }
                                // (#1092) Array literal field: armazena handle de Vec.
                                // Sem isso, `data.keys` retorna I64 default, e
                                // `data.keys[0]` / `data.keys.length` operam sobre
                                // valor errado.
                                swc_ecma_ast::Expr::Array(_) => {
                                    field_types.insert(key.clone(), ValTy::Handle);
                                }
                                // (#210) Sub-object literal: registra tipos
                                // dos campos do nested para nested
                                // destructuring conseguir inferir.
                                // (#602) Para nesting >=3 niveis (ex:
                                // `{ db: { conn: { url: ... } } }`), tambem
                                // registramos a folha sob a chave imediata
                                // (root, last_key) para que `chain_root_path`
                                // em members.rs ache os tipos via last key.
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
                                                    swc_ecma_ast::Expr::Object(_) => {
                                                        Some(ValTy::Handle)
                                                    }
                                                    _ => None,
                                                };
                                                if let Some(t) = sty {
                                                    sub_types.insert(sk.clone(), t);
                                                }
                                                // (#602) Recursa em sub-objs:
                                                // registra os tipos das folhas
                                                // sob a chave imediata, pra que
                                                // `chain_root_path((root,sk))`
                                                // em member access nested
                                                // resolva o tipo da folha.
                                                if let swc_ecma_ast::Expr::Object(deep_obj) =
                                                    skv.value.as_ref()
                                                {
                                                    let mut deep_types:
                                                        std::collections::HashMap<String, ValTy> =
                                                        std::collections::HashMap::new();
                                                    for dp in &deep_obj.props {
                                                        if let swc_ecma_ast::PropOrSpread::Prop(
                                                            dsp,
                                                        ) = dp
                                                        {
                                                            if let swc_ecma_ast::Prop::KeyValue(
                                                                dkv,
                                                            ) = dsp.as_ref()
                                                            {
                                                                let dk = match &dkv.key {
                                                                    swc_ecma_ast::PropName::Ident(
                                                                        id,
                                                                    ) => id.sym.as_str().to_string(),
                                                                    swc_ecma_ast::PropName::Str(
                                                                        s,
                                                                    ) => s
                                                                        .value
                                                                        .to_string_lossy()
                                                                        .to_string(),
                                                                    _ => continue,
                                                                };
                                                                let dty = match dkv.value.as_ref() {
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
                                                                if let Some(t) = dty {
                                                                    deep_types.insert(dk, t);
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if !deep_types.is_empty() {
                                                        // Indexa por (root, sk)
                                                        // pra match com
                                                        // `chain_root_path` que
                                                        // usa last key do path.
                                                        ctx.local_nested_obj_field_types.insert(
                                                            (name.clone(), sk.clone()),
                                                            deep_types,
                                                        );
                                                    }
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
            // (cross-runtime) `const m = Object.assign(target, ...sources)`:
            // coleta os tipos de campo dos args (object literais + idents ja
            // registrados), igual ao spread. Sem isso, `m.a + m.b` [campos
            // string vindos de Object.assign] vira soma numerica dos handles
            // em vez de concat. Sources posteriores sobrescrevem (semantica
            // JS de Object.assign).
            if let swc_ecma_ast::Expr::Call(call) = init_peeled {
                let is_object_assign = matches!(&call.callee,
                    swc_ecma_ast::Callee::Expr(cb) if matches!(cb.as_ref(),
                        swc_ecma_ast::Expr::Member(m)
                            if matches!(m.obj.as_ref(),
                                swc_ecma_ast::Expr::Ident(o) if o.sym.as_str() == "Object")
                            && matches!(&m.prop,
                                swc_ecma_ast::MemberProp::Ident(p) if p.sym.as_str() == "assign")));
                if is_object_assign {
                    let mut ft: std::collections::HashMap<String, ValTy> =
                        std::collections::HashMap::new();
                    for arg in &call.args {
                        if arg.spread.is_some() {
                            continue;
                        }
                        match arg.expr.as_ref() {
                            swc_ecma_ast::Expr::Object(src_obj) => {
                                for prop in &src_obj.props {
                                    if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                                        if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                                            let key = match &kv.key {
                                                swc_ecma_ast::PropName::Ident(id) =>
                                                    id.sym.as_str().to_string(),
                                                swc_ecma_ast::PropName::Str(s) =>
                                                    s.value.to_string_lossy().to_string(),
                                                _ => continue,
                                            };
                                            let ty = match kv.value.as_ref() {
                                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(_)) =>
                                                    Some(ValTy::Handle),
                                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)) =>
                                                    Some(ValTy::I64),
                                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Bool(_)) =>
                                                    Some(ValTy::Bool),
                                                swc_ecma_ast::Expr::Array(_)
                                                | swc_ecma_ast::Expr::Object(_) =>
                                                    Some(ValTy::Handle),
                                                _ => None,
                                            };
                                            if let Some(t) = ty {
                                                ft.insert(key, t);
                                            }
                                        }
                                    }
                                }
                            }
                            swc_ecma_ast::Expr::Ident(src_id) => {
                                if let Some(src_types) =
                                    ctx.local_obj_field_types.get(src_id.sym.as_str()).cloned()
                                {
                                    for (k, v) in src_types {
                                        ft.insert(k, v);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    if !ft.is_empty() {
                        ctx.local_obj_field_types.insert(name.clone(), ft);
                    }
                }
                // (cross-runtime) `const r = mk()` onde mk eh user fn que
                // retorna object literal: propaga os tipos de campo. Sem
                // isso, `r.label + r.label` [string] vira soma numerica.
                if let swc_ecma_ast::Callee::Expr(cb) = &call.callee {
                    if let swc_ecma_ast::Expr::Ident(fid) = cb.as_ref() {
                        if let Some(ft) =
                            crate::codegen::lower::compile::program::fn_ret_obj_field_types(
                                fid.sym.as_str(),
                            )
                        {
                            ctx.local_obj_field_types.entry(name.clone()).or_default().extend(ft);
                        }
                    }
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
            // (#592) `const u = users[i]` onde users tem
            // local_array_obj_field_types registrado — propaga os tipos.
            // Idem para users: User[] (local_array_class_ty) — bind herda
            // a classe para que u.field saiba o tipo certo.
            if let swc_ecma_ast::Expr::Member(m) = init.as_ref() {
                if matches!(&m.prop, swc_ecma_ast::MemberProp::Computed(_)) {
                    if let swc_ecma_ast::Expr::Ident(arr_id) = m.obj.as_ref() {
                        let arr_name = arr_id.sym.as_str();
                        if let Some(types) = ctx.local_array_obj_field_types.get(arr_name).cloned() {
                            ctx.local_obj_field_types.insert(name.clone(), types);
                        }
                        if let Some(elem_cls) = ctx.local_array_class_ty.get(arr_name).cloned() {
                            ctx.local_class_ty.insert(name.clone(), elem_cls);
                        }
                    }
                }
            }
            // (#210) Nested destructuring — `const x = cfg.db` onde cfg
            // tem nested object literal registrado. Recursivamente
            // propaga tipos dos sub-campos. Sem isso, `const { db: { host } } = cfg`
            // (depois de expand_destructuring vira `const __d1 = cfg.db; const { host } = __d1;`)
            // perde o tipo de `host` e mostra handle bruto em template literal.
            // Member access (\`cfg.server\`) ou OptChain (\`cfg?.server\`)
            // — propaga nested types pra var.
            let propagate_member = |obj: &swc_ecma_ast::Expr, prop: &swc_ecma_ast::MemberProp, ctx: &mut crate::codegen::lower::ctx::FnCtx| {
                if let (swc_ecma_ast::Expr::Ident(obj_id), swc_ecma_ast::MemberProp::Ident(p)) =
                    (obj, prop)
                {
                    let obj_name = obj_id.sym.as_str();
                    let key = p.sym.as_str();
                    if let Some(nested) = ctx
                        .local_nested_obj_field_types
                        .get(&(obj_name.to_string(), key.to_string()))
                        .cloned()
                    {
                        ctx.local_obj_field_types.insert(name.clone(), nested);
                    }
                }
            };
            if let swc_ecma_ast::Expr::Member(m) = init.as_ref() {
                propagate_member(m.obj.as_ref(), &m.prop, ctx);
                // (cross-runtime 107) `const sp = u.searchParams` — propaga
                // local_class_ty = URLSearchParams para que .get/.set/etc
                // roteiem corretamente. Detecta via instance_method spec.
                if let swc_ecma_ast::MemberProp::Ident(prop_id) = &m.prop {
                    let prop_name = prop_id.sym.as_str();
                    // Tipo do receiver eh URL? Tem que olhar var_ty.
                    if let swc_ecma_ast::Expr::Ident(obj_id) = m.obj.as_ref() {
                        let obj_name = obj_id.sym.as_str();
                        if let Some(cls) = ctx.local_class_ty.get(obj_name).cloned() {
                            if let Some(spec) = crate::abi::global_class_lookup(&cls) {
                                if let Some(member) = spec.instance_method(prop_name) {
                                    // Extrai class name do ts_signature
                                    // (e.g. "readonly searchParams: URLSearchParams").
                                    let sig = member.ts_signature;
                                    if let Some(colon_pos) = sig.rfind(':') {
                                        let ret_ty = sig[colon_pos + 1..].trim();
                                        // Remove modifiers tipo "readonly " ou "?".
                                        let ret_ty = ret_ty.trim_end_matches(';').trim();
                                        if crate::abi::global_class_lookup(ret_ty).is_some()
                                            || ctx.classes.contains_key(ret_ty)
                                        {
                                            ctx.local_class_ty.insert(name.clone(), ret_ty.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let swc_ecma_ast::Expr::OptChain(opt) = init.as_ref() {
                if let swc_ecma_ast::OptChainBase::Member(m) = opt.base.as_ref() {
                    propagate_member(m.obj.as_ref(), &m.prop, ctx);
                }
            }
        }

        if !ctx.local_class_ty.contains_key(&name) {
            if let Some(init) = decl.init.as_ref() {
                // (#39) `const r = /pat/` -> marca como RegExp para que
                // `s.match(r)` use STRING_MATCH_REGEX (aceita handle Regex)
                // em vez do path string-only que falha.
                if matches!(init.as_ref(), swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Regex(_))) {
                    ctx.local_class_ty.insert(name.clone(), "RegExp".to_string());
                }
                if let swc_ecma_ast::Expr::New(ne) = init.as_ref() {
                    // `new Foo<T>(...)` em TS vem como TsInstantiation
                    // envelopando o Ident — peel para detectar a classe.
                    let callee_inner: &swc_ecma_ast::Expr = match ne.callee.as_ref() {
                        swc_ecma_ast::Expr::TsInstantiation(ti) => ti.expr.as_ref(),
                        other => other,
                    };
                    if let swc_ecma_ast::Expr::Ident(cid) = callee_inner {
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
                        } else if cn == "Map" || cn == "Set" {
                            // (#222) Map/Set nao estao em GLOBAL_CLASS_SPECS
                            // (sao codegen-only via collections.map_*) mas
                            // for-of precisa saber para usar MAP_ENTRIES_INSERTION.
                            ctx.local_class_ty.insert(name.clone(), cn.clone());
                        } else if cn == "ArrayBuffer" || cn == "SharedArrayBuffer" {
                            // (#69) ArrayBuffer/SharedArrayBuffer nao estao em
                            // GLOBAL_CLASS_SPECS (backing eh Buffer via codegen)
                            // mas `new Int32Array(buf)` precisa saber que buf eh
                            // um (Shared)ArrayBuffer pra criar view-viva (TA_*_ELEM)
                            // em vez de copiar pro Vec. SharedArrayBuffer = backing
                            // identico a ArrayBuffer no RTS (single-thread).
                            ctx.local_class_ty.insert(name.clone(), cn.clone());
                        } else if ctx.user_fns.contains_key(&cn) {
                            // (#proto-instance) Constructor function: `new Animal(...)` onde
                            // Animal eh user fn. Marca var como instance "ProtoInstance" pra
                            // skipar lower_string_builtin/lower_map_set_builtin no dispatch
                            // de a.toString() — esses callers leriam handle de Map/object como
                            // string e retornariam vazio. Sem isso, methods em Animal.prototype
                            // (toString, etc.) nao sao chamados.
                            ctx.local_class_ty
                                .insert(name.clone(), "__proto_instance".to_string());
                        }
                        // (#811/205) `const v = new Uint8Array(buffer)` onde
                        // `buffer` eh ArrayBuffer: v eh uma VIEW sobre o mesmo
                        // buffer (escritas compartilhadas). Marca `local_ta_view`
                        // com (elem_bytes, signed, is_float) e o lower do new
                        // retorna o handle do buffer direto.
                        if let Some((eb, sg, fl)) = ta_elem_meta(&cn) {
                            // (#69) SharedArrayBuffer = backing identico a
                            // ArrayBuffer no RTS; ambos geram view-viva.
                            let arg_is_arraybuffer = ne.args.as_ref()
                                .and_then(|a| a.first())
                                .map(|a| matches!(a.expr.as_ref(),
                                    swc_ecma_ast::Expr::Ident(id)
                                        if ctx.local_class_ty.get(id.sym.as_str())
                                            .map(|c| c == "ArrayBuffer" || c == "SharedArrayBuffer").unwrap_or(false)))
                                .unwrap_or(false);
                            if arg_is_arraybuffer {
                                ctx.local_ta_view.insert(name.clone(), (eb, sg, fl));
                            } else {
                                // (#93) `new Uint8Array([...])`/`(n)` — Vec backing.
                                // Marca como array para que metodos genericos
                                // (at/fill/slice/copyWithin/includes/...) funcionem.
                                ctx.local_array_vars.insert(name.clone());
                            }
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
                            } else if fname == "structuredClone" {
                                // (#68/#394) structuredClone preserva o tipo do
                                // valor clonado. Propaga local_class_ty/local_map_vars
                                // do arg (ident) pro receptor — sem isso o clone
                                // (Date/RegExp/Map/Set/ArrayBuffer) chega sem tipo e
                                // metodos (.getUTCFullYear/.has/view) mis-despacham.
                                if let Some(a) = call.args.first() {
                                    if let swc_ecma_ast::Expr::Ident(aid) = a.expr.as_ref() {
                                        let src = aid.sym.as_str();
                                        if let Some(src_cls) = ctx.local_class_ty.get(src).cloned() {
                                            // SharedArrayBuffer clona como ArrayBuffer.
                                            let cls = if src_cls == "SharedArrayBuffer" {
                                                "ArrayBuffer".to_string()
                                            } else {
                                                src_cls
                                            };
                                            ctx.local_class_ty.insert(name.clone(), cls);
                                        }
                                        if ctx.local_map_vars.contains(src) {
                                            ctx.local_map_vars.insert(name.clone());
                                        }
                                    }
                                }
                            } else if let Some(cn) = ctx.fn_class_returns.get(fname) {
                                ctx.local_class_ty.insert(name.clone(), cn.clone());
                            }
                        }
                        // Function global (#359): `.bind(...)` retorna Function.
                        // Propaga local_class_ty pro var receptor.
                        // (cross-runtime #38) `Class.staticMethod(...)`:
                        // propaga class ret type para var receptor; getters em
                        // a.field funcionam.
                        if let swc_ecma_ast::Expr::Member(m) = cb.as_ref() {
                            if let swc_ecma_ast::MemberProp::Ident(mid) = &m.prop {
                                if mid.sym.as_str() == "bind" {
                                    ctx.local_class_ty.insert(name.clone(), "Function".to_string());
                                }
                                // (cross-runtime) `const p = Promise.resolve(x)`
                                // (e .reject/.all/.race/.allSettled/.any) marca
                                // `p` como Promise pra que `p.then(cb)` resolva
                                // o handler de Promise em vez de cair no
                                // fallback Map -> trapz -> SIGILL. O chain
                                // direto `Promise.resolve(x).then(...)` ja'
                                // funciona; faltava o caso com var.
                                // (#306) `const it = Iterator.from(arr)` marca
                                // `it` como Iterator pra que `it.toArray()` resolva.
                                if let swc_ecma_ast::Expr::Ident(obj_id) = m.obj.as_ref() {
                                    if obj_id.sym.as_str() == "Iterator"
                                        && mid.sym.as_str() == "from"
                                    {
                                        ctx.local_class_ty
                                            .insert(name.clone(), "Iterator".to_string());
                                    }
                                }
                                if let swc_ecma_ast::Expr::Ident(obj_id) = m.obj.as_ref() {
                                    if obj_id.sym.as_str() == "Promise"
                                        && matches!(
                                            mid.sym.as_str(),
                                            "resolve" | "reject" | "all" | "race"
                                            | "allSettled" | "any"
                                        )
                                    {
                                        ctx.local_class_ty
                                            .insert(name.clone(), "Promise".to_string());
                                    }
                                }
                                // Class.staticMethod(...) → propaga ret_class.
                                if let swc_ecma_ast::Expr::Ident(obj_id) = m.obj.as_ref() {
                                    let cls_name = obj_id.sym.as_str();
                                    if let Some(meta) = ctx.classes.get(cls_name) {
                                        let mn = mid.sym.as_str();
                                        if meta.static_methods.iter().any(|s| s == mn) {
                                            let fname = crate::codegen::lower::compile::class::class_static_method_name(cls_name, mn);
                                            if let Some(cn) = ctx.fn_class_returns.get(&fname).cloned() {
                                                ctx.local_class_ty.insert(name.clone(), cn);
                                            }
                                        }
                                    }
                                    // (cross-runtime builder) `const c2 = inst.method()`
                                    // onde `inst` tem classe C conhecida e o metodo
                                    // de instancia retorna `this` ou C: marca `c2`
                                    // como C pra que `c2.method()` resolva o metodo
                                    // (builder pattern). Sem isso, `c.inc().inc()`
                                    // / `const c2 = c.inc(); c2.inc()` caia no
                                    // fallback MAP_GET("inc") -> trapz -> SIGILL.
                                    if let Some(recv_cls) = ctx.local_class_ty.get(obj_id.sym.as_str()).cloned() {
                                        let mn = mid.sym.as_str();
                                        let fname = crate::codegen::lower::compile::class::class_method_name(&recv_cls, mn);
                                        // ret_class do metodo == a propria classe (ex:
                                        // `inc(): C { ...; return this; }`).
                                        let ret_is_self = ctx.user_fns.get(&fname)
                                            .map(|abi| abi.ret_class.as_deref() == Some(recv_cls.as_str()))
                                            .unwrap_or(false);
                                        if ret_is_self {
                                            ctx.local_class_ty.insert(name.clone(), recv_cls);
                                        }
                                    }
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
        } else if ann_ty.is_none() {
            // (cross-runtime) `let u;` / `let u: any;` sem init -> undefined real
            // (sentinel i64::MIN+2), nao 0. Faz `typeof u`, `u === undefined` e
            // template formatarem corretamente. So' quando nao ha tipo concreto
            // anotado (number/string mantem zero_for_ty p/ nao quebrar uso
            // tipado posterior `let n: number; n = 5`).
            let undef = ctx.builder.ins().iconst(
                cranelift_codegen::ir::types::I64,
                i64::MIN + 2,
            );
            (undef, ValTy::I64)
        } else {
            let ty = ann_ty.unwrap_or(ValTy::I64);
            (zero_for_ty(ctx, ty), ty)
        };

        // (cross-runtime #752) Quando init eh literal `undefined` ou `null`,
        // mantemos como I64 mesmo se a anotacao tipa como F64 (`number |
        // undefined`). Coerce F64 do sentinela MIN+2 perde precisao e
        // quebra `??=` que detecta sentinelas por bit-exact comparison.
        let init_is_undef_or_null_literal = matches!(
            decl.init.as_deref(),
            Some(swc_ecma_ast::Expr::Ident(id)) if id.sym.as_str() == "undefined"
        ) || matches!(
            decl.init.as_deref(),
            Some(swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Null(_)))
        );
        // (cross-runtime #372) Para `let` (mutavel) sem anotacao cujo init
        // eh literal int (I32/I64), promover para F64. Justifica porque
        // reatribuicao com expressao F64 (ex: `k = c + 273.15`) coerce
        // f64 -> int via fcvt_to_sint_sat perdendo a parte decimal. JS
        // spec trata todos os numbers como f64. `const` fica com tipo
        // inferido (sem reatribuicao). `var` segue mesma regra de let.
        let is_mutable_kind = matches!(
            var_decl.kind,
            swc_ecma_ast::VarDeclKind::Let | swc_ecma_ast::VarDeclKind::Var
        );
        let init_is_int_lit = matches!(
            decl.init.as_deref(),
            Some(swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)))
        ) && matches!(inferred_ty, ValTy::I32 | ValTy::I64);
        let promote_to_f64 = is_mutable_kind
            && init_is_int_lit
            && ann_ty.is_none()
            && !init_is_undef_or_null_literal;

        let ty = if ctx.module_scope && ctx.has_global(&name) {
            ctx.var_ty(&name).unwrap_or(ann_ty.unwrap_or(inferred_ty))
        } else if init_is_undef_or_null_literal {
            // Mantem I64 para preservar sentinela; coerce_to_f64 do MIN+2
            // arredondaria para -9.2e18 e ??= falharia.
            ValTy::I64
        } else if promote_to_f64 {
            ValTy::F64
        } else {
            ann_ty.unwrap_or(inferred_ty)
        };
        // (372) Init ambiguo (resultado de invoke/member call) atribuido a
        // var `: number`: o valor i64 carrega BITS de f64 (ex: arrow
        // `() => this.campoF64`). fcvt_from_sint o corromperia. Preserva como
        // I64 ambiguo — igual ao caminho sem anotacao, onde INSPECT/coercao
        // resolve em runtime. Sem isso `const r: number = gf()` lia lixo.
        let init_is_ambiguous_call = ctx.var_member_call_values.contains(&init_val)
            && matches!(inferred_ty, ValTy::I64 | ValTy::U64);
        let init_coerced = match ty {
            ValTy::I32 => ctx.coerce_to_i32(TypedVal::new(init_val, inferred_ty)).val,
            ValTy::I64 => ctx.coerce_to_i64(TypedVal::new(init_val, inferred_ty)).val,
            ValTy::F64 if init_is_ambiguous_call => init_val,
            ValTy::F64 => ctx.coerce_to_f64(TypedVal::new(init_val, inferred_ty)).val,
            _ => init_val,
        };
        // Quando preservamos o init ambiguo como I64, a var precisa ser
        // tipada I64 (nao F64) pra leituras subsequentes nao fazerem fcvt.
        let ty = if matches!(ty, ValTy::F64) && init_is_ambiguous_call {
            ValTy::I64
        } else {
            ty
        };

        // (#627) Propaga flag de ambiguidade — quando init eh resultado de
        // obj.x sem tipo declarado, a var herda var_member_call_values para
        // que `+` em concat use TPL_COERCE_AUTO.
        let init_was_ambiguous = ctx.var_member_call_values.contains(&init_val);
        if init_was_ambiguous && ann_ty.is_none() {
            ctx.local_ambiguous_vars.insert(name.clone());
        }
        // (372) Var `: number` que preservou init ambiguo como I64 (arrow
        // retornando campo f64): propaga ambiguidade pra que leituras/console
        // usem INSPECT/TPL_COERCE_AUTO em runtime em vez de fcvt.
        if init_is_ambiguous_call {
            ctx.local_ambiguous_vars.insert(name.clone());
            ctx.var_member_call_values.insert(init_coerced);
        }
        // (cross-runtime) `let u;`/`let u: any;` sem init (sentinel undefined):
        // marca ambigua p/ typeof/concat despacharem runtime (TYPEOF_HANDLE
        // detecta MIN+2 -> "undefined"). Sem isso, typeof via ValTy I64 -> "number".
        if decl.init.is_none() && ann_ty.is_none() {
            ctx.local_ambiguous_vars.insert(name.clone());
            ctx.var_member_call_values.insert(init_coerced);
        }
        // (cross-runtime edge_json5) Quando var declarada com `: any` e init
        // eh uma Call, marca como ambiguous para que `obj.X` member access
        // NAO colida com GLOBAL_CLASS_SPECS getters (URL.port/RegExp.flags).
        // Restringido a `any` explicito pra nao quebrar `const x: number =
        // f()` ou inferencia normal.
        if let Some(init_expr) = decl.init.as_deref() {
            let is_any_ann = if let Pat::Ident(bid) = &decl.name {
                bid.type_ann.as_ref()
                    .and_then(|ta| match ta.type_ann.as_ref() {
                        swc_ecma_ast::TsType::TsKeywordType(k) => Some(matches!(
                            k.kind,
                            swc_ecma_ast::TsKeywordTypeKind::TsAnyKeyword
                        )),
                        _ => None,
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            if is_any_ann && matches!(init_expr, swc_ecma_ast::Expr::Call(_)) {
                ctx.local_ambiguous_vars.insert(name.clone());
            }
        }

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
    // Function types `() => T` / `(a: T) => U` are Function handles.
    if let TsType::TsFnOrConstructorType(_) = ty {
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

/// (cross-runtime) `f` eh user fn/metodo cujo return_type eh Map/Set/WeakMap/
/// WeakSet? Consulta o set populado no array_methods_pass.
fn fn_returns_map_set(name: &str) -> bool {
    crate::codegen::lower::passes::parallelism::FNS_RET_MAPSET
        .with(|c| c.borrow().contains(name))
}

/// (#811/205) Metadados do elemento de um TypedArray: (elem_bytes, signed,
/// is_float). None se o nome nao for um TypedArray conhecido.
pub(crate) fn ta_elem_meta(name: &str) -> Option<(i64, i64, i64)> {
    match name {
        "Int8Array" => Some((1, 1, 0)),
        "Uint8Array" | "Uint8ClampedArray" => Some((1, 0, 0)),
        "Int16Array" => Some((2, 1, 0)),
        "Uint16Array" => Some((2, 0, 0)),
        "Int32Array" => Some((4, 1, 0)),
        "Uint32Array" => Some((4, 0, 0)),
        "Float32Array" => Some((4, 0, 1)),
        "Float64Array" => Some((8, 0, 1)),
        _ => None,
    }
}
