//! Capturas ESCRITAS de um generator-expressão, passadas por REFERÊNCIA.
//!
//! O `GenExprHoister` (em [`crate::lowering_items`]) leva um `function*` que é
//! EXPRESSÃO para o topo, porque a state-machine de generators só existe para
//! declarações de topo. As capturas viajam como ARGUMENTOS do wrapper, isto é,
//! POR VALOR — e por isso ele recusava içar quando alguma capturada era
//! ESCRITA: a escrita ficaria no parâmetro e nunca chegaria ao escopo de
//! origem, trocando uma recusa honesta por um efeito que some em silêncio.
//!
//! Essa recusa custava caro: é exatamente a forma que todo `async` transpilado
//! produz, porque o `asyncToGenerator` do Babel embrulha um `function*` que
//! memoiza requires preguiçosos (`s || (s = n("Promise"))`).
//!
//! ## O mecanismo
//!
//! Em vez de um esquema novo, este módulo reusa o que o motor JÁ tem: o lifter
//! de closures transforma um local capturado-E-escrito numa CÉLULA
//! compartilhada. O wrapper é uma fn-expressão comum no escopo declarante, logo
//! passa por esse lifter. Então basta o wrapper entregar ao generator um PAR de
//! funções por capturada escrita:
//!
//! ```text
//!   var s;
//!   const g = function* () { return (s || (s = f())).x; };
//! vira
//!   function* __genexpr_N(__gs_s, __ss_s) { return (__gs_s() || __ss_s(f())).x; }
//!   const g = function () { return __genexpr_N(() => s, (v) => (s = v)); };
//! ```
//!
//! As duas arrows do wrapper capturam e escrevem `s`, então o lifter faz de `s`
//! uma célula e ambas enxergam a MESMA caixa — que é o comportamento correto.
//! O setter devolve o valor atribuído, porque `s = v` é uma expressão cujo valor
//! é `v` (`x = (s = 1)` tem de dar `1`).
//!
//! ## O que ele RECUSA
//!
//! `s++` / `s--` e a atribuição desestruturante (`[s] = xs`) não são reescritos:
//! exigiriam sintetizar um temporário para preservar o valor antigo, e uma
//! reescrita parcial devolveria um valor errado calado. Nesses casos o hoist
//! não acontece e o generator continua recusado, como antes.

use std::collections::HashSet;

use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignOp, AssignTarget, BindingIdent, BlockStmtOrExpr, CallExpr, Callee,
    Expr, ExprOrSpread, Function, Ident, Param, Pat, SimpleAssignTarget,
};
use swc_ecma_visit::{VisitMut, VisitMutWith};

/// O nome do parâmetro-getter de `name`.
pub fn getter_name(name: &str) -> String {
    format!("__gs_{name}")
}

/// O nome do parâmetro-setter de `name`.
pub fn setter_name(name: &str) -> String {
    format!("__ss_{name}")
}

fn ident(n: &str) -> Ident {
    Ident {
        span: Default::default(),
        ctxt: Default::default(),
        sym: n.into(),
        optional: false,
    }
}

fn ident_expr(n: &str) -> Expr {
    Expr::Ident(ident(n))
}

fn call(callee: &str, args: Vec<Expr>) -> Expr {
    Expr::Call(CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(ident_expr(callee))),
        args: args
            .into_iter()
            .map(|e| ExprOrSpread {
                spread: None,
                expr: Box::new(e),
            })
            .collect(),
        type_args: None,
    })
}

/// `(nome) => corpo` — a forma que o lifter de closures reconhece.
fn arrow(params: Vec<&str>, body: Expr) -> Expr {
    Expr::Arrow(ArrowExpr {
        span: Default::default(),
        ctxt: Default::default(),
        params: params
            .into_iter()
            .map(|p| Pat::Ident(BindingIdent::from(ident(p))))
            .collect(),
        body: Box::new(BlockStmtOrExpr::Expr(Box::new(body))),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
    })
}

/// Os argumentos que o WRAPPER passa por `name`: `() => name` e
/// `(v) => (name = v)`.
pub fn wrapper_args(name: &str) -> [Expr; 2] {
    let get = arrow(vec![], ident_expr(name));
    let set = arrow(
        vec!["__v"],
        Expr::Assign(AssignExpr {
            span: Default::default(),
            op: AssignOp::Assign,
            left: AssignTarget::Simple(SimpleAssignTarget::Ident(BindingIdent::from(ident(name)))),
            right: Box::new(ident_expr("__v")),
        }),
    );
    [get, set]
}

/// Os PARÂMETROS que o generator içado recebe por `name`, em ordem.
pub fn hoisted_params(name: &str) -> [Param; 2] {
    let p = |n: String| Param {
        span: Default::default(),
        decorators: Vec::new(),
        pat: Pat::Ident(BindingIdent::from(ident(&n))),
    };
    [p(getter_name(name)), p(setter_name(name))]
}

/// Reescreve, no corpo de `f`, toda LEITURA e ESCRITA dos nomes em `names` para
/// o par getter/setter. Devolve `false` (sem tocar em `f`) quando encontra uma
/// forma que não sabe reescrever — o chamador então não iça, mantendo a recusa
/// honesta em vez de uma reescrita pela metade.
pub fn rewrite_by_ref(f: &mut Function, names: &HashSet<String>) -> bool {
    if names.is_empty() {
        return true;
    }
    let mut v = Rewriter {
        names,
        shadowed: Vec::new(),
        ok: true,
    };
    f.visit_mut_with(&mut v);
    v.ok
}

struct Rewriter<'a> {
    names: &'a HashSet<String>,
    /// Escopos aninhados que REDECLARAM um dos nomes — dentro deles o nome é
    /// outra variável e não pode ser reescrito.
    shadowed: Vec<HashSet<String>>,
    ok: bool,
}

impl Rewriter<'_> {
    fn targets(&self, n: &str) -> bool {
        self.names.contains(n) && !self.shadowed.iter().any(|s| s.contains(n))
    }

    /// Nomes que `params` liga — um deles sombreia a capturada de mesmo nome.
    fn bound_by(params: &[Pat]) -> HashSet<String> {
        let mut out = HashSet::new();
        for p in params {
            if let Pat::Ident(i) = p {
                out.insert(i.id.sym.to_string());
            }
        }
        out
    }
}

impl VisitMut for Rewriter<'_> {
    fn visit_mut_function(&mut self, f: &mut Function) {
        let pats: Vec<Pat> = f.params.iter().map(|p| p.pat.clone()).collect();
        self.shadowed.push(Self::bound_by(&pats));
        f.visit_mut_children_with(self);
        self.shadowed.pop();
    }

    fn visit_mut_arrow_expr(&mut self, a: &mut ArrowExpr) {
        self.shadowed.push(Self::bound_by(&a.params));
        a.visit_mut_children_with(self);
        self.shadowed.pop();
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        // ATRIBUIÇÃO primeiro: o alvo é um `Ident` que NÃO pode virar leitura.
        if let Expr::Assign(a) = e {
            let target = match &a.left {
                AssignTarget::Simple(SimpleAssignTarget::Ident(i))
                    if self.targets(&i.id.sym.to_string()) =>
                {
                    Some(i.id.sym.to_string())
                }
                _ => None,
            };
            if let Some(name) = target {
                // O RHS é reescrito normalmente (pode ler a mesma capturada).
                a.right.visit_mut_with(self);
                let rhs = (*a.right).clone();
                let value = match a.op {
                    AssignOp::Assign => rhs,
                    // `s <op>= v` → `__ss_s(__gs_s() <op> v)`. O valor da
                    // expressão continua sendo o novo valor, que é o que o
                    // setter devolve.
                    op => match op.to_update() {
                        Some(bin) => Expr::Bin(swc_ecma_ast::BinExpr {
                            span: Default::default(),
                            op: bin,
                            left: Box::new(call(&getter_name(&name), vec![])),
                            right: Box::new(rhs),
                        }),
                        // Lógicos (`&&=`/`||=`/`??=`) precisariam curto-circuitar
                        // a ESCRITA; reescrever sem isso dispararia o setter
                        // sempre. Recusa.
                        None => {
                            self.ok = false;
                            return;
                        }
                    },
                };
                *e = call(&setter_name(&name), vec![value]);
                return;
            }
        }
        // `s++` / `s--`: precisaria de um temporário para o valor antigo.
        if let Expr::Update(u) = e {
            if let Expr::Ident(i) = &*u.arg {
                if self.targets(&i.sym.to_string()) {
                    self.ok = false;
                    return;
                }
            }
        }
        e.visit_mut_children_with(self);
        // LEITURA — depois dos filhos, para não reescrever um `Ident` que já
        // virou parte de uma chamada acima.
        if let Expr::Ident(i) = e {
            let n = i.sym.to_string();
            if self.targets(&n) {
                *e = call(&getter_name(&n), vec![]);
            }
        }
    }

    /// Um alvo de atribuição DESESTRUTURANTE (`[s] = xs`, `({s} = o)`) escreve
    /// sem passar pelo arm de `Expr::Assign` acima. Recusa em vez de deixar a
    /// escrita cair no parâmetro.
    fn visit_mut_assign_target(&mut self, t: &mut AssignTarget) {
        if let AssignTarget::Pat(p) = t {
            let mut names = HashSet::new();
            collect_pat_names(p, &mut names);
            if names.iter().any(|n| self.targets(n)) {
                self.ok = false;
                return;
            }
        }
        t.visit_mut_children_with(self);
    }
}

fn collect_pat_names(p: &swc_ecma_ast::AssignTargetPat, out: &mut HashSet<String>) {
    use swc_ecma_ast::AssignTargetPat;
    match p {
        AssignTargetPat::Array(a) => {
            for el in a.elems.iter().flatten() {
                if let Pat::Ident(i) = el {
                    out.insert(i.id.sym.to_string());
                }
            }
        }
        AssignTargetPat::Object(o) => {
            for pr in &o.props {
                if let swc_ecma_ast::ObjectPatProp::Assign(a) = pr {
                    out.insert(a.key.sym.to_string());
                }
            }
        }
        AssignTargetPat::Invalid(_) => {}
    }
}
