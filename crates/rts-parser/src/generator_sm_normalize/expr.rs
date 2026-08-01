//! O normalizador de EXPRESSÃO: reescreve `e` empurrando para uma lista os
//! statements que precisam rodar antes, e devolve a expressão residual sem
//! `yield` em posição de subexpressão.
//!
//! Separado de [`super`] só por tamanho: aquele arquivo dirige STATEMENTS, este
//! desce na expressão. As duas regras que este arquivo existe para não violar —
//! curto-circuito e ordem de avaliação — estão explicadas no doc de [`super`].

use swc_ecma_ast::{
    AssignTarget, BinExpr, BinaryOp, Callee, Expr, ExprOrSpread, Lit, MemberExpr, MemberProp,
    PropOrSpread, SimpleAssignTarget, Stmt, UnaryOp,
};

use crate::generator_sm::{
    assign_stmt, call_stmt, expr_has_yield, ident_expr, let_decl, sp, undef_expr,
};

use super::temp;

/// Reescreve `e` empurrando para `out` os statements que precisam rodar antes,
/// e devolve a expressão RESIDUAL (sem `yield` em posição de subexpressão).
/// Uma subárvore sem `yield` volta intacta.
pub(super) fn norm_expr(e: &Expr, out: &mut Vec<Stmt>, ctr: &mut usize) -> Option<Expr> {
    if !expr_has_yield(e) {
        return Some(e.clone());
    }
    match e {
        Expr::Paren(p) => norm_expr(&p.expr, out, ctr),
        Expr::TsAs(a) => norm_expr(&a.expr, out, ctr),
        Expr::TsNonNull(a) => norm_expr(&a.expr, out, ctr),
        Expr::TsTypeAssertion(a) => norm_expr(&a.expr, out, ctr),
        Expr::TsConstAssertion(a) => norm_expr(&a.expr, out, ctr),

        // O próprio `yield`: vira uma LIGAÇÃO, que é a forma da máquina. Vale
        // igual para `yield*` — a máquina tem um arm para cada. O que difere é o
        // valor produzido (`yield` lê o `sent` da retomada; `yield*` lê o
        // `return` do delegado), e isso é decidido lá, não aqui.
        Expr::Yield(y) => {
            let arg = match y.arg.as_deref() {
                Some(a) if expr_has_yield(a) => Some(Box::new(norm_expr(a, out, ctr)?)),
                other => other.map(|a| Box::new(a.clone())),
            };
            let t = temp(ctr);
            let mut y2 = y.clone();
            y2.arg = arg;
            out.push(let_decl(&t, Expr::Yield(y2)));
            Some(ident_expr(&t))
        }

        Expr::Bin(b) => norm_bin(b, out, ctr),

        // `A ? (yield B) : C` — o yield precisa ficar DENTRO do ramo.
        Expr::Cond(c) => {
            let test = norm_expr(&c.test, out, ctr)?;
            if !expr_has_yield(&c.cons) && !expr_has_yield(&c.alt) {
                return Some(Expr::Cond(swc_ecma_ast::CondExpr {
                    span: sp(),
                    test: Box::new(test),
                    cons: c.cons.clone(),
                    alt: c.alt.clone(),
                }));
            }
            let t = temp(ctr);
            out.push(let_decl(&t, undef_expr()));
            let then_blk = branch_assign(&t, &c.cons, ctr)?;
            let else_blk = branch_assign(&t, &c.alt, ctr)?;
            out.push(Stmt::If(swc_ecma_ast::IfStmt {
                span: sp(),
                test: Box::new(test),
                cons: Box::new(then_blk),
                alt: Some(Box::new(else_blk)),
            }));
            Some(ident_expr(&t))
        }

        Expr::Unary(u) => {
            let arg = norm_expr(&u.arg, out, ctr)?;
            Some(Expr::Unary(swc_ecma_ast::UnaryExpr {
                span: sp(),
                op: u.op,
                arg: Box::new(arg),
            }))
        }

        // `(a, b, c)` — o valor é o ÚLTIMO, mas os anteriores TÊM de rodar.
        // Devolver só o último descartava os demais: `if (u = yield* g(), u)`
        // — a forma que o Babel emite, e que aparece no bundle real — perdia a
        // atribuição `u = …` e o teste lia `u` velho. Cada residual não-final
        // vira um statement de expressão, na ordem.
        Expr::Seq(s) => {
            let mut last = None;
            for (i, x) in s.exprs.iter().enumerate() {
                let v = norm_expr(x, out, ctr)?;
                if i + 1 < s.exprs.len() {
                    out.push(call_stmt(v));
                } else {
                    last = Some(v);
                }
            }
            last
        }

        // `obj.m(yield x)` NÃO pode virar `let t = obj.m; t(…)` — perderia o
        // `this`. O RECEPTOR é que vai para o temporário.
        Expr::Call(c) => {
            let Callee::Expr(callee) = &c.callee else {
                return None; // `super(…)` / `import(…)`
            };
            let callee = norm_callee(callee, out, ctr)?;
            let args = norm_args(&c.args, out, ctr)?;
            Some(Expr::Call(swc_ecma_ast::CallExpr {
                span: sp(),
                ctxt: Default::default(),
                callee: Callee::Expr(Box::new(callee)),
                args,
                type_args: None,
            }))
        }

        Expr::New(n) => {
            let callee = norm_callee(&n.callee, out, ctr)?;
            let args = match n.args.as_ref() {
                Some(a) => Some(norm_args(a, out, ctr)?),
                None => None,
            };
            Some(Expr::New(swc_ecma_ast::NewExpr {
                span: sp(),
                ctxt: Default::default(),
                callee: Box::new(callee),
                args,
                type_args: None,
            }))
        }

        Expr::Member(m) => {
            let obj = hoist(&m.obj, out, ctr)?;
            let prop = match &m.prop {
                MemberProp::Computed(c) => {
                    MemberProp::Computed(swc_ecma_ast::ComputedPropName {
                        span: sp(),
                        expr: Box::new(norm_expr(&c.expr, out, ctr)?),
                    })
                }
                other => other.clone(),
            };
            Some(Expr::Member(MemberExpr {
                span: sp(),
                obj: Box::new(obj),
                prop,
            }))
        }

        Expr::Array(a) => {
            let mut elems = Vec::with_capacity(a.elems.len());
            for el in &a.elems {
                match el {
                    Some(x) if x.spread.is_none() => {
                        elems.push(Some(ExprOrSpread {
                            spread: None,
                            expr: Box::new(hoist(&x.expr, out, ctr)?),
                        }));
                    }
                    // Buraco esparso: preservado.
                    None => elems.push(None),
                    // Spread junto de yield: fora desta fatia.
                    Some(_) => return None,
                }
            }
            Some(Expr::Array(swc_ecma_ast::ArrayLit { span: sp(), elems }))
        }

        Expr::Object(o) => {
            let mut props = Vec::with_capacity(o.props.len());
            for p in &o.props {
                let PropOrSpread::Prop(prop) = p else {
                    return None; // spread
                };
                let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_ref() else {
                    return None; // getter/setter/method/shorthand computado
                };
                let mut kv2 = kv.clone();
                kv2.value = Box::new(hoist(&kv.value, out, ctr)?);
                props.push(PropOrSpread::Prop(Box::new(swc_ecma_ast::Prop::KeyValue(
                    kv2,
                ))));
            }
            Some(Expr::Object(swc_ecma_ast::ObjectLit { span: sp(), props }))
        }

        Expr::Tpl(t) => {
            let mut exprs = Vec::with_capacity(t.exprs.len());
            for x in &t.exprs {
                exprs.push(Box::new(hoist(x, out, ctr)?));
            }
            let mut t2 = t.clone();
            t2.exprs = exprs;
            Some(Expr::Tpl(t2))
        }

        // `x = <algo com yield>`: o alvo não pode conter yield (seria uma
        // posição de escrita suspensa); o RHS normaliza.
        Expr::Assign(a) => {
            if assign_target_has_yield(&a.left) {
                return None;
            }
            let right = norm_expr(&a.right, out, ctr)?;
            Some(Expr::Assign(swc_ecma_ast::AssignExpr {
                span: sp(),
                op: a.op,
                left: a.left.clone(),
                right: Box::new(right),
            }))
        }

        // `await`, `?.`, tagged template, etc. com yield dentro: recusa.
        _ => None,
    }
}

/// Operadores binários. Os curto-circuitantes viram ramos (o `yield` só executa
/// se o operando esquerdo permitir); os demais preservam a ordem içando o
/// esquerdo para um temporário quando o direito suspende.
fn norm_bin(b: &BinExpr, out: &mut Vec<Stmt>, ctr: &mut usize) -> Option<Expr> {
    let short_circuit = matches!(
        b.op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
    );
    if short_circuit && expr_has_yield(&b.right) {
        let left = norm_expr(&b.left, out, ctr)?;
        let t = temp(ctr);
        out.push(let_decl(&t, left));
        let test = match b.op {
            BinaryOp::LogicalAnd => ident_expr(&t),
            BinaryOp::LogicalOr => not(ident_expr(&t)),
            // `??`: o direito roda só quando o esquerdo é null OU undefined.
            _ => Expr::Bin(BinExpr {
                span: sp(),
                op: BinaryOp::LogicalOr,
                left: Box::new(eq_null(&t)),
                right: Box::new(eq_undefined(&t)),
            }),
        };
        let blk = branch_assign(&t, &b.right, ctr)?;
        out.push(Stmt::If(swc_ecma_ast::IfStmt {
            span: sp(),
            test: Box::new(test),
            cons: Box::new(blk),
            alt: None,
        }));
        return Some(ident_expr(&t));
    }
    // Operador comum. Se o DIREITO suspende, o esquerdo tem de ir para um
    // temporário — deixá-lo na residual o faria rodar depois da suspensão.
    if expr_has_yield(&b.right) {
        let left = hoist(&b.left, out, ctr)?;
        let right = norm_expr(&b.right, out, ctr)?;
        return Some(Expr::Bin(BinExpr {
            span: sp(),
            op: b.op,
            left: Box::new(left),
            right: Box::new(right),
        }));
    }
    // Só o esquerdo suspende: o direito já é avaliado depois dele em JS, então
    // pode ficar inline.
    let left = norm_expr(&b.left, out, ctr)?;
    Some(Expr::Bin(BinExpr {
        span: sp(),
        op: b.op,
        left: Box::new(left),
        right: b.right.clone(),
    }))
}

/// Normaliza `e` e o fixa num temporário, para que sua avaliação aconteça ANTES
/// do que vier depois. Um literal ou identificador já é estável e fica inline
/// (nenhum temporário à toa, nenhum slot de frame extra).
fn hoist(e: &Expr, out: &mut Vec<Stmt>, ctr: &mut usize) -> Option<Expr> {
    let v = norm_expr(e, out, ctr)?;
    if matches!(v, Expr::Ident(_) | Expr::Lit(_) | Expr::This(_)) {
        return Some(v);
    }
    let t = temp(ctr);
    out.push(let_decl(&t, v));
    Some(ident_expr(&t))
}

/// O callee de uma chamada. Um `obj.m` mantém a FORMA de acesso a membro (o
/// `this` da chamada vem dela); só o receptor vai para um temporário. Um callee
/// simples fica como está, o que também mantém a resolução estática do motor.
fn norm_callee(callee: &Expr, out: &mut Vec<Stmt>, ctr: &mut usize) -> Option<Expr> {
    match callee {
        Expr::Member(m) => {
            let obj = hoist(&m.obj, out, ctr)?;
            let prop = match &m.prop {
                MemberProp::Computed(c) => {
                    MemberProp::Computed(swc_ecma_ast::ComputedPropName {
                        span: sp(),
                        expr: Box::new(hoist(&c.expr, out, ctr)?),
                    })
                }
                other => other.clone(),
            };
            Some(Expr::Member(MemberExpr {
                span: sp(),
                obj: Box::new(obj),
                prop,
            }))
        }
        other if !expr_has_yield(other) => Some(other.clone()),
        other => hoist(other, out, ctr),
    }
}

/// Argumentos, da esquerda para a direita, cada um fixado num temporário para
/// que a ordem sobreviva à suspensão. Um `spread` junto de `yield` recusa.
fn norm_args(
    args: &[ExprOrSpread],
    out: &mut Vec<Stmt>,
    ctr: &mut usize,
) -> Option<Vec<ExprOrSpread>> {
    let mut done = Vec::with_capacity(args.len());
    for a in args {
        if a.spread.is_some() {
            return None;
        }
        done.push(ExprOrSpread {
            spread: None,
            expr: Box::new(hoist(&a.expr, out, ctr)?),
        });
    }
    Some(done)
}

/// `{ …statements de `e`…; <t> = <residual de e>; }` — o bloco de um ramo que
/// produz valor.
fn branch_assign(t: &str, e: &Expr, ctr: &mut usize) -> Option<Stmt> {
    let mut inner = Vec::new();
    let v = norm_expr(e, &mut inner, ctr)?;
    inner.push(assign_stmt(t, v));
    Some(Stmt::Block(swc_ecma_ast::BlockStmt {
        span: sp(),
        ctxt: Default::default(),
        stmts: inner,
    }))
}

fn not(e: Expr) -> Expr {
    Expr::Unary(swc_ecma_ast::UnaryExpr {
        span: sp(),
        op: UnaryOp::Bang,
        arg: Box::new(e),
    })
}

fn eq_null(t: &str) -> Expr {
    Expr::Bin(BinExpr {
        span: sp(),
        op: BinaryOp::EqEqEq,
        left: Box::new(ident_expr(t)),
        right: Box::new(Expr::Lit(Lit::Null(swc_ecma_ast::Null { span: sp() }))),
    })
}

fn eq_undefined(t: &str) -> Expr {
    Expr::Bin(BinExpr {
        span: sp(),
        op: BinaryOp::EqEqEq,
        left: Box::new(ident_expr(t)),
        right: Box::new(undef_expr()),
    })
}

fn assign_target_has_yield(t: &AssignTarget) -> bool {
    match t {
        AssignTarget::Simple(SimpleAssignTarget::Member(m)) => {
            expr_has_yield(&m.obj)
                || matches!(&m.prop, MemberProp::Computed(c) if expr_has_yield(&c.expr))
        }
        _ => false,
    }
}
