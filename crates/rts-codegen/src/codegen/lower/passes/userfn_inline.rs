//! (RTS_INLINE_AST) Elegibilidade de inline de user-fn no call-site.
//!
//! O inline do MIR (`rts-mir/passes/inline.rs`) NUNCA toca o `__RTS_MAIN`
//! top-level (compilado 100% via AST). Loops quentes que chamam uma fn pequena
//! (ex: `pure(x)=(x*16807)%N` 25M×) pagam o overhead de call inteiro. Esta
//! analise marca fns pequenas e CONSERVADORAS para inline direto no lowering
//! AST do call-site — eliminando a call (2 externs STACK_PUSH/POP + diamante de
//! 4 blocos com phi).
//!
//! **Default-DENY.** Uma fn so' qualifica se TODAS as condicoes de
//! `inline_eligible` valem. Na menor duvida, NAO inlina (cai na call normal,
//! zero regressao por construcao). Gate por env `RTS_INLINE_AST` (default ON;
//! `=0` desliga = kill-switch).
//!
//! Ver `docs/specs/userfn-inline.md`.

use swc_ecma_ast::{Expr, Pat, Stmt};

use crate::codegen::lower::ctx::{InlineCandidate, ValTy};
use crate::parser::ast::{FunctionDecl, Statement};

/// Profundidade maxima de inline transitivo (a()->b()->c()). Conter code bloat
/// e recursao mutua. Lido em `lower_user_call`.
pub(crate) const MAX_INLINE_DEPTH: usize = 3;
/// Aridade maxima de uma fn elegivel.
pub(crate) const MAX_INLINE_ARITY: usize = 6;
/// Orcamento de custo (nos AST) do corpo. Acima disto nao inlina.
pub(crate) const INLINE_AST_BUDGET: usize = 12;

/// Tipo numerico aceito (param e ret). Sem Handle/string/U64 na v1 — strings
/// exigem gestao de handle/GC que o inline simples nao modela.
fn is_numeric(ty: ValTy) -> bool {
    matches!(
        ty,
        ValTy::I8
            | ValTy::I16
            | ValTy::I32
            | ValTy::I64
            | ValTy::U8
            | ValTy::U16
            | ValTy::Bool
            | ValTy::F64
    )
}

/// True quando a anotacao textual e' um ESCALAR puro (number/int/float/bool).
///
/// CRUCIAL: o `ValTy` da ABI colapsa muitos tipos logicos distintos em I64
/// (fn-type `(n)=>number`, `Map<...>`, sem-anotacao, `any`, etc.). O codegen de
/// `compile_user_fn` faz tagging especial por param a partir da ANOTACAO crua
/// (`local_fn_ret_f64`, `local_map_vars`, `local_string_vars`, `local_array_
/// vars`, `var_member_call_values` para `any`/sem-anotacao, `generator_vars`).
/// O inline simples NAO replica esse tagging — entao um param tagged como
/// fn-type/Map/string/array/any inlinado vira um I64 cru e o corpo gera lixo
/// (ex: `f(n)` indireto devolve bits-f64 sem normalizar). Por isso so' aceitamos
/// params/ret cuja anotacao e' um escalar literal sem ambiguidade. Default-DENY:
/// anotacao ausente, fn-type, generico, ou qualquer coisa nao listada => nao
/// inlina (cai na call normal).
fn is_scalar_annotation(ann: Option<&str>) -> bool {
    let Some(a) = ann else {
        // Sem anotacao => param vira I64 ambiguo (var_member_call_values).
        // Inline perderia esse tagging. DENY.
        return false;
    };
    let a = a.trim();
    matches!(
        a,
        "number"
            | "i8" | "I8" | "i16" | "I16" | "i32" | "I32" | "i64" | "I64"
            | "u8" | "U8" | "u16" | "U16"
            | "f32" | "F32" | "f64" | "F64"
            | "boolean" | "bool"
    )
}

/// Tenta produzir um `InlineCandidate` para `fn_decl`. Retorna `None` quando a
/// fn nao e' elegivel (cai na call normal). CONSERVADORA — ver doc do modulo.
///
/// `params_ty` / `ret_ty` vem do `UserFnAbi` ja' resolvido (mesma fonte que o
/// call-site usa para coercao de args), garantindo consistencia de tipos.
pub(crate) fn inline_eligible(
    fn_decl: &FunctionDecl,
    params_ty: &[ValTy],
    ret_ty: Option<ValTy>,
) -> Option<InlineCandidate> {
    // (1) Nao-async.
    if fn_decl.is_async {
        return None;
    }

    // Nomes sinteticos liftados/hoistados carregam semantica especial de ABI
    // (capturas, handles ambiguos, this capturado) que o inline simples nao
    // modela — exclui por nome. So' fns "normais" de usuario qualificam.
    let n = fn_decl.name.as_str();
    if n.starts_with("__lifted_")
        || n.starts_with("__hoisted_")
        || n.starts_with("__captured_")
        || n.starts_with("__async_inner_")
        || n.starts_with("__class_")
    {
        return None;
    }

    // (3) Aridade <= MAX_INLINE_ARITY; sem variadic/default/this.
    if fn_decl.parameters.len() > MAX_INLINE_ARITY {
        return None;
    }
    if fn_decl.parameters.len() != params_ty.len() {
        return None;
    }
    for p in &fn_decl.parameters {
        // (2) sem param `this`.
        if p.name == "this" || p.name == "arguments" {
            return None;
        }
        if p.variadic || p.default.is_some() {
            return None;
        }
        // (4-strict) Anotacao crua DEVE ser um escalar puro. Sem isso o param
        // recebe tagging especial em compile_user_fn (fn-type -> local_fn_ret_
        // f64, Map -> local_map_vars, string -> local_string_vars, array ->
        // local_array_vars, any/sem-anotacao -> var_member_call_values,
        // Iterator -> generator_vars) que o inline NAO replica => lixo no corpo.
        if !is_scalar_annotation(p.type_annotation.as_deref()) {
            return None;
        }
    }

    // (4) params e ret TODOS numericos (ABI) — redundante com a checagem de
    // anotacao acima, mas defensivo (params_ty e' a fonte usada na coercao).
    if !params_ty.iter().copied().all(is_numeric) {
        return None;
    }
    let ret_ty = ret_ty?;
    if !is_numeric(ret_ty) {
        return None;
    }
    // (4-strict) Anotacao de retorno tambem escalar pura. Uma fn `: string` que
    // o codegen infere como F64 (corpo `"x"+s`) passaria o teste de ValTy mas
    // devolveria bits errados; exigir anotacao escalar fecha esse buraco.
    if !is_scalar_annotation(fn_decl.return_type.as_deref()) {
        return None;
    }

    // (5)+(6)+(7) Walk do corpo: so' constructs seguros, sem self-recursao,
    // sem this/throw/loop/try/etc. Conta custo e exige >= 1 return.
    let mut checker = BodyChecker {
        self_name: fn_decl.name.as_str(),
        cost: 0,
        returns: 0,
        ok: true,
    };
    for stmt_raw in &fn_decl.body {
        let Statement::Raw(raw) = stmt_raw;
        match raw.stmt.as_ref() {
            Some(stmt) => checker.check_stmt(stmt),
            None => {
                // Statement sintetico sem AST SWC — nao da' pra inlinar com
                // seguranca.
                return None;
            }
        }
        if !checker.ok {
            return None;
        }
    }
    if !checker.ok || checker.returns == 0 {
        return None;
    }
    // (8) cost <= budget.
    if checker.cost > INLINE_AST_BUDGET {
        return None;
    }

    let params: Vec<(String, ValTy)> = fn_decl
        .parameters
        .iter()
        .zip(params_ty.iter().copied())
        .map(|(p, ty)| (p.name.clone(), ty))
        .collect();

    Some(InlineCandidate {
        params,
        ret: ret_ty,
        body: fn_decl.body.clone(),
        cost: checker.cost,
    })
}

/// Walker conservador (default-DENY) sobre o corpo da fn candidata.
struct BodyChecker<'a> {
    self_name: &'a str,
    cost: usize,
    returns: usize,
    ok: bool,
}

impl BodyChecker<'_> {
    fn deny(&mut self) {
        self.ok = false;
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        if !self.ok {
            return;
        }
        self.cost += 1;
        match stmt {
            // `const NAME = init` (binding simples). Nao aceita `let`/`var`
            // (mutacao escaparia o modelo SSA do inline simples) nem
            // destructuring.
            Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
                if !matches!(v.kind, swc_ecma_ast::VarDeclKind::Const) {
                    self.deny();
                    return;
                }
                for d in &v.decls {
                    if !matches!(&d.name, Pat::Ident(_)) {
                        self.deny();
                        return;
                    }
                    match d.init.as_deref() {
                        Some(init) => self.check_expr(init),
                        None => {
                            self.deny();
                            return;
                        }
                    }
                }
            }
            // `return <expr>` — interceptado no inline. Exige arg (Some).
            Stmt::Return(r) => match r.arg.as_deref() {
                Some(arg) => {
                    self.returns += 1;
                    self.check_expr(arg);
                }
                None => self.deny(),
            },
            // `if (c) {...} else {...}` — ambos ramos walked. Necessario para
            // os casos de control-flow (early-return, if/else ambos retornam).
            Stmt::If(if_stmt) => {
                self.check_expr(&if_stmt.test);
                self.check_stmt(&if_stmt.cons);
                if let Some(alt) = if_stmt.alt.as_deref() {
                    self.check_stmt(alt);
                }
            }
            // Bloco `{ ... }` — recursa nos statements internos.
            Stmt::Block(b) => {
                for s in &b.stmts {
                    self.check_stmt(s);
                    if !self.ok {
                        return;
                    }
                }
            }
            // Statement de expressao: o UNICO caso aceito e' uma atribuicao
            // simples a um IDENT (local OU global), com `=` ou compound
            // aritmetico/bitwise (`+=`,`-=`,…,`>>>=`), cujo RHS passe no
            // `check_expr`. Esse e' o padrao quente `seed = (seed*16807)%N` que
            // antes barrava a fn inteira do inline. Seguro porque o assign
            // inlinado roda na MESMA ordem (lower_stmt copia os stmts em
            // sequencia) — vira o mesmo store que a fn faria, sem novo race
            // (preserva ordem de loads/stores). Conservador: SO' Ident simples
            // — member (`this.x=`, `o.f=`) e index (`a[i]=`) e destructuring
            // continuam deny (complexidade de handle/array fora do modelo). Toda
            // outra forma de Stmt::Expr (call descartada, etc) => deny.
            Stmt::Expr(e) => match e.expr.as_ref() {
                Expr::Assign(a) => {
                    use swc_ecma_ast::{AssignOp, AssignTarget, SimpleAssignTarget};
                    // Target precisa ser Ident simples (nao member/index/destruct).
                    if !matches!(
                        &a.left,
                        AssignTarget::Simple(SimpleAssignTarget::Ident(_))
                    ) {
                        self.deny();
                        return;
                    }
                    // Operador `=` ou compound aritmetico/bitwise. Logicos
                    // (`&&=`,`||=`,`??=`) ficam de fora (short-circuit muda
                    // ordem de avaliacao do RHS — conservador).
                    let op_ok = matches!(
                        a.op,
                        AssignOp::Assign
                            | AssignOp::AddAssign
                            | AssignOp::SubAssign
                            | AssignOp::MulAssign
                            | AssignOp::DivAssign
                            | AssignOp::ModAssign
                            | AssignOp::BitAndAssign
                            | AssignOp::BitOrAssign
                            | AssignOp::BitXorAssign
                            | AssignOp::LShiftAssign
                            | AssignOp::RShiftAssign
                            | AssignOp::ZeroFillRShiftAssign
                    );
                    if !op_ok {
                        self.deny();
                        return;
                    }
                    // RHS DEVE ser uma expr segura (reusa a validacao existente).
                    self.check_expr(&a.right);
                }
                _ => self.deny(),
            },
            // Empty/Debugger — inofensivos, custo zero adicional.
            Stmt::Empty(_) => {
                self.cost -= 1;
            }
            // Tudo o mais (loops, while, for, switch, try, throw, break,
            // continue, labeled, with, fn-decl aninhada, class) => deny.
            _ => self.deny(),
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        if !self.ok {
            return;
        }
        self.cost += 1;
        match expr {
            Expr::Lit(_) => {}
            // Ident permitido, EXCETO `arguments` (que o codegen sintetiza como
            // Vec local em compile_user_fn — nao replicado no inline => crash).
            Expr::Ident(id) => {
                if id.sym.as_str() == "arguments" {
                    self.deny();
                }
            }
            // `this` proibido (2).
            Expr::This(_) => self.deny(),
            Expr::Paren(p) => self.check_expr(&p.expr),
            Expr::Unary(u) => {
                // delete/typeof/void sao seguros sobre numericos, mas para
                // simplicidade so' permitimos os aritmeticos/logicos.
                match u.op {
                    swc_ecma_ast::UnaryOp::Minus
                    | swc_ecma_ast::UnaryOp::Plus
                    | swc_ecma_ast::UnaryOp::Bang
                    | swc_ecma_ast::UnaryOp::Tilde => self.check_expr(&u.arg),
                    _ => self.deny(),
                }
            }
            Expr::Bin(b) => {
                self.check_expr(&b.left);
                self.check_expr(&b.right);
            }
            Expr::Cond(c) => {
                self.check_expr(&c.test);
                self.check_expr(&c.cons);
                self.check_expr(&c.alt);
            }
            // Chamada a OUTRA user fn (por nome) e' permitida — habilita inline
            // transitivo (depth-limitado no call-site). Self-recursao direta e'
            // proibida (7). Chamadas a member (`x.y()`) / namespace sao
            // permitidas se o callee for resolvido normalmente; mas para
            // conservadorismo so' aceitamos call de Ident simples e member-call
            // de namespace conhecido. Aqui aceitamos apenas Ident-callee que
            // NAO seja a propria fn.
            Expr::Call(call) => {
                match &call.callee {
                    swc_ecma_ast::Callee::Expr(e) => match e.as_ref() {
                        Expr::Ident(id) => {
                            if id.sym.as_str() == self.self_name {
                                self.deny();
                                return;
                            }
                        }
                        // Member call (ns.fn() ou Math.x()) — permitido; o
                        // lowering normal resolve. Conservador: aceita Member.
                        Expr::Member(_) => {}
                        _ => {
                            self.deny();
                            return;
                        }
                    },
                    _ => {
                        self.deny();
                        return;
                    }
                }
                if call.args.iter().any(|a| a.spread.is_some()) {
                    self.deny();
                    return;
                }
                for a in &call.args {
                    self.check_expr(&a.expr);
                    if !self.ok {
                        return;
                    }
                }
            }
            // Member access `a.b` / `a[b]` — leitura simples permitida (ex:
            // Math.PI). Computed index sobre numero raro mas inofensivo.
            Expr::Member(m) => {
                self.check_expr(&m.obj);
                if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                    self.check_expr(&c.expr);
                }
            }
            // Tudo o mais proibido: new, arrow/fn aninhada, await, yield,
            // assign (mutacao), seq, template, tagged, array/object literal,
            // spread, optional-chain, etc. => deny.
            _ => self.deny(),
        }
    }
}
