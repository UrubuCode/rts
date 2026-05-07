//! Inferencia de tipos ABI a partir de Expr / TsType.
//!
//! Extraido de `func.rs`. Mantem comportamento exato — nenhuma mudanca
//! de logica.

use swc_ecma_ast::{Expr, Lit, TsType, TsTypeRef};

use crate::codegen::lower::ctx::ValTy;

/// Sanitiza um identificador de usuario para uso como sufixo de simbolo
/// ASCII (so' alfanumerico + underscore). String vazia vira `"global"`.
pub(crate) fn sanitize_symbol(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "global".to_string()
    } else {
        out
    }
}

/// Infere ValTy de uma expressao para escolha de storage de globais e
/// locais sem anotacao TS.
pub(crate) fn infer_expr_ty(expr: Option<&Expr>) -> ValTy {
    let Some(expr) = expr else {
        return ValTy::I64;
    };

    match expr {
        Expr::Lit(Lit::Num(n)) => {
            let v = n.value;
            let wrote_as_float = n
                .raw
                .as_ref()
                .map(|r| {
                    let s = r.as_bytes();
                    s.iter().any(|&b| b == b'.' || b == b'e' || b == b'E')
                })
                .unwrap_or(false);
            if wrote_as_float || !v.is_finite() || v.fract() != 0.0 {
                ValTy::F64
            } else if v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                ValTy::I32
            } else {
                ValTy::I64
            }
        }
        Expr::Lit(Lit::Bool(_)) => ValTy::Bool,
        Expr::Lit(Lit::Str(_)) => ValTy::Handle,
        Expr::Tpl(_) => ValTy::Handle,
        Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Add) => {
            let l = infer_expr_ty(Some(&b.left));
            let r = infer_expr_ty(Some(&b.right));
            if l == ValTy::Handle || r == ValTy::Handle {
                ValTy::Handle
            } else if l == ValTy::F64 || r == ValTy::F64 {
                ValTy::F64
            } else {
                ValTy::I64
            }
        }
        // `**` sempre retorna F64 (roteado via libc pow).
        Expr::Bin(b) if matches!(b.op, swc_ecma_ast::BinaryOp::Exp) => ValTy::F64,
        Expr::Bin(b) => {
            // Numeric ops other than + (string concat handled above).
            // Propagate F64 so globals holding `math.PI * x` get the right
            // storage; otherwise default to I64.
            let l = infer_expr_ty(Some(&b.left));
            let r = infer_expr_ty(Some(&b.right));
            if l == ValTy::F64 || r == ValTy::F64 {
                ValTy::F64
            } else {
                ValTy::I64
            }
        }
        Expr::Cond(c) => {
            let l = infer_expr_ty(Some(&c.cons));
            let r = infer_expr_ty(Some(&c.alt));
            if l == r { l } else { ValTy::I64 }
        }
        Expr::Paren(p) => infer_expr_ty(Some(&p.expr)),
        Expr::Unary(u) => infer_expr_ty(Some(&u.arg)),
        Expr::Member(_) => infer_abi_member_ty(expr).unwrap_or(ValTy::I64),
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                if let Some(ty) = infer_abi_member_ty(callee) {
                    return ty;
                }
                // Function global #359: `<expr>.bind/call/apply/toString`
                // retornam handle Function (Handle). Sem isso o global
                // recebe storage I64 e `f(...)` cai em lower_user_call
                // em vez de indirect via FUNCTION_CALL.
                if let Expr::Member(m) = callee.as_ref() {
                    if let swc_ecma_ast::MemberProp::Ident(p) = &m.prop {
                        if matches!(p.sym.as_ref(), "bind" | "toString") {
                            return ValTy::Handle;
                        }
                    }
                }
            }
            ValTy::I64
        }
        // `new X(...)` sempre produz uma instancia (handle GC). Cobre
        // classes do usuario E Map/Set v0 (#222) em top-level — sem
        // isso o global eh declarado como I64 e member calls em
        // receiver Handle nao disparam.
        Expr::New(_) => ValTy::Handle,
        // Array literal `[...]` aloca Vec via collections.vec_new e armazena
        // o handle. Sem isso `let arr: T[] = []` em top-level vira I64 e
        // `.length`/`.push` no codegen nao reconhecem como Handle (caem em
        // map_get nominal — o que segfauta ou retorna lixo).
        Expr::Array(_) => ValTy::Handle,
        // Object literal `{ ... }` idem — aloca Map via collections.map_new.
        Expr::Object(_) => ValTy::Handle,
        _ => ValTy::I64,
    }
}

/// If `expr` is an ABI member reference (`ns.name`), returns the ValTy of
/// its return/value type.
pub(crate) fn infer_abi_member_ty(expr: &Expr) -> Option<ValTy> {
    let Expr::Member(m) = expr else { return None };
    let Expr::Ident(ns) = m.obj.as_ref() else {
        return None;
    };
    let name = match &m.prop {
        swc_ecma_ast::MemberProp::Ident(id) => id.sym.as_str(),
        _ => return None,
    };
    let qualified = format!("{}.{}", ns.sym.as_str(), name);
    if let Some((_, member)) = crate::abi::lookup(&qualified) {
        return Some(ValTy::from_abi(member.returns));
    }
    // Redirect implicito JSON.* → json.* (#215). Sem isso o tipo
    // do global declarado em top-level via `const x = JSON.parse(...)`
    // cai no fallback I64 e a leitura subsequente perde o tipo Handle.
    if let Some(method) = qualified.strip_prefix("JSON.") {
        let target = format!("json.{method}");
        if let Some((_, member)) = crate::abi::lookup(&target) {
            return Some(ValTy::from_abi(member.returns));
        }
    }
    if let Some(method) = qualified.strip_prefix("Date.") {
        let target = match method {
            "now" => "date.now_ms",
            "parse" => "date.from_iso",
            _ => "",
        };
        if !target.is_empty() {
            if let Some((_, member)) = crate::abi::lookup(target) {
                return Some(ValTy::from_abi(member.returns));
            }
        }
    }
    None
}

/// Converte `TsType` para `ValTy` quando a anotacao mapeia diretamente
/// para um tipo ABI. Retorna `None` quando o tipo nao tem mapping
/// canonico (ex: union heterogenea).
pub(crate) fn ts_type_to_val_ty(ty: &TsType) -> Option<ValTy> {
    use swc_ecma_ast::{TsKeywordTypeKind, TsLit, TsLitType, TsUnionOrIntersectionType};

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

    if let TsType::TsTypeRef(TsTypeRef { type_name, .. }) = ty {
        let name = match type_name {
            swc_ecma_ast::TsEntityName::Ident(id) => id.sym.as_str(),
            _ => return None,
        };
        return Some(ValTy::from_annotation(name));
    }

    if let TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(u)) = ty {
        let mut acc: Option<ValTy> = None;
        for member in &u.types {
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
                _ => return None,
            }
        }
        return acc;
    }

    if let TsType::TsParenthesizedType(p) = ty {
        return ts_type_to_val_ty(&p.type_ann);
    }

    if let TsType::TsArrayType(_) = ty {
        return Some(ValTy::Handle);
    }

    // Function types `() => T` / `(a: T) => U` etc. are stored as Function handles.
    if let TsType::TsFnOrConstructorType(_) = ty {
        return Some(ValTy::Handle);
    }

    None
}
