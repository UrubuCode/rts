//! Silent parallelism (Level-1): 4 passes que reescrevem padroes JS
//! comuns em chamadas `parallel.*` automaticamente.
//!
//! - `lift_inline_arrows_in_array_methods` — lifta arrows inline
//!   passados para `arr.map/forEach/reduce/filter/...` em fns sinteticas
//!   top-level pra que `array_methods_pass` reconheca.
//! - `array_methods_pass` — reescreve `arr.map(fn)` etc para
//!   `parallel.map(arr, fn)` quando `fn` eh user fn ident.
//! - `reduce_pass` — detecta `let acc = init; for (x of arr) acc = acc + EXPR`
//!   e reescreve para `acc = parallel.reduce(arr, init, __par_reduce_N)`.
//! - `purity_pass` — detecta `for...of` puro e reescreve para
//!   `parallel.for_each(arr, __par_forof_N)`.

use std::collections::HashSet;

use swc_ecma_ast::{Callee, Decl, Expr, ForHead, Lit, MemberProp, Pat, Stmt};

use crate::parser::ast::{
    FunctionDecl, Item, MemberModifiers, Parameter, Program, RawStmt, Statement,
};
use crate::parser::span::Span;

/// Builds the set of (namespace, member) pairs marked `pure: true` in SPECS.
fn build_pure_ns_set() -> HashSet<(&'static str, &'static str)> {
    let mut s = HashSet::new();
    for spec in crate::abi::SPECS {
        for member in spec.members {
            if member.pure {
                s.insert((spec.name, member.name));
            }
        }
    }
    s
}

/// Returns true if `e` is a pure expression in the context of a ForOf body.
/// Pure: literals, the loop variable, inner-declared idents, arithmetic on
/// pure sub-expressions, and calls to pure namespace members.
fn is_pure_expr_for_parallel(
    e: &Expr,
    loop_var: &str,
    inner: &HashSet<String>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
) -> bool {
    match e {
        Expr::Lit(_) => true,
        Expr::Ident(id) => {
            let n = id.sym.as_str();
            n == loop_var || inner.contains(n)
        }
        Expr::Bin(b) => {
            is_pure_expr_for_parallel(&b.left, loop_var, inner, pure_ns)
                && is_pure_expr_for_parallel(&b.right, loop_var, inner, pure_ns)
        }
        Expr::Unary(u) => is_pure_expr_for_parallel(&u.arg, loop_var, inner, pure_ns),
        Expr::Paren(p) => is_pure_expr_for_parallel(&p.expr, loop_var, inner, pure_ns),
        Expr::TsAs(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::TsTypeAssertion(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::TsNonNull(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::TsConstAssertion(a) => is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns),
        Expr::Call(call) => {
            let Callee::Expr(ce) = &call.callee else { return false };
            let Expr::Member(m) = ce.as_ref() else { return false };
            let Expr::Ident(ns_id) = m.obj.as_ref() else { return false };
            let MemberProp::Ident(prop_id) = &m.prop else { return false };
            if !pure_ns.contains(&(ns_id.sym.as_str(), prop_id.sym.as_str())) {
                return false;
            }
            call.args.iter().all(|a| {
                a.spread.is_none()
                    && is_pure_expr_for_parallel(&a.expr, loop_var, inner, pure_ns)
            })
        }
        _ => false,
    }
}

/// Returns true if the ForOf body is parallelisable: no assignments, no
/// control flow escapes, only pure namespace calls, all idents are either
/// the loop variable or declared within the body.
fn analyze_for_of_body_pure(
    body: &Stmt,
    loop_var: &str,
    pure_ns: &HashSet<(&'static str, &'static str)>,
) -> bool {
    let stmts: &[Stmt] = match body {
        Stmt::Block(b) => &b.stmts,
        Stmt::Expr(e) => {
            return is_pure_expr_for_parallel(&e.expr, loop_var, &HashSet::new(), pure_ns);
        }
        _ => return false,
    };
    let mut inner: HashSet<String> = HashSet::new();
    for stmt in stmts {
        match stmt {
            Stmt::Decl(Decl::Var(vd)) => {
                for d in &vd.decls {
                    let Pat::Ident(id) = &d.name else { return false };
                    if let Some(init) = &d.init {
                        if !is_pure_expr_for_parallel(init, loop_var, &inner, pure_ns) {
                            return false;
                        }
                    }
                    inner.insert(id.sym.as_str().to_string());
                }
            }
            Stmt::Expr(e) => {
                if !is_pure_expr_for_parallel(&e.expr, loop_var, &inner, pure_ns) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Builds a `parallel.for_each(arr_expr, fn_ident)` expression statement.
fn make_par_foreach_stmt(arr_expr: &Expr, fn_name: &str) -> Stmt {
    Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(Expr::Call(swc_ecma_ast::CallExpr {
            span: Default::default(),
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Member(swc_ecma_ast::MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: "parallel".into(),
                    optional: false,
                })),
                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                    span: Default::default(),
                    sym: "for_each".into(),
                }),
            }))),
            args: vec![
                swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(arr_expr.clone()),
                },
                swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: fn_name.to_string().into(),
                        optional: false,
                    })),
                },
            ],
            type_args: None,
        })),
    })
}

/// Lift de arrows inline em call sites de array methods (`map`, `forEach`,
/// `reduce`). Para cada arrow simples (1 ou 2 params, body = expr ou
/// `{ return expr; }`, sem captura de locals), cria um
/// `Item::Function(__lifted_arrow_N)` top-level e substitui o arg pelo
/// Ident. Roda antes de `array_methods_pass`, que entao reconhece o
/// arg como user fn ident e reescreve para `parallel.*`.
pub(crate) fn lift_inline_arrows_in_array_methods(program: &mut Program) {
    use std::sync::atomic::AtomicU32;
    let counter: AtomicU32 = AtomicU32::new(0);

    // Snapshot inicial de user fn names (top-level). Usado pra
    // detectar captura: se ident referenciado no body nao for param
    // da arrow nem user fn nem builtin namespace, skip lift.
    let mut user_fn_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();

    let mut new_fns: Vec<Item> = Vec::new();

    // Top-level statements.
    let n_items = program.items.len();
    for i in 0..n_items {
        let Item::Statement(Statement::Raw(raw)) = &mut program.items[i] else { continue };
        if let Some(stmt) = raw.stmt.as_mut() {
            lift_arrows_in_stmt(stmt, &mut user_fn_names, &mut new_fns, &counter);
        }
    }

    // Bodies de user fns.
    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            for stmt_raw in &mut f.body {
                let Statement::Raw(raw) = stmt_raw;
                if let Some(stmt) = raw.stmt.as_mut() {
                    lift_arrows_in_stmt(stmt, &mut user_fn_names, &mut new_fns, &counter);
                }
            }
        }
    }

    // Prepend novas fns para que `array_methods_pass` veja-as no
    // user_fn_names snapshot inicial.
    for fn_item in new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }
}

fn lift_arrows_in_stmt(
    stmt: &mut Stmt,
    user_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
    counter: &std::sync::atomic::AtomicU32,
) {
    match stmt {
        Stmt::Expr(e) => lift_arrows_in_expr(&mut e.expr, user_fn_names, new_fns, counter),
        Stmt::Decl(Decl::Var(vd)) => {
            for d in &mut vd.decls {
                if let Some(init) = d.init.as_deref_mut() {
                    lift_arrows_in_expr(init, user_fn_names, new_fns, counter);
                }
            }
        }
        Stmt::Return(r) => {
            if let Some(arg) = r.arg.as_deref_mut() {
                lift_arrows_in_expr(arg, user_fn_names, new_fns, counter);
            }
        }
        _ => {}
    }
}

fn lift_arrows_in_expr(
    expr: &mut Expr,
    user_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
    counter: &std::sync::atomic::AtomicU32,
) {
    if let Expr::Call(call) = expr {
        if let Callee::Expr(callee) = &call.callee {
            if let Expr::Member(m) = callee.as_ref() {
                if let MemberProp::Ident(prop) = &m.prop {
                    let method = prop.sym.as_str();
                    // (#208) Para `Array.from(arrayLike, arrow)` o callback
                    // e' o segundo arg, e arrow tem 2 params (item, idx).
                    let is_array_from = matches!(
                        m.obj.as_ref(),
                        Expr::Ident(id) if id.sym.as_str() == "Array"
                    ) && method == "from";
                    let lift_info: Option<(usize, usize, usize)> = match method {
                        // (arg_idx_to_lift, n_args, arrow_arity)
                        "map" | "forEach" => Some((0, 1, 1)),
                        "reduce" if call.args.len() == 1 => Some((0, 1, 2)),
                        "reduce" => Some((0, 2, 2)),
                        "filter" | "find" | "findIndex" | "some" | "every" => {
                            Some((0, 1, 1))
                        }
                        _ if is_array_from && call.args.len() == 2 => {
                            // Array.from(arrayLike, mapper) — mapper e' arg 1, 2 params.
                            Some((1, 2, 2))
                        }
                        _ => None,
                    };
                    if let Some((arg_idx, n_args, arrow_arity)) = lift_info {
                        if call.args.len() == n_args && arg_idx < call.args.len() {
                            // (#479 follow-up) Se o receiver e' Object.entries(...),
                            // o param recebe [string, V] — slot 0 do destructure
                            // deveria ser Handle, nao I64.
                            let recv_is_object_entries = matches!(
                                m.obj.as_ref(),
                                Expr::Call(c) if matches!(&c.callee,
                                    swc_ecma_ast::Callee::Expr(e) if matches!(e.as_ref(),
                                        Expr::Member(mi) if matches!(&mi.prop,
                                            MemberProp::Ident(p) if p.sym.as_str() == "entries"
                                        ) && matches!(mi.obj.as_ref(),
                                            Expr::Ident(id) if id.sym.as_str() == "Object"
                                        )
                                    )
                                )
                            );
                            // Array.from(string, mapper) — primeiro arg do
                            // mapper eh char (Handle). Sem essa flag, params
                            // sem type-annotation default para I64 e member
                            // call \`c.toUpperCase()\` cai em map_get + trapz.
                            // Detecta call.args[0] como Tpl/Lit::Str.
                            let mapper_slot0_is_string = is_array_from
                                && matches!(
                                    call.args[0].expr.as_ref(),
                                    Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
                                );
                            // Quando arg eh Ident referenciando user fn (named callback),
                            // wrappamos em arrow inline `(p1..pN) => ident(p1..pN)` antes
                            // do try_lift_arrow_arg. Garante adapter de signature
                            // (parallel ABI usa `extern "C" fn(i64) -> i64`, user fn TS
                            // usa `tail (f64) -> X`). Sem isso, parallel.* recebe ptr nu
                            // e callback interpreta i64 bits como f64 (#XXX).
                            if let Expr::Ident(ident) = call.args[arg_idx].expr.as_ref() {
                                if user_fn_names.contains(ident.sym.as_str()) {
                                    let ident_clone = ident.clone();
                                    let mut params: Vec<swc_ecma_ast::Pat> = Vec::with_capacity(arrow_arity);
                                    let mut call_args: Vec<swc_ecma_ast::ExprOrSpread> = Vec::with_capacity(arrow_arity);
                                    for i in 0..arrow_arity {
                                        let pname = format!("__wrap_p_{}_{}", counter.load(std::sync::atomic::Ordering::Relaxed), i);
                                        params.push(swc_ecma_ast::Pat::Ident(swc_ecma_ast::BindingIdent {
                                            id: swc_ecma_ast::Ident {
                                                span: Default::default(),
                                                ctxt: Default::default(),
                                                sym: pname.clone().into(),
                                                optional: false,
                                            },
                                            type_ann: None,
                                        }));
                                        call_args.push(swc_ecma_ast::ExprOrSpread {
                                            spread: None,
                                            expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                                                span: Default::default(),
                                                ctxt: Default::default(),
                                                sym: pname.into(),
                                                optional: false,
                                            })),
                                        });
                                    }
                                    let inner_call = Expr::Call(swc_ecma_ast::CallExpr {
                                        span: Default::default(),
                                        ctxt: Default::default(),
                                        callee: Callee::Expr(Box::new(Expr::Ident(ident_clone))),
                                        args: call_args,
                                        type_args: None,
                                    });
                                    // Wrap em ternario `(call) ? 1 : 0` para boolean cb,
                                    // ou multiply by 1 (`+(call)`) para forcar conversao
                                    // numerica e quebrar tail call (lifted retorna i64,
                                    // user fn pode retornar f64 — mismatch sem coercao).
                                    // Para callbacks que retornam number (map/reduce),
                                    // o codegen faz fcvt_to_sint_sat naturalmente. Para
                                    // boolean (filter/find/some/every/findIndex), o ternario
                                    // mapeia true→1, false→0 sem ambiguidade.
                                    let is_bool_cb = matches!(
                                        method,
                                        "filter" | "find" | "findIndex" | "some" | "every"
                                    );
                                    let body_expr: Expr = if is_bool_cb {
                                        // `cb(p) ? 1 : 0` — boolean -> int explicit.
                                        Expr::Cond(swc_ecma_ast::CondExpr {
                                            span: Default::default(),
                                            test: Box::new(inner_call),
                                            cons: Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(
                                                swc_ecma_ast::Number {
                                                    span: Default::default(),
                                                    value: 1.0,
                                                    raw: None,
                                                },
                                            ))),
                                            alt: Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(
                                                swc_ecma_ast::Number {
                                                    span: Default::default(),
                                                    value: 0.0,
                                                    raw: None,
                                                },
                                            ))),
                                        })
                                    } else {
                                        // `cb(p) + 0` quebra TCO e forca conversao numerica
                                        // (user fn retorna f64, lifted ABI retorna i64).
                                        Expr::Bin(swc_ecma_ast::BinExpr {
                                            span: Default::default(),
                                            op: swc_ecma_ast::BinaryOp::Add,
                                            left: Box::new(inner_call),
                                            right: Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(
                                                swc_ecma_ast::Number {
                                                    span: Default::default(),
                                                    value: 0.0,
                                                    raw: None,
                                                },
                                            ))),
                                        })
                                    };
                                    let arrow = swc_ecma_ast::ArrowExpr {
                                        span: Default::default(),
                                        ctxt: Default::default(),
                                        params,
                                        body: Box::new(swc_ecma_ast::BlockStmtOrExpr::Expr(Box::new(body_expr))),
                                        is_async: false,
                                        is_generator: false,
                                        type_params: None,
                                        return_type: None,
                                    };
                                    call.args[arg_idx].expr = Box::new(Expr::Arrow(arrow));
                                }
                            }
                            let try_lift_now = matches!(
                                call.args[arg_idx].expr.as_ref(),
                                Expr::Arrow(_)
                            );
                            if try_lift_now {
                                if let Some(fn_name) = try_lift_arrow_arg(
                                    &call.args[arg_idx].expr,
                                    arrow_arity,
                                    user_fn_names,
                                    new_fns,
                                    counter,
                                    method == "forEach",
                                    recv_is_object_entries || mapper_slot0_is_string,
                                ) {
                                    // Substitui arg por Ident.
                                    call.args[arg_idx].expr = Box::new(Expr::Ident(swc_ecma_ast::Ident {
                                        span: Default::default(),
                                        ctxt: Default::default(),
                                        sym: fn_name.into(),
                                        optional: false,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
        // Recursa em sub-args.
        for a in &mut call.args {
            lift_arrows_in_expr(&mut a.expr, user_fn_names, new_fns, counter);
        }
        // (#821 follow-up) Recursa no obj do callee tambem para suportar
        // chains como `arr.map(x => x.id).join(",")`. Sem isso, o arrow
        // inline em `.map` ficava sem lift e o `.join` em chain crashava.
        if let Callee::Expr(callee) = &mut call.callee {
            if let Expr::Member(m) = callee.as_mut() {
                lift_arrows_in_expr(&mut m.obj, user_fn_names, new_fns, counter);
            }
        }
        return;
    }
    // (#593) Descer em Tpl/Bin/Cond/Paren/Unary para que arrows em
    // \`${arr.find(x => ...)}\` e similares sejam liftados.
    match expr {
        Expr::Tpl(tpl) => {
            for e in &mut tpl.exprs {
                lift_arrows_in_expr(e, user_fn_names, new_fns, counter);
            }
        }
        Expr::Bin(b) => {
            lift_arrows_in_expr(&mut b.left, user_fn_names, new_fns, counter);
            lift_arrows_in_expr(&mut b.right, user_fn_names, new_fns, counter);
        }
        Expr::Cond(c) => {
            lift_arrows_in_expr(&mut c.test, user_fn_names, new_fns, counter);
            lift_arrows_in_expr(&mut c.cons, user_fn_names, new_fns, counter);
            lift_arrows_in_expr(&mut c.alt, user_fn_names, new_fns, counter);
        }
        Expr::Paren(p) => {
            lift_arrows_in_expr(&mut p.expr, user_fn_names, new_fns, counter);
        }
        Expr::Unary(u) => {
            lift_arrows_in_expr(&mut u.arg, user_fn_names, new_fns, counter);
        }
        _ => {}
    }
}

/// Tenta liftar arrow para top-level fn. Retorna Some(name) em caso
/// de sucesso, None se a arrow nao casa o padrao simples.
fn try_lift_arrow_arg(
    arg: &Expr,
    expected_arity: usize,
    user_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
    counter: &std::sync::atomic::AtomicU32,
    is_void_callback: bool,
    slot0_is_handle: bool,
) -> Option<String> {
    use swc_ecma_ast::BlockStmtOrExpr;
    let arrow = match arg {
        Expr::Arrow(a) => a,
        _ => return None,
    };

    // JS spec permite callback com menos params do que o esperado
    // (extras ficam undefined). Aceita arrow.params.len() <= expected_arity.
    // Lifted vai gerar fn(p1, p2, ...) com expected_arity params; o body
    // referencia apenas os declarados na arrow original.
    if arrow.params.len() > expected_arity {
        return None;
    }

    // Coleta nomes dos params. Aceita Pat::Ident e Pat::Array/Object
    // (destructure). #568 — \`forEach(([k, v]) => ...)\`. Renomeia o
    // param para __p_N e injeta `const [k, v] = __p_N;` no inicio do body.
    let mut param_names: Vec<String> = Vec::with_capacity(expected_arity);
    let mut destructure_prelude: Vec<Stmt> = Vec::new();
    let mut destructured_names: Vec<String> = Vec::new();
    let mut destruct_counter: u32 = 0;
    for p in &arrow.params {
        match p {
            Pat::Ident(bi) => param_names.push(bi.id.sym.to_string()),
            Pat::Array(arr_pat) => {
                let synth = format!("__p_{}_{}", counter.load(std::sync::atomic::Ordering::Relaxed), destruct_counter);
                destruct_counter += 1;
                param_names.push(synth.clone());
                // (#479) Quando slot0_is_handle (e.g. Object.entries), reescreve
                // o pattern para anotar slot 0 como string. Sem isso o slot 0
                // herda I64 default e templates printam handle bruto.
                let mut adjusted_pat = arr_pat.clone();
                if slot0_is_handle {
                    if let Some(Some(Pat::Ident(bi))) = adjusted_pat.elems.first().cloned() {
                        adjusted_pat.elems[0] = Some(Pat::Ident(swc_ecma_ast::BindingIdent {
                            id: bi.id.clone(),
                            type_ann: Some(Box::new(swc_ecma_ast::TsTypeAnn {
                                span: Default::default(),
                                type_ann: Box::new(swc_ecma_ast::TsType::TsKeywordType(
                                    swc_ecma_ast::TsKeywordType {
                                        span: Default::default(),
                                        kind: swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword,
                                    }
                                )),
                            })),
                        }));
                    }
                }
                // Coleta names de elementos Ident para has_capture.
                for el in &adjusted_pat.elems {
                    if let Some(Pat::Ident(bi)) = el {
                        destructured_names.push(bi.id.sym.to_string());
                    }
                }
                destructure_prelude.push(Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
                    span: Default::default(),
                    ctxt: Default::default(),
                    kind: swc_ecma_ast::VarDeclKind::Const,
                    declare: false,
                    decls: vec![swc_ecma_ast::VarDeclarator {
                        span: Default::default(),
                        name: Pat::Array(adjusted_pat),
                        init: Some(Box::new(Expr::Ident(swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: synth.into(),
                            optional: false,
                        }))),
                        definite: false,
                    }],
                }))));
            }
            Pat::Object(obj_pat) => {
                let synth = format!("__p_{}_{}", counter.load(std::sync::atomic::Ordering::Relaxed), destruct_counter);
                destruct_counter += 1;
                param_names.push(synth.clone());
                for prop in &obj_pat.props {
                    use swc_ecma_ast::ObjectPatProp;
                    match prop {
                        ObjectPatProp::Assign(a) => destructured_names.push(a.key.id.sym.to_string()),
                        ObjectPatProp::KeyValue(kv) => {
                            if let Pat::Ident(bi) = kv.value.as_ref() {
                                destructured_names.push(bi.id.sym.to_string());
                            }
                        }
                        _ => {}
                    }
                }
                destructure_prelude.push(Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
                    span: Default::default(),
                    ctxt: Default::default(),
                    kind: swc_ecma_ast::VarDeclKind::Const,
                    declare: false,
                    decls: vec![swc_ecma_ast::VarDeclarator {
                        span: Default::default(),
                        name: p.clone(),
                        init: Some(Box::new(Expr::Ident(swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: synth.into(),
                            optional: false,
                        }))),
                        definite: false,
                    }],
                }))));
            }
            _ => return None,
        }
    }

    // Completa param_names com synth ate expected_arity (JS spec aceita
    // callback com menos params; extras viram unused).
    while param_names.len() < expected_arity {
        let synth = format!(
            "__unused_p_{}_{}",
            counter.load(std::sync::atomic::Ordering::Relaxed),
            param_names.len()
        );
        param_names.push(synth);
    }

    // Body: Expr direto OU BlockStmt com 1 return.
    let body_expr: Expr = match arrow.body.as_ref() {
        BlockStmtOrExpr::Expr(e) => (**e).clone(),
        BlockStmtOrExpr::BlockStmt(b) => {
            if b.stmts.len() != 1 {
                return None;
            }
            match &b.stmts[0] {
                Stmt::Return(r) => match r.arg.as_deref() {
                    Some(e) => e.clone(),
                    None => return None,
                },
                _ => return None,
            }
        }
    };

    // Captura check: idents no body devem ser params, user fns,
    // builtins de namespaces, ou nomes de classes globais. Senao skip.
    let mut all_bound = param_names.clone();
    all_bound.extend(destructured_names.iter().cloned());
    if has_capture(&body_expr, &all_bound, user_fn_names) {
        return None;
    }

    let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let fn_name = format!("__lifted_arr_method_{}", n);

    let parameters: Vec<Parameter> = param_names
        .iter()
        .enumerate()
        .map(|(i, n)| Parameter {
            name: n.clone(),
            // slot 0 marcado como string quando recv eh string ou
            // Object.entries (param eh [string, V] destructured); demais
            // como i64 default.
            type_annotation: Some(if i == 0 && slot0_is_handle {
                "string".to_string()
            } else {
                "i64".to_string()
            }),
            modifiers: MemberModifiers::default(),
            variadic: false,
            default: None,
            span: Span::default(),
        })
        .collect();

    // Heuristica: lifted arrow tem return_type=i64. Se o body e' uma Call
    // (print/console.log/user fn de retorno desconhecido), nao envolver em
    // Return — usa Stmt::Expr seguido de `return 0` para satisfazer signature
    // sem tail-call mismatch. (forEach descarta retorno mesmo.)
    // Para forEach, body Call e' descartado — usar Stmt::Expr + return 0.
    let is_call_expr = is_void_callback && matches!(&body_expr, Expr::Call(_));
    let mut body_stmts: Vec<Statement> = Vec::new();
    for d in destructure_prelude {
        body_stmts.push(Statement::Raw(
            RawStmt::new("<lifted-arrow-destructure>".to_string(), Span::default()).with_stmt(d),
        ));
    }
    if is_call_expr {
        let expr_stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
            span: Default::default(),
            expr: Box::new(body_expr),
        });
        body_stmts.push(Statement::Raw(
            RawStmt::new("<lifted-arrow-call>".to_string(), Span::default()).with_stmt(expr_stmt),
        ));
        let zero_ret = Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
                span: Default::default(),
                value: 0.0,
                raw: None,
            })))),
        });
        body_stmts.push(Statement::Raw(
            RawStmt::new("<lifted-arrow-zero>".to_string(), Span::default()).with_stmt(zero_ret),
        ));
    } else {
        let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(Box::new(body_expr)),
        });
        body_stmts.push(Statement::Raw(
            RawStmt::new("<lifted-arrow>".to_string(), Span::default()).with_stmt(return_stmt),
        ));
    }

    new_fns.push(Item::Function(FunctionDecl {
        name: fn_name.clone(),
        parameters,
        return_type: Some("i64".to_string()),
        body: body_stmts,
        span: Span::default(),
        is_async: false,
    }));
    user_fn_names.insert(fn_name.clone());
    Some(fn_name)
}

/// Conjunto conservador de namespace/builtin idents que sao OK
/// referenciar do body sem caracterizar captura de local.
fn is_known_global_ident(name: &str) -> bool {
    matches!(
        name,
        "math" | "string" | "num" | "fmt" | "path" | "hash" | "mem"
        | "io" | "fs" | "gc" | "buffer" | "time" | "env" | "os"
        | "collections" | "crypto" | "regex" | "json" | "date"
        | "Math" | "String" | "Number" | "Date" | "JSON" | "RegExp"
        | "Error" | "TypeError" | "RangeError" | "SyntaxError"
        | "Array" | "Object" | "Boolean" | "Symbol"
        | "console" | "performance" | "globalThis"
        | "undefined" | "null" | "NaN" | "Infinity"
        | "true" | "false"
        | "isNaN" | "isFinite" | "parseInt" | "parseFloat"
        | "encodeURIComponent" | "decodeURIComponent"
        | "atob" | "btoa" | "structuredClone"
    )
}

fn has_capture(expr: &Expr, params: &[String], user_fn_names: &HashSet<String>) -> bool {
    match expr {
        Expr::Ident(i) => {
            let n = i.sym.as_str();
            if params.iter().any(|p| p == n) { return false; }
            if user_fn_names.contains(n) { return false; }
            if is_known_global_ident(n) { return false; }
            true
        }
        Expr::Lit(_) => false,
        Expr::This(_) => true,
        Expr::Bin(b) => has_capture(&b.left, params, user_fn_names) || has_capture(&b.right, params, user_fn_names),
        Expr::Unary(u) => has_capture(&u.arg, params, user_fn_names),
        Expr::Paren(p) => has_capture(&p.expr, params, user_fn_names),
        Expr::Cond(c) => has_capture(&c.test, params, user_fn_names)
            || has_capture(&c.cons, params, user_fn_names)
            || has_capture(&c.alt, params, user_fn_names),
        Expr::Member(m) => {
            // obj pode ser ident; prop nao conta.
            has_capture(&m.obj, params, user_fn_names)
        }
        Expr::Call(c) => {
            let callee_cap = match &c.callee {
                Callee::Expr(e) => has_capture(e, params, user_fn_names),
                _ => false,
            };
            if callee_cap { return true; }
            for a in &c.args {
                if a.spread.is_some() { return true; }
                if has_capture(&a.expr, params, user_fn_names) { return true; }
            }
            false
        }
        Expr::Tpl(t) => {
            for e in &t.exprs {
                if has_capture(e, params, user_fn_names) { return true; }
            }
            false
        }
        Expr::Array(a) => {
            for el in &a.elems {
                if let Some(el) = el {
                    if el.spread.is_some() { return true; }
                    if has_capture(&el.expr, params, user_fn_names) { return true; }
                }
            }
            false
        }
        Expr::TsAs(t) => has_capture(&t.expr, params, user_fn_names),
        Expr::TsTypeAssertion(t) => has_capture(&t.expr, params, user_fn_names),
        Expr::TsConstAssertion(t) => has_capture(&t.expr, params, user_fn_names),
        Expr::TsNonNull(t) => has_capture(&t.expr, params, user_fn_names),
        // Conservador: qualquer outra forma (Fn, Arrow nested, Assign, Update, etc) → trata como captura.
        _ => true,
    }
}

/// Level-1 silent array methods: reescreve `arr.map(fn)`,
/// `arr.forEach(fn)`, `arr.reduce(fn, init)` para `parallel.map(arr, fn)`,
/// `parallel.for_each(arr, fn)`, `parallel.reduce(arr, init, fn)` quando
/// `fn` e um Ident apontando pra uma user fn top-level.
pub(crate) fn array_methods_pass(program: &mut Program) {
    // Coleta nomes de user fns top-level pra validar que o arg e ident
    // de user fn (caso contrario fica serial — pode ser arrow inline
    // que ja e lifted por outros passes).
    let user_fn_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();

    // Visita top-level statements.
    let n_items = program.items.len();
    for i in 0..n_items {
        let Item::Statement(Statement::Raw(raw)) = &mut program.items[i] else { continue };
        if let Some(stmt) = raw.stmt.as_mut() {
            rewrite_array_methods_in_stmt(stmt, &user_fn_names);
        }
    }

    // Visita body de cada user fn.
    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            for stmt_raw in &mut f.body {
                let Statement::Raw(raw) = stmt_raw;
                if let Some(stmt) = raw.stmt.as_mut() {
                    rewrite_array_methods_in_stmt(stmt, &user_fn_names);
                }
            }
        }
    }
}

fn rewrite_array_methods_in_stmt(stmt: &mut Stmt, user_fn_names: &HashSet<String>) {
    match stmt {
        Stmt::Expr(e) => rewrite_array_methods_in_expr(&mut e.expr, user_fn_names),
        Stmt::Decl(Decl::Var(vd)) => {
            for d in &mut vd.decls {
                if let Some(init) = d.init.as_deref_mut() {
                    rewrite_array_methods_in_expr(init, user_fn_names);
                }
            }
        }
        Stmt::Return(r) => {
            if let Some(arg) = r.arg.as_deref_mut() {
                rewrite_array_methods_in_expr(arg, user_fn_names);
            }
        }
        _ => {}
    }
}

fn rewrite_array_methods_in_expr(expr: &mut Expr, user_fn_names: &HashSet<String>) {
    match expr {
        Expr::Tpl(tpl) => {
            for e in &mut tpl.exprs {
                rewrite_array_methods_in_expr(e, user_fn_names);
            }
            return;
        }
        Expr::Bin(b) => {
            rewrite_array_methods_in_expr(&mut b.left, user_fn_names);
            rewrite_array_methods_in_expr(&mut b.right, user_fn_names);
            return;
        }
        Expr::Cond(c) => {
            rewrite_array_methods_in_expr(&mut c.test, user_fn_names);
            rewrite_array_methods_in_expr(&mut c.cons, user_fn_names);
            rewrite_array_methods_in_expr(&mut c.alt, user_fn_names);
            return;
        }
        Expr::Paren(p) => {
            rewrite_array_methods_in_expr(&mut p.expr, user_fn_names);
            return;
        }
        Expr::Unary(u) => {
            rewrite_array_methods_in_expr(&mut u.arg, user_fn_names);
            return;
        }
        _ => {}
    }
    if let Expr::Call(call) = expr {
        if let Callee::Expr(callee) = &call.callee {
            if let Expr::Member(m) = callee.as_ref() {
                if let MemberProp::Ident(prop) = &m.prop {
                    let method = prop.sym.as_str();
                    let arg0_is_user_fn = call.args.first()
                        .and_then(|a| match a.expr.as_ref() {
                            Expr::Ident(i) => Some(i.sym.to_string()),
                            _ => None,
                        })
                        .map(|n| user_fn_names.contains(&n))
                        .unwrap_or(false);

                    let target_method: Option<&str> = match method {
                        "map" if call.args.len() == 1 && arg0_is_user_fn => Some("map"),
                        "forEach" if call.args.len() == 1 && arg0_is_user_fn => Some("for_each"),
                        "reduce" if call.args.len() == 2 && arg0_is_user_fn => Some("reduce"),
                        // (cross-runtime #254) reduce sem initial value.
                        "reduce" if call.args.len() == 1 && arg0_is_user_fn => Some("reduce_no_init"),
                        "filter" if call.args.len() == 1 && arg0_is_user_fn => Some("filter"),
                        "find" if call.args.len() == 1 && arg0_is_user_fn => Some("find"),
                        "findIndex" if call.args.len() == 1 && arg0_is_user_fn => {
                            Some("find_index")
                        }
                        "some" if call.args.len() == 1 && arg0_is_user_fn => Some("some"),
                        "every" if call.args.len() == 1 && arg0_is_user_fn => Some("every"),
                        _ => None,
                    };

                    if let Some(par_method) = target_method {
                        let arr_expr = (*m.obj).clone();
                        let fn_arg = call.args[0].expr.clone();
                        let new_args: Vec<swc_ecma_ast::ExprOrSpread> = if par_method == "reduce" {
                            let init_arg = call.args[1].expr.clone();
                            vec![
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: Box::new(arr_expr) },
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: init_arg },
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: fn_arg },
                            ]
                        } else {
                            // reduce_no_init / map / forEach / etc: (arr, fn)
                            vec![
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: Box::new(arr_expr) },
                                swc_ecma_ast::ExprOrSpread { spread: None, expr: fn_arg },
                            ]
                        };
                        *call = swc_ecma_ast::CallExpr {
                            span: call.span,
                            ctxt: call.ctxt,
                            callee: Callee::Expr(Box::new(Expr::Member(swc_ecma_ast::MemberExpr {
                                span: Default::default(),
                                obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                                    span: Default::default(), ctxt: Default::default(),
                                    sym: "parallel".into(), optional: false,
                                })),
                                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                                    span: Default::default(),
                                    sym: par_method.to_string().into(),
                                }),
                            }))),
                            args: new_args,
                            type_args: None,
                        };
                        return;
                    }
                }
            }
        }
        for a in &mut call.args {
            rewrite_array_methods_in_expr(&mut a.expr, user_fn_names);
        }
        // (#821 follow-up) Recursa no obj do callee para chains
        // (`arr.map(fn).join(...)`).
        if let Callee::Expr(callee) = &mut call.callee {
            if let Expr::Member(m) = callee.as_mut() {
                rewrite_array_methods_in_expr(&mut m.obj, user_fn_names);
            }
        }
    }
}

/// Level-1 silent reduce: detecta padrao `let acc = init; for (x of arr) acc = acc + EXPR;`
/// e reescreve para `let acc = parallel.reduce(arr, init, __par_reduce_N);`.
pub(crate) fn reduce_pass(program: &mut Program) -> HashSet<String> {
    let pure_ns = build_pure_ns_set();
    let mut counter = 0u32;
    let mut par_fn_names: HashSet<String> = HashSet::new();
    let mut new_fns: Vec<Item> = Vec::new();

    apply_reduce_pass_to_top_level(
        &mut program.items, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
    );

    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            apply_reduce_pass_to_body(
                &mut f.body, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
            );
        }
    }

    for fn_item in new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }

    par_fn_names
}

fn apply_reduce_pass_to_top_level(
    items: &mut Vec<Item>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Match {
        decl_idx: usize,
        for_idx: usize,
        acc_name: String,
        init_expr: Expr,
        arr_expr: Expr,
        loop_var: String,
        rhs_expr: Expr,
        fn_name: String,
        op: AssocOp,
    }
    let mut matches: Vec<Match> = Vec::new();
    let n_items = items.len();
    for i in 0..n_items.saturating_sub(1) {
        let Item::Statement(Statement::Raw(decl_raw)) = &items[i] else { continue };
        let Some(Stmt::Decl(Decl::Var(vd))) = decl_raw.stmt.as_ref() else { continue };
        if vd.decls.len() != 1 {
            continue;
        }
        let Pat::Ident(acc_pat) = &vd.decls[0].name else { continue };
        let acc_name = acc_pat.id.sym.as_str().to_string();
        let Some(init) = vd.decls[0].init.as_deref() else { continue };
        if !matches!(init, Expr::Lit(Lit::Num(_))) {
            continue;
        }

        let Item::Statement(Statement::Raw(for_raw)) = &items[i + 1] else { continue };
        let Some(Stmt::ForOf(for_of)) = for_raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }

        let loop_var = match &for_of.left {
            ForHead::VarDecl(lvd) if lvd.decls.len() == 1 => match &lvd.decls[0].name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => continue,
            },
            _ => continue,
        };

        let stmts: &[Stmt] = match for_of.body.as_ref() {
            Stmt::Block(b) => &b.stmts,
            other => std::slice::from_ref(other),
        };
        if stmts.len() != 1 {
            continue;
        }
        let Stmt::Expr(expr_stmt) = &stmts[0] else { continue };
        let Expr::Assign(assign) = expr_stmt.expr.as_ref() else { continue };

        let lhs_ok = matches!(
            &assign.left,
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id))
                if id.id.sym.as_str() == acc_name
        );
        if !lhs_ok { continue; }

        let (op, rhs_expr): (AssocOp, Expr) = match assign.op {
            swc_ecma_ast::AssignOp::AddAssign => (AssocOp::Add, (*assign.right).clone()),
            swc_ecma_ast::AssignOp::MulAssign => (AssocOp::Mul, (*assign.right).clone()),
            swc_ecma_ast::AssignOp::Assign => {
                let Expr::Bin(bin) = assign.right.as_ref() else { continue };
                let acc_lhs_ok = matches!(
                    bin.left.as_ref(),
                    Expr::Ident(i) if i.sym.as_str() == acc_name
                );
                if !acc_lhs_ok { continue; }
                let op = match bin.op {
                    swc_ecma_ast::BinaryOp::Add => AssocOp::Add,
                    swc_ecma_ast::BinaryOp::Mul => AssocOp::Mul,
                    _ => continue,
                };
                (op, (*bin.right).clone())
            }
            _ => continue,
        };

        if !is_pure_expr_for_parallel(&rhs_expr, &loop_var, &HashSet::new(), pure_ns) {
            continue;
        }

        let fn_name = format!("__par_reduce_{counter}");
        *counter += 1;
        matches.push(Match {
            decl_idx: i,
            for_idx: i + 1,
            acc_name,
            init_expr: init.clone(),
            arr_expr: for_of.right.as_ref().clone(),
            loop_var,
            rhs_expr,
            fn_name,
            op,
        });
    }

    if matches.is_empty() {
        return;
    }

    for m in &matches {
        let bin_op = match m.op {
            AssocOp::Add => swc_ecma_ast::BinaryOp::Add,
            AssocOp::Mul => swc_ecma_ast::BinaryOp::Mul,
        };
        let body_expr = Expr::Bin(swc_ecma_ast::BinExpr {
            span: Default::default(),
            op: bin_op,
            left: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: m.acc_name.clone().into(),
                optional: false,
            })),
            right: Box::new(m.rhs_expr.clone()),
        });
        let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(Box::new(body_expr)),
        });
        let body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-reduce>".to_string(), Span::default()).with_stmt(return_stmt),
        )];

        new_fns.push(Item::Function(FunctionDecl {
            name: m.fn_name.clone(),
            parameters: vec![
                Parameter {
                    name: m.acc_name.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false,
                    default: None,
                    span: Span::default(),
                },
                Parameter {
                    name: m.loop_var.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false,
                    default: None,
                    span: Span::default(),
                },
            ],
            return_type: Some("i64".to_string()),
            body: body_stmts,
            span: Span::default(),
            is_async: false,
        }));
        par_fn_names.insert(m.fn_name.clone());
    }

    for m in &matches {
        if let Item::Statement(Statement::Raw(raw)) = &mut items[m.for_idx] {
            raw.stmt = Some(Stmt::Expr(swc_ecma_ast::ExprStmt {
                span: Default::default(),
                expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                    span: Default::default(),
                    value: 0.0,
                    raw: None,
                }))),
            }));
        }

        if let Item::Statement(Statement::Raw(raw)) = &mut items[m.decl_idx] {
            let reduce_call = make_par_reduce_expr(&m.arr_expr, &m.init_expr, &m.fn_name);
            raw.stmt = Some(Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
                span: Default::default(),
                ctxt: Default::default(),
                kind: swc_ecma_ast::VarDeclKind::Let,
                declare: false,
                decls: vec![swc_ecma_ast::VarDeclarator {
                    span: Default::default(),
                    name: Pat::Ident(swc_ecma_ast::BindingIdent {
                        id: swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: m.acc_name.clone().into(),
                            optional: false,
                        },
                        type_ann: None,
                    }),
                    init: Some(Box::new(reduce_call)),
                    definite: false,
                }],
            }))));
        }
    }
}

fn apply_reduce_pass_to_body(
    body: &mut Vec<Statement>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Match {
        decl_idx: usize,
        for_idx: usize,
        acc_name: String,
        init_expr: Expr,
        arr_expr: Expr,
        loop_var: String,
        rhs_expr: Expr,
        fn_name: String,
        op: AssocOp,
    }
    let mut matches: Vec<Match> = Vec::new();
    let n = body.len();
    for i in 0..n.saturating_sub(1) {
        let Statement::Raw(decl_raw) = &body[i];
        let Some(Stmt::Decl(Decl::Var(vd))) = decl_raw.stmt.as_ref() else { continue };
        if vd.decls.len() != 1 { continue; }
        let Pat::Ident(acc_pat) = &vd.decls[0].name else { continue };
        let acc_name = acc_pat.id.sym.as_str().to_string();
        let Some(init) = vd.decls[0].init.as_deref() else { continue };
        if !matches!(init, Expr::Lit(Lit::Num(_))) { continue; }
        let Statement::Raw(for_raw) = &body[i + 1];
        let Some(Stmt::ForOf(for_of)) = for_raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }
        let loop_var = match &for_of.left {
            ForHead::VarDecl(lvd) if lvd.decls.len() == 1 => match &lvd.decls[0].name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => continue,
            },
            _ => continue,
        };
        let stmts: &[Stmt] = match for_of.body.as_ref() {
            Stmt::Block(b) => &b.stmts,
            other => std::slice::from_ref(other),
        };
        if stmts.len() != 1 { continue; }
        let Stmt::Expr(expr_stmt) = &stmts[0] else { continue };
        let Expr::Assign(assign) = expr_stmt.expr.as_ref() else { continue };
        let lhs_ok = matches!(
            &assign.left,
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id))
                if id.id.sym.as_str() == acc_name
        );
        if !lhs_ok { continue; }
        let (op, rhs_expr): (AssocOp, Expr) = match assign.op {
            swc_ecma_ast::AssignOp::AddAssign => (AssocOp::Add, (*assign.right).clone()),
            swc_ecma_ast::AssignOp::MulAssign => (AssocOp::Mul, (*assign.right).clone()),
            swc_ecma_ast::AssignOp::Assign => {
                let Expr::Bin(bin) = assign.right.as_ref() else { continue };
                let acc_lhs_ok = matches!(
                    bin.left.as_ref(),
                    Expr::Ident(i) if i.sym.as_str() == acc_name
                );
                if !acc_lhs_ok { continue; }
                let op = match bin.op {
                    swc_ecma_ast::BinaryOp::Add => AssocOp::Add,
                    swc_ecma_ast::BinaryOp::Mul => AssocOp::Mul,
                    _ => continue,
                };
                (op, (*bin.right).clone())
            }
            _ => continue,
        };
        if !is_pure_expr_for_parallel(&rhs_expr, &loop_var, &HashSet::new(), pure_ns) {
            continue;
        }
        let fn_name = format!("__par_reduce_{counter}");
        *counter += 1;
        matches.push(Match {
            decl_idx: i, for_idx: i + 1, acc_name, init_expr: init.clone(),
            arr_expr: for_of.right.as_ref().clone(),
            loop_var, rhs_expr, fn_name, op,
        });
    }
    if matches.is_empty() { return; }
    for m in &matches {
        let bin_op = match m.op {
            AssocOp::Add => swc_ecma_ast::BinaryOp::Add,
            AssocOp::Mul => swc_ecma_ast::BinaryOp::Mul,
        };
        let body_expr = Expr::Bin(swc_ecma_ast::BinExpr {
            span: Default::default(),
            op: bin_op,
            left: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(), ctxt: Default::default(),
                sym: m.acc_name.clone().into(), optional: false,
            })),
            right: Box::new(m.rhs_expr.clone()),
        });
        let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(), arg: Some(Box::new(body_expr)),
        });
        let fn_body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-reduce>".to_string(), Span::default()).with_stmt(return_stmt),
        )];
        new_fns.push(Item::Function(FunctionDecl {
            name: m.fn_name.clone(),
            parameters: vec![
                Parameter {
                    name: m.acc_name.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false, default: None, span: Span::default(),
                },
                Parameter {
                    name: m.loop_var.clone(),
                    type_annotation: Some("i64".to_string()),
                    modifiers: MemberModifiers::default(),
                    variadic: false, default: None, span: Span::default(),
                },
            ],
            return_type: Some("i64".to_string()),
            body: fn_body_stmts,
            span: Span::default(),
            is_async: false,
        }));
        par_fn_names.insert(m.fn_name.clone());
    }
    for m in &matches {
        let Statement::Raw(raw) = &mut body[m.for_idx];
        raw.stmt = Some(Stmt::Expr(swc_ecma_ast::ExprStmt {
            span: Default::default(),
            expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                span: Default::default(), value: 0.0, raw: None,
            }))),
        }));
        let reduce_call = make_par_reduce_expr(&m.arr_expr, &m.init_expr, &m.fn_name);
        let Statement::Raw(raw2) = &mut body[m.decl_idx];
        raw2.stmt = Some(Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
            span: Default::default(), ctxt: Default::default(),
            kind: swc_ecma_ast::VarDeclKind::Let, declare: false,
            decls: vec![swc_ecma_ast::VarDeclarator {
                span: Default::default(),
                name: Pat::Ident(swc_ecma_ast::BindingIdent {
                    id: swc_ecma_ast::Ident {
                        span: Default::default(), ctxt: Default::default(),
                        sym: m.acc_name.clone().into(), optional: false,
                    },
                    type_ann: None,
                }),
                init: Some(Box::new(reduce_call)), definite: false,
            }],
        }))));
    }
}

#[derive(Clone, Copy)]
enum AssocOp {
    Add,
    Mul,
}

fn make_par_reduce_expr(arr_expr: &Expr, init_expr: &Expr, fn_name: &str) -> Expr {
    Expr::Call(swc_ecma_ast::CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Member(swc_ecma_ast::MemberExpr {
            span: Default::default(),
            obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: "parallel".into(),
                optional: false,
            })),
            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                span: Default::default(),
                sym: "reduce".into(),
            }),
        }))),
        args: vec![
            swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(arr_expr.clone()),
            },
            swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(init_expr.clone()),
            },
            swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: fn_name.to_string().into(),
                    optional: false,
                })),
            },
        ],
        type_args: None,
    })
}

/// Level-1 silent parallelism: rewrites pure top-level `for...of` loops into
/// `parallel.for_each(arr, __par_forof_N)` calls backed by a Rayon thread
/// pool. A ForOf is eligible when:
///   - no assignments in the body
///   - all function calls are to pure namespace members
///   - all idents in the body are either the loop variable or inner decls
///   - no break / continue / return / throw
pub(crate) fn purity_pass(program: &mut Program) -> HashSet<String> {
    let pure_ns = build_pure_ns_set();
    let mut counter = 0u32;
    let mut par_fn_names: HashSet<String> = HashSet::new();
    let mut new_fns: Vec<Item> = Vec::new();

    apply_purity_pass_to_top_level(
        &mut program.items, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
    );

    let fn_indices: Vec<usize> = program.items.iter().enumerate()
        .filter_map(|(i, it)| if matches!(it, Item::Function(_)) { Some(i) } else { None })
        .collect();
    for i in fn_indices {
        if let Item::Function(f) = &mut program.items[i] {
            apply_purity_pass_to_body(
                &mut f.body, &pure_ns, &mut counter, &mut par_fn_names, &mut new_fns,
            );
        }
    }

    for fn_item in new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }

    par_fn_names
}

fn apply_purity_pass_to_top_level(
    items: &mut Vec<Item>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Transform {
        idx: usize,
        arr_expr: Expr,
        body_stmt: Stmt,
        loop_var: String,
        fn_name: String,
    }
    let mut transforms: Vec<Transform> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::ForOf(for_of)) = raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }
        let loop_var = match &for_of.left {
            ForHead::VarDecl(vd) => {
                if vd.decls.len() != 1 { continue; }
                match &vd.decls[0].name {
                    Pat::Ident(id) => id.sym.as_str().to_string(),
                    _ => continue,
                }
            }
            _ => continue,
        };
        if !analyze_for_of_body_pure(&for_of.body, &loop_var, pure_ns) { continue; }
        let fn_name = format!("__par_forof_{counter}");
        *counter += 1;
        transforms.push(Transform {
            idx, arr_expr: for_of.right.as_ref().clone(),
            body_stmt: for_of.body.as_ref().clone(),
            loop_var, fn_name,
        });
    }
    if transforms.is_empty() { return; }
    for t in &transforms {
        let body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-forof>".to_string(), Span::default())
                .with_stmt(t.body_stmt.clone()),
        )];
        new_fns.push(Item::Function(FunctionDecl {
            name: t.fn_name.clone(),
            parameters: vec![Parameter {
                name: t.loop_var.clone(),
                type_annotation: Some("i64".to_string()),
                modifiers: MemberModifiers::default(),
                variadic: false, default: None, span: Span::default(),
            }],
            return_type: Some("void".to_string()),
            body: body_stmts, span: Span::default(),
            is_async: false,
        }));
        par_fn_names.insert(t.fn_name.clone());
    }
    for t in &transforms {
        if let Item::Statement(Statement::Raw(raw)) = &mut items[t.idx] {
            raw.stmt = Some(make_par_foreach_stmt(&t.arr_expr, &t.fn_name));
        }
    }
}

fn apply_purity_pass_to_body(
    body: &mut Vec<Statement>,
    pure_ns: &HashSet<(&'static str, &'static str)>,
    counter: &mut u32,
    par_fn_names: &mut HashSet<String>,
    new_fns: &mut Vec<Item>,
) {
    struct Transform {
        idx: usize,
        arr_expr: Expr,
        body_stmt: Stmt,
        loop_var: String,
        fn_name: String,
    }
    let mut transforms: Vec<Transform> = Vec::new();
    for (idx, stmt) in body.iter().enumerate() {
        let Statement::Raw(raw) = stmt;
        let Some(Stmt::ForOf(for_of)) = raw.stmt.as_ref() else { continue };
        if for_of.is_await { continue; }
        let loop_var = match &for_of.left {
            ForHead::VarDecl(vd) => {
                if vd.decls.len() != 1 { continue; }
                match &vd.decls[0].name {
                    Pat::Ident(id) => id.sym.as_str().to_string(),
                    _ => continue,
                }
            }
            _ => continue,
        };
        if !analyze_for_of_body_pure(&for_of.body, &loop_var, pure_ns) { continue; }
        let fn_name = format!("__par_forof_{counter}");
        *counter += 1;
        transforms.push(Transform {
            idx, arr_expr: for_of.right.as_ref().clone(),
            body_stmt: for_of.body.as_ref().clone(),
            loop_var, fn_name,
        });
    }
    if transforms.is_empty() { return; }
    for t in &transforms {
        let body_stmts = vec![Statement::Raw(
            RawStmt::new("<par-forof>".to_string(), Span::default())
                .with_stmt(t.body_stmt.clone()),
        )];
        new_fns.push(Item::Function(FunctionDecl {
            name: t.fn_name.clone(),
            parameters: vec![Parameter {
                name: t.loop_var.clone(),
                type_annotation: Some("i64".to_string()),
                modifiers: MemberModifiers::default(),
                variadic: false, default: None, span: Span::default(),
            }],
            return_type: Some("void".to_string()),
            body: body_stmts, span: Span::default(),
            is_async: false,
        }));
        par_fn_names.insert(t.fn_name.clone());
    }
    for t in &transforms {
        let Statement::Raw(raw) = &mut body[t.idx];
        raw.stmt = Some(make_par_foreach_stmt(&t.arr_expr, &t.fn_name));
    }
}
