//! Parameter-pattern destructuring: `function f([a, b]) {…}` /
//! `function g({x, y}) {…}`.
//!
//! rts-ast does not carry param patterns (a destructured param flattens to name
//! `"_"`), so we recover them from a FRESH swc re-parse of the source and pair each
//! named function declaration with its lowered `HirFunc` by name. For each param
//! whose swc pattern is an array/object pattern, the HIR param is renamed to a fresh
//! temp `__rtsd_param_N` and the function body is PREFIXED with
//! `const <pat> = <temp>;` (which the body-pass already lowered alongside — but this
//! runs in the same `desugar_destructure` call, so we expand the prefix HERE via the
//! same `expand_pat`).
//!
//! Only PLAIN named functions are handled. Arrow params are extracted to fresh
//! functions AFTER this pass with their pattern already dropped to `"_"`, so a
//! destructured arrow param keeps `"_"` and bails at lowering — sound, never wrong.

use std::collections::HashMap;

use rts_hir::{HirFunc, HirStmt};

use super::{Gen, expand_pat};

/// Rewrite destructuring function parameters in every plain (non-arrow) `HirFunc`.
pub(super) fn rewrite_params(src: &str, funcs: &mut Vec<HirFunc>, g: &mut Gen) {
    let Some(params_by_fn) = parse_param_patterns(src) else {
        if std::env::var("RTS_DEBUG_DESTRUCTURE").is_ok() {
            eprintln!(
                "[destructure] param re-parse FAILED (src head: {:?})",
                &src[..src.len().min(80)]
            );
        }
        return;
    };
    for f in funcs.iter_mut() {
        if f.is_arrow {
            continue;
        }
        let Some(swc_params) = params_by_fn.get(&f.name) else {
            if std::env::var("RTS_DEBUG_DESTRUCTURE").is_ok() {
                eprintln!("[destructure] no swc params for `{}`", f.name);
            }
            continue;
        };
        if std::env::var("RTS_DEBUG_DESTRUCTURE").is_ok() {
            let names: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
            eprintln!("[destructure] `{}` hir params {names:?}", f.name);
        }
        rewrite_one_fn(f, swc_params, g);
    }
}

/// Expand each array/object-pattern param of one function: rename the `"_"` HIR
/// param to a fresh temp and prepend `const <pat> = <temp>;` to the body. Prefixes
/// are pushed in REVERSE param order onto the front so earlier params bind first.
fn rewrite_one_fn(f: &mut HirFunc, swc_params: &[swc_ecma_ast::Pat], g: &mut Gen) {
    let mut prefix: Vec<HirStmt> = Vec::new();
    // A synthesized class method/ctor PREPENDS the `this` param, so its HIR param
    // list is shifted by one relative to the swc source params.
    let offset = usize::from(f.params.first().is_some_and(|p| p.name == "this"));
    let n = (f.params.len().saturating_sub(offset)).min(swc_params.len());
    for i in 0..n {
        let hir_i = i + offset;
        let is_pattern = matches!(
            swc_params[i],
            swc_ecma_ast::Pat::Array(_) | swc_ecma_ast::Pat::Object(_)
        );
        // A flattened pattern param is `"_"` (rts-hir's fn path), the raw source
        // SNIPPET (`"{ x, y }"`), or the parser's synthetic
        // `__rts_param_destruct_N` (the class-method path renames the param and
        // injects a `let {…} = __rts_param_destruct_N` prologue — which rts-hir
        // flattens back to a dead `"_"` let, so the bindings are re-expanded here
        // from the swc pattern).
        let flat = f.params[hir_i].name.clone();
        let is_flattened = flat == "_"
            || flat.starts_with('{')
            || flat.starts_with('[')
            || flat.starts_with("__rts_param_destruct_");
        if !is_pattern || !is_flattened {
            continue;
        }
        // The parser's synthetic name is already a valid ident carrying the arg —
        // keep it (its injected prologue flattens to a dead `"_"` let) and only
        // prepend the expanded binds; the other flattened shapes get a fresh temp.
        let tmp = if flat.starts_with("__rts_param_destruct_") {
            flat.clone()
        } else {
            g.fresh("param")
        };
        match expand_pat(&swc_params[i], &tmp, g) {
            Some(binds) => {
                f.params[hir_i].name = tmp;
                prefix.extend(binds);
            }
            // Unsupported pattern: leave the param `"_"` (bails at lowering) — sound.
            None => {}
        }
    }
    if !prefix.is_empty() {
        prefix.append(&mut f.body);
        f.body = prefix;
    }
}

/// Re-parse `src` to a raw swc module and collect, per top-level named function,
/// its parameter patterns in order. `None` if the re-parse fails (the body pass
/// already ran, so a failure here just means params are not destructured — leave
/// them).
fn parse_param_patterns(src: &str) -> Option<HashMap<String, Vec<swc_ecma_ast::Pat>>> {
    let module = rts_parser::parse_swc_module(src)?;

    let mut out: HashMap<String, Vec<swc_ecma_ast::Pat>> = HashMap::new();
    for item in &module.body {
        let fdecl = match item {
            swc_ecma_ast::ModuleItem::Stmt(swc_ecma_ast::Stmt::Decl(swc_ecma_ast::Decl::Fn(f))) => {
                f
            }
            swc_ecma_ast::ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::ExportDecl(e)) => {
                match &e.decl {
                    swc_ecma_ast::Decl::Fn(f) => f,
                    _ => continue,
                }
            }
            _ => continue,
        };
        let name = fdecl.ident.sym.to_string();
        let pats: Vec<swc_ecma_ast::Pat> = fdecl
            .function
            .params
            .iter()
            .map(|p| p.pat.clone())
            .collect();
        out.insert(name, pats);
    }
    // CLASS methods: the lowered `HirFunc` names follow the synth conventions
    // (`__rtsn_method_<C>_<m>` / `__rtsn_static_<C>_<m>` / `__rtsn_ctor_<C>`), so a
    // destructured method/ctor param pairs the same way a top-level fn does.
    for item in &module.body {
        let cdecl = match item {
            swc_ecma_ast::ModuleItem::Stmt(swc_ecma_ast::Stmt::Decl(
                swc_ecma_ast::Decl::Class(c),
            )) => c,
            swc_ecma_ast::ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::ExportDecl(e)) => {
                match &e.decl {
                    swc_ecma_ast::Decl::Class(c) => c,
                    _ => continue,
                }
            }
            _ => continue,
        };
        let class = cdecl.ident.sym.to_string();
        for member in &cdecl.class.body {
            match member {
                swc_ecma_ast::ClassMember::Method(m) => {
                    let swc_ecma_ast::PropName::Ident(id) = &m.key else {
                        continue;
                    };
                    let lowered = if m.is_static {
                        format!("__rtsn_static_{class}_{}", id.sym)
                    } else {
                        format!("__rtsn_method_{class}_{}", id.sym)
                    };
                    let pats: Vec<swc_ecma_ast::Pat> =
                        m.function.params.iter().map(|p| p.pat.clone()).collect();
                    out.insert(lowered, pats);
                }
                swc_ecma_ast::ClassMember::Constructor(c) => {
                    let pats: Vec<swc_ecma_ast::Pat> = c
                        .params
                        .iter()
                        .filter_map(|p| match p {
                            swc_ecma_ast::ParamOrTsParamProp::Param(p) => Some(p.pat.clone()),
                            // A TS param-property (`constructor(private x)`) is a
                            // plain ident — nothing to destructure.
                            swc_ecma_ast::ParamOrTsParamProp::TsParamProp(_) => None,
                        })
                        .collect();
                    if c.params.len() == pats.len() {
                        out.insert(format!("__rtsn_ctor_{class}"), pats);
                    }
                }
                _ => {}
            }
        }
    }
    Some(out)
}
