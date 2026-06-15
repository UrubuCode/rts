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
        return;
    };
    for f in funcs.iter_mut() {
        if f.is_arrow {
            continue;
        }
        let Some(swc_params) = params_by_fn.get(&f.name) else {
            continue;
        };
        rewrite_one_fn(f, swc_params, g);
    }
}

/// Expand each array/object-pattern param of one function: rename the `"_"` HIR
/// param to a fresh temp and prepend `const <pat> = <temp>;` to the body. Prefixes
/// are pushed in REVERSE param order onto the front so earlier params bind first.
fn rewrite_one_fn(f: &mut HirFunc, swc_params: &[swc_ecma_ast::Pat], g: &mut Gen) {
    let mut prefix: Vec<HirStmt> = Vec::new();
    let n = f.params.len().min(swc_params.len());
    for i in 0..n {
        let is_pattern = matches!(
            swc_params[i],
            swc_ecma_ast::Pat::Array(_) | swc_ecma_ast::Pat::Object(_)
        );
        if !is_pattern || f.params[i].name != "_" {
            continue;
        }
        let tmp = g.fresh("param");
        match expand_pat(&swc_params[i], &tmp) {
            Some(binds) => {
                f.params[i].name = tmp;
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
    Some(out)
}
