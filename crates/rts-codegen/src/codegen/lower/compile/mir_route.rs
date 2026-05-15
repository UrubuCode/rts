//! Routing AST → MIR para compilacao de user fns.
//!
//! `try_compile_via_mir` tenta compilar uma user fn via pipeline
//! HIR → MIR → Cranelift. Se bate em construct nao modelado (member
//! em `this`/objetos, classes, async/await, address-taken fns,
//! string em params/ret), retorna None e o caller cai no codegen AST.
//!
//! O cache thread-local `MIR_CACHE` mantem as `MirFunc`s lowered
//! durante uma passada de `compile_program` para que inline possa
//! aplicar (callee declarado antes do caller no source). Eh limpo
//! por `clear_mir_cache_for_program` no inicio de cada compilacao.

use std::collections::HashMap;

use anyhow::Result;
use cranelift_module::Module;

use crate::parser::ast::FunctionDecl;

use super::super::ctx::ValTy;
use super::program::UserFn;

// Thread-local cache de MirFuncs ja lowered nesta passada de
// compile_program. Permite inline aplicar quando o callee foi
// declarado antes do caller no source. Limpado por
// clear_mir_cache_for_program no inicio de cada nova chamada.
thread_local! {
    static MIR_CACHE: std::cell::RefCell<HashMap<String, rts_mir::ir::MirFunc>>
        = std::cell::RefCell::new(HashMap::new());
}

pub(crate) fn clear_mir_cache_for_program() {
    MIR_CACHE.with(|c| c.borrow_mut().clear());
}

/// Try to compile `fn_decl` through the MIR pipeline (HIR → MIR → optimize
/// → mir_codegen). Returns `Ok(true)` when MIR took over and the function
/// is fully defined in `module`; `Ok(false)` when MIR bailed (unsupported
/// shape) and the AST path should run.
pub(crate) fn try_compile_via_mir(
    module: &mut dyn Module,
    fn_decl: &FunctionDecl,
    info: &UserFn,
    address_taken: bool,
) -> Result<bool> {
    use rts_mir::ir::{Inst, Terminator, TrapHint};

    if std::env::var("RTS_MIR_DEBUG").is_ok() {
        eprintln!("[mir-trace] enter try_compile_via_mir({})", fn_decl.name);
    }

    // Conservative gate: bail on any synthetic name (starts with `__`),
    // any async fn, or any synthetic body. The MIR doesn't yet model the
    // runtime hooks these need (Promise, this binding, closure capture).
    if fn_decl.is_async || fn_decl.name.starts_with("__") {
        debug_bail(fn_decl, "synthetic / async fn");
        return Ok(false);
    }
    let body_synthetic = fn_decl.body.iter().any(|stmt| {
        let crate::parser::ast::Statement::Raw(raw) = stmt;
        raw.text.starts_with('<') && raw.text.ends_with('>')
    });
    if body_synthetic {
        debug_bail(fn_decl, "synthetic body");
        return Ok(false);
    }

    // Whitelist by signature: only fns with explicit primitive numeric
    // ret + params route through MIR. Anything taking/returning string,
    // class, void with side effects, etc. stays on the AST path until
    // those features are modeled in MIR.
    fn ann_is_supported(ann: &str) -> bool {
        matches!(
            ann,
            "number" | "i64" | "i32" | "f64" | "bool" | "boolean" | "void"
                | "i8" | "i16" | "u8" | "u16" | "u32" | "u64" | "f32"
        )
    }
    let ret_ok = fn_decl
        .return_type
        .as_deref()
        .map(ann_is_supported)
        .unwrap_or(false);
    if !ret_ok {
        debug_bail(fn_decl, "ret type not whitelisted");
        return Ok(false);
    }
    for p in &fn_decl.parameters {
        let ok = p
            .type_annotation
            .as_deref()
            .map(ann_is_supported)
            .unwrap_or(false);
        if !ok {
            debug_bail(fn_decl, "param type not whitelisted");
            return Ok(false);
        }
    }

    // 1. Lower TS AST → HIR. O scope local recebe pre-registro das
    //    assinaturas das fns ja cached em MIR_CACHE, para que o lower
    //    HIR resolve `inc(x)` no body de `caller` quando `inc` foi
    //    compilado pelo MIR antes (ordem natural: callees declarados
    //    antes dos callers).
    let mut hir_scope = rts_hir::scope::Scope::new();
    MIR_CACHE.with(|c| {
        let cache = c.borrow();
        for (name, mir) in cache.iter() {
            let param_tys: Vec<rts_hir::ir::HirType> =
                mir.params.iter().map(|(_, t)| t.clone()).collect();
            hir_scope.register_param_types(name.clone(), param_tys);
            if !matches!(mir.ret, rts_hir::ir::HirType::Unknown) {
                hir_scope.register_return_type(name.clone(), mir.ret.clone());
            }
        }
    });
    let hir_fn = rts_hir::lower::lower_func(fn_decl, &mut hir_scope);

    // 2. Lower HIR → MIR with both extern + intrinsic resolvers.
    let extern_resolver = crate::mir_codegen::extern_resolver_default();
    let intrinsic_resolver = crate::mir_codegen::intrinsic_resolver_default();
    let mut mir_fn = rts_mir::lower::lower_func_full_with_intrinsics(
        &hir_fn,
        &hir_scope,
        Some(&extern_resolver),
        Some(&intrinsic_resolver),
    );

    // Attach source location for stack trace instrumentation.
    mir_fn.source_line = fn_decl.span.start.line as u32;
    mir_fn.source_file = fn_decl.span.file
        .and_then(rts_diagnostics::source_store::path_of)
        .map(|p| {
            let s = p.display().to_string();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
        })
        .unwrap_or_default();

    // 3a. Bail if any block trapped (unsupported shape) — AST handles it.
    let has_trap = mir_fn.blocks.iter().any(|b| {
        matches!(b.term, Terminator::Trap { code: TrapHint::User(_) })
    });
    if has_trap {
        debug_bail(fn_decl, "has trap");
        return Ok(false);
    }

    // 3b. Bail if the lower silently inserted placeholder zeros — that
    // means an expression (member access, unknown ident, unresolved call)
    // fell through to a default i64 0. Compiling this would silently
    // return wrong values; AST path handles those cases properly.
    if mir_fn.had_placeholders {
        debug_bail(fn_decl, "had placeholders");
        return Ok(false);
    }

    // 4a. Inline em fixed-point: cada passada substitui CallUser por
    //     corpo inline quando elegivel; o optimize subsequente colapsa
    //     fold/cse/dce, possivelmente revelando novas oportunidades de
    //     inline em CallUser que so' agora se tornaram alcançáveis. Max
    //     4 iterações (programs profundos sao raros — past N=2 já
    //     colapsa quase tudo na pratica).
    const MAX_INLINE_ITERS: usize = 4;
    let mut total_inlined = 0;
    for _iter in 0..MAX_INLINE_ITERS {
        let changed = MIR_CACHE.with(|c| {
            let cache = c.borrow();
            rts_mir::passes::inline(&mut mir_fn, &cache)
        });
        if !changed {
            break;
        }
        total_inlined += 1;
        // Roda optimize entre as iteracoes pra simplificar antes do
        // proximo inline ver o IR atualizado.
        rts_mir::passes::optimize(&mut mir_fn);
    }
    if total_inlined > 0 && std::env::var("RTS_MIR_DEBUG").is_ok() {
        eprintln!(
            "[mir-trace] inline applied in {} ({} iters)",
            fn_decl.name, total_inlined
        );
    }

    // 4b. Final optimize + verify. A verify failure is a MIR bug, not
    //     user code — fall back to AST and don't panic.
    rts_mir::passes::optimize(&mut mir_fn);
    if rts_mir::passes::verify(&mir_fn).is_err() {
        return Ok(false);
    }

    // 5. Override conv to match what `compile_program` declared for this
    //    user fn (Tail vs host default depending on address_taken). We
    //    detect from `info.id`'s declared signature via the module — but
    //    that's not directly exposed, so use the conservative rule: if the
    //    fn was declared with Tail conv (most user fns), keep Tail; if
    //    address-taken (host default), match it.
    //
    //    `compile_user_fn` signature uses `info.params/info.ret` derived
    //    from the AST. We compare against the MIR signature; if param
    //    types or arity don't match, the JIT would call with wrong ABI →
    //    bail.
    if mir_fn.params.len() != info.params.len() {
        debug_bail(fn_decl, "param arity mismatch");
        return Ok(false);
    }
    for (i, &ast_ty) in info.params.iter().enumerate() {
        if !mir_param_compatible(&mir_fn.params[i].1, ast_ty) {
            debug_bail(fn_decl, &format!("param[{i}] type mismatch"));
            return Ok(false);
        }
    }
    let ast_ret = info.ret;
    if !mir_ret_compatible(&mir_fn.ret, ast_ret) {
        debug_bail(fn_decl, "ret type mismatch");
        return Ok(false);
    }

    // Match the conv the AST path would have used. Address-taken fns
    // (passed as fn pointers to thread.spawn, promise.then, etc.) precisam
    // de host default extern "C" conv, mas o caminho MIR ainda revela
    // edge cases sutis (ex.: spawned thread crashando) — bail por
    // segurança até que a integração com runtime callers (thread.spawn,
    // promise.then) seja auditada caso a caso.
    if address_taken {
        debug_bail(fn_decl, "address-taken (MIR routing not yet safe)");
        return Ok(false);
    }
    mir_fn.conv = rts_mir::ir::CallConvHint::Tail;

    // CallExtern reativado: o MIR resolveu ABI mismatches via address-taken
    // gating + GC stack maps + StrLit expansion + correct Bool/Float iconst.
    // Se um caso edge surgir, o gate generico (had_placeholders, sig
    // mismatch, ret/param whitelist) ainda bail antes de produzir codigo
    // errado.

    // Auto-recursão tail: o AST emite `return_call` (TCO Cranelift) que
    // permite recursão profunda sem stack overflow. O MIR ainda emite
    // call regular — bail quando a fn chama a si própria pra preservar
    // semantica de tail call.
    let self_recursive = mir_fn.blocks.iter().flat_map(|b| &b.insts).any(|i| {
        matches!(i, rts_mir::ir::Inst::CallUser { name, .. } if name == &fn_decl.name)
    });
    if self_recursive {
        debug_bail(fn_decl, "self-recursive (TCO needed)");
        return Ok(false);
    }

    // 6. Build a `decls` map containing only `info.id` for self (recursion).
    //    Cross-fn calls fall back to AST if `mir_fn` references unknown
    //    fns (CallUser to non-self) — we bail in that case.
    let mut decls = HashMap::new();
    decls.insert(fn_decl.name.clone(), info.id);
    for block in &mir_fn.blocks {
        for inst in &block.insts {
            if let Inst::CallUser { name, .. } = inst {
                if !decls.contains_key(name) {
                    debug_bail(fn_decl, "unknown CallUser target");
                    return Ok(false);
                }
            }
        }
    }

    // 7. Lower MIR → Cranelift IR using the existing FuncId.
    match crate::mir_codegen::lower::lower_mir_func_with_decls(module, &mir_fn, &decls) {
        Ok(_) => {
            if std::env::var("RTS_MIR_DEBUG").is_ok() {
                eprintln!("[mir] compiled `{}` via MIR path", fn_decl.name);
            }
            // Cache este MirFunc para que callers subsequentes possam
            // inline esta fn. Pre-inline (antes da etapa 4a) pra que a
            // versao cached nao tenha CallUser inlinados redundantes —
            // mas o mir_fn aqui ja sofreu inline + optimize. Isso eh OK:
            // se este caller eventualmente vira callee de outro caller,
            // o inline do outro caller vai expandir tudo de uma vez.
            MIR_CACHE.with(|c| {
                c.borrow_mut().insert(fn_decl.name.clone(), mir_fn.clone());
            });
            Ok(true)
        }
        Err(e) => {
            debug_bail(fn_decl, &format!("lower err: {e}"));
            Ok(false)
        }
    }
}

fn debug_bail(fn_decl: &FunctionDecl, reason: &str) {
    if std::env::var("RTS_MIR_DEBUG").is_ok() {
        eprintln!("[mir] {} bail: {}", fn_decl.name, reason);
    }
}

fn mir_param_compatible(mir_ty: &rts_hir::ir::HirType, ast_ty: ValTy) -> bool {
    use rts_hir::ir::HirType;
    // Conservative: only allow numeric primitive types. Handle/Str/anything
    // else routes to the AST path so the MIR doesn't accidentally produce
    // wrong ABI for GC-tracked values.
    match (mir_ty, ast_ty) {
        (HirType::I64, ValTy::I64) => true,
        (HirType::Bool, ValTy::Bool) => true,
        (HirType::F64 | HirType::Number, ValTy::F64) => true,
        (HirType::I32, ValTy::I32) => true,
        // Narrow ints — MIR roteia overflow via pass `narrow`. ABI
        // permanece I64 nos dois lados (callsite AST chama com i64,
        // MIR mascara internamente; sign-extend de signed e' feito
        // no return via narrow_return_signed_extend pass).
        (HirType::I8, ValTy::I8) => true,
        (HirType::I16, ValTy::I16) => true,
        (HirType::U8, ValTy::U8) => true,
        (HirType::U16, ValTy::U16) => true,
        // Mismatch → caller bails to AST path
        _ => false,
    }
}

fn mir_ret_compatible(mir_ret: &rts_hir::ir::HirType, ast_ret: Option<ValTy>) -> bool {
    use rts_hir::ir::HirType;
    match (mir_ret, ast_ret) {
        (HirType::Void, None) => true,
        (HirType::Void, Some(_)) | (_, None) => false,
        (mir_ty, Some(ast_ty)) => mir_param_compatible(mir_ty, ast_ty),
    }
}

