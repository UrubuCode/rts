use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
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
                    if ctx.classes.contains_key(&cn) {
                        ctx.local_class_ty.insert(name.clone(), cn);
                    }
                }
                if let swc_ecma_ast::TsType::TsArrayType(arr) = ann.type_ann.as_ref() {
                    if let Some(cn) = class_name_from_annotation(&arr.elem_type) {
                        if ctx.classes.contains_key(&cn) {
                            ctx.local_array_class_ty.insert(name.clone(), cn);
                        }
                    }
                }
            }
        }

        // Capture field types for object literals (used by enum string).
        if let Some(init) = decl.init.as_ref() {
            if let swc_ecma_ast::Expr::Object(obj) = init.as_ref() {
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
                                    field_types.insert(key, ValTy::Handle);
                                }
                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)) => {
                                    field_types.insert(key, ValTy::I64);
                                }
                                swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => {
                                    field_types.insert(key, ValTy::Bool);
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
            }
        }

        if !ctx.local_class_ty.contains_key(&name) {
            if let Some(init) = decl.init.as_ref() {
                if let swc_ecma_ast::Expr::New(ne) = init.as_ref() {
                    if let swc_ecma_ast::Expr::Ident(cid) = ne.callee.as_ref() {
                        let cn = cid.sym.as_str().to_string();
                        // (#214) Error builtin classes: registra field
                        // types pra que \`e.message\`/\`e.name\` retorne
                        // Handle em vez de I64 anonimo.
                        let is_error_class = matches!(
                            cn.as_str(),
                            "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
                        );
                        if ctx.classes.contains_key(&cn) {
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
                if let swc_ecma_ast::Expr::Call(call) = init.as_ref() {
                    if let swc_ecma_ast::Callee::Expr(cb) = &call.callee {
                        if let swc_ecma_ast::Expr::Ident(fid) = cb.as_ref() {
                            if let Some(cn) = ctx.fn_class_returns.get(fid.sym.as_str()) {
                                ctx.local_class_ty.insert(name.clone(), cn.clone());
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
            TsKeywordTypeKind::TsNumberKeyword => ValTy::I32,
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
