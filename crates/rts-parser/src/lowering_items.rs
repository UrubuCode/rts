/// (cross-runtime #344) Hoists anonymous/named generator function *expressions*
/// (`function*() {}`) appearing at top level into named top-level function
/// declarations (`__genexpr_N`), rewriting the expression site to an identifier.
/// The named-decl path is the only one that builds a lazy state-machine
/// generator (Entry::GenState) supporting `g.next(v)` value-passing; a bare
/// fn-expr generator would otherwise fall to the eager buffer and lose the
/// resumption value. `visit_mut_function` is a no-op so we never descend into a
/// function body — only generators that capture nothing (true top-level) are
/// hoisted, avoiding broken captures.
/// Nomes que qualquer escopo enxerga — mencioná-los NÃO é captura.
///
/// É uma HEURÍSTICA de otimização, não um requisito de correção, e é por isso
/// que uma lista parcial é tolerável: um global ausente daqui é classificado
/// como captura, e o único efeito é o generator levantado recebê-lo como
/// PARÂMETRO — que o wrapper passa a partir do escopo original, onde o nome
/// resolve como global de qualquer forma. O resultado continua certo; perde-se
/// apenas a chance de levantar sem wrapper.
///
/// Uma fonte de verdade só: `sem_captura_ext` (que responde se a lista é vazia)
/// e `capturas` (que devolve a lista) leem daqui.
const GLOBAIS_VISIVEIS: &[&str] = &[
    "undefined", "null", "true", "false", "this", "arguments", "console",
    "Object", "Array", "String", "Number", "Boolean", "Math", "JSON",
    "Date", "RegExp", "Error", "TypeError", "RangeError", "Promise",
    "Symbol", "Map", "Set", "WeakMap", "WeakSet", "Infinity", "NaN",
    "globalThis", "window", "document", "self", "parseInt", "parseFloat",
];

#[derive(Default)]
struct GenExprHoister {
    hoisted: Vec<swc_ecma_ast::FnDecl>,
    counter: usize,
    /// `(nome original, nome levantado)` dos generators DECLARADOS levantados no
    /// bloco que está sendo visitado. Consumido por `visit_mut_block_stmt`, que
    /// aplica o rename APENAS àquele bloco — o escopo em que a declaração valia.
    pendentes: Vec<(String, String)>,
    /// Profundidade de blocos: `0` = topo do módulo, onde `lower_decl` já cuida
    /// dos generators e este hoister não deve mexer.
    dentro_de_bloco: usize,
    /// Nomes de funções AUXILIARES já copiadas para o topo — impede subir a
    /// mesma irmã duas vezes (dois generators do mesmo bloco a usam).
    aux_subidas: std::collections::HashSet<String>,
    /// Funções declaradas no bloco corrente: contam como ligadas ao decidir se
    /// um generator pode ser levantado, porque sobem junto com ele.
    irmas_do_bloco: std::collections::HashSet<String>,
}

impl swc_ecma_visit::VisitMut for GenExprHoister {
    /// DESCE em corpos de função para achar generators ANINHADOS.
    ///
    /// Antes era no-op ("avoid capturing hoists"), e a consequência era que o
    /// desugar de generator só alcançava o TOPO do módulo: `function* g(){…}`
    /// dentro de qualquer função — ou dentro de `new Function` — chegava ao
    /// lowering como `Raw` e morria em "unrecognized statement". Num bundle
    /// minificado real isso derruba o arquivo inteiro.
    ///
    /// O receio da captura continua respeitado: só se hoisteia um generator cujo
    /// corpo NÃO referencia nada do escopo em que está (ver `sem_captura`) — um
    /// generator que captura fica onde está e mantém a falha honesta.
    fn visit_mut_function(&mut self, f: &mut SwcFunction) {
        use swc_ecma_visit::VisitMutWith;
        f.visit_mut_children_with(self);
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        use swc_ecma_visit::VisitMutWith;
        e.visit_mut_children_with(self);
        if let Expr::Fn(fe) = e {
            // Sem `sem_captura` aqui: uma fn-EXPRESSÃO já era levantada
            // incondicionalmente antes (é o caminho do cross-runtime #344), e
            // acrescentar a guarda quebrava até `const g = function*(){…}` no
            // TOPO — `g()` deixava de devolver iterador. A guarda vale só para a
            // DECLARAÇÃO aninhada, que é o caso novo.
            // Levantar SEMPRE perdia as capturas: `const g = function*(){ yield
            // r(o.v) }` dentro de uma função virava `__genexpr_N` no topo, onde
            // `o`/`r` não existem mais — o bundle morria em
            // `call to unknown function 'o'` (e no motor, em ReferenceError).
            //
            // No TOPO do módulo (`dentro_de_bloco == 0`) as livres já são globais
            // e continuam alcançáveis, então o hoist é sempre seguro — é por isso
            // que ligar `sem_captura` incondicionalmente quebrava esse caso.
            // Dentro de um bloco, só levanta quem não captura; quem captura fica
            // onde está e é desugarado no lugar, com o escopo intacto.
            let pode_levantar = self.dentro_de_bloco == 0
                || Self::sem_captura_ext(&fe.function, &self.irmas_do_bloco);
            // Quem CAPTURA é desugarado NO LUGAR e deixa de ser generator: o
            // corpo vira o eager-buffer (`__gen_buf` + `__RTS_GEN_FINISH`) e o
            // nó passa a ser uma fn-expression COMUM — que a maquinaria de
            // closure já sabe extrair com as capturas. O `FnSig` reconhece o
            // sentinela `GEN_FINISH` e marca `ret_eager_gen`, então `.next()` /
            // for-of / spread seguem funcionando sobre o buffer.
            if fe.function.is_generator && !pode_levantar {
                if let Some(corpo) = fe.function.body.as_ref() {
                    let de_valor = Self::usa_yield_como_valor(corpo);
                    let desugarado = crate::generator_desugar::desugar_generator_body(corpo);
                    // O eager-buffer só expressa `yield` em posição de STATEMENT
                    // (vira `__gen_buf.push`). Um `yield` usado como VALOR
                    // (`const a = yield b`) sobra no corpo desugarado e chegaria
                    // ao lowering como `Yield` cru. Esse caso precisa da
                    // state-machine — que exige o hoist —, então cai no caminho
                    // de sempre: continua levantando (e continua perdendo a
                    // captura, honestamente, como antes).
                    if !de_valor {
                        fe.function.body = Some(desugarado);
                        fe.function.is_generator = false;
                        return;
                    }
                    // Corpo com `yield` em posição de VALOR precisa da
                    // state-machine, que exige o hoist — e o hoist perderia a
                    // captura. Sai da contradição levantando com as CAPTURAS
                    // COMO PARÂMETROS (o corpo já as referencia por esses nomes,
                    // então nada é renomeado) e deixando no lugar um wrapper
                    // COMUM que as repassa: o wrapper captura pelo caminho de
                    // closure que já existe, e o generator vira decl de topo,
                    // onde a state-machine funciona.
                    //
                    //   const g = function*(){ const a = yield o.v; };
                    // vira
                    //   function* __genexpr_N(o){ const a = yield o.v; }
                    //   const g = function(){ return __genexpr_N(o); };
                    let caps = Self::capturas(&fe.function, &self.irmas_do_bloco);
                    if !caps.is_empty() {
                        let name = format!("__genexpr_{}", self.counter);
                        self.counter += 1;
                        let ident_de = |n: &str| swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: n.into(),
                            optional: false,
                        };
                        let id_gen = ident_de(&name);
                        let mut levantada = fe.function.clone();
                        let mut ps: Vec<swc_ecma_ast::Param> = caps
                            .iter()
                            .map(|c| swc_ecma_ast::Param {
                                span: Default::default(),
                                decorators: Vec::new(),
                                pat: swc_ecma_ast::Pat::Ident(ident_de(c).into()),
                            })
                            .collect();
                        ps.extend(fe.function.params.clone());
                        levantada.params = ps;
                        self.hoisted.push(swc_ecma_ast::FnDecl {
                            ident: id_gen.clone(),
                            declare: false,
                            function: levantada,
                        });
                        let mut args: Vec<swc_ecma_ast::ExprOrSpread> = caps
                            .iter()
                            .map(|c| swc_ecma_ast::ExprOrSpread {
                                spread: None,
                                expr: Box::new(Expr::Ident(ident_de(c))),
                            })
                            .collect();
                        for pp in &fe.function.params {
                            if let swc_ecma_ast::Pat::Ident(bi) = &pp.pat {
                                args.push(swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(Expr::Ident(ident_de(&bi.id.sym.to_string()))),
                                });
                            }
                        }
                        let chamada = Expr::Call(swc_ecma_ast::CallExpr {
                            span: Default::default(),
                            ctxt: Default::default(),
                            callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Ident(id_gen))),
                            args,
                            type_args: None,
                        });
                        let mut wrapper = fe.function.clone();
                        wrapper.is_generator = false;
                        wrapper.body = Some(swc_ecma_ast::BlockStmt {
                            span: Default::default(),
                            ctxt: Default::default(),
                            stmts: vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
                                span: Default::default(),
                                arg: Some(Box::new(chamada)),
                            })],
                        });
                        *e = Expr::Fn(swc_ecma_ast::FnExpr {
                            ident: None,
                            function: wrapper,
                        });
                        return;
                    }
                }
            }
            if fe.function.is_generator && pode_levantar {
                let name = format!("__genexpr_{}", self.counter);
                self.counter += 1;
                let ident = swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: name.as_str().into(),
                    optional: false,
                };
                self.hoisted.push(swc_ecma_ast::FnDecl {
                    ident: ident.clone(),
                    declare: false,
                    function: fe.function.clone(),
                });
                *e = Expr::Ident(ident);
            }
        }
    }

    /// Uma DECLARAÇÃO de generator aninhada (`function* g(){…}` dentro de outra
    /// função) é levantada para o topo pelo nome que ela já tem, e o statement
    /// original vira vazio. É o mesmo tratamento da fn-expression, só que o nome
    /// não precisa ser sintetizado.
    /// Depois de visitar um BLOCO, aplica os renames que os hoists feitos dentro
    /// dele deixaram pendentes — e só a ele. É o que mantém o rename escopado:
    /// `function* g` de duas funções irmãs vira `__gendecl_0`/`__gendecl_1`, cada
    /// um visível apenas no corpo que o declarava.
    fn visit_mut_block_stmt(&mut self, b: &mut swc_ecma_ast::BlockStmt) {
        use swc_ecma_visit::VisitMutWith;
        let marca = self.pendentes.len();
        let marca_hoisted = self.hoisted.len();
        let irmas_anteriores = std::mem::replace(
            &mut self.irmas_do_bloco,
            b.stmts
                .iter()
                .filter_map(|st| match st {
                    swc_ecma_ast::Stmt::Decl(swc_ecma_ast::Decl::Fn(fd))
                        if !fd.function.is_generator =>
                    {
                        Some(fd.ident.sym.to_string())
                    }
                    _ => None,
                })
                .collect(),
        );
        self.dentro_de_bloco += 1;
        b.visit_mut_children_with(self);
        self.dentro_de_bloco -= 1;

        // Um generator levantado que CHAMA uma função irmã (`function* g(){
        // yield o() }` ao lado de `function o(){…}`) perderia acesso a ela no
        // topo — a irmã continua no escopo original. É a forma canônica de um
        // módulo minificado (generator + auxiliares lado a lado na fábrica do
        // módulo), então as irmãs que ele usa sobem JUNTO.
        //
        // A cópia vai com o NOME ORIGINAL, não renomeada: a referência dentro da
        // fn levantada já usa esse nome, e renomeá-la exigiria reescrever de
        // forma consistente os dois lados — foi assim que uma tentativa anterior
        // produziu `call to unknown function __genaux_N`. Só sobe irmã cujo nome
        // ainda não exista no topo, para não colidir.
        if self.hoisted.len() > marca_hoisted {
            let usados: std::collections::HashSet<String> = self.hoisted[marca_hoisted..]
                .iter()
                .flat_map(|fd| Self::idents_livres(&fd.function))
                .collect();
            let ja_no_topo: std::collections::HashSet<String> = self
                .hoisted
                .iter()
                .map(|fd| fd.ident.sym.to_string())
                .collect();
            let mut sobem: Vec<swc_ecma_ast::FnDecl> = Vec::new();
            let mut renomes_aux: Vec<(String, String)> = Vec::new();
            for st in &b.stmts {
                if let swc_ecma_ast::Stmt::Decl(swc_ecma_ast::Decl::Fn(fd)) = st {
                    let nome = fd.ident.sym.to_string();
                    if !fd.function.is_generator
                        && usados.contains(&nome)
                        && !ja_no_topo.contains(&nome)
                        && !self.aux_subidas.contains(&nome)
                        && Self::sem_captura(&fd.function)
                    {
                        self.aux_subidas.insert(nome);
                        sobem.push(fd.clone());
                    } else if !fd.function.is_generator
                        && usados.contains(&nome)
                        && (ja_no_topo.contains(&nome) || self.aux_subidas.contains(&nome))
                        && Self::sem_captura(&fd.function)
                    {
                        // Uma irmã HOMÔNIMA de outro bloco já subiu (nome curto
                        // reciclado é a regra em código minificado). Sobe ESTA com
                        // um nome único e reescreve as referências a ela DENTRO
                        // das fns levantadas deste bloco — sem isto o generator
                        // daqui chamaria a irmã do OUTRO bloco, resultado errado e
                        // calado (`1,1` onde o Node dá `1,2`).
                        let unico = format!("__genaux_{}", self.counter);
                        self.counter += 1;
                        let mut copia = fd.clone();
                        copia.ident = swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: unico.as_str().into(),
                            optional: false,
                        };
                        renomes_aux.push((nome, unico));
                        sobem.push(copia);
                    }
                }
            }
            self.hoisted.extend(sobem);
            // As fns levantadas DESTE bloco passam a chamar a cópia renomeada.
            for (de, para) in &renomes_aux {
                for fd in &mut self.hoisted[marca_hoisted..] {
                    fd.function.visit_mut_with(&mut RenomeiaIdent {
                        de: de.clone(),
                        para: para.clone(),
                    });
                }
            }
        }

        if self.pendentes.len() > marca {
            let novos: Vec<(String, String)> = self.pendentes.drain(marca..).collect();
            for (de, para) in &novos {
                let mut r = RenomeiaIdent {
                    de: de.clone(),
                    para: para.clone(),
                };
                b.visit_mut_with(&mut r);
            }
        }
        self.irmas_do_bloco = irmas_anteriores;
    }

    fn visit_mut_stmt(&mut self, s: &mut swc_ecma_ast::Stmt) {
        use swc_ecma_visit::VisitMutWith;
        s.visit_mut_children_with(self);
        if let swc_ecma_ast::Stmt::Decl(swc_ecma_ast::Decl::Fn(fd)) = s {
            // `dentro_de_bloco == 0` é uma declaração de TOPO do módulo: aquela
            // já é tratada por `lower_decl` (o caminho que sempre funcionou), e
            // levantá-la de novo só renomeava a função e quebrava as chamadas.
            if self.dentro_de_bloco > 0
                && fd.function.is_generator
                && Self::sem_captura_ext(&fd.function, &self.irmas_do_bloco)
            {
                // O nome vai para o TOPO do módulo, então tem de ser único: duas
                // funções irmãs podem ambas declarar `function* g(){…}` (nome
                // curto é a regra em código minificado, e `g` é o favorito), e
                // levantar as duas com o nome original dava "Duplicate
                // definition of identifier". Renomeia para `__gendecl_N` e
                // reescreve as referências DENTRO do escopo que a declarava.
                let novo = format!("__gendecl_{}", self.counter);
                self.counter += 1;
                let ident = swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: novo.as_str().into(),
                    optional: false,
                };
                let original = fd.ident.sym.to_string();
                let mut levantada = fd.clone();
                levantada.ident = ident.clone();
                self.hoisted.push(levantada);
                // O rename fica PENDENTE e é aplicado só ao bloco que continha a
                // declaração (ver `visit_mut_block_stmt`). Aplicá-lo globalmente
                // vazava entre escopos: dois `function* g` irmãos faziam o
                // segundo resolver para o primeiro — resultado errado, calado.
                self.pendentes.push((original, novo));
                *s = swc_ecma_ast::Stmt::Empty(swc_ecma_ast::EmptyStmt {
                    span: Default::default(),
                });
            }
        }
    }
}

/// Troca um identificador por outro numa subárvore (o rename escopado do
/// generator levantado).
struct RenomeiaIdent {
    de: String,
    para: String,
}

impl swc_ecma_visit::VisitMut for RenomeiaIdent {
    fn visit_mut_ident(&mut self, i: &mut swc_ecma_ast::Ident) {
        if i.sym.as_ref() == self.de {
            i.sym = self.para.as_str().into();
        }
    }
}

impl GenExprHoister {
    /// O corpo do generator só menciona nomes que ele mesmo liga (params +
    /// declarações locais) ou globais conhecidos? Levantar um que CAPTURA o
    /// escopo de fora quebraria a captura silenciosamente — nesse caso ele fica
    /// onde está e o lowering falha honestamente, como antes.
    ///
    /// Aproximação conservadora e barata: coleta os identificadores livres do
    /// corpo e exige que todos estejam ligados pelo próprio generator. Um falso
    /// negativo (deixar de hoistear algo que seria seguro) só preserva o
    /// comportamento anterior; um falso positivo é o que não pode acontecer.
    /// Os identificadores que o corpo de `f` menciona (aproximação barata, usada
    /// só para decidir quais irmãs precisam subir junto).
    fn idents_livres(f: &SwcFunction) -> std::collections::HashSet<String> {
        use swc_ecma_visit::{Visit, VisitWith};
        #[derive(Default)]
        struct Usa(std::collections::HashSet<String>);
        impl Visit for Usa {
            fn visit_ident(&mut self, i: &swc_ecma_ast::Ident) {
                self.0.insert(i.sym.to_string());
            }
        }
        let mut u = Usa::default();
        if let Some(b) = &f.body {
            b.visit_with(&mut u);
        }
        u.0
    }

    /// O corpo usa `yield` em posição de VALOR (`const a = yield b`, `f(yield b)`)?
    ///
    /// Tem de ser decidido no corpo ORIGINAL, ANTES do desugar: o eager-buffer
    /// NÃO deixa o `yield` sobrando para ser detectado depois — ele reescreve
    /// todo `yield X` para `__gen_buf.push(X)`, inclusive em posição de valor,
    /// e aí `const a = yield x` silenciosamente vira `const a = push(...)`. Um
    /// valor errado, não um erro.
    ///
    /// A regra: um `yield` em posição de STATEMENT é o `expr` direto de um
    /// `ExprStmt` — esse o buffer expressa. Qualquer outro é de valor.
    fn usa_yield_como_valor(b: &swc_ecma_ast::BlockStmt) -> bool {
        use swc_ecma_visit::{Visit, VisitWith};
        #[derive(Default)]
        struct Achou {
            de_valor: bool,
        }
        impl Visit for Achou {
            fn visit_stmt(&mut self, st: &Stmt) {
                // `yield X;` sozinho: o buffer expressa. Desce só nos FILHOS do
                // yield (o argumento pode conter outro yield de valor).
                if let Stmt::Expr(es) = st {
                    if let Expr::Yield(y) = es.expr.as_ref() {
                        if let Some(arg) = &y.arg {
                            arg.visit_with(self);
                        }
                        return;
                    }
                }
                st.visit_children_with(self);
            }
            fn visit_yield_expr(&mut self, y: &swc_ecma_ast::YieldExpr) {
                self.de_valor = true;
                y.visit_children_with(self);
            }
            // NÃO desce em funções aninhadas: um `yield` lá dentro pertence a
            // OUTRO generator, não a este corpo.
            fn visit_function(&mut self, _: &SwcFunction) {}
            fn visit_arrow_expr(&mut self, _: &swc_ecma_ast::ArrowExpr) {}
        }
        let mut v = Achou::default();
        b.visit_with(&mut v);
        v.de_valor
    }

    /// Os nomes LIVRES do corpo — o que a função captura do escopo em que está.
    /// Mesma análise de `sem_captura_ext` (que só responde se a lista é vazia),
    /// aqui devolvendo a lista em ordem de primeiro uso, sem repetição.
    fn capturas(f: &SwcFunction, irmas: &std::collections::HashSet<String>) -> Vec<String> {
        use swc_ecma_visit::{Visit, VisitWith};

        #[derive(Default)]
        struct Livres {
            ligados: std::collections::HashSet<String>,
            usados: Vec<String>,
        }
        impl Visit for Livres {
            fn visit_ident(&mut self, i: &swc_ecma_ast::Ident) {
                self.usados.push(i.sym.to_string());
            }
            fn visit_var_declarator(&mut self, d: &swc_ecma_ast::VarDeclarator) {
                if let swc_ecma_ast::Pat::Ident(bi) = &d.name {
                    self.ligados.insert(bi.id.sym.to_string());
                }
                d.visit_children_with(self);
            }
            fn visit_fn_decl(&mut self, fd: &swc_ecma_ast::FnDecl) {
                self.ligados.insert(fd.ident.sym.to_string());
                fd.visit_children_with(self);
            }
            fn visit_param(&mut self, p: &swc_ecma_ast::Param) {
                if let swc_ecma_ast::Pat::Ident(bi) = &p.pat {
                    self.ligados.insert(bi.id.sym.to_string());
                }
                p.visit_children_with(self);
            }
        }

        let mut v = Livres::default();
        for p in &f.params {
            if let swc_ecma_ast::Pat::Ident(bi) = &p.pat {
                v.ligados.insert(bi.id.sym.to_string());
            }
        }
        if let Some(b) = &f.body {
            b.visit_with(&mut v);
        }
        let mut out: Vec<String> = Vec::new();
        for u in &v.usados {
            if v.ligados.contains(u) || GLOBAIS_VISIVEIS.contains(&u.as_str()) || irmas.contains(u) {
                continue;
            }
            if !out.contains(u) {
                out.push(u.clone());
            }
        }
        out
    }

    fn sem_captura(f: &SwcFunction) -> bool {
        Self::sem_captura_ext(f, &std::collections::HashSet::new())
    }

    fn sem_captura_ext(f: &SwcFunction, irmas: &std::collections::HashSet<String>) -> bool {
        use swc_ecma_visit::{Visit, VisitWith};

        #[derive(Default)]
        struct Livres {
            ligados: std::collections::HashSet<String>,
            usados: Vec<String>,
        }
        impl Visit for Livres {
            fn visit_ident(&mut self, i: &swc_ecma_ast::Ident) {
                self.usados.push(i.sym.to_string());
            }
            fn visit_var_declarator(&mut self, d: &swc_ecma_ast::VarDeclarator) {
                if let swc_ecma_ast::Pat::Ident(bi) = &d.name {
                    self.ligados.insert(bi.id.sym.to_string());
                }
                d.visit_children_with(self);
            }
            fn visit_fn_decl(&mut self, fd: &swc_ecma_ast::FnDecl) {
                self.ligados.insert(fd.ident.sym.to_string());
                fd.visit_children_with(self);
            }
            fn visit_param(&mut self, p: &swc_ecma_ast::Param) {
                if let swc_ecma_ast::Pat::Ident(bi) = &p.pat {
                    self.ligados.insert(bi.id.sym.to_string());
                }
                p.visit_children_with(self);
            }
        }

        let mut v = Livres::default();
        for p in &f.params {
            if let swc_ecma_ast::Pat::Ident(bi) = &p.pat {
                v.ligados.insert(bi.id.sym.to_string());
            }
        }
        if let Some(b) = &f.body {
            b.visit_with(&mut v);
        }
        v.usados.iter().all(|u| {
            v.ligados.contains(u) || GLOBAIS_VISIVEIS.contains(&u.as_str()) || irmas.contains(u)
        })
    }
}

/// Hoists a NON-generator function/arrow expression used as the RHS *value* of a
/// top-level member-assignment (`F.make = function(x){ … }` / `F.make = (x) => …`)
/// into a named top-level function declaration (`__fnprop_N`), rewriting the
/// assignment value to an identifier reference. This lets the assignment lower
/// through the function-property side-table (`F.make = __fnprop_N`, recorded via
/// `__rtsadp_fn_set_prop` with the reified function VALUE) and a later
/// `F.make(args)` invoke that stored function value.
///
/// `visit_mut_function` is a no-op so we never descend into a function body —
/// only true top-level member-assignment RHS fn-exprs are hoisted, avoiding
/// broken captures (a fn-expr that captures outer locals is left in place and
/// still bails downstream, the honesty floor).
#[derive(Default)]
struct MemberAssignFnHoister {
    hoisted: Vec<swc_ecma_ast::FnDecl>,
    counter: usize,
}

thread_local! {
    /// Namespace prefix for hoisted `__fnprop_N` names, so a PRELUDE build and a
    /// USER build (parsed SEPARATELY, each counter starting at 0) do not collide
    /// on `__fnprop_0` when merged — the exact problem `PRELUDE_ARROW_NS` already
    /// solves for extracted arrows. Empty for the normal (user) parse; the codegen
    /// sets it around the prelude parse via [`set_fnprop_ns`].
    static FNPROP_NS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Set the `__fnprop_N` namespace prefix for subsequent parses on this thread
/// (see [`FNPROP_NS`]). Pass `""` to clear it. The codegen wraps the prelude
/// parse: `set_fnprop_ns("p")` … parse … `set_fnprop_ns("")`.
pub fn set_fnprop_ns(ns: &str) {
    FNPROP_NS.with(|n| *n.borrow_mut() = ns.to_string());
}

impl MemberAssignFnHoister {
    /// Build a fresh `__fnprop_<ns>N` ident and hoist `function`/`is_generator=false`
    /// expr into a named top-level decl; returns the ident to replace the site.
    fn hoist(&mut self, function: SwcFunction) -> swc_ecma_ast::Ident {
        let name = FNPROP_NS.with(|n| format!("__fnprop_{}{}", n.borrow(), self.counter));
        self.counter += 1;
        let ident = swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: name.as_str().into(),
            optional: false,
        };
        self.hoisted.push(swc_ecma_ast::FnDecl {
            ident: ident.clone(),
            declare: false,
            function: Box::new(function),
        });
        ident
    }
}

impl swc_ecma_visit::VisitMut for MemberAssignFnHoister {
    fn visit_mut_function(&mut self, _f: &mut SwcFunction) {
        // no-op: do not descend into function bodies (avoid capturing hoists).
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        use swc_ecma_visit::VisitMutWith;
        e.visit_mut_children_with(self);
        // Target only `<member> = <fn-expr/arrow>` (a Member assign target with a
        // function VALUE) — the dual-callable static pattern (`F.make = function`).
        let Expr::Assign(assign) = e else { return };
        if !matches!(
            assign.left,
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(_))
        ) {
            return;
        }
        match assign.right.as_mut() {
            Expr::Fn(fe) if !fe.function.is_generator && !fe.function.is_async => {
                let ident = self.hoist((*fe.function).clone());
                assign.right = Box::new(Expr::Ident(ident));
            }
            Expr::Arrow(arrow) if !arrow.is_generator && !arrow.is_async => {
                let function = arrow_to_function(arrow);
                let ident = self.hoist(function);
                assign.right = Box::new(Expr::Ident(ident));
            }
            // `F.prototype = Object.create(proto, { m: { value: function(){…} },
            // … })` — the ES5 descriptor-map form: hoist each descriptor's
            // `value:` fn-expr exactly like the direct `F.prototype.m = fn`
            // form, so the ctor-fn lift (`ctorfn::parse_proto_descriptor`) sees
            // hoisted idents. Same capture-risk profile as the direct form (a
            // top-level statement, no enclosing fn scope).
            Expr::Call(call) => {
                self.hoist_object_create_descriptor_values(call);
            }
            _ => {}
        }
    }
}

impl MemberAssignFnHoister {
    /// When `call` is `Object.create(_, { k: { value: <fn-expr>, … }, … })`,
    /// hoist each `value:` fn/arrow to a `__fnprop_N` decl and rewrite the
    /// descriptor value to the ident. Any other call is left untouched.
    fn hoist_object_create_descriptor_values(&mut self, call: &mut swc_ecma_ast::CallExpr) {
        use swc_ecma_ast::{Callee, MemberProp, Prop, PropName, PropOrSpread};
        let Callee::Expr(callee) = &call.callee else {
            return;
        };
        let Expr::Member(cm) = callee.as_ref() else {
            return;
        };
        let (MemberProp::Ident(create), Expr::Ident(obj)) = (&cm.prop, cm.obj.as_ref()) else {
            return;
        };
        if create.sym.as_ref() != "create" || obj.sym.as_ref() != "Object" || call.args.len() != 2
        {
            return;
        }
        let Expr::Object(desc_map) = call.args[1].expr.as_mut() else {
            return;
        };
        for p in &mut desc_map.props {
            let PropOrSpread::Prop(prop) = p else { continue };
            let Prop::KeyValue(kv) = prop.as_mut() else {
                continue;
            };
            let Expr::Object(desc) = kv.value.as_mut() else {
                continue;
            };
            for dp in &mut desc.props {
                let PropOrSpread::Prop(dprop) = dp else { continue };
                let Prop::KeyValue(dkv) = dprop.as_mut() else {
                    continue;
                };
                let is_value = matches!(&dkv.key, PropName::Ident(id) if id.sym.as_ref() == "value");
                if !is_value {
                    continue;
                }
                match dkv.value.as_mut() {
                    Expr::Fn(fe) if !fe.function.is_generator && !fe.function.is_async => {
                        let ident = self.hoist((*fe.function).clone());
                        dkv.value = Box::new(Expr::Ident(ident));
                    }
                    Expr::Arrow(arrow) if !arrow.is_generator && !arrow.is_async => {
                        let function = arrow_to_function(arrow);
                        let ident = self.hoist(function);
                        dkv.value = Box::new(Expr::Ident(ident));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Hoists a class EXPRESSION (`const D = class {…}` / `globalThis.X = class M {…}`)
/// into a named top-level class declaration (`__classexpr_N`), rewriting the
/// expression site to an identifier. Once the site is an `Ident` naming a
/// top-level class, the existing class-as-value machinery takes over: `const D =
/// __classexpr_N` reifies the class VALUE, `new D()` constructs, and methods
/// dispatch on the result (D is a static class reference). `globalThis.X = class
/// M {…}` becomes `globalThis.X = __classexpr_N` — a known-class ident — so the
/// caminho-A pre-pass tracks it and `const G = globalThis.X; new G().m()` works
/// end-to-end. `visit_mut_function` is a no-op so we never descend into a
/// function/method body: only TRUE top-level class-exprs are hoisted (one nested
/// inside a body could capture outer locals; left in place it still bails
/// downstream — the honesty floor). A named class-expr's inner self-reference
/// (`class Foo { m() { return Foo; } }`) resolves to the original name, which is
/// not the hoisted decl's name, so it bails downstream (an accepted edge).
#[derive(Default)]
struct ClassExprHoister {
    hoisted: Vec<swc_ecma_ast::ClassDecl>,
    counter: usize,
    /// `const X = class {…}` → `X` → the hoisted decl's name. Lets a LATER
    /// `class extends X` resolve to the hoisted class: the binding is a value
    /// alias of a top-level class, so `extends X` names a user class in this
    /// program — without this it bailed as "extends unknown class `X`".
    aliases: std::collections::HashMap<String, String>,
    /// Names already bound at the top level (decls + earlier hoists) — a named
    /// class expression may only hoist under its own name when it is free here.
    taken: std::collections::HashSet<String>,
}

impl ClassExprHoister {
    /// Rewrite every hoisted decl's `extends <alias-ident>` to the hoisted class
    /// it aliases. Runs after the whole traversal so a forward-ordered alias
    /// (declared before its user, the only order JS allows for a `const`) is
    /// already known.
    fn resolve_alias_supers(&mut self) {
        for d in &mut self.hoisted {
            let Some(sup) = d.class.super_class.as_deref() else {
                continue;
            };
            let Expr::Ident(id) = sup else { continue };
            if let Some(target) = self.aliases.get(&id.sym.to_string()) {
                let mut ident = id.clone();
                ident.sym = target.as_str().into();
                d.class.super_class = Some(Box::new(Expr::Ident(ident)));
            }
        }
    }
}

impl swc_ecma_visit::VisitMut for ClassExprHoister {
    fn visit_mut_function(&mut self, _f: &mut SwcFunction) {
        // no-op: do not descend into function/method bodies (avoid capturing hoists).
    }

    fn visit_mut_var_declarator(&mut self, d: &mut swc_ecma_ast::VarDeclarator) {
        use swc_ecma_visit::VisitMutWith;
        // NAME INFERENCE (`const Point = class {…}` → the class's name IS
        // "Point", per JS): stamp the binding's name onto the anonymous class
        // BEFORE the hoist so it hoists under that name — which is what makes
        // `Point.name` and an inner self-reference read correctly. The binding is
        // then `const Point = Point`, an alias of the hoisted class (the RHS ident
        // resolves to the top-level decl, not the local).
        if let (swc_ecma_ast::Pat::Ident(bind), Some(Expr::Class(ce))) =
            (&d.name, d.init.as_deref_mut())
        {
            let n = bind.id.sym.to_string();
            if ce.ident.is_none() && !self.taken.contains(&n) {
                ce.ident = Some(bind.id.clone());
            }
        }
        d.visit_mut_children_with(self);
        // After the child visit the initializer is already the hoisted ident.
        let (Some(swc_ecma_ast::Pat::Ident(name)), Some(init)) = (Some(&d.name), d.init.as_deref())
        else {
            return;
        };
        if let Expr::Ident(src) = init {
            let src = src.sym.to_string();
            if src.starts_with("__classexpr_") {
                self.aliases.insert(name.id.sym.to_string(), src);
            }
        }
    }

    fn visit_mut_expr(&mut self, e: &mut Expr) {
        use swc_ecma_visit::VisitMutWith;
        e.visit_mut_children_with(self);
        if let Expr::Class(ce) = e {
            // A NAMED class expression (`const N = class Inner {…}`) hoists under
            // ITS OWN name whenever that name is free at the top level: the inner
            // self-reference the JS scoping gives it (`Inner.name` inside a method,
            // `static make(): Inner`) then resolves to the hoisted decl. Under a
            // synthetic `__classexpr_N` name those references were unbound idents.
            // A taken name falls back to the synthetic one (the reference still
            // bails downstream — never a wrong binding).
            let name = match ce.ident.as_ref().map(|i| i.sym.to_string()) {
                Some(n) if !self.taken.contains(&n) => {
                    self.taken.insert(n.clone());
                    n
                }
                _ => {
                    let n = format!("__classexpr_{}", self.counter);
                    self.counter += 1;
                    n
                }
            };
            let ident = swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: name.as_str().into(),
                optional: false,
            };
            self.hoisted.push(swc_ecma_ast::ClassDecl {
                ident: ident.clone(),
                declare: false,
                class: ce.class.clone(),
            });
            *e = Expr::Ident(ident);
        }
    }
}

fn lower_program(cm: &Lrc<SourceMap>, source: &SwcProgram) -> Program {
    let mut program = Program::default();

    // (cross-runtime #344) Hoist top-level generator fn-exprs to named decls so
    // they take the lazy state-machine path (value-passing `g.next(v)`).
    let mut owned = source.clone();
    {
        use swc_ecma_visit::VisitMutWith;
        let mut hoister = GenExprHoister::default();
        owned.visit_mut_with(&mut hoister);
        if !hoister.hoisted.is_empty() {
            match &mut owned {
                SwcProgram::Module(m) => {
                    for (i, fd) in hoister.hoisted.into_iter().enumerate() {
                        m.body
                            .insert(i, ModuleItem::Stmt(Stmt::Decl(Decl::Fn(fd))));
                    }
                }
                SwcProgram::Script(s) => {
                    for (i, fd) in hoister.hoisted.into_iter().enumerate() {
                        s.body.insert(i, Stmt::Decl(Decl::Fn(fd)));
                    }
                }
            }
        }
    }
    // Hoist a function/arrow expression assigned to a member (`F.make = function`)
    // into a named top-level decl so the assignment stores a function VALUE.
    {
        use swc_ecma_visit::VisitMutWith;
        let mut hoister = MemberAssignFnHoister::default();
        owned.visit_mut_with(&mut hoister);
        if !hoister.hoisted.is_empty() {
            match &mut owned {
                SwcProgram::Module(m) => {
                    for (i, fd) in hoister.hoisted.into_iter().enumerate() {
                        m.body
                            .insert(i, ModuleItem::Stmt(Stmt::Decl(Decl::Fn(fd))));
                    }
                }
                SwcProgram::Script(s) => {
                    for (i, fd) in hoister.hoisted.into_iter().enumerate() {
                        s.body.insert(i, Stmt::Decl(Decl::Fn(fd)));
                    }
                }
            }
        }
    }
    // Hoist class EXPRESSIONS (`const D = class {…}` / `globalThis.X = class M {…}`)
    // to named top-level class decls so the class-as-value machinery (reify +
    // new-thunk, caminho A) takes over via the rewritten ident.
    {
        use swc_ecma_visit::VisitMutWith;
        let mut hoister = ClassExprHoister::default();
        hoister.taken = top_level_names(&owned);
        owned.visit_mut_with(&mut hoister);
        hoister.resolve_alias_supers();
        if !hoister.hoisted.is_empty() {
            match &mut owned {
                SwcProgram::Module(m) => {
                    for (i, cd) in hoister.hoisted.into_iter().enumerate() {
                        m.body
                            .insert(i, ModuleItem::Stmt(Stmt::Decl(Decl::Class(cd))));
                    }
                }
                SwcProgram::Script(s) => {
                    for (i, cd) in hoister.hoisted.into_iter().enumerate() {
                        s.body.insert(i, Stmt::Decl(Decl::Class(cd)));
                    }
                }
            }
        }
    }
    let source = &owned;

    // (#271) Coleta const strings top-level antes do lowering para resolver
    // computed class keys (`[key]() {}` com `const key = "sum"`).
    register_const_strings(source);

    match source {
        SwcProgram::Module(module) => {
            for item in &module.body {
                lower_module_item(cm, item, &mut program.items);
            }
        }
        SwcProgram::Script(script) => {
            for stmt in &script.body {
                lower_stmt(cm, stmt, &mut program.items);
            }
        }
    }

    program
}

fn lower_module_item(cm: &Lrc<SourceMap>, item: &ModuleItem, out: &mut Vec<Item>) {
    match item {
        ModuleItem::ModuleDecl(decl) => lower_module_decl(cm, decl, out),
        ModuleItem::Stmt(stmt) => lower_stmt(cm, stmt, out),
    }
}

fn lower_module_decl(cm: &Lrc<SourceMap>, decl: &ModuleDecl, out: &mut Vec<Item>) {
    match decl {
        ModuleDecl::Import(import_decl) => {
            out.push(Item::Import(lower_import_decl(cm, import_decl)));
        }
        ModuleDecl::ExportDecl(export_decl) => {
            let before = out.len();
            lower_decl(cm, &export_decl.decl, out);
            // Marca os Items produzidos por esta `export` decl como exportados,
            // para o resolver de modulos (motor novo) montar o export-set.
            for item in &mut out[before..] {
                match item {
                    Item::Function(f) => f.exported = true,
                    Item::Class(c) => c.exported = true,
                    // `export const x = 1` vira Item::Statement — marca o
                    // RawStmt pro resolver incluir consts no export-set.
                    Item::Statement(rts_ast::ast::Statement::Raw(r)) => r.exported = true,
                    _ => {}
                }
            }
        }
        ModuleDecl::ExportNamed(export_named) => {
            // (#307) Re-export: \`export { x } from \"./mod\"\` traduzido como
            // import implicito + propaga os nomes pra resolver downstream.
            // Sem o \`from\`, e' apenas re-export local de Item ja' definido
            // no modulo — codegen ja' inclui Item::Function/Class no scope,
            // entao nao precisa fazer nada (o nome esta visivel).
            if let Some(src) = export_named.src.as_ref() {
                use swc_ecma_ast::ExportSpecifier;
                let mut names = Vec::new();
                for spec in &export_named.specifiers {
                    match spec {
                        ExportSpecifier::Named(n) => {
                            // `export { orig } from "./mod"`         — local == orig
                            // `export { orig as exported } from "./mod"` — local == exported
                            //
                            // O alias `exported` afeta o nome visivel no consumidor
                            // (este modulo). No nivel do Item::Import emitido aqui,
                            // queremos que o binding local desse modulo seja
                            // `exported` (ou `orig` se nao houver alias) — assim
                            // outros que importarem deste modulo encontram o nome
                            // certo.
                            let orig = module_export_name(&n.orig);
                            let local = n
                                .exported
                                .as_ref()
                                .map(module_export_name)
                                .unwrap_or_else(|| orig.clone());
                            names.push(ImportName { orig, local });
                        }
                        ExportSpecifier::Namespace(ns) => {
                            // `export * as foo from "./mod"`. SWC empacota dentro
                            // de ExportNamed quando ha agrupamento; emit Item
                            // dedicado pra que o flatten possa expandir
                            // <foo>.<exp> -> <exp> em local_alias_map.
                            let local = match &ns.name {
                                swc_ecma_ast::ModuleExportName::Ident(id) => {
                                    id.sym.to_string()
                                }
                                swc_ecma_ast::ModuleExportName::Str(s) => {
                                    s.value.to_string_lossy().to_string()
                                }
                            };
                            out.push(Item::ExportNamespace(ExportNamespaceDecl {
                                local,
                                from: src.value.to_string_lossy().to_string(),
                                span: convert_span(cm, export_named.span),
                            }));
                        }
                        ExportSpecifier::Default(_) => {
                            // `export foo from src` (re-export do default) — follow-up.
                        }
                    }
                }
                if !names.is_empty() {
                    let import = ImportDecl {
                        names,
                        default_name: None,
                        from: src.value.to_string_lossy().to_string(),
                        span: convert_span(cm, export_named.span),
                        reexport: true,
                    };
                    out.push(Item::Import(import));
                }
            }
            // \`export { foo }\` (sem from) — Item ja' definido localmente,
            // nada a emitir. Caso \`export * from \"./mod\"\` (sem named
            // specifiers, com src) nao e' coberto aqui — follow-up.
        }
        ModuleDecl::ExportAll(export_all) => {
            // (#307) \`export * from \"./mod\"\` — re-exporta todos os names
            // do modulo source. Sem visibilidade do graph aqui; emit o
            // import com names vazio sinaliza pro pipeline de imports
            // resolver e injetar todos os exports.
            // Workaround minimalista: import vazio que o pipeline pode
            // expandir. Se nao expandir, e' no-op — usuario pode usar
            // import explicito como fallback.
            let import = ImportDecl {
                names: Vec::new(),
                default_name: None,
                from: export_all.src.value.to_string_lossy().to_string(),
                span: convert_span(cm, export_all.span),
                reexport: true,
            };
            out.push(Item::Import(import));
        }
        ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
            DefaultDecl::Class(class_expr) => {
                if let Some(name) = class_expr.ident.as_ref().map(|ident| ident.sym.to_string()) {
                    let mut c = lower_class(cm, &name, &class_expr.class, class_expr.span());
                    c.exported = true;
                    c.exported_default = true;
                    out.push(Item::Class(c));
                } else {
                    push_raw_statement(cm, decl.span(), out);
                }
            }
            DefaultDecl::Fn(fn_expr) => {
                if let Some(name) = fn_expr.ident.as_ref().map(|ident| ident.sym.to_string()) {
                    let mut f =
                        lower_function(cm, &name, &fn_expr.function, fn_expr.function.span);
                    f.exported = true;
                    f.exported_default = true;
                    out.push(Item::Function(f));
                } else {
                    push_raw_statement(cm, decl.span(), out);
                }
            }
            DefaultDecl::TsInterfaceDecl(interface_decl) => {
                out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
            }
        },
        _ => push_raw_statement(cm, decl.span(), out),
    }
}

fn lower_stmt(cm: &Lrc<SourceMap>, stmt: &Stmt, out: &mut Vec<Item>) {
    match stmt {
        Stmt::Decl(decl) => lower_decl(cm, decl, out),
        _ => push_raw_statement_with_stmt(cm, stmt.span(), Some(stmt), out),
    }
}

/// (#207 fatia 3) Flag `RTS_ASYNC_SM`: agora default ON (async via state-machine
/// cooperativo). So' desliga com `0`/`off`/`false`/`none` — volta ao caminho
/// thread-blocking de fallback. Inelegiveis (try/catch em volta de await, await
/// aninhado) caem no fallback automaticamente, entao o flip e' seguro.
fn async_sm_enabled() -> bool {
    !matches!(
        std::env::var("RTS_ASYNC_SM").ok().as_deref(),
        Some("0") | Some("off") | Some("false") | Some("none")
    )
}

/// True when `decl` is a TypeScript AMBIENT declaration (`declare ...`) — a
/// type-only construct that carries NO implementation and must generate NO code.
/// Covers `declare function f(): T;`, `declare var/let/const x: T;`,
/// `declare class C {}`, `declare enum E {}`, and `declare module`/
/// `declare namespace`/`declare global { ... }`. An ambient `declare function`
/// has no body at all (a real, empty `function f() {}` is NOT ambient). Dropping
/// these avoids synthesizing bodyless functions (e.g. `__ns_global_String`) that
/// would later bail in codegen with "may fall through without returning".
fn is_ambient_decl(decl: &Decl) -> bool {
    match decl {
        Decl::Fn(d) => d.declare,
        Decl::Class(d) => d.declare,
        Decl::Var(d) => d.declare,
        Decl::TsEnum(d) => d.declare,
        // `declare module`, `declare namespace`, and `declare global { ... }`
        // are all `TsModuleDecl` with `declare: true`. `declare global` also sets
        // `global: true`; either way it is ambient and emits no code.
        Decl::TsModule(d) => d.declare || d.global,
        // Interfaces are always type-only (no runtime form) — handled as no-op
        // below regardless; not treated as "ambient" here.
        Decl::TsInterface(_) | Decl::TsTypeAlias(_) | Decl::Using(_) => false,
    }
}

fn lower_decl(cm: &Lrc<SourceMap>, decl: &Decl, out: &mut Vec<Item>) {
    // Drop TS ambient declarations (`declare ...`) entirely — type-only, no code.
    if is_ambient_decl(decl) {
        return;
    }
    match decl {
        Decl::Class(class_decl) => {
            out.push(Item::Class(lower_class_decl(cm, class_decl)));
            // Decorators TC39: emite chamada a cada decorator com target=0
            // (handle nominal). Resultado eh descartado (registration-style
            // decorators tem efeito por side-effect). Decoradores de
            // metodo/param sao parseados mas tambem ignorados ate ter
            // metadata real.
            // Decorators TS executam bottom-up (do mais perto da classe
            // para o mais distante).
            for dec in class_decl.class.decorators.iter().rev() {
                emit_decorator_call_stmt(cm, &dec.expr, dec.span, out);
            }
        }
        Decl::Fn(fn_decl) => {
            // (cross-runtime #392) `async function*` -> async generator lazy SM
            // (yield + await combinados). `.next()` devolve Promise<{value,done}>.
            // ANTES do generator sync e do eager-buffer (que nao fazem await/
            // .next()/throw lazy). Inelegivel => cai nos caminhos abaixo.
            if fn_decl.function.is_generator
                && fn_decl.function.is_async
                && async_sm_enabled()
            {
                if let Some((ctor, state_fn)) = crate::generator_sm::try_build_async_gen(
                    &fn_decl.ident.sym.to_string(),
                    &fn_decl.function,
                ) {
                    let mut sf = lower_fn_decl(cm, &state_fn);
                    sf.return_type = Some("i64".to_string());
                    let mut cf = lower_fn_decl(cm, &ctor);
                    cf.return_type = Some("i64".to_string());
                    out.push(Item::Function(sf));
                    out.push(Item::Function(cf));
                    return;
                }
            }
            // (#477) Generator elegivel -> state-machine lazy (2 fns). Caso
            // contrario, caminho normal (eager-buffer para generators).
            if fn_decl.function.is_generator {
                if let Some((ctor, state_fn)) = crate::generator_sm::try_build(
                    &fn_decl.ident.sym.to_string(),
                    &fn_decl.function,
                ) {
                    // Ambas as fns sinteticas retornam i64 (handle/valor).
                    let mut sf = lower_fn_decl(cm, &state_fn);
                    sf.return_type = Some("i64".to_string());
                    let mut cf = lower_fn_decl(cm, &ctor);
                    cf.return_type = Some("i64".to_string());
                    out.push(Item::Function(sf));
                    out.push(Item::Function(cf));
                    return;
                }
            }
            // (#207 async-SM, flag RTS_ASYNC_SM) async fn elegivel -> state-
            // machine cooperativa (await=suspensao que cede a microtask queue).
            // Inelegivel ou flag off => caminho thread-blocking de
            // expand_async_functions (intacto).
            if fn_decl.function.is_async
                && !fn_decl.function.is_generator
                && async_sm_enabled()
            {
                if let Some((ctor, state_fn)) = crate::generator_sm::try_build_async(
                    &fn_decl.ident.sym.to_string(),
                    &fn_decl.function,
                ) {
                    let mut sf = lower_fn_decl(cm, &state_fn);
                    sf.return_type = Some("i64".to_string());
                    let mut cf = lower_fn_decl(cm, &ctor);
                    cf.return_type = Some("i64".to_string());
                    out.push(Item::Function(sf));
                    out.push(Item::Function(cf));
                    return;
                }
            }
            out.push(Item::Function(lower_fn_decl(cm, fn_decl)));
        }
        Decl::TsInterface(interface_decl) => {
            out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
        }
        Decl::TsEnum(enum_decl) => {
            // Desugar `enum E { A, B = 5, C }` em
            // `const E = { A: 0, B: 5, C: 6 };` — objeto literal que o
            // codegen já trata via path normal de member access.
            //
            // Numeric enums: auto-incremento começando em 0; init explícito
            // (numérico) reseta o contador.
            // String enums: init obrigatório, valor literal.
            // Mistos seguem a regra do membro vigente.
            if let Some(stmt) = lower_ts_enum_to_const(enum_decl) {
                push_raw_statement_with_stmt(cm, enum_decl.span, Some(&stmt), out);
            }
        }
        Decl::TsModule(module_decl) => {
            // \`namespace Foo { export function f() {} ... }\`
            // Desugar:
            //   - Cada \`export function bar(...)\` vira \`function __ns_Foo_bar(...)\`
            //     no top-level (mangled).
            //   - Cada \`export class C {}\` vira \`class __ns_Foo_C {}\`.
            //   - Cada \`export const x = ...\` vira \`const __ns_Foo_x = ...\`.
            //   - Por fim, gera \`const Foo = { bar: __ns_Foo_bar, ... }\` pra
            //     habilitar \`Foo.bar()\` via member access + call_indirect.
            lower_ts_namespace(cm, module_decl, out);
        }
        Decl::Var(var_decl) if try_lower_fn_expr_decl(cm, var_decl, out) => {
            // All declarators were function/arrow expressions and have been
            // emitted as Item::Function above.
        }
        _ => {
            // Preserve non-function/class declarations (e.g. let/const) as a
            // real SWC statement so codegen can lower module-scope globals.
            let stmt = Stmt::Decl(decl.clone());
            push_raw_statement_with_stmt(cm, decl.span(), Some(&stmt), out);
        }
    }
}

/// Rewrites `const NAME = function(...) { ... }` (or arrow with block body)
/// into a synthetic `Item::Function` so callers can invoke it like a regular
/// named function. Returns true only if *every* declarator was a supported
/// function expression; otherwise the caller falls back to the statement path.
fn try_lower_fn_expr_decl(cm: &Lrc<SourceMap>, var_decl: &VarDecl, out: &mut Vec<Item>) -> bool {
    let mut pending = Vec::new();
    for decl in &var_decl.decls {
        let Pat::Ident(binding) = &decl.name else {
            return false;
        };
        let Some(init) = &decl.init else {
            return false;
        };
        let name = binding.id.sym.to_string();

        match init.as_ref() {
            Expr::Fn(fn_expr) => {
                let span = fn_expr.function.span;
                // Named function expression: o nome interno (`function fact(){...}`)
                // so e visivel dentro do body. Reescreve referencias a `fact`
                // para o binding externo (`factorial`) antes de descer.
                let mut function = (*fn_expr.function).clone();
                if let Some(inner_id) = &fn_expr.ident {
                    let inner_name = inner_id.sym.as_ref();
                    if inner_name != name {
                        rename_ident_in_function(&mut function, inner_name, &name);
                    }
                }
                pending.push(lower_function(cm, &name, &function, span));
            }
            Expr::Arrow(arrow) => {
                let mut synthetic = arrow_to_function(arrow);
                // (#455) Quando a binding tem annotation `() => T` mas o arrow
                // nao tem `: T` proprio, propaga o return type. Sem isto,
                // codegen inferia `i64` por heuristica e `fn()` retornava 0
                // pra arrows que retornam f64.
                if synthetic.return_type.is_none() {
                    if let Some(ann) = &binding.type_ann {
                        if let Some(ret) = extract_fn_return_type_ann(&ann.type_ann) {
                            synthetic.return_type = Some(ret);
                        }
                    }
                }
                let mut decl = lower_function(cm, &name, &synthetic, arrow.span);
                // (#450 follow-up) Se a fn ainda nao tem return_type
                // declarado mas o body tem `return <expr>`, usa heuristica
                // i64 (mesmo padrao de hoist_fn). Sem isto, arrow expression
                // body `(a,b) => a+b` virava fn void e o codegen descartava
                // o valor de retorno.
                if decl.return_type.is_none() && body_has_return_value(&decl.body) {
                    // (cross-runtime #294) Detecta string concat em return.
                    // Se body retorna `a + ":" + b`, inferir handle (string).
                    if body_returns_string_concat(arrow) {
                        decl.return_type = Some("string".to_string());
                    } else {
                        decl.return_type = Some("i64".to_string());
                    }
                }
                pending.push(decl);
            }
            _ => return false,
        }
    }
    for fn_decl in pending {
        out.push(Item::Function(fn_decl));
    }
    true
}

/// (#455) Extrai o return type annotation de uma TS function type
/// (\`() => T\` ou \`(x) => T\`). Retorna `Some(Box<TsTypeAnn>)` que
/// pode ser plugado no \`SwcFunction.return_type\` do arrow synthetic.
fn extract_fn_return_type_ann(ts_type: &swc_ecma_ast::TsType) -> Option<Box<swc_ecma_ast::TsTypeAnn>> {
    use swc_ecma_ast::{TsFnOrConstructorType, TsType, TsUnionOrIntersectionType};
    match ts_type {
        TsType::TsFnOrConstructorType(TsFnOrConstructorType::TsFnType(fn_type)) => {
            Some(fn_type.type_ann.clone())
        }
        // `(() => T) | null` ou `(() => T) | undefined` — extrai o primeiro
        // TsFnType da uniao.
        TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(u)) => {
            u.types.iter().find_map(|t| extract_fn_return_type_ann(t))
        }
        TsType::TsParenthesizedType(inner) => extract_fn_return_type_ann(&inner.type_ann),
        _ => None,
    }
}

/// Builds a `swc_ecma_ast::Function` from an `ArrowExpr` so it can flow
/// through the same lowering path as regular function declarations.
///
/// For expression-bodied arrows (`(x) => x * 2`) the single expression is
/// wrapped in a synthetic `{ return <expr>; }` so downstream codegen only
/// needs to know how to handle block-bodied functions.
/// (#450) Checa se algum stmt do body lower'ed contem return com valor.
/// Usado pra inferir return_type quando arrow nao tem anotacao explicita.
/// (cross-runtime #294) Heuristica simples: body do arrow contem
/// retorno cuja expr eh BinExpr Add com algum operando String/Tpl.
fn body_returns_string_concat(arrow: &ArrowExpr) -> bool {
    use swc_ecma_ast::{BinaryOp, Expr};
    // (cross-runtime) Coleta params anotados `string` — `(a:string,b:string)
    // => a + b` deve inferir string. Sem isto a heuristica nao sabe o tipo
    // dos params e o concat vira i64 (handle cru).
    let mut str_params: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for p in &arrow.params {
        if let swc_ecma_ast::Pat::Ident(bi) = p {
            if let Some(ann) = &bi.type_ann {
                if let swc_ecma_ast::TsType::TsKeywordType(k) = ann.type_ann.as_ref() {
                    if matches!(k.kind, swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword) {
                        str_params.insert(bi.id.sym.to_string());
                    }
                }
            }
        }
    }
    fn expr_yields_string(e: &Expr, sp: &std::collections::HashSet<String>) -> bool {
        match e {
            Expr::Lit(swc_ecma_ast::Lit::Str(_)) => true,
            Expr::Tpl(_) => true,
            // param anotado string.
            Expr::Ident(id) => sp.contains(id.sym.as_str()),
            Expr::Bin(b) if b.op == BinaryOp::Add => {
                expr_yields_string(&b.left, sp) || expr_yields_string(&b.right, sp)
            }
            // (cross-runtime) `s || "default"` / `s ?? "none"`: o fallback
            // string (ou lado string) garante resultado string. Espelha
            // expr_yields_string de program.rs. `n || 0` (ambos numericos)
            // nao dispara.
            Expr::Bin(b) if matches!(
                b.op,
                BinaryOp::NullishCoalescing | BinaryOp::LogicalOr
            ) => {
                expr_yields_string(&b.left, sp) || expr_yields_string(&b.right, sp)
            }
            Expr::Paren(p) => expr_yields_string(&p.expr, sp),
            // (cross-runtime #270) `() => String(x)` em arrow body produz
            // handle string. Sem isso, return_type vira "i64" e caller
            // formata handle bruto como int. Restrito a `String(x)` direto
            // — adicionar `.toString()` ou Cond regride callbacks polimorficos
            // de JSON.stringify replacer (que esperam passthrough i64).
            Expr::Call(c) => {
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    if let Expr::Ident(id) = callee.as_ref() {
                        if id.sym.as_str() == "String" {
                            return true;
                        }
                    }
                    // (cross-runtime) metodo que retorna string inequivoca:
                    // `(s) => s.toUpperCase()` etc. Sem isso o arrow sem
                    // anotacao infere i64 e o handle sai como numero cru.
                    // Mesma lista de block_returns_string (program.rs).
                    if let Expr::Member(m) = callee.as_ref() {
                        if let swc_ecma_ast::MemberProp::Ident(p) = &m.prop {
                            return matches!(p.sym.as_str(),
                                "toString" | "join" | "concat" | "replace" | "replaceAll"
                                | "trim" | "trimStart" | "trimEnd" | "toUpperCase"
                                | "toLowerCase" | "slice" | "padStart" | "padEnd"
                                | "repeat" | "substring" | "charAt" | "at");
                        }
                    }
                }
                false
            }
            // (cross-runtime) ternary (incl. encadeado) cujos AMBOS os ramos
            // produzem string: `n>=90?"A":n>=80?"B":"C"`. Sem isto, arrow
            // `const f = (n) => ...ternary...` sem anotacao nao infere string
            // e o handle sai como numero cru. Exige AMBOS (cons && alt) string
            // para nao classificar ternary polimorfico (ex: `c ? "x" : 0`,
            // replacers de JSON.stringify) como string — esses ficam ambiguos.
            Expr::Cond(c) => {
                expr_yields_string(&c.cons, sp) && expr_yields_string(&c.alt, sp)
            }
            _ => false,
        }
    }
    match arrow.body.as_ref() {
        swc_ecma_ast::BlockStmtOrExpr::Expr(e) => expr_yields_string(e, &str_params),
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
            b.stmts.iter().any(|s| {
                if let Stmt::Return(r) = s {
                    if let Some(e) = r.arg.as_deref() {
                        return expr_yields_string(e, &str_params);
                    }
                }
                false
            })
        }
    }
}

fn body_has_return_value(body: &[rts_ast::ast::Statement]) -> bool {
    use rts_ast::ast::Statement;
    for stmt in body {
        let Statement::Raw(raw) = stmt;
        let Some(s) = raw.stmt.as_ref() else { continue };
        if stmt_has_return_value(s) {
            return true;
        }
    }
    false
}

fn stmt_has_return_value(s: &Stmt) -> bool {
    match s {
        Stmt::Return(r) => r.arg.is_some(),
        Stmt::Block(b) => b.stmts.iter().any(stmt_has_return_value),
        Stmt::If(i) => {
            stmt_has_return_value(&i.cons)
                || i.alt.as_deref().is_some_and(stmt_has_return_value)
        }
        Stmt::While(w) => stmt_has_return_value(&w.body),
        Stmt::DoWhile(d) => stmt_has_return_value(&d.body),
        Stmt::For(f) => stmt_has_return_value(&f.body),
        Stmt::ForOf(f) => stmt_has_return_value(&f.body),
        Stmt::ForIn(f) => stmt_has_return_value(&f.body),
        Stmt::Try(t) => {
            t.block.stmts.iter().any(stmt_has_return_value)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.stmts.iter().any(stmt_has_return_value))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.stmts.iter().any(stmt_has_return_value))
        }
        Stmt::Switch(sw) => sw
            .cases
            .iter()
            .any(|c| c.cons.iter().any(stmt_has_return_value)),
        _ => false,
    }
}

fn arrow_to_function(arrow: &ArrowExpr) -> SwcFunction {
    let body = match &*arrow.body {
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(block) => Some(block.clone()),
        swc_ecma_ast::BlockStmtOrExpr::Expr(expr) => {
            let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: arrow.span,
                arg: Some(expr.clone()),
            });
            Some(BlockStmt {
                span: arrow.span,
                ctxt: arrow.ctxt,
                stmts: vec![return_stmt],
            })
        }
    };
    let params = arrow
        .params
        .iter()
        .map(|pat| swc_ecma_ast::Param {
            span: pat.span(),
            decorators: Vec::new(),
            pat: pat.clone(),
        })
        .collect();
    SwcFunction {
        params,
        decorators: Vec::new(),
        span: arrow.span,
        ctxt: arrow.ctxt,
        body,
        is_generator: false,
        is_async: arrow.is_async,
        type_params: arrow.type_params.clone(),
        return_type: arrow.return_type.clone(),
    }
}

fn lower_import_decl(cm: &Lrc<SourceMap>, import_decl: &SwcImportDecl) -> ImportDecl {
    let mut names = Vec::new();
    let mut default_name = None;

    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                // SWC: `imported` carrega o nome no source quando ha alias
                // (`import { orig as local }`); ausente quando nao ha
                // (`import { name }`, em que orig == local == name).
                let local = named.local.sym.to_string();
                let orig = named
                    .imported
                    .as_ref()
                    .map(module_export_name)
                    .unwrap_or_else(|| local.clone());
                names.push(ImportName { orig, local });
            }
            ImportSpecifier::Default(def) => {
                default_name = Some(def.local.sym.to_string());
            }
            ImportSpecifier::Namespace(_) => {}
        }
    }

    ImportDecl {
        names,
        default_name,
        from: import_decl.src.value.to_string_lossy().to_string(),
        span: convert_span(cm, import_decl.span),
        reexport: false,
    }
}

fn lower_interface_decl(cm: &Lrc<SourceMap>, interface_decl: &SwcTsInterfaceDecl) -> InterfaceDecl {
    let mut fields = Vec::new();

    for member in &interface_decl.body.body {
        if let TsTypeElement::TsPropertySignature(property) = member {
            if let Some(name) = property_key_name(&property.key, cm) {
                let field = FieldDecl {
                    name,
                    type_annotation: property
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation))
                        .unwrap_or_else(|| "any".to_string()),
                    span: convert_span(cm, property.span),
                };
                fields.push(field);
            }
        }
    }

    InterfaceDecl {
        name: interface_decl.id.sym.to_string(),
        fields,
        span: convert_span(cm, interface_decl.span),
    }
}

/// Desugar `enum E { A, B = 5 }` em `const E = { A: 0, B: 5 };`.
fn lower_ts_enum_to_const(enum_decl: &swc_ecma_ast::TsEnumDecl) -> Option<Stmt> {
    use swc_ecma_ast::*;

    let enum_name = enum_decl.id.sym.to_string();
    let mut props: Vec<PropOrSpread> = Vec::with_capacity(enum_decl.members.len());
    // Auto-counter pra membros numéricos sem init.
    let mut next_numeric: i64 = 0;

    for member in &enum_decl.members {
        let key_str = match &member.id {
            TsEnumMemberId::Ident(id) => id.sym.to_string(),
            TsEnumMemberId::Str(s) => s.value.to_string_lossy().to_string(),
        };

        // Determina o valor: usa init se presente, senão auto-incremento.
        // numeric_val: Some(n) quando o valor eh inteiro literal — habilita o
        // reverse mapping `[n]: "name"` (JS spec para enum numerico).
        let mut numeric_val: Option<i64> = None;
        let value_expr: Expr = if let Some(init) = &member.init {
            // Quando init é Lit::Num, atualiza o counter.
            if let Expr::Lit(Lit::Num(n)) = init.as_ref() {
                next_numeric = n.value as i64 + 1;
                numeric_val = Some(n.value as i64);
            }
            (**init).clone()
        } else {
            let val = next_numeric;
            next_numeric += 1;
            numeric_val = Some(val);
            Expr::Lit(Lit::Num(Number {
                span: Default::default(),
                value: val as f64,
                raw: Some(format!("{val}").into()),
            }))
        };

        let prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(IdentName {
                span: Default::default(),
                sym: key_str.clone().into(),
            }),
            value: Box::new(value_expr),
        })));
        props.push(prop);

        // (cross-runtime) Reverse mapping para enum numerico: `State[0]` ->
        // "Idle". JS gera as entradas reversas `[value]: "name"`. String enums
        // nao tem reverse. Key numerica via PropName::Num.
        if let Some(n) = numeric_val {
            props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Num(Number {
                    span: Default::default(),
                    value: n as f64,
                    raw: Some(format!("{n}").into()),
                }),
                value: Box::new(Expr::Lit(Lit::Str(Str {
                    span: Default::default(),
                    value: key_str.clone().into(),
                    raw: None,
                }))),
            }))));
        }
    }

    let obj_lit = Expr::Object(ObjectLit {
        span: Default::default(),
        props,
    });

    let var_decl = VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(BindingIdent {
                id: Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: enum_name.into(),
                    optional: false,
                },
                type_ann: None,
            }),
            init: Some(Box::new(obj_lit)),
            definite: false,
        }],
    };
    Some(Stmt::Decl(Decl::Var(Box::new(var_decl))))
}

/// Desugar \`namespace Foo { export function f() {} }\`:
/// 1. Members exportados viram top-level com nome mangled \`__ns_<NS>_<member>\`.
/// 2. Gera \`const <NS> = { member: __ns_<NS>_member, ... }\` no fim
///    pra habilitar \`<NS>.member()\` via member access + call_indirect.
fn lower_ts_namespace(
    cm: &Lrc<SourceMap>,
    module_decl: &swc_ecma_ast::TsModuleDecl,
    out: &mut Vec<Item>,
) {
    use swc_ecma_ast::*;

    // Pega o nome do namespace (skip strings — só Ident).
    let ns_name: String = match &module_decl.id {
        TsModuleName::Ident(id) => id.sym.to_string(),
        TsModuleName::Str(_) => return, // ambient module string — skip MVP
    };

    // Body é \`TsModuleBlock\` ou \`TsNamespaceDecl\` (nested).
    let block: &TsModuleBlock = match module_decl.body.as_ref() {
        Some(TsNamespaceBody::TsModuleBlock(b)) => b,
        Some(TsNamespaceBody::TsNamespaceDecl(_)) => {
            // Nested namespace (`namespace A.B {}`) — não suportado MVP.
            return;
        }
        None => return,
    };

    // Coleta nomes dos membros pra gerar o objeto const final.
    let mut member_names: Vec<String> = Vec::new();

    for item in &block.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(decl)) => {
                process_namespace_member(cm, &ns_name, decl, &mut member_names, out);
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ed)) => {
                process_namespace_member(cm, &ns_name, &ed.decl, &mut member_names, out);
            }
            _ => {}
        }
    }

    // Gera \`const <NS> = { member: __ns_<NS>_member, ... };\`
    if !member_names.is_empty() {
        let mut props: Vec<PropOrSpread> = Vec::with_capacity(member_names.len());
        for member in &member_names {
            let mangled = format!("__ns_{ns_name}_{member}");
            let prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName {
                    span: Default::default(),
                    sym: member.as_str().into(),
                }),
                value: Box::new(Expr::Ident(Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: mangled.into(),
                    optional: false,
                })),
            })));
            props.push(prop);
        }
        let obj_lit = Expr::Object(ObjectLit {
            span: Default::default(),
            props,
        });
        let var_decl = VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: Default::default(),
                name: Pat::Ident(BindingIdent {
                    id: Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: ns_name.clone().into(),
                        optional: false,
                    },
                    type_ann: None,
                }),
                init: Some(Box::new(obj_lit)),
                definite: false,
            }],
        };
        let stmt = Stmt::Decl(Decl::Var(Box::new(var_decl)));
        push_raw_statement_with_stmt(cm, module_decl.span, Some(&stmt), out);
    }
}

fn process_namespace_member(
    cm: &Lrc<SourceMap>,
    ns_name: &str,
    decl: &swc_ecma_ast::Decl,
    member_names: &mut Vec<String>,
    out: &mut Vec<Item>,
) {
    use swc_ecma_ast::*;
    match decl {
        Decl::Fn(fn_decl) => {
            // Renomeia para \`__ns_<NS>_<name>\`.
            let original_name = fn_decl.ident.sym.to_string();
            let mangled = format!("__ns_{ns_name}_{original_name}");
            // Constrói uma cópia do FnDecl com o ident renomeado.
            let mut renamed = fn_decl.clone();
            renamed.ident.sym = mangled.into();
            out.push(Item::Function(lower_fn_decl(cm, &renamed)));
            member_names.push(original_name);
        }
        Decl::Class(class_decl) => {
            let original_name = class_decl.ident.sym.to_string();
            let mangled = format!("__ns_{ns_name}_{original_name}");
            let mut renamed = class_decl.clone();
            renamed.ident.sym = mangled.into();
            out.push(Item::Class(lower_class_decl(cm, &renamed)));
            // Classes não vão para o objeto namespace porque \`Foo.C\` não
            // é \`new\` direto sem suporte adicional. Documentamos como
            // limitação. Por enquanto, ainda registramos o nome para
            // que o usuário possa fazer \`Foo.C\` (mas \`new Foo.C()\` não
            // funciona — usar \`new __ns_Foo_C()\` ou alias).
            // Skip do member_names: melhor não confundir.
            let _ = original_name;
        }
        Decl::Var(var_decl) => {
            // \`export const x = ...\` ou \`let\`/\`var\`. Renomeia cada decl.
            for d in &var_decl.decls {
                if let Pat::Ident(id) = &d.name {
                    let original_name = id.id.sym.to_string();
                    let mangled = format!("__ns_{ns_name}_{original_name}");
                    let new_decl = VarDeclarator {
                        span: d.span,
                        name: Pat::Ident(BindingIdent {
                            id: Ident {
                                span: Default::default(),
                                ctxt: Default::default(),
                                sym: mangled.into(),
                                optional: false,
                            },
                            type_ann: id.type_ann.clone(),
                        }),
                        init: d.init.clone(),
                        definite: d.definite,
                    };
                    let renamed_decl = VarDecl {
                        span: var_decl.span,
                        ctxt: var_decl.ctxt,
                        kind: var_decl.kind,
                        declare: var_decl.declare,
                        decls: vec![new_decl],
                    };
                    let stmt = Stmt::Decl(Decl::Var(Box::new(renamed_decl)));
                    push_raw_statement_with_stmt(cm, var_decl.span, Some(&stmt), out);
                    member_names.push(original_name);
                }
            }
        }
        Decl::TsEnum(enum_decl) => {
            // Enum interno: gera com nome mangled e adiciona ao namespace.
            let original_name = enum_decl.id.sym.to_string();
            let mut renamed = enum_decl.clone();
            renamed.id.sym = format!("__ns_{ns_name}_{original_name}").into();
            if let Some(stmt) = lower_ts_enum_to_const(&renamed) {
                push_raw_statement_with_stmt(cm, enum_decl.span, Some(&stmt), out);
                member_names.push(original_name);
            }
        }
        _ => {}
    }
}

/// Emite a chamada do decorator como statement de side-effect:
/// `decoratorExpr(0);`. Resultado descartado (decorators TC39 com
/// retorno modificando target nao sao suportados em runtime).
fn emit_decorator_call_stmt(
    cm: &Lrc<SourceMap>,
    decorator_expr: &Expr,
    span: SwcSpan,
    out: &mut Vec<Item>,
) {
    use swc_ecma_ast::*;
    // Se o decorator ja e uma chamada (factory: @tag("x")), executa direto.
    // Caso contrario (@log), envolve com (target=0).
    let call_expr = if let Expr::Call(_) = decorator_expr {
        decorator_expr.clone()
    } else {
        Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(decorator_expr.clone())),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: 0.0,
                    raw: Some("0".into()),
                }))),
            }],
            type_args: None,
        })
    };
    let stmt = Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(call_expr),
    });
    push_raw_statement_with_stmt(cm, span, Some(&stmt), out);
}

fn lower_class_decl(cm: &Lrc<SourceMap>, class_decl: &SwcClassDecl) -> ClassDecl {
    lower_class(
        cm,
        &class_decl.ident.sym.to_string(),
        &class_decl.class,
        class_decl.class.span,
    )
}

/// Reescreve ocorrencias de `Expr::Ident(old)` para `new` no body
/// inteiro de uma `Function`. Conservador: para em escopos onde o
/// nome e' rebound (param/var local com mesmo nome).
fn rename_ident_in_function(f: &mut swc_ecma_ast::Function, old: &str, new: &str) {
    for p in &f.params {
        if pat_binds(&p.pat, old) {
            return;
        }
    }
    if let Some(body) = f.body.as_mut() {
        for s in &mut body.stmts {
            rename_in_stmt(s, old, new);
        }
    }
}

fn pat_binds(pat: &swc_ecma_ast::Pat, name: &str) -> bool {
    use swc_ecma_ast::Pat;
    match pat {
        Pat::Ident(b) => b.id.sym.as_ref() == name,
        Pat::Array(a) => a.elems.iter().flatten().any(|p| pat_binds(p, name)),
        Pat::Object(o) => o.props.iter().any(|prop| match prop {
            swc_ecma_ast::ObjectPatProp::KeyValue(kv) => pat_binds(&kv.value, name),
            swc_ecma_ast::ObjectPatProp::Assign(a) => a.key.sym.as_ref() == name,
            swc_ecma_ast::ObjectPatProp::Rest(r) => pat_binds(&r.arg, name),
        }),
        Pat::Rest(r) => pat_binds(&r.arg, name),
        Pat::Assign(a) => pat_binds(&a.left, name),
        _ => false,
    }
}

fn rename_in_stmt(s: &mut swc_ecma_ast::Stmt, old: &str, new: &str) {
    use swc_ecma_ast::Stmt;
    match s {
        Stmt::Block(b) => {
            for s in &mut b.stmts {
                rename_in_stmt(s, old, new);
            }
        }
        Stmt::Expr(e) => rename_in_expr(&mut e.expr, old, new),
        Stmt::Return(r) => {
            if let Some(e) = r.arg.as_mut() {
                rename_in_expr(e, old, new);
            }
        }
        Stmt::If(i) => {
            rename_in_expr(&mut i.test, old, new);
            rename_in_stmt(&mut i.cons, old, new);
            if let Some(alt) = i.alt.as_mut() {
                rename_in_stmt(alt, old, new);
            }
        }
        Stmt::While(w) => {
            rename_in_expr(&mut w.test, old, new);
            rename_in_stmt(&mut w.body, old, new);
        }
        Stmt::DoWhile(d) => {
            rename_in_expr(&mut d.test, old, new);
            rename_in_stmt(&mut d.body, old, new);
        }
        Stmt::For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => rename_in_expr(e, old, new),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                        for d in &mut vd.decls {
                            if let Some(e) = d.init.as_mut() {
                                rename_in_expr(e, old, new);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_mut() {
                rename_in_expr(t, old, new);
            }
            if let Some(u) = f.update.as_mut() {
                rename_in_expr(u, old, new);
            }
            rename_in_stmt(&mut f.body, old, new);
        }
        Stmt::ForOf(f) => {
            rename_in_expr(&mut f.right, old, new);
            rename_in_stmt(&mut f.body, old, new);
        }
        Stmt::ForIn(f) => {
            rename_in_expr(&mut f.right, old, new);
            rename_in_stmt(&mut f.body, old, new);
        }
        Stmt::Switch(s) => {
            rename_in_expr(&mut s.discriminant, old, new);
            for c in &mut s.cases {
                if let Some(t) = c.test.as_mut() {
                    rename_in_expr(t, old, new);
                }
                for s in &mut c.cons {
                    rename_in_stmt(s, old, new);
                }
            }
        }
        Stmt::Throw(t) => rename_in_expr(&mut t.arg, old, new),
        Stmt::Try(t) => {
            for s in &mut t.block.stmts {
                rename_in_stmt(s, old, new);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in &mut h.body.stmts {
                    rename_in_stmt(s, old, new);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in &mut f.stmts {
                    rename_in_stmt(s, old, new);
                }
            }
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_mut() {
                    rename_in_expr(e, old, new);
                }
            }
        }
        Stmt::Labeled(l) => rename_in_stmt(&mut l.body, old, new),
        _ => {}
    }
}

fn rename_in_expr(e: &mut swc_ecma_ast::Expr, old: &str, new: &str) {
    use swc_ecma_ast::Expr;
    match e {
        Expr::Ident(id) if id.sym.as_ref() == old => {
            id.sym = new.into();
        }
        Expr::Bin(b) => {
            rename_in_expr(&mut b.left, old, new);
            rename_in_expr(&mut b.right, old, new);
        }
        Expr::Unary(u) => rename_in_expr(&mut u.arg, old, new),
        Expr::Update(u) => rename_in_expr(&mut u.arg, old, new),
        Expr::Assign(a) => {
            rename_in_expr(&mut a.right, old, new);
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &mut a.left
            {
                rename_in_expr(&mut m.obj, old, new);
            }
        }
        Expr::Cond(c) => {
            rename_in_expr(&mut c.test, old, new);
            rename_in_expr(&mut c.cons, old, new);
            rename_in_expr(&mut c.alt, old, new);
        }
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &mut c.callee {
                rename_in_expr(callee, old, new);
            }
            for a in &mut c.args {
                rename_in_expr(&mut a.expr, old, new);
            }
        }
        Expr::New(n) => {
            rename_in_expr(&mut n.callee, old, new);
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    rename_in_expr(&mut a.expr, old, new);
                }
            }
        }
        Expr::Member(m) => {
            rename_in_expr(&mut m.obj, old, new);
            if let swc_ecma_ast::MemberProp::Computed(c) = &mut m.prop {
                rename_in_expr(&mut c.expr, old, new);
            }
        }
        Expr::Paren(p) => rename_in_expr(&mut p.expr, old, new),
        Expr::Seq(s) => {
            for e in &mut s.exprs {
                rename_in_expr(e, old, new);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rename_in_expr(&mut el.expr, old, new);
            }
        }
        Expr::Object(o) => {
            for p in &mut o.props {
                if let swc_ecma_ast::PropOrSpread::Prop(p) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_mut() {
                        rename_in_expr(&mut kv.value, old, new);
                    }
                }
            }
        }
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                rename_in_expr(e, old, new);
            }
        }
        Expr::TsAs(a) => rename_in_expr(&mut a.expr, old, new),
        Expr::TsTypeAssertion(a) => rename_in_expr(&mut a.expr, old, new),
        Expr::TsNonNull(n) => rename_in_expr(&mut n.expr, old, new),
        Expr::TsConstAssertion(a) => rename_in_expr(&mut a.expr, old, new),
        Expr::Arrow(a) => {
            if a.params.iter().any(|p| pat_binds(p, old)) {
                return;
            }
            match a.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &mut b.stmts {
                        rename_in_stmt(s, old, new);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rename_in_expr(e, old, new),
            }
        }
        Expr::Fn(f) => {
            if f.ident.as_ref().map(|i| i.sym.as_ref() == old).unwrap_or(false) {
                return;
            }
            if f.function.params.iter().any(|p| pat_binds(&p.pat, old)) {
                return;
            }
            if let Some(body) = f.function.body.as_mut() {
                for s in &mut body.stmts {
                    rename_in_stmt(s, old, new);
                }
            }
        }
        _ => {}
    }
}

/// The names bound by TOP-LEVEL declarations (class / function / var-pattern
/// idents). The class-expression hoister consults it so a named class expression
/// only claims its own name when nothing else already owns it.
fn top_level_names(p: &SwcProgram) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let stmts: Vec<&Stmt> = match p {
        SwcProgram::Module(m) => m
            .body
            .iter()
            .filter_map(|it| match it {
                ModuleItem::Stmt(s) => Some(s),
                _ => None,
            })
            .collect(),
        SwcProgram::Script(s) => s.body.iter().collect(),
    };
    for s in stmts {
        let Stmt::Decl(d) = s else { continue };
        match d {
            Decl::Class(c) => {
                out.insert(c.ident.sym.to_string());
            }
            Decl::Fn(f) => {
                out.insert(f.ident.sym.to_string());
            }
            Decl::Var(v) => {
                for d in &v.decls {
                    if let swc_ecma_ast::Pat::Ident(i) = &d.name {
                        out.insert(i.id.sym.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}
