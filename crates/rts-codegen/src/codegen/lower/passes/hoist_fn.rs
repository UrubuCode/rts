//! Pass que hoista `Expr::Fn` / `Expr::Arrow` em posicoes arbitrarias
//! (call args, IIFE, valor em object literal, etc) para `Item::Function`
//! sintetico no topo do programa, substituindo a expressao por
//! `Expr::Ident`.
//!
//! Top-level `const x = function() {}` ja foi convertido pelo parser
//! (try_lower_fn_expr_decl); este pass cobre o que sobrou. Nao tenta
//! resolver capturas de escopo — se o body referenciar variaveis
//! fora dele, falha em codegen como ident undefined.

use swc_ecma_ast::{Callee, Decl, Expr, MemberProp, Pat, Stmt};

use crate::parser::ast::{
    ClassMember, FunctionDecl, Item, MemberModifiers, Parameter, Program, RawStmt, Statement,
};
use crate::parser::span::Span;

pub(crate) fn hoist_fn_expressions(program: &mut Program) {
    use swc_ecma_ast::{ArrowExpr, BlockStmtOrExpr};

    let mut counter: u32 = 0;
    let mut new_fns: Vec<Item> = Vec::new();

    fn fresh_ident(name: &str) -> Expr {
        Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: name.into(),
            optional: false,
        })
    }

    fn pat_to_param_name(p: &Pat) -> Option<String> {
        if let Pat::Ident(b) = p {
            Some(b.id.sym.to_string())
        } else {
            None
        }
    }

    /// (cross-runtime #294) Detecta se body retorna expr concat com algum
    /// operando String literal ou Tpl. Heuristica simples — cobre arrows
    /// como `(x) => x.status + ":" + x.value`.
    fn body_returns_string_concat(stmts: &[Stmt]) -> bool {
        use swc_ecma_ast::BinaryOp;
        fn expr_yields_string(e: &Expr) -> bool {
            match e {
                Expr::Lit(swc_ecma_ast::Lit::Str(_)) => true,
                Expr::Tpl(_) => true,
                Expr::Bin(b) if b.op == BinaryOp::Add => {
                    expr_yields_string(&b.left) || expr_yields_string(&b.right)
                }
                Expr::Paren(p) => expr_yields_string(&p.expr),
                // (cross-runtime) ternary (incl. encadeado) cujos ramos
                // produzem string: `n>=90?"A":n>=80?"B":"C"`. Sem isto, arrow
                // hoisted sem anotacao nao infere return string e o handle
                // sai como numero cru. Espelha inspect_return_kind (program.rs).
                Expr::Cond(c) => {
                    expr_yields_string(&c.cons) && expr_yields_string(&c.alt)
                }
                _ => false,
            }
        }
        fn check_stmt(s: &Stmt) -> bool {
            match s {
                Stmt::Return(r) => r.arg.as_deref().is_some_and(expr_yields_string),
                Stmt::Block(b) => b.stmts.iter().any(check_stmt),
                Stmt::If(i) => {
                    check_stmt(&i.cons) || i.alt.as_deref().is_some_and(check_stmt)
                }
                _ => false,
            }
        }
        stmts.iter().any(check_stmt)
    }

    /// (cross-runtime #300) Detecta se body retorna expr bool: literal,
    /// comparison, logical, or unary not.
    fn body_returns_bool(stmts: &[Stmt]) -> bool {
        use swc_ecma_ast::BinaryOp;
        fn expr_yields_bool(e: &Expr) -> bool {
            match e {
                Expr::Lit(swc_ecma_ast::Lit::Bool(_)) => true,
                Expr::Bin(b) => matches!(
                    b.op,
                    BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
                    | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
                    | BinaryOp::In | BinaryOp::InstanceOf
                ),
                Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Bang) => true,
                Expr::Paren(p) => expr_yields_bool(&p.expr),
                _ => false,
            }
        }
        fn check_stmt(s: &Stmt) -> bool {
            match s {
                Stmt::Return(r) => r.arg.as_deref().is_some_and(expr_yields_bool),
                Stmt::Block(b) => b.stmts.iter().any(check_stmt),
                Stmt::If(i) => check_stmt(&i.cons) || i.alt.as_deref().is_some_and(check_stmt),
                _ => false,
            }
        }
        stmts.iter().any(check_stmt)
    }

    fn body_has_return_value(stmts: &[Stmt]) -> bool {
        for s in stmts {
            if has_return_value_stmt(s) {
                return true;
            }
        }
        false
    }
    fn has_return_value_stmt(s: &Stmt) -> bool {
        match s {
            Stmt::Return(r) => r.arg.is_some(),
            Stmt::Block(b) => body_has_return_value(&b.stmts),
            Stmt::If(i) => {
                has_return_value_stmt(&i.cons)
                    || i.alt.as_deref().map_or(false, has_return_value_stmt)
            }
            Stmt::While(w) => has_return_value_stmt(&w.body),
            Stmt::DoWhile(w) => has_return_value_stmt(&w.body),
            Stmt::For(f) => has_return_value_stmt(&f.body),
            Stmt::ForOf(f) => has_return_value_stmt(&f.body),
            Stmt::ForIn(f) => has_return_value_stmt(&f.body),
            Stmt::Try(t) => {
                body_has_return_value(&t.block.stmts)
                    || t.handler
                        .as_ref()
                        .map_or(false, |h| body_has_return_value(&h.body.stmts))
                    || t.finalizer
                        .as_ref()
                        .map_or(false, |f| body_has_return_value(&f.stmts))
            }
            _ => false,
        }
    }

    fn build_fn_decl(
        name: &str,
        params: &[swc_ecma_ast::Param],
        body_stmts: Vec<Stmt>,
    ) -> FunctionDecl {
        build_fn_decl_async(name, params, body_stmts, false)
    }

    fn build_fn_decl_async(
        name: &str,
        params: &[swc_ecma_ast::Param],
        body_stmts: Vec<Stmt>,
        is_async: bool,
    ) -> FunctionDecl {
        // (#object_builtins) Suporte a destructuring em params de arrow/fn
        // hoisted: \`forEach(([k,v]) => ...)\`. Geramos param sintetico por
        // posicao e prepend \`const [k,v] = __destruct_param_N;\` ao body.
        let mut prologue: Vec<Stmt> = Vec::new();
        let mut parameters: Vec<Parameter> = Vec::with_capacity(params.len());
        for (i, p) in params.iter().enumerate() {
            let pat_for_destruct: Option<&Pat> = match &p.pat {
                Pat::Array(_) | Pat::Object(_) => Some(&p.pat),
                Pat::Assign(a) if matches!(a.left.as_ref(), Pat::Array(_) | Pat::Object(_)) => {
                    Some(a.left.as_ref())
                }
                _ => None,
            };
            if let Some(pat) = pat_for_destruct {
                let synth_name = format!("__hoist_destruct_{}_{}", name, i);
                // const <pat> = <synth>;
                let synth_ident = swc_ecma_ast::Ident::new(
                    synth_name.as_str().into(),
                    Default::default(),
                    Default::default(),
                );
                let init = Expr::Ident(synth_ident);
                let var_decl = swc_ecma_ast::VarDecl {
                    span: Default::default(),
                    ctxt: Default::default(),
                    kind: swc_ecma_ast::VarDeclKind::Const,
                    declare: false,
                    decls: vec![swc_ecma_ast::VarDeclarator {
                        span: Default::default(),
                        name: pat.clone(),
                        init: Some(Box::new(init)),
                        definite: false,
                    }],
                };
                prologue.push(Stmt::Decl(swc_ecma_ast::Decl::Var(Box::new(var_decl))));
                parameters.push(Parameter {
                    name: synth_name,
                    type_annotation: None,
                    modifiers: MemberModifiers::default(),
                    variadic: false,
                    default: None,
                    span: Span::default(),
                });
            } else if let Pat::Rest(rest) = &p.pat {
                // (cross-runtime #294) Rest param em arrow/fn: marca variadic
                // pra que expand_rest_args reescreva acessos como `rest[i]`
                // via arguments slicing.
                if let Some(n) = pat_to_param_name(&rest.arg) {
                    parameters.push(Parameter {
                        name: n,
                        type_annotation: None,
                        modifiers: MemberModifiers::default(),
                        variadic: true,
                        default: None,
                        span: Span::default(),
                    });
                }
            } else if let Some(n) = pat_to_param_name(&p.pat) {
                // (cross-runtime #341) `function(this: any, ...args)` — TS
                // permite anotar o tipo do `this` implicito como primeiro
                // pseudo-param. Em RTS, `this` eh thread-local slot, nao
                // parametro. Filtrar evita que callers passem `obj` como
                // valor de `this` e o body leia args+1 (lixo) em vez de
                // THIS_GET correto.
                if n == "this" {
                    continue;
                }
                parameters.push(Parameter {
                    name: n,
                    type_annotation: None,
                    modifiers: MemberModifiers::default(),
                    variadic: false,
                    default: None,
                    span: Span::default(),
                });
            }
        }
        // Heuristica: se body tem \`return <expr>\`, declaramos retorno
        // i64. Sem isso, declare_user_fn vira void e o codegen descarta
        // o valor retornado, quebrando \`apply(fn, x)\` e IIFE com retorno.
        // (cross-runtime #294) Se body retorna string concat
        // (\`a + ":" + b\`), inferir handle (string) — sem isso, arrow
        // como `.map(x => x.k + ":" + x.v)` retorna handle bruto.
        let return_type = if body_has_return_value(&body_stmts) {
            if body_returns_string_concat(&body_stmts) {
                Some("string".to_string())
            } else if body_returns_bool(&body_stmts) {
                Some("boolean".to_string())
            } else {
                Some("i64".to_string())
            }
        } else {
            None
        };
        let mut all_stmts: Vec<Stmt> = prologue;
        all_stmts.extend(body_stmts);
        let body: Vec<Statement> = all_stmts
            .into_iter()
            .map(|s| {
                Statement::Raw(
                    RawStmt::new("<hoisted-fn>".to_string(), Span::default()).with_stmt(s),
                )
            })
            .collect();
        FunctionDecl {
            name: name.to_string(),
            parameters,
            return_type,
            body,
            span: Span::default(),
            is_async,
        }
    }

    fn arrow_body_stmts(arrow: &ArrowExpr) -> Vec<Stmt> {
        match arrow.body.as_ref() {
            BlockStmtOrExpr::BlockStmt(b) => b.stmts.clone(),
            BlockStmtOrExpr::Expr(e) => vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: arrow.span,
                arg: Some(e.clone()),
            })],
        }
    }

    fn arrow_params(arrow: &ArrowExpr) -> Vec<swc_ecma_ast::Param> {
        arrow
            .params
            .iter()
            .map(|pat| swc_ecma_ast::Param {
                span: Default::default(),
                decorators: Vec::new(),
                pat: pat.clone(),
            })
            .collect()
    }

    fn visit_expr(expr: &mut Expr, counter: &mut u32, new_fns: &mut Vec<Item>) {
        // Pre-order: primeiro visitamos sub-exprs (para que Fn/Arrow
        // aninhadas tambem sejam hoisted) e depois substituimos a
        // raiz se for Fn/Arrow.
        descend_expr(expr, counter, new_fns);

        // Peel \`(expr)\` wrappers — IIFE \`(function(){...})(...)\` apos
        // hoist vira \`(__hoisted_fn_N)(...)\` e o codegen so' reconhece
        // Expr::Ident como callee direto.
        loop {
            let take = matches!(expr, Expr::Paren(_));
            if !take {
                break;
            }
            if let Expr::Paren(p) = std::mem::replace(
                expr,
                Expr::Invalid(swc_ecma_ast::Invalid {
                    span: Default::default(),
                }),
            ) {
                *expr = *p.expr;
            }
        }

        let replace = match expr {
            Expr::Fn(_) | Expr::Arrow(_) => true,
            _ => false,
        };
        if !replace {
            return;
        }

        let owned = std::mem::replace(
            expr,
            Expr::Invalid(swc_ecma_ast::Invalid {
                span: Default::default(),
            }),
        );
        let is_arrow_expr = matches!(owned, Expr::Arrow(_));
        let name = if is_arrow_expr {
            format!("__hoisted_arrow_{}", *counter)
        } else {
            format!("__hoisted_fn_{}", *counter)
        };
        *counter += 1;

        let fn_decl = match owned {
            Expr::Fn(fn_expr) => {
                let func = &fn_expr.function;
                let body = func.body.as_ref().map(|b| b.stmts.clone()).unwrap_or_default();
                build_fn_decl(&name, &func.params, body)
            }
            Expr::Arrow(arrow) => {
                let params = arrow_params(&arrow);
                let body = arrow_body_stmts(&arrow);
                build_fn_decl(&name, &params, body)
            }
            _ => unreachable!(),
        };
        new_fns.push(Item::Function(fn_decl));
        *expr = fresh_ident(&name);
    }

    fn descend_expr(expr: &mut Expr, counter: &mut u32, new_fns: &mut Vec<Item>) {
        match expr {
            Expr::Call(c) => {
                if let Callee::Expr(callee) = &mut c.callee {
                    visit_expr(callee.as_mut(), counter, new_fns);
                }
                for arg in c.args.iter_mut() {
                    visit_expr(arg.expr.as_mut(), counter, new_fns);
                }
            }
            Expr::New(n) => {
                visit_expr(n.callee.as_mut(), counter, new_fns);
                if let Some(args) = n.args.as_mut() {
                    for arg in args.iter_mut() {
                        visit_expr(arg.expr.as_mut(), counter, new_fns);
                    }
                }
            }
            Expr::Bin(b) => {
                visit_expr(b.left.as_mut(), counter, new_fns);
                visit_expr(b.right.as_mut(), counter, new_fns);
            }
            Expr::Assign(a) => {
                visit_expr(a.right.as_mut(), counter, new_fns);
            }
            Expr::Unary(u) => visit_expr(u.arg.as_mut(), counter, new_fns),
            Expr::Update(u) => visit_expr(u.arg.as_mut(), counter, new_fns),
            Expr::Cond(c) => {
                visit_expr(c.test.as_mut(), counter, new_fns);
                visit_expr(c.cons.as_mut(), counter, new_fns);
                visit_expr(c.alt.as_mut(), counter, new_fns);
            }
            Expr::Seq(s) => {
                for e in s.exprs.iter_mut() {
                    visit_expr(e.as_mut(), counter, new_fns);
                }
            }
            Expr::Member(m) => {
                visit_expr(m.obj.as_mut(), counter, new_fns);
                if let MemberProp::Computed(c) = &mut m.prop {
                    visit_expr(c.expr.as_mut(), counter, new_fns);
                }
            }
            Expr::OptChain(o) => {
                use swc_ecma_ast::OptChainBase;
                match o.base.as_mut() {
                    OptChainBase::Member(m) => {
                        visit_expr(m.obj.as_mut(), counter, new_fns);
                    }
                    OptChainBase::Call(c) => {
                        visit_expr(c.callee.as_mut(), counter, new_fns);
                        for arg in c.args.iter_mut() {
                            visit_expr(arg.expr.as_mut(), counter, new_fns);
                        }
                    }
                }
            }
            Expr::Array(arr) => {
                for elem in arr.elems.iter_mut().flatten() {
                    visit_expr(elem.expr.as_mut(), counter, new_fns);
                }
            }
            Expr::Object(obj) => {
                for prop in obj.props.iter_mut() {
                    if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_mut() {
                            visit_expr(kv.value.as_mut(), counter, new_fns);
                        }
                    }
                }
            }
            Expr::Tpl(t) => {
                for e in t.exprs.iter_mut() {
                    visit_expr(e.as_mut(), counter, new_fns);
                }
            }
            Expr::Paren(p) => visit_expr(p.expr.as_mut(), counter, new_fns),
            Expr::TsAs(a) => visit_expr(a.expr.as_mut(), counter, new_fns),
            Expr::TsTypeAssertion(a) => visit_expr(a.expr.as_mut(), counter, new_fns),
            Expr::TsConstAssertion(a) => visit_expr(a.expr.as_mut(), counter, new_fns),
            Expr::TsSatisfies(a) => visit_expr(a.expr.as_mut(), counter, new_fns),
            Expr::TsNonNull(n) => visit_expr(n.expr.as_mut(), counter, new_fns),
            Expr::Await(a) => visit_expr(a.arg.as_mut(), counter, new_fns),
            Expr::Yield(y) => {
                if let Some(e) = y.arg.as_mut() {
                    visit_expr(e.as_mut(), counter, new_fns);
                }
            }
            // Fn/Arrow: nao descer dentro do body — body sera tratado
            // quando o pass rodar de novo recursivamente nos new_fns.
            // Aqui apenas paramos. Se body tiver mais lambdas, elas
            // sao hoisted no proximo loop fixed-point.
            _ => {}
        }
    }

    fn visit_stmt(stmt: &mut Stmt, counter: &mut u32, new_fns: &mut Vec<Item>) {
        match stmt {
            Stmt::Expr(e) => visit_expr(e.expr.as_mut(), counter, new_fns),
            Stmt::Return(r) => {
                if let Some(e) = r.arg.as_mut() {
                    visit_expr(e.as_mut(), counter, new_fns);
                }
            }
            Stmt::Decl(Decl::Var(vd)) => {
                for d in vd.decls.iter_mut() {
                    if let Some(init) = d.init.as_mut() {
                        visit_expr(init.as_mut(), counter, new_fns);
                    }
                }
            }
            Stmt::Decl(Decl::Fn(_)) => {
                // Nested function declaration: hoist para Item::Function
                // top-level mantendo o nome original (assume unicidade no
                // programa) e substitui o stmt por Empty. Sub-stmts no body
                // sao re-processados quando o pass roda em fixed-point sobre
                // new_fns.
                let owned = std::mem::replace(
                    stmt,
                    Stmt::Empty(swc_ecma_ast::EmptyStmt {
                        span: Default::default(),
                    }),
                );
                if let Stmt::Decl(Decl::Fn(fn_decl)) = owned {
                    let name = fn_decl.ident.sym.to_string();
                    let func = &fn_decl.function;
                    let body = func
                        .body
                        .as_ref()
                        .map(|b| b.stmts.clone())
                        .unwrap_or_default();
                    let decl = build_fn_decl_async(&name, &func.params, body, func.is_async);
                    new_fns.push(Item::Function(decl));
                }
            }
            Stmt::Block(b) => {
                for s in b.stmts.iter_mut() {
                    visit_stmt(s, counter, new_fns);
                }
            }
            Stmt::If(i) => {
                visit_expr(i.test.as_mut(), counter, new_fns);
                visit_stmt(i.cons.as_mut(), counter, new_fns);
                if let Some(alt) = i.alt.as_mut() {
                    visit_stmt(alt.as_mut(), counter, new_fns);
                }
            }
            Stmt::While(w) => {
                visit_expr(w.test.as_mut(), counter, new_fns);
                visit_stmt(w.body.as_mut(), counter, new_fns);
            }
            Stmt::DoWhile(d) => {
                visit_expr(d.test.as_mut(), counter, new_fns);
                visit_stmt(d.body.as_mut(), counter, new_fns);
            }
            Stmt::For(f) => {
                if let Some(init) = f.init.as_mut() {
                    if let swc_ecma_ast::VarDeclOrExpr::Expr(e) = init {
                        visit_expr(e.as_mut(), counter, new_fns);
                    }
                    if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                        for d in vd.decls.iter_mut() {
                            if let Some(init) = d.init.as_mut() {
                                visit_expr(init.as_mut(), counter, new_fns);
                            }
                        }
                    }
                }
                if let Some(t) = f.test.as_mut() {
                    visit_expr(t.as_mut(), counter, new_fns);
                }
                if let Some(u) = f.update.as_mut() {
                    visit_expr(u.as_mut(), counter, new_fns);
                }
                visit_stmt(f.body.as_mut(), counter, new_fns);
            }
            Stmt::ForOf(f) => {
                visit_expr(f.right.as_mut(), counter, new_fns);
                visit_stmt(f.body.as_mut(), counter, new_fns);
            }
            Stmt::ForIn(f) => {
                visit_expr(f.right.as_mut(), counter, new_fns);
                visit_stmt(f.body.as_mut(), counter, new_fns);
            }
            Stmt::Switch(s) => {
                visit_expr(s.discriminant.as_mut(), counter, new_fns);
                for c in s.cases.iter_mut() {
                    if let Some(t) = c.test.as_mut() {
                        visit_expr(t.as_mut(), counter, new_fns);
                    }
                    for s in c.cons.iter_mut() {
                        visit_stmt(s, counter, new_fns);
                    }
                }
            }
            Stmt::Throw(t) => visit_expr(t.arg.as_mut(), counter, new_fns),
            Stmt::Try(t) => {
                for s in t.block.stmts.iter_mut() {
                    visit_stmt(s, counter, new_fns);
                }
                if let Some(handler) = t.handler.as_mut() {
                    for s in handler.body.stmts.iter_mut() {
                        visit_stmt(s, counter, new_fns);
                    }
                }
                if let Some(finalizer) = t.finalizer.as_mut() {
                    for s in finalizer.stmts.iter_mut() {
                        visit_stmt(s, counter, new_fns);
                    }
                }
            }
            _ => {}
        }
    }

    // Iterate ate fixed-point: visitar items, hoistar, e depois
    // re-visitar new_fns (que podem conter outras Fn/Arrow aninhadas).
    let mut to_visit_items: Vec<&mut Item> = program.items.iter_mut().collect();
    for item in to_visit_items.drain(..) {
        match item {
            Item::Function(f) => {
                for s in f.body.iter_mut() {
                    let Statement::Raw(raw) = s;
                    if let Some(stmt) = raw.stmt.as_mut() {
                        visit_stmt(stmt, &mut counter, &mut new_fns);
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    visit_stmt(stmt, &mut counter, &mut new_fns);
                }
            }
            Item::Class(class) => {
                for member in class.members.iter_mut() {
                    let body = match member {
                        ClassMember::Method(m) => &mut m.body,
                        ClassMember::Constructor(c) => &mut c.body,
                        _ => continue,
                    };
                    for s in body.iter_mut() {
                        let Statement::Raw(raw) = s;
                        if let Some(stmt) = raw.stmt.as_mut() {
                            visit_stmt(stmt, &mut counter, &mut new_fns);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Process new_fns recursivamente (lambdas dentro de lambdas).
    let mut work = std::mem::take(&mut new_fns);
    let mut acc: Vec<Item> = Vec::new();
    while let Some(item) = work.pop() {
        if let Item::Function(mut f) = item {
            for s in f.body.iter_mut() {
                let Statement::Raw(raw) = s;
                if let Some(stmt) = raw.stmt.as_mut() {
                    let mut local_new: Vec<Item> = Vec::new();
                    visit_stmt(stmt, &mut counter, &mut local_new);
                    work.extend(local_new);
                }
            }
            acc.push(Item::Function(f));
        } else {
            acc.push(item);
        }
    }
    program.items.extend(acc);
}
