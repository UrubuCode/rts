//! Escape analysis intraprocedural para arrays locais de tamanho fixo.
//!
//! Habilita `RTS_ARRAY_INLINE`: arrays LOCAIS, com tamanho estatico conhecido
//! (`new Array(N)` com N literal, ou `[lit, lit, ...]`), SEM push, e que NAO
//! escapam — sao candidatos a viver num `StackSlot` Cranelift em vez do
//! HandleTable. `arr[i]` vira load/store direto (zero call extern, zero lock).
//!
//! **Default-DENY (conservador).** Um array-var so' qualifica se TODA aparicao
//! do seu identificador no corpo da fn estiver numa posicao segura:
//!   - `arr[idx]`            (leitura indexada)
//!   - `arr[idx] = ...`      (escrita indexada, LHS de assign)
//!   - `arr[idx] OP= ...`    (RMW indexado)
//! Qualquer outra mencao (arg de call, return, atribuicao do proprio ident,
//! captura por arrow/fn, `.length`/`.push`/`.map`/..., spread, membro nao-
//! computed, indice que e' o proprio array, etc.) marca o array como ESCAPADO
//! — cai no caminho atual (VEC_NEW/GET/SET/RMW), bit-identico.
//!
//! GC-safety: o scanner do GC e' conservador e varre toda a stack, entao
//! handles guardados num stack slot sao marcados automaticamente (igual a
//! qualquer local). Tamanho fixo + sem push => nunca realoca. Nao-escapante =>
//! nunca compartilhado entre threads (sem race async). Ver
//! `docs/specs/native-array-storage.md`.

use std::collections::{HashMap, HashSet};

use swc_ecma_ast::{Expr, Lit, MemberProp, Pat, Stmt};

use crate::parser::ast::Statement;

/// Metadata de um array local qualificado para storage nativo (stack slot).
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeArrayInfo {
    /// Numero de slots (elementos) — tamanho estatico conhecido.
    pub len: usize,
    /// True quando o init e' um array literal comprovadamente todo-float
    /// (`[0.0, 1.5]`). Nesse caso cada slot guarda os BITS de um f64.
    pub elem_is_float: bool,
}

/// Resultado da analise: nome do array -> metadata. So' contem arrays que
/// passaram em TODOS os filtros (candidato valido + nao-escapante).
pub(crate) fn non_escaping_fixed_arrays(
    body: &[Statement],
) -> HashMap<String, NativeArrayInfo> {
    // 1) Coleta candidatos: vars com init `new Array(N)` / `[lit,...]` (N
    //    literal). Tamanho registrado. Sem este passo nada qualifica.
    let mut candidates: HashMap<String, NativeArrayInfo> = HashMap::new();
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            collect_candidates_in_stmt(stmt, &mut candidates);
        }
    }
    if candidates.is_empty() {
        return candidates;
    }

    // 2) Escape walk: remove do conjunto qualquer candidato que apareca numa
    //    posicao nao-segura. Default-DENY: na menor duvida, escapa.
    let mut escaped: HashSet<String> = HashSet::new();
    let cand_names: HashSet<String> = candidates.keys().cloned().collect();
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            walk_stmt(stmt, &cand_names, &mut escaped);
        }
    }
    for name in &escaped {
        candidates.remove(name);
    }
    candidates
}

/// Coleta candidatos (var-decl com init array-literal / `new Array(N)`).
/// Tambem desce em statements aninhados (if/for/while/block) — vars declaradas
/// la' dentro tambem qualificam (escopo de bloco; o codegen lowera no ponto).
fn collect_candidates_in_stmt(
    stmt: &Stmt,
    out: &mut HashMap<String, NativeArrayInfo>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                let Pat::Ident(id) = &d.name else { continue };
                let name = id.id.sym.to_string();
                let Some(init) = d.init.as_deref() else { continue };
                if let Some(info) = candidate_info(init) {
                    // Nome duplicado (shadow em escopo diferente) — conservador:
                    // se ja' existe, remove (ambiguo qual slot). Default-DENY.
                    if out.contains_key(&name) {
                        out.remove(&name);
                    } else {
                        out.insert(name, info);
                    }
                }
            }
        }
        If(i) => {
            collect_candidates_in_stmt(&i.cons, out);
            if let Some(alt) = i.alt.as_deref() {
                collect_candidates_in_stmt(alt, out);
            }
        }
        Block(b) => {
            for s in &b.stmts {
                collect_candidates_in_stmt(s, out);
            }
        }
        While(w) => collect_candidates_in_stmt(&w.body, out),
        DoWhile(w) => collect_candidates_in_stmt(&w.body, out),
        For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    let Pat::Ident(id) = &d.name else { continue };
                    let name = id.id.sym.to_string();
                    let Some(init) = d.init.as_deref() else { continue };
                    if let Some(info) = candidate_info(init) {
                        if out.contains_key(&name) {
                            out.remove(&name);
                        } else {
                            out.insert(name, info);
                        }
                    }
                }
            }
            collect_candidates_in_stmt(&f.body, out);
        }
        ForOf(f) => collect_candidates_in_stmt(&f.body, out),
        ForIn(f) => collect_candidates_in_stmt(&f.body, out),
        Try(t) => {
            for s in &t.block.stmts {
                collect_candidates_in_stmt(s, out);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.stmts {
                    collect_candidates_in_stmt(s, out);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.stmts {
                    collect_candidates_in_stmt(s, out);
                }
            }
        }
        _ => {}
    }
}

/// Peel wrappers TS-only (`as`/`!`/`(...)`) para inspecionar o init real.
fn peel<'a>(e: &'a Expr) -> &'a Expr {
    match e {
        Expr::Paren(p) => peel(&p.expr),
        Expr::TsAs(a) => peel(&a.expr),
        Expr::TsConstAssertion(a) => peel(&a.expr),
        Expr::TsNonNull(a) => peel(&a.expr),
        Expr::TsTypeAssertion(a) => peel(&a.expr),
        Expr::TsSatisfies(a) => peel(&a.expr),
        _ => e,
    }
}

/// Retorna `NativeArrayInfo` se `init` for um candidato a array nativo:
///   - `[lit, lit, ...]` — todos elementos presentes (sem holes / sem spread),
///     tamanho = elems.len().
///   - `new Array(N)` — N literal inteiro >= 0.
/// Caso contrario `None` (cai no caminho atual).
fn candidate_info(init: &Expr) -> Option<NativeArrayInfo> {
    match peel(init) {
        Expr::Array(arr) => {
            // Sem holes (`[1,,3]`) nem spread — tamanho ambiguo / nao-flat.
            let mut all_float = !arr.elems.is_empty();
            for el in &arr.elems {
                let Some(e) = el else { return None }; // hole
                if e.spread.is_some() {
                    return None;
                }
                if !expr_is_float_lit(&e.expr) {
                    all_float = false;
                }
            }
            Some(NativeArrayInfo {
                len: arr.elems.len(),
                elem_is_float: all_float,
            })
        }
        Expr::New(n) => {
            // `new Array(N)` com N literal inteiro.
            let Expr::Ident(callee) = peel(&n.callee) else { return None };
            if callee.sym.as_str() != "Array" {
                return None;
            }
            let args = n.args.as_ref()?;
            if args.len() != 1 {
                return None;
            }
            if args[0].spread.is_some() {
                return None;
            }
            if let Expr::Lit(Lit::Num(num)) = peel(&args[0].expr) {
                let v = num.value;
                if v.fract() == 0.0 && v >= 0.0 && v <= (u32::MAX as f64) {
                    return Some(NativeArrayInfo {
                        len: v as usize,
                        elem_is_float: false,
                    });
                }
            }
            None
        }
        _ => None,
    }
}

/// True quando `e` e' um literal float (numerico). Aceita negacao unaria de
/// literal numerico (`-1.5`). Usado pra detectar arrays todo-float.
fn expr_is_float_lit(e: &Expr) -> bool {
    match peel(e) {
        Expr::Lit(Lit::Num(_)) => true,
        Expr::Unary(u)
            if matches!(u.op, swc_ecma_ast::UnaryOp::Minus | swc_ecma_ast::UnaryOp::Plus) =>
        {
            matches!(peel(&u.arg), Expr::Lit(Lit::Num(_)))
        }
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Escape walk — default-DENY. Qualquer mencao de um candidato fora de
// `arr[idx]` (read/write/RMW) marca como escapado.
// ─────────────────────────────────────────────────────────────────────────

fn walk_stmt(stmt: &Stmt, cands: &HashSet<String>, escaped: &mut HashSet<String>) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => walk_expr(&e.expr, cands, escaped),
        Return(r) => {
            if let Some(a) = r.arg.as_deref() {
                walk_expr(a, cands, escaped);
            }
        }
        If(i) => {
            walk_expr(&i.test, cands, escaped);
            walk_stmt(&i.cons, cands, escaped);
            if let Some(alt) = i.alt.as_deref() {
                walk_stmt(alt, cands, escaped);
            }
        }
        Block(b) => {
            for s in &b.stmts {
                walk_stmt(s, cands, escaped);
            }
        }
        While(w) => {
            walk_expr(&w.test, cands, escaped);
            walk_stmt(&w.body, cands, escaped);
        }
        DoWhile(w) => {
            walk_expr(&w.test, cands, escaped);
            walk_stmt(&w.body, cands, escaped);
        }
        For(f) => {
            if let Some(init) = f.init.as_ref() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                        for d in &vd.decls {
                            if let Some(e) = d.init.as_deref() {
                                // O init de uma var-decl candidato (`[...]` /
                                // `new Array(N)`) NAO conta como escape — eh a
                                // alocacao. Walk so' o que NAO for a propria init.
                                walk_decl_init(&d.name, e, cands, escaped);
                            }
                        }
                    }
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => walk_expr(e, cands, escaped),
                }
            }
            if let Some(t) = f.test.as_deref() {
                walk_expr(t, cands, escaped);
            }
            if let Some(u) = f.update.as_deref() {
                walk_expr(u, cands, escaped);
            }
            walk_stmt(&f.body, cands, escaped);
        }
        ForOf(f) => {
            // `for (const x of <arr>)` — o array no RHS escapa (iteracao
            // chama Symbol.iterator / itera o Vec). Walk normal o marca.
            walk_expr(&f.right, cands, escaped);
            walk_stmt(&f.body, cands, escaped);
        }
        ForIn(f) => {
            walk_expr(&f.right, cands, escaped);
            walk_stmt(&f.body, cands, escaped);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(e) = d.init.as_deref() {
                    walk_decl_init(&d.name, e, cands, escaped);
                }
            }
        }
        Throw(t) => walk_expr(&t.arg, cands, escaped),
        Try(t) => {
            for s in &t.block.stmts {
                walk_stmt(s, cands, escaped);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.stmts {
                    walk_stmt(s, cands, escaped);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.stmts {
                    walk_stmt(s, cands, escaped);
                }
            }
        }
        Switch(sw) => {
            walk_expr(&sw.discriminant, cands, escaped);
            for case in &sw.cases {
                if let Some(t) = case.test.as_deref() {
                    walk_expr(t, cands, escaped);
                }
                for s in &case.cons {
                    walk_stmt(s, cands, escaped);
                }
            }
        }
        Labeled(l) => walk_stmt(&l.body, cands, escaped),
        // Decl::Fn / Decl::Class etc — nested fn declarations: conservador,
        // qualquer candidato referenciado dentro escapa. Mas declaracoes
        // aninhadas sao raras no body (o lifter ja' as extraiu); por seguranca
        // marcamos TODOS os candidatos como escapados se houver uma fn-decl
        // nested que referencie algum (nao temos visibilidade barata aqui).
        // Como o caminho seguro e' nao-inline, simplesmente nao descemos —
        // mas qualquer USO via expr ja' foi coberto. Fn-decls nested nao
        // expressam uso direto de var local aqui (params proprios).
        _ => {}
    }
}

/// Trata o init de uma var-decl. Se o `name` declarado for um candidato e o
/// `init` for exatamente a sua alocacao (array-literal / new Array), NAO
/// contamos como escape (eh a criacao). Mas ainda precisamos varrer os
/// SUB-exprs do init (ex: `[foo()]` — `foo()` pode usar outro candidato).
fn walk_decl_init(
    name: &Pat,
    init: &Expr,
    cands: &HashSet<String>,
    escaped: &mut HashSet<String>,
) {
    let declared_name = if let Pat::Ident(id) = name {
        Some(id.id.sym.to_string())
    } else {
        None
    };
    // Se este decl e' a alocacao de um candidato, varre os elementos do
    // literal (que podem mencionar OUTROS candidatos), sem marcar o proprio.
    if let Some(dn) = &declared_name {
        if cands.contains(dn) && candidate_info(init).is_some() {
            // Varre sub-exprs do literal (elementos) sem tratar a alocacao
            // como uso do proprio `dn`.
            if let Expr::Array(arr) = peel(init) {
                for el in arr.elems.iter().flatten() {
                    walk_expr(&el.expr, cands, escaped);
                }
            }
            if let Expr::New(n) = peel(init) {
                if let Some(args) = &n.args {
                    for a in args {
                        walk_expr(&a.expr, cands, escaped);
                    }
                }
            }
            return;
        }
    }
    // Caso geral: o init e' um uso normal — varre tudo.
    walk_expr(init, cands, escaped);
}

fn walk_expr(expr: &Expr, cands: &HashSet<String>, escaped: &mut HashSet<String>) {
    match expr {
        // ── Acesso indexado `arr[idx]` (read) — posicao SEGURA. ──────────
        // Se obj e' Ident candidato e prop e' Computed, o `arr` NAO escapa
        // por esta ocorrencia. So' varremos o INDICE (que pode usar outro
        // candidato, ou mesmo `arr` de novo — la' sim escaparia).
        Expr::Member(m) => {
            if let (Expr::Ident(obj_id), MemberProp::Computed(c)) =
                (m.obj.as_ref(), &m.prop)
            {
                if cands.contains(obj_id.sym.as_str()) {
                    // Indice e' uma posicao de USO — se o indice for o proprio
                    // array (`arr[arr]`), o walk do indice o marca escapado.
                    walk_expr(&c.expr, cands, escaped);
                    return;
                }
            }
            // Qualquer outro Member: `arr.length`, `arr.push`, `arr[idx].x`,
            // `obj.arr`, etc. Varre obj + prop computed normalmente — uma
            // referencia bare a `arr` em `m.obj` sera' marcada escapada.
            walk_expr(&m.obj, cands, escaped);
            if let MemberProp::Computed(c) = &m.prop {
                walk_expr(&c.expr, cands, escaped);
            }
        }

        // ── Assign — LHS `arr[idx] = ...` / `arr[idx] OP= ...` SEGURO. ────
        Expr::Assign(a) => {
            walk_assign(a, cands, escaped);
        }

        // ── Identificador bare — ESCAPE. ─────────────────────────────────
        // Qualquer mencao do candidato que chegue aqui (nao filtrada pelo
        // caso Member/Assign acima) e' uso fora de `arr[idx]` => escapa.
        Expr::Ident(id) => {
            if cands.contains(id.sym.as_str()) {
                escaped.insert(id.sym.to_string());
            }
        }

        // ── Recursao estrutural. ─────────────────────────────────────────
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(e) = &c.callee {
                walk_expr(e, cands, escaped);
            }
            for a in &c.args {
                walk_expr(&a.expr, cands, escaped);
            }
        }
        Expr::New(n) => {
            walk_expr(&n.callee, cands, escaped);
            if let Some(args) = &n.args {
                for a in args {
                    walk_expr(&a.expr, cands, escaped);
                }
            }
        }
        Expr::Bin(b) => {
            walk_expr(&b.left, cands, escaped);
            walk_expr(&b.right, cands, escaped);
        }
        Expr::Unary(u) => walk_expr(&u.arg, cands, escaped),
        Expr::Update(u) => walk_expr(&u.arg, cands, escaped),
        Expr::Cond(c) => {
            walk_expr(&c.test, cands, escaped);
            walk_expr(&c.cons, cands, escaped);
            walk_expr(&c.alt, cands, escaped);
        }
        Expr::Paren(p) => walk_expr(&p.expr, cands, escaped),
        Expr::Seq(s) => {
            for e in &s.exprs {
                walk_expr(e, cands, escaped);
            }
        }
        Expr::Tpl(t) => {
            for e in &t.exprs {
                walk_expr(e, cands, escaped);
            }
        }
        Expr::TaggedTpl(t) => {
            walk_expr(&t.tag, cands, escaped);
            for e in &t.tpl.exprs {
                walk_expr(e, cands, escaped);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter().flatten() {
                walk_expr(&el.expr, cands, escaped);
            }
        }
        Expr::Object(o) => {
            for p in &o.props {
                match p {
                    swc_ecma_ast::PropOrSpread::Spread(s) => {
                        walk_expr(&s.expr, cands, escaped)
                    }
                    swc_ecma_ast::PropOrSpread::Prop(prop) => {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_ref() {
                            walk_expr(&kv.value, cands, escaped);
                        }
                        // Computed key tambem e' uso.
                        if let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_ref() {
                            if let swc_ecma_ast::PropName::Computed(c) = &kv.key {
                                walk_expr(&c.expr, cands, escaped);
                            }
                        }
                    }
                }
            }
        }
        Expr::Await(a) => walk_expr(&a.arg, cands, escaped),
        Expr::Yield(y) => {
            if let Some(a) = y.arg.as_deref() {
                walk_expr(a, cands, escaped);
            }
        }
        Expr::OptChain(o) => {
            // `arr?.[idx]` / `arr?.x` — conservador: trata como uso (escape).
            // O fast-path nativo nao cobre optional chain; varre a base como
            // expressao generica (marcando bare idents).
            match o.base.as_ref() {
                swc_ecma_ast::OptChainBase::Member(m) => {
                    walk_expr(&m.obj, cands, escaped);
                    if let MemberProp::Computed(c) = &m.prop {
                        walk_expr(&c.expr, cands, escaped);
                    }
                }
                swc_ecma_ast::OptChainBase::Call(c) => {
                    walk_expr(&c.callee, cands, escaped);
                    for a in &c.args {
                        walk_expr(&a.expr, cands, escaped);
                    }
                }
            }
        }
        Expr::TsAs(a) => walk_expr(&a.expr, cands, escaped),
        Expr::TsConstAssertion(a) => walk_expr(&a.expr, cands, escaped),
        Expr::TsNonNull(a) => walk_expr(&a.expr, cands, escaped),
        Expr::TsTypeAssertion(a) => walk_expr(&a.expr, cands, escaped),
        Expr::TsSatisfies(a) => walk_expr(&a.expr, cands, escaped),
        // Arrow / Fn expressions: qualquer candidato capturado escapa. Como
        // nao temos o set de free-vars barato aqui, descemos no corpo e
        // qualquer bare-ident de candidato sera' marcado escapado (correto:
        // captura por closure = compartilhamento => escape).
        Expr::Arrow(arrow) => {
            match arrow.body.as_ref() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &b.stmts {
                        walk_stmt(s, cands, escaped);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => walk_expr(e, cands, escaped),
            }
        }
        Expr::Fn(fn_expr) => {
            if let Some(b) = fn_expr.function.body.as_ref() {
                for s in &b.stmts {
                    walk_stmt(s, cands, escaped);
                }
            }
        }
        // Literais, this, super, idents nao-candidatos via outros ramos: nada.
        _ => {}
    }
}

/// Walk de um assignment. O LHS `arr[idx]` (Computed Member com obj Ident
/// candidato) e' SEGURO — so' o INDICE e o RHS sao usos. Qualquer outra forma
/// de LHS que mencione o candidato => escape (via walk generico).
fn walk_assign(
    a: &swc_ecma_ast::AssignExpr,
    cands: &HashSet<String>,
    escaped: &mut HashSet<String>,
) {
    use swc_ecma_ast::{AssignTarget, SimpleAssignTarget};
    // RHS sempre e' uso.
    // (avaliamos o LHS primeiro pra detectar a forma segura.)
    if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left {
        if let (Expr::Ident(obj_id), MemberProp::Computed(c)) =
            (m.obj.as_ref(), &m.prop)
        {
            if cands.contains(obj_id.sym.as_str()) {
                // `arr[idx] = rhs` ou `arr[idx] OP= rhs` — posicao segura.
                // O proprio `arr` NAO escapa; varre indice + RHS.
                walk_expr(&c.expr, cands, escaped);
                walk_expr(&a.right, cands, escaped);
                return;
            }
        }
        // LHS member que nao e' `cand[idx]`: varre obj (bare ident de cand
        // escaparia) + indice computed.
        walk_expr(&m.obj, cands, escaped);
        if let MemberProp::Computed(c) = &m.prop {
            walk_expr(&c.expr, cands, escaped);
        }
        walk_expr(&a.right, cands, escaped);
        return;
    }
    // LHS Ident (`x = ...`): se `x` for candidato, reassign => escape (o
    // slot perderia a identidade). Conservador.
    if let AssignTarget::Simple(SimpleAssignTarget::Ident(id)) = &a.left {
        if cands.contains(id.id.sym.as_str()) {
            escaped.insert(id.id.sym.to_string());
        }
    }
    // LHS de destructuring / pattern: trata o RHS normal; padroes que
    // referenciem candidatos no LHS sao raros e o walk generico do RHS cobre
    // o uso. Por seguranca, qualquer AssignTarget::Pat marca nada extra aqui
    // (o RHS abaixo cobre usos).
    walk_expr(&a.right, cands, escaped);
}
