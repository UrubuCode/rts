/// AST → HIR lowering.
///
/// Converts `rts_ast` items (which wrap `swc_ecma_ast` nodes) into typed
/// `HirFunc` / `HirStmt` / `HirExpr` nodes. Type annotations are parsed from
/// string annotations produced by `rts-parser`; unannotated variables get
/// `HirType::Unknown` which `type_refine` narrows further.

use swc_ecma_ast as swc;
use rts_ast::ast::{
    FunctionDecl, Parameter, Statement,
};

use crate::ir::*;
use crate::scope::Scope;
use crate::type_refine::{refine_func_ret, refine_type_from_init};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Lower a `FunctionDecl` from the AST into a `HirFunc`.
pub fn lower_func(decl: &FunctionDecl, scope: &mut Scope) -> HirFunc {
    scope.push();

    let params: Vec<HirParam> = decl.parameters.iter().map(|p| lower_param(p, scope)).collect();
    let ret = decl.return_type.as_deref()
        .map(parse_type_annotation)
        .unwrap_or(HirType::Unknown);

    // Registra a assinatura ANTES de lower do body para que chamadas
    // recursivas dentro do body resolvam o tipo de retorno corretamente
    // (caso contrário, `fact(n-1)` fica com ty=Unknown e o
    // `numeric_promotion(I64, Unknown)` cai em Number, levando o codegen
    // a emitir fmul indevidamente).
    if !matches!(ret, HirType::Unknown) {
        scope.register_return_type(&decl.name, ret.clone());
    }
    scope.register_param_types(
        &decl.name,
        params.iter().map(|p| p.ty.clone()).collect(),
    );

    let body = lower_stmts(&decl.body, scope);
    let refined_ret = refine_func_ret(&ret, &body, scope);
    scope.pop();

    // Re-register com tipo refinado (caso body tenha sido mais informativo).
    if !matches!(refined_ret, HirType::Unknown) {
        scope.register_return_type(&decl.name, refined_ret.clone());
    }

    HirFunc {
        name: decl.name.clone(),
        params,
        ret: refined_ret,
        body,
        is_async: decl.is_async,
        is_arrow: false,
    }
}

/// Lower a slice of `Statement`s into `HirStmt`s.
pub fn lower_stmts(stmts: &[Statement], scope: &mut Scope) -> Vec<HirStmt> {
    stmts.iter().map(|s| lower_stmt(s, scope)).collect()
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

fn lower_stmt(stmt: &Statement, scope: &mut Scope) -> HirStmt {
    let Statement::Raw(raw) = stmt;
    match &raw.stmt {
        Some(swc_stmt) => lower_swc_stmt(swc_stmt, scope, &raw.text),
        None => HirStmt::Raw(raw.text.clone()),
    }
}

fn lower_swc_stmt(s: &swc::Stmt, scope: &mut Scope, raw_text: &str) -> HirStmt {
    match s {
        swc::Stmt::Expr(e) => {
            HirStmt::Expr(lower_swc_expr(&e.expr, scope))
        }

        swc::Stmt::Return(r) => {
            let val = r.arg.as_deref().map(|e| lower_swc_expr(e, scope));
            HirStmt::Return(val)
        }

        swc::Stmt::Decl(decl) => lower_decl(decl, scope, raw_text),

        swc::Stmt::If(if_stmt) => {
            let cond = lower_swc_expr(&if_stmt.test, scope);
            scope.push();
            let then = lower_swc_stmt_to_vec(&if_stmt.cons, scope);
            scope.pop();
            let else_ = if_stmt.alt.as_deref().map(|e| {
                scope.push();
                let stmts = lower_swc_stmt_to_vec(e, scope);
                scope.pop();
                stmts
            });
            HirStmt::If { cond, then, else_ }
        }

        swc::Stmt::While(w) => {
            let cond = lower_swc_expr(&w.test, scope);
            scope.push();
            let body = lower_swc_stmt_to_vec(&w.body, scope);
            scope.pop();
            HirStmt::While { cond, body }
        }

        swc::Stmt::DoWhile(dw) => {
            scope.push();
            let body = lower_swc_stmt_to_vec(&dw.body, scope);
            scope.pop();
            let cond = lower_swc_expr(&dw.test, scope);
            HirStmt::DoWhile { body, cond }
        }

        swc::Stmt::For(f) => {
            scope.push();
            let init = f.init.as_ref().map(|i| {
                Box::new(match i {
                    swc::VarDeclOrExpr::VarDecl(vd) => lower_var_decl(vd, scope, ""),
                    swc::VarDeclOrExpr::Expr(e) => HirStmt::Expr(lower_swc_expr(e, scope)),
                })
            });
            let cond = f.test.as_deref().map(|e| lower_swc_expr(e, scope));
            let update = f.update.as_deref().map(|e| lower_swc_expr(e, scope));
            let body = lower_swc_stmt_to_vec(&f.body, scope);
            scope.pop();
            HirStmt::For { init, cond, update, body }
        }

        swc::Stmt::ForOf(fo) => {
            let iterable = lower_swc_expr(&fo.right, scope);
            scope.push();
            let binding = extract_pat_binding(&fo.left);
            let binding_ty = HirType::Unknown;
            scope.define(&binding, binding_ty.clone());
            let body = lower_swc_stmt_to_vec(&fo.body, scope);
            scope.pop();
            HirStmt::ForOf { binding, binding_ty, iterable, body }
        }

        swc::Stmt::ForIn(fi) => {
            let object = lower_swc_expr(&fi.right, scope);
            scope.push();
            let binding = extract_pat_binding(&fi.left);
            scope.define(&binding, HirType::Str);
            let body = lower_swc_stmt_to_vec(&fi.body, scope);
            scope.pop();
            HirStmt::ForIn { binding, object, body }
        }

        swc::Stmt::Break(b) => HirStmt::Break(b.label.as_ref().map(|l| l.sym.to_string())),
        swc::Stmt::Continue(c) => HirStmt::Continue(c.label.as_ref().map(|l| l.sym.to_string())),

        swc::Stmt::Throw(t) => HirStmt::Throw(lower_swc_expr(&t.arg, scope)),

        swc::Stmt::Try(t) => {
            scope.push();
            let body = lower_swc_stmts_block(&t.block, scope);
            scope.pop();

            let catch = t.handler.as_ref().map(|h| {
                scope.push();
                let binding = h.param.as_ref().and_then(|p| Some(extract_swc_pat_name(p)));
                if let Some(ref name) = binding {
                    scope.define(name.as_str(), HirType::Any);
                }
                let body = lower_swc_stmts_block(&h.body, scope);
                scope.pop();
                HirCatch { binding, body }
            });

            let finally = t.finalizer.as_ref().map(|f| {
                scope.push();
                let stmts = lower_swc_stmts_block(f, scope);
                scope.pop();
                stmts
            });

            HirStmt::Try { body, catch, finally }
        }

        swc::Stmt::Switch(sw) => {
            let discriminant = lower_swc_expr(&sw.discriminant, scope);
            let cases = sw.cases.iter().map(|c| {
                let test = c.test.as_deref().map(|e| lower_swc_expr(e, scope));
                scope.push();
                let body = c.cons.iter()
                    .map(|s| lower_swc_stmt(s, scope, raw_text))
                    .collect();
                scope.pop();
                HirSwitchCase { test, body }
            }).collect();
            HirStmt::Switch { discriminant, cases }
        }

        swc::Stmt::Block(b) => {
            scope.push();
            let stmts = lower_swc_stmts_block(b, scope);
            scope.pop();
            HirStmt::Block(stmts)
        }

        swc::Stmt::Labeled(l) => {
            let label = l.label.sym.to_string();
            let body = Box::new(lower_swc_stmt(&l.body, scope, raw_text));
            HirStmt::Labeled { label, body }
        }

        swc::Stmt::Empty(_) => HirStmt::Block(vec![]),

        // Anything we don't recognise falls back to raw text for the legacy
        // codegen path — no information is lost.
        _ => HirStmt::Raw(raw_text.to_string()),
    }
}

fn lower_decl(decl: &swc::Decl, scope: &mut Scope, raw_text: &str) -> HirStmt {
    match decl {
        swc::Decl::Var(vd) => lower_var_decl(vd, scope, raw_text),
        // TYPE-ONLY declarations (`type X = …`, `interface X {…}`) are erased at
        // runtime — they carry no value. Elide to an empty block (a no-op) instead
        // of a `Raw` the lowering would bail on as an "unrecognized statement".
        swc::Decl::TsTypeAlias(_) | swc::Decl::TsInterface(_) => HirStmt::Block(Vec::new()),
        // Function/class declarations inside a block fall back to raw text
        // because they're hoisted — handled at the program level, not here.
        _ => HirStmt::Raw(raw_text.to_string()),
    }
}

fn lower_var_decl(vd: &swc::VarDecl, scope: &mut Scope, _raw_text: &str) -> HirStmt {
    if vd.decls.len() == 1 {
        let d = &vd.decls[0];
        let name = extract_swc_pat_name(&d.name);
        let annotation = extract_ts_type_annotation(&d.name);
        let annotated_ty = annotation
            .as_deref()
            .map(parse_type_annotation)
            .unwrap_or(HirType::Unknown);

        let init_expr = d.init.as_deref().map(|e| lower_swc_expr(e, scope));

        let ty = if annotated_ty != HirType::Unknown {
            annotated_ty.clone()
        } else if let Some(ref init) = init_expr {
            refine_type_from_init(init, scope)
        } else {
            HirType::Unknown
        };

        scope.define(&name, ty.clone());

        match vd.kind {
            swc::VarDeclKind::Const => {
                let init = init_expr.unwrap_or_else(|| {
                    HirExpr::new(HirExprKind::Lit(HirLit::Undefined), HirType::Any)
                });
                HirStmt::Const { name, ty, init }
            }
            _ => HirStmt::Let { name, ty, init: init_expr },
        }
    } else {
        // Multi-declarator: emit as a block of individual lets
        let stmts = vd.decls.iter().map(|d| {
            let name = extract_swc_pat_name(&d.name);
            let init_expr = d.init.as_deref().map(|e| lower_swc_expr(e, scope));
            let ty = if let Some(ref init) = init_expr {
                refine_type_from_init(init, scope)
            } else {
                HirType::Unknown
            };
            scope.define(&name, ty.clone());
            match vd.kind {
                swc::VarDeclKind::Const => {
                    let init = init_expr.unwrap_or_else(|| {
                        HirExpr::new(HirExprKind::Lit(HirLit::Undefined), HirType::Any)
                    });
                    HirStmt::Const { name, ty, init }
                }
                _ => HirStmt::Let { name, ty, init: init_expr },
            }
        }).collect();
        HirStmt::Block(stmts)
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

pub fn lower_swc_expr(e: &swc::Expr, scope: &Scope) -> HirExpr {
    match e {
        swc::Expr::Lit(lit) => lower_lit(lit),

        swc::Expr::Ident(id) => {
            let name = id.sym.to_string();
            let ty = scope.lookup(&name).cloned().unwrap_or(HirType::Unknown);
            HirExpr::new(HirExprKind::Ident(name), ty)
        }

        swc::Expr::Bin(bin) => {
            let op = swc_bin_op_to_hir(bin.op);
            let lhs = Box::new(lower_swc_expr(&bin.left, scope));
            let rhs = Box::new(lower_swc_expr(&bin.right, scope));
            let ty = if op.is_comparison() {
                HirType::Bool
            } else {
                let lt = lhs.ty.clone();
                let rt = rhs.ty.clone();
                crate::type_refine::numeric_promotion(lt, rt)
            };
            HirExpr::new(HirExprKind::Bin { op, lhs, rhs }, ty)
        }

        swc::Expr::Unary(u) => {
            let op = swc_unary_op_to_hir(u.op);
            let operand = Box::new(lower_swc_expr(&u.arg, scope));
            let ty = match op {
                HirUnOp::Not => HirType::Bool,
                HirUnOp::Plus => HirType::Number,
                HirUnOp::Neg => operand.ty.clone(),
                HirUnOp::BitNot => {
                    if operand.ty.is_integer() { operand.ty.clone() } else { HirType::I64 }
                }
                HirUnOp::TypeOf => HirType::Str,
                _ => HirType::Any,
            };
            HirExpr::new(HirExprKind::Unary { op, operand }, ty)
        }

        swc::Expr::Assign(a) => {
            let target = Box::new(lower_swc_pat_or_expr(&a.left, scope));
            let value = Box::new(lower_swc_expr(&a.right, scope));
            let ty = value.ty.clone();
            if a.op == swc::AssignOp::Assign {
                HirExpr::new(HirExprKind::Assign { target, value }, ty)
            } else {
                let op = swc_assign_op_to_hir(a.op);
                HirExpr::new(HirExprKind::AssignOp { op, target, value }, ty)
            }
        }

        swc::Expr::Call(call) => lower_call(call, scope),

        swc::Expr::New(n) => {
            let class = expr_to_ident_name(&n.callee).unwrap_or_default();
            let args = n.args.as_deref().unwrap_or(&[]).iter()
                .map(|a| {
                    let inner = lower_swc_expr(&a.expr, scope);
                    if a.spread.is_some() {
                        HirExpr::new(HirExprKind::Spread(Box::new(inner)), HirType::Any)
                    } else {
                        inner
                    }
                })
                .collect();
            let ty = scope.resolve_class(&class)
                .map(HirType::Class)
                .unwrap_or(HirType::Unknown);
            HirExpr::new(HirExprKind::New { class, args }, ty)
        }

        swc::Expr::Member(m) => {
            let object = Box::new(lower_swc_expr(&m.obj, scope));
            match &m.prop {
                swc::MemberProp::Computed(c) => {
                    let index = Box::new(lower_swc_expr(&c.expr, scope));
                    // arr[i] em handle de array herda o tipo do elemento
                    // (Array<I64> → I64, etc.) pra que numeric_promotion não
                    // promova indevidamente expressões mistas pra Number.
                    let elem_ty = match &object.ty {
                        HirType::Array(inner) => (**inner).clone(),
                        _ => HirType::Unknown,
                    };
                    HirExpr::new(HirExprKind::Index { object, index }, elem_ty)
                }
                prop => {
                    let name = member_prop_name(prop);
                    // arr.length sempre retorna I64
                    let prop_ty = if name == "length"
                        && matches!(object.ty, HirType::Array(_))
                    {
                        HirType::I64
                    } else {
                        HirType::Unknown
                    };
                    HirExpr::new(HirExprKind::Member { object, prop: name }, prop_ty)
                }
            }
        }

        swc::Expr::Array(arr) => {
            let elems: Vec<HirExpr> = arr.elems.iter()
                .filter_map(|e| e.as_ref())
                .map(|e| {
                    if e.spread.is_some() {
                        let inner = lower_swc_expr(&e.expr, scope);
                        HirExpr::new(HirExprKind::Spread(Box::new(inner)), HirType::Any)
                    } else {
                        lower_swc_expr(&e.expr, scope)
                    }
                })
                .collect();
            let elem_ty = elems.first().map(|e| e.ty.clone()).unwrap_or(HirType::Any);
            HirExpr::new(HirExprKind::Array(elems), HirType::Array(Box::new(elem_ty)))
        }

        swc::Expr::Object(obj) => {
            let fields: Vec<(String, HirExpr)> = obj.props.iter().filter_map(|p| {
                match p {
                    swc::PropOrSpread::Prop(prop) => {
                        match prop.as_ref() {
                            swc::Prop::KeyValue(kv) => {
                                let key = prop_name_to_str(&kv.key);
                                let val = lower_swc_expr(&kv.value, scope);
                                Some((key, val))
                            }
                            swc::Prop::Shorthand(id) => {
                                let name = id.sym.to_string();
                                let ty = scope.lookup(&name).cloned().unwrap_or(HirType::Unknown);
                                Some((name.clone(), HirExpr::new(HirExprKind::Ident(name), ty)))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }).collect();
            HirExpr::new(HirExprKind::Object(fields), HirType::Object)
        }

        swc::Expr::Cond(c) => {
            let cond = Box::new(lower_swc_expr(&c.test, scope));
            let then = Box::new(lower_swc_expr(&c.cons, scope));
            let else_ = Box::new(lower_swc_expr(&c.alt, scope));
            let ty = if then.ty == else_.ty { then.ty.clone() } else { HirType::Any };
            HirExpr::new(HirExprKind::Ternary { cond, then, else_ }, ty)
        }

        swc::Expr::Await(aw) => {
            let inner = Box::new(lower_swc_expr(&aw.arg, scope));
            let ty = inner.ty.clone();
            HirExpr::new(HirExprKind::Await(inner), ty)
        }

        swc::Expr::Paren(p) => lower_swc_expr(&p.expr, scope),

        swc::Expr::Update(u) => {
            let operand = Box::new(lower_swc_expr(&u.arg, scope));
            let ty = operand.ty.clone();
            let kind = match (u.op, u.prefix) {
                (swc::UpdateOp::PlusPlus, true)   => HirExprKind::PreInc(operand),
                (swc::UpdateOp::MinusMinus, true)  => HirExprKind::PreDec(operand),
                (swc::UpdateOp::PlusPlus, false)   => HirExprKind::PostInc(operand),
                (swc::UpdateOp::MinusMinus, false)  => HirExprKind::PostDec(operand),
            };
            HirExpr::new(kind, ty)
        }

        swc::Expr::Seq(s) => {
            let exprs: Vec<HirExpr> = s.exprs.iter().map(|e| lower_swc_expr(e, scope)).collect();
            let ty = exprs.last().map(|e| e.ty.clone()).unwrap_or(HirType::Void);
            HirExpr::new(HirExprKind::Seq(exprs), ty)
        }

        swc::Expr::Arrow(arrow) => {
            let params: Vec<HirParam> = arrow.params.iter().map(|p| {
                let name = extract_swc_pat_name(p);
                let annotation = extract_ts_type_annotation(p);
                let ty = annotation.as_deref().map(parse_type_annotation).unwrap_or(HirType::Unknown);
                HirParam { name, ty, variadic: false, has_default: false, optional: false, default_expr: None }
            }).collect();
            let ret = HirType::Unknown;
            let body = match &*arrow.body {
                swc::BlockStmtOrExpr::Expr(e) => {
                    let expr = lower_swc_expr(e, scope);
                    HirArrowBody::Expr(Box::new(expr))
                }
                swc::BlockStmtOrExpr::BlockStmt(b) => {
                    let mut inner_scope = Scope::new();
                    let stmts = lower_swc_stmts_block(b, &mut inner_scope);
                    HirArrowBody::Block(stmts)
                }
            };
            HirExpr::new(
                HirExprKind::Arrow { params, ret: ret.clone(), body },
                HirType::Function { params: vec![], ret: Box::new(ret) },
            )
        }

        // A `function (…) { … }` EXPRESSION (anonymous, named, or IIFE). It has the
        // same param/body shape as an arrow, so we lower it to an `Arrow` node and let
        // the engine's arrow-extraction machinery (lift / closure / bail) handle it
        // uniformly. `this`/`arguments`-dependent and async/generator forms are left
        // to the downstream bail (an arrow that references `this`/an unknown name is
        // not soundly liftable — never a wrong value). A NAMED function expression's
        // self-name is visible only inside its body; we do NOT bind it, so a
        // self-recursive named fn-expr surfaces the name as a free ident and bails
        // (sound) — a later increment can bind the self-name.
        swc::Expr::Fn(fn_expr) => {
            let func = &fn_expr.function;
            if func.is_async || func.is_generator {
                return HirExpr::new(HirExprKind::Raw(format!("{:?}", e)), HirType::Unknown);
            }
            let params: Vec<HirParam> = func.params.iter().map(|p| {
                let name = extract_swc_pat_name(&p.pat);
                let annotation = extract_ts_type_annotation(&p.pat);
                let ty = annotation.as_deref().map(parse_type_annotation).unwrap_or(HirType::Unknown);
                HirParam { name, ty, variadic: false, has_default: false, optional: false, default_expr: None }
            }).collect();
            let ret = HirType::Unknown;
            let body = match &func.body {
                Some(b) => {
                    let mut inner_scope = Scope::new();
                    HirArrowBody::Block(lower_swc_stmts_block(b, &mut inner_scope))
                }
                None => HirArrowBody::Block(Vec::new()),
            };
            HirExpr::new(
                HirExprKind::Arrow { params, ret: ret.clone(), body },
                HirType::Function { params: vec![], ret: Box::new(ret) },
            )
        }

        swc::Expr::Tpl(tpl) => {
            // Template literal: lower as Raw for now
            let _ = tpl;
            HirExpr::new(HirExprKind::Raw(format!("template_literal")), HirType::Str)
        }

        // TypeScript type-only expression wrappers carry ZERO runtime effect:
        // the value is just the inner expression. Erase the type and lower the
        // inner expr directly, instead of falling through to a `Raw` node that
        // the new engine cannot recognize.
        swc::Expr::TsAs(e) => lower_swc_expr(&e.expr, scope),
        swc::Expr::TsNonNull(e) => lower_swc_expr(&e.expr, scope),
        swc::Expr::TsConstAssertion(e) => lower_swc_expr(&e.expr, scope),
        swc::Expr::TsSatisfies(e) => lower_swc_expr(&e.expr, scope),
        swc::Expr::TsTypeAssertion(e) => lower_swc_expr(&e.expr, scope),

        _ => HirExpr::new(HirExprKind::Raw(format!("{:?}", e)), HirType::Unknown),
    }
}

// ---------------------------------------------------------------------------
// Call lowering
// ---------------------------------------------------------------------------

fn lower_call(call: &swc::CallExpr, scope: &Scope) -> HirExpr {
    // Preserve the spread flag on each argument: `f(...xs)` must be
    // distinguishable from `f(xs)`. A spread arg is wrapped in
    // `HirExprKind::Spread` (the same marker array literals use) so consumers
    // that don't yet model argument spread BAIL instead of miscompiling.
    let args: Vec<HirExpr> = call.args.iter().map(|a| {
        let inner = lower_swc_expr(&a.expr, scope);
        if a.spread.is_some() {
            HirExpr::new(HirExprKind::Spread(Box::new(inner)), HirType::Any)
        } else {
            inner
        }
    }).collect();

    match &call.callee {
        swc::Callee::Expr(callee_expr) => {
            match callee_expr.as_ref() {
                swc::Expr::Member(m) => {
                    let object = Box::new(lower_swc_expr(&m.obj, scope));
                    let method = member_prop_name(&m.prop);
                    HirExpr::new(
                        HirExprKind::MethodCall { object, method, args },
                        HirType::Unknown,
                    )
                }
                swc::Expr::Ident(id) => {
                    let name = id.sym.to_string();
                    let ret_ty = scope.return_type_of(&name).unwrap_or(HirType::Unknown);
                    let callee = Box::new(HirExpr::new(
                        HirExprKind::Ident(name.clone()),
                        scope.lookup(&name).cloned().unwrap_or(HirType::Unknown),
                    ));
                    HirExpr::new(HirExprKind::Call { callee, args }, ret_ty)
                }
                other => {
                    let callee = Box::new(lower_swc_expr(other, scope));
                    let ty = HirType::Unknown;
                    HirExpr::new(HirExprKind::Call { callee, args }, ty)
                }
            }
        }
        _ => {
            let callee = Box::new(HirExpr::new(HirExprKind::Raw("callee".into()), HirType::Unknown));
            HirExpr::new(HirExprKind::Call { callee, args }, HirType::Unknown)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn lower_swc_stmt_to_vec(stmt: &swc::Stmt, scope: &mut Scope) -> Vec<HirStmt> {
    match stmt {
        swc::Stmt::Block(b) => lower_swc_stmts_block(b, scope),
        other => vec![lower_swc_stmt(other, scope, "")],
    }
}

fn lower_swc_stmts_block(block: &swc::BlockStmt, scope: &mut Scope) -> Vec<HirStmt> {
    block.stmts.iter()
        .map(|s| lower_swc_stmt(s, scope, ""))
        .collect()
}

fn lower_swc_pat_or_expr(pat: &swc::AssignTarget, scope: &Scope) -> HirExpr {
    match pat {
        swc::AssignTarget::Simple(s) => match s {
            swc::SimpleAssignTarget::Ident(id) => {
                let name = id.id.sym.to_string();
                let ty = scope.lookup(&name).cloned().unwrap_or(HirType::Unknown);
                HirExpr::new(HirExprKind::Ident(name), ty)
            }
            swc::SimpleAssignTarget::Member(m) => {
                let object = Box::new(lower_swc_expr(&m.obj, scope));
                match &m.prop {
                    swc::MemberProp::Computed(c) => {
                        let index = Box::new(lower_swc_expr(&c.expr, scope));
                        HirExpr::new(HirExprKind::Index { object, index }, HirType::Unknown)
                    }
                    prop => {
                        let name = member_prop_name(prop);
                        HirExpr::new(HirExprKind::Member { object, prop: name }, HirType::Unknown)
                    }
                }
            }
            swc::SimpleAssignTarget::SuperProp(s) => {
                // `super.x = v`: lower the target to the SAME `Raw("SuperProp(…)")`
                // shape the READ path produces (`lower_swc_expr`'s `{:?}` fallback on
                // `Expr::SuperProp`), so `inherit::rewrite_super_expr` rewrites it to
                // `this.<name>` (the inherited field is an OWN slot in the flat shape).
                HirExpr::new(
                    HirExprKind::Raw(format!("SuperProp({s:?})")),
                    HirType::Unknown,
                )
            }
            _ => HirExpr::new(HirExprKind::Raw("assign_target".into()), HirType::Unknown),
        },
        _ => HirExpr::new(HirExprKind::Raw("assign_target_pat".into()), HirType::Unknown),
    }
}

fn lower_param(p: &Parameter, scope: &mut Scope) -> HirParam {
    let ty = p.type_annotation.as_deref()
        .map(parse_type_annotation)
        .unwrap_or(HirType::Unknown);
    scope.define(&p.name, ty.clone());
    // Lower the default initializer expr (`y = expr`) so the call site can lower
    // it for an omitted trailing arg. If it references a not-yet-defined name it
    // degrades to Unknown, which is fine.
    let default_expr = p.default.as_ref().map(|e| Box::new(lower_swc_expr(e, scope)));
    HirParam {
        name: p.name.clone(),
        ty,
        variadic: p.variadic,
        has_default: p.default.is_some(),
        optional: p.optional,
        default_expr,
    }
}

fn lower_lit(lit: &swc::Lit) -> HirExpr {
    match lit {
        swc::Lit::Num(n) => {
            let v = n.value;
            // If value is an integer and fits in i64, prefer HirLit::Int
            if v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                HirExpr::new(HirExprKind::Lit(HirLit::Int(v as i64)), HirType::I64)
            } else {
                HirExpr::new(HirExprKind::Lit(HirLit::Number(v)), HirType::Number)
            }
        }
        swc::Lit::Bool(b) => HirExpr::new(HirExprKind::Lit(HirLit::Bool(b.value)), HirType::Bool),
        swc::Lit::Str(s) => HirExpr::new(HirExprKind::Lit(HirLit::Str(s.value.to_string_lossy().into_owned())), HirType::Str),
        swc::Lit::Null(_) => HirExpr::new(HirExprKind::Lit(HirLit::Null), HirType::Any),
        swc::Lit::BigInt(_) => HirExpr::new(HirExprKind::Raw("bigint".into()), HirType::I64),
        swc::Lit::Regex(r) => {
            // Carry the pattern + flags in the Raw payload so the new engine can
            // recover them (the old engine / MIR match `Raw(_)` and ignore the
            // content, so this is backward-compatible). Encoding: NUL-separated
            // `regex\0<pattern>\0<flags>` — NUL never appears in a JS regex source.
            let payload = format!("regex\0{}\0{}", r.exp, r.flags);
            HirExpr::new(HirExprKind::Raw(payload), HirType::Handle(HandleKind::Regex))
        }
        swc::Lit::JSXText(_) => HirExpr::new(HirExprKind::Raw("jsx".into()), HirType::Str),
    }
}

fn extract_pat_binding(pat: &swc::ForHead) -> String {
    match pat {
        swc::ForHead::VarDecl(vd) => vd.decls.first()
            .map(|d| extract_swc_pat_name(&d.name))
            .unwrap_or_default(),
        swc::ForHead::Pat(p) => extract_swc_pat_name(p),
        swc::ForHead::UsingDecl(u) => u.decls.first()
            .map(|d| extract_swc_pat_name(&d.name))
            .unwrap_or_default(),
    }
}

fn extract_swc_pat_name(pat: &swc::Pat) -> String {
    match pat {
        swc::Pat::Ident(id) => id.id.sym.to_string(),
        swc::Pat::Assign(a) => extract_swc_pat_name(&a.left),
        swc::Pat::Rest(r) => extract_swc_pat_name(&r.arg),
        _ => "_".to_string(),
    }
}

fn extract_ts_type_annotation(pat: &swc::Pat) -> Option<String> {
    match pat {
        swc::Pat::Ident(id) => id.type_ann.as_ref().map(|ann| ts_type_to_str(&ann.type_ann)),
        _ => None,
    }
}

fn ts_type_to_str(ty: &swc::TsType) -> String {
    match ty {
        swc::TsType::TsKeywordType(kw) => match kw.kind {
            swc::TsKeywordTypeKind::TsNumberKeyword => "number".into(),
            swc::TsKeywordTypeKind::TsBooleanKeyword => "boolean".into(),
            swc::TsKeywordTypeKind::TsStringKeyword => "string".into(),
            swc::TsKeywordTypeKind::TsVoidKeyword => "void".into(),
            swc::TsKeywordTypeKind::TsNeverKeyword => "never".into(),
            swc::TsKeywordTypeKind::TsAnyKeyword => "any".into(),
            _ => "any".into(),
        },
        swc::TsType::TsTypeRef(r) => match &r.type_name {
            swc::TsEntityName::Ident(id) => id.sym.to_string(),
            _ => "any".into(),
        },
        // `T[]` em TS — array de elementos de tipo T. Mapeia recursivamente
        // pra string `T[]` que `parse_type_annotation` reconhece como
        // HirType::Array(parse_type_annotation(T)).
        swc::TsType::TsArrayType(arr) => {
            format!("{}[]", ts_type_to_str(&arr.elem_type))
        }
        // An object-type-literal `{ x: T; ... }` — a distinct keyed object type.
        // Encoded as the sentinel `{object}` that `parse_type_annotation` maps to
        // `HirType::Object`, so the new engine can tell an object param from a
        // primitive/`any`. (Field names/types are not captured here yet.)
        swc::TsType::TsTypeLit(_) => "{object}".into(),
        _ => "any".into(),
    }
}

/// Parse a TS type annotation string (from rts-parser) into a `HirType`.
pub fn parse_type_annotation(s: &str) -> HirType {
    match s.trim() {
        "number" => HirType::Number,
        "boolean" | "bool" => HirType::Bool,
        "string" => HirType::Str,
        "void" => HirType::Void,
        "any" => HirType::Any,
        // A keyed object type — `{ ... }` literal or the `object` keyword.
        "object" | "{object}" => HirType::Object,
        "never" => HirType::Unknown,
        "i8"  => HirType::I8,
        "i16" => HirType::I16,
        "i32" => HirType::I32,
        "i64" => HirType::I64,
        "i128" => HirType::I128,
        "u8"  => HirType::U8,
        "u16" => HirType::U16,
        "u32" => HirType::U32,
        "u64" => HirType::U64,
        "u128" => HirType::U128,
        "f32" => HirType::F32,
        "f64" => HirType::F64,
        _ if s.ends_with("[]") => {
            let inner = parse_type_annotation(&s[..s.len()-2]);
            HirType::Array(Box::new(inner))
        }
        _ => HirType::Unknown,
    }
}

fn swc_bin_op_to_hir(op: swc::BinaryOp) -> HirBinOp {
    match op {
        swc::BinaryOp::Add => HirBinOp::Add,
        swc::BinaryOp::Sub => HirBinOp::Sub,
        swc::BinaryOp::Mul => HirBinOp::Mul,
        swc::BinaryOp::Div => HirBinOp::Div,
        swc::BinaryOp::Mod => HirBinOp::Rem,
        swc::BinaryOp::Exp => HirBinOp::Exp,
        swc::BinaryOp::BitAnd => HirBinOp::BitAnd,
        swc::BinaryOp::BitOr => HirBinOp::BitOr,
        swc::BinaryOp::BitXor => HirBinOp::BitXor,
        swc::BinaryOp::LShift => HirBinOp::Shl,
        swc::BinaryOp::RShift => HirBinOp::Shr,
        swc::BinaryOp::ZeroFillRShift => HirBinOp::UShr,
        swc::BinaryOp::EqEq => HirBinOp::Eq,
        swc::BinaryOp::NotEq => HirBinOp::Ne,
        swc::BinaryOp::EqEqEq => HirBinOp::StrictEq,
        swc::BinaryOp::NotEqEq => HirBinOp::StrictNe,
        swc::BinaryOp::Lt => HirBinOp::Lt,
        swc::BinaryOp::LtEq => HirBinOp::Le,
        swc::BinaryOp::Gt => HirBinOp::Gt,
        swc::BinaryOp::GtEq => HirBinOp::Ge,
        swc::BinaryOp::LogicalAnd => HirBinOp::LogAnd,
        swc::BinaryOp::LogicalOr => HirBinOp::LogOr,
        swc::BinaryOp::NullishCoalescing => HirBinOp::NullCoalesce,
        // `key in obj` — own-property membership; the engine lowers it to a real
        // has-key check (distinct from the `Unsupported` sentinel).
        swc::BinaryOp::In => HirBinOp::In,
        // `instanceof`, etc. — handled by the engine's rhs-class-ident path off
        // `Unsupported`; an unmapped op lands here and the consumer BAILs.
        _ => HirBinOp::Unsupported,
    }
}

fn swc_unary_op_to_hir(op: swc::UnaryOp) -> HirUnOp {
    match op {
        swc::UnaryOp::Minus => HirUnOp::Neg,
        swc::UnaryOp::Plus => HirUnOp::Plus,
        swc::UnaryOp::Bang => HirUnOp::Not,
        swc::UnaryOp::Tilde => HirUnOp::BitNot,
        swc::UnaryOp::TypeOf => HirUnOp::TypeOf,
        swc::UnaryOp::Void => HirUnOp::Void,
        swc::UnaryOp::Delete => HirUnOp::Delete,
    }
}

fn swc_assign_op_to_hir(op: swc::AssignOp) -> HirBinOp {
    match op {
        swc::AssignOp::AddAssign => HirBinOp::Add,
        swc::AssignOp::SubAssign => HirBinOp::Sub,
        swc::AssignOp::MulAssign => HirBinOp::Mul,
        swc::AssignOp::DivAssign => HirBinOp::Div,
        swc::AssignOp::ModAssign => HirBinOp::Rem,
        swc::AssignOp::BitAndAssign => HirBinOp::BitAnd,
        swc::AssignOp::BitOrAssign => HirBinOp::BitOr,
        swc::AssignOp::BitXorAssign => HirBinOp::BitXor,
        swc::AssignOp::LShiftAssign => HirBinOp::Shl,
        swc::AssignOp::RShiftAssign => HirBinOp::Shr,
        swc::AssignOp::ZeroFillRShiftAssign => HirBinOp::UShr,
        swc::AssignOp::ExpAssign => HirBinOp::Exp,
        swc::AssignOp::AndAssign => HirBinOp::LogAnd,
        swc::AssignOp::OrAssign => HirBinOp::LogOr,
        swc::AssignOp::NullishAssign => HirBinOp::NullCoalesce,
        // `=` (plain Assign) never reaches here (handled separately); any other
        // compound op we don't model maps to `Unsupported` so consumers BAIL —
        // never silently to `Add` (the old bug: `**=`/`&&=` miscompiled as `+=`).
        _ => HirBinOp::Unsupported,
    }
}

fn member_prop_name(prop: &swc::MemberProp) -> String {
    match prop {
        swc::MemberProp::Ident(id) => id.sym.to_string(),
        swc::MemberProp::Computed(_) => format!("[computed]"),
        swc::MemberProp::PrivateName(p) => format!("#{}", p.name),
    }
}

fn expr_to_ident_name(expr: &swc::Expr) -> Option<String> {
    match expr {
        swc::Expr::Ident(id) => Some(id.sym.to_string()),
        // See through a type-only wrapper around the constructor name: `new (Foo as
        // any)()`, `new (Foo)()`, `new (Foo satisfies T)()`, `new (Foo!)()`,
        // `new (<any>Foo)()`. These are TS no-ops at runtime — the constructed class
        // is the inner ident. Without this the callee is not a bare `Ident`, the name
        // came back empty, and `new (Foo as any)()` bailed with `class ``.
        swc::Expr::Paren(p) => expr_to_ident_name(&p.expr),
        swc::Expr::TsAs(a) => expr_to_ident_name(&a.expr),
        swc::Expr::TsConstAssertion(a) => expr_to_ident_name(&a.expr),
        swc::Expr::TsSatisfies(a) => expr_to_ident_name(&a.expr),
        swc::Expr::TsNonNull(a) => expr_to_ident_name(&a.expr),
        swc::Expr::TsTypeAssertion(a) => expr_to_ident_name(&a.expr),
        _ => None,
    }
}

fn prop_name_to_str(key: &swc::PropName) -> String {
    match key {
        swc::PropName::Ident(id) => id.sym.to_string(),
        swc::PropName::Str(s) => s.value.to_string_lossy().into_owned(),
        swc::PropName::Num(n) => n.value.to_string(),
        swc::PropName::Computed(_) => "[computed]".into(),
        swc::PropName::BigInt(b) => b.value.to_string(),
    }
}
