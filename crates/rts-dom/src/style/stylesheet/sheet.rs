//! O `Stylesheet` — as consultas derivadas, as caches e a CASCATA por elemento
//!
//! Extraído de `stylesheet.rs` sem alterar uma linha.

use super::*;

impl Default for Stylesheet {
    fn default() -> Stylesheet { Stylesheet::new() }
}

/// A chave `!important`: layer e `origin` invertidas (a UA vence o autor —
/// CSS Cascade 5 §6.1), partilhada por `custom_important_from`/`declarations_from`.
pub(super) fn important_key((origin, layer, spec, order, _): (u32, u32, u32, u32, usize)) -> (u32, u32, u32, u32) {
    let layer = if layer == u32::MAX { 0 } else { u32::MAX - layer };
    (1 - origin, layer, spec, order)
}

impl Stylesheet {
    /// SEM regras de autor — mas não vazio: carrega a UA-stylesheet (`style::ua`, lote I).
    pub fn new() -> Stylesheet {
        Stylesheet {
            rules: crate::style::ua::rules(),
            syntax: Vec::new(),
            keyframes: std::collections::HashMap::new(),
            index: std::cell::RefCell::new(super::ruleindex::RuleIndex::default()),
            candidate_scratch: std::cell::RefCell::new(Vec::new()),
            hover_reach: std::cell::RefCell::new(None),
            position_sensitive: std::cell::RefCell::new(None),
            out_of_flow: std::cell::RefCell::new(None),
            layer_names: std::cell::RefCell::new(Vec::new()),
            properties: super::property::CustomPropertyRegistry::default(),
        }
    }

    /// Garante o índice de regras sincronizado com `self.rules`. A construção é lazy
    /// porque o parser anexa as regras depois de criar o stylesheet.
    fn ensure_rule_index(&self) {
        let idx = self.index.borrow();
        if !idx.is_current(self.rules.len()) {
            drop(idx);
            *self.index.borrow_mut() = super::ruleindex::RuleIndex::build(&self.rules);
        }
    }

    /// Garante o índice e devolve os índices das regras CANDIDATAS a casar um nó
    /// `(tag, id, classes)` — a base do fast-path da cascade.
    fn candidate_indices(
        &self,
        tag: &str,
        id: Option<&str>,
        classes: &[&str],
    ) -> std::cell::RefMut<'_, Vec<usize>> {
        self.ensure_rule_index();
        let mut scratch = self.candidate_scratch.borrow_mut();
        self.index
            .borrow()
            .candidates_into(tag, id, classes, &mut scratch);
        scratch
    }

    fn has_custom_rules(&self) -> bool {
        self.ensure_rule_index();
        self.index.borrow().has_custom_rules()
    }

    /// As regras que contêm `:hover`, e se alguma delas ALCANÇA outros nós além
    /// do que casa o compound com `:hover`.
    ///
    /// É o que transforma "mover o mouse invalida a página" em "mover o mouse
    /// invalida os nós que podem mudar": sem esta separação, `set_hovered` só
    /// tinha a opção grossa. Derivado das regras e recalculado quando elas
    /// mudam, porque a alternativa era varrer 2643 regras a cada frame de mouse
    /// — o que o `set_hovered` fazia.
    pub fn hover_reach(&self) -> HoverReach {
        self.ensure_rule_index();
        if let Some(cached) = *self.hover_reach.borrow() {
            return cached;
        }
        let mut reach = HoverReach::None;
        // Os seletores aninhados (`:is(...)`) entram na varredura: um `:hover`
        // dentro de um deles muda estilo na mesma, e ignorá-lo dava
        // `HoverReach::None` — o mouse deixava de invalidar o que devia.
        let mut all_selectors: Vec<&super::ComplexSelector> = Vec::new();
        for r in &self.rules {
            super::selector::visit_selectors(&r.selector, &mut |s| all_selectors.push(s));
        }
        for sel in all_selectors {
            let n = sel.compounds.len();
            for (i, c) in sel.compounds.iter().enumerate() {
                if !super::selector::compound_has_hover(c) {
                    continue;
                }
                // O `:hover` no ÚLTIMO compound afeta só quem casa (`.btn:hover`).
                // Antes do último, o alcance depende do combinador que o segue:
                // descendente/filho desce na subárvore, irmão sai dela — e sair
                // dela é o caso que a invalidação por subárvore NÃO cobre.
                let next = if i + 1 < n {
                    sel.combinators.get(i)
                } else {
                    None
                };
                reach = reach.max(match next {
                    None => HoverReach::SelfOnly,
                    Some(super::Combinator::Descendant | super::Combinator::Child) => {
                        HoverReach::Subtree
                    }
                    Some(_) => HoverReach::Siblings,
                });
            }
        }
        *self.hover_reach.borrow_mut() = Some(reach);
        reach
    }

    /// `true` quando o estilo de um nó pode depender da POSIÇÃO dele entre os
    /// irmãos — `:first-child`, `:last-child`, `:only-child`, `:nth-child()`,
    /// `:empty`, ou um combinador de irmão (`+`, `~`).
    ///
    /// É a guarda da invalidação por subárvore na INSERÇÃO e na REMOÇÃO: sem
    /// nenhuma dessas formas, acrescentar um `<li>` não muda o estilo de nenhum
    /// outro nó, e invalidar a página (o que se fazia) é jogar fora o memo de
    /// todos os nós a cada `appendChild`. Com alguma delas, os irmãos mudam de
    /// verdade e o global é o que responde certo. Derivado das regras e
    /// cacheado — a pergunta é feita a cada mutação de árvore.
    pub fn position_sensitive(&self) -> bool {
        if let Some(cached) = *self.position_sensitive.borrow() {
            return cached;
        }
        use super::{Combinator, PseudoClass as P, SimpleSelector as S};
        let answer = self.rules.iter().any(|r| {
            // Varre também o que está dentro de `:is()`/`:not()`: `:not(:first-child)`
            // depende da posição exatamente como `:first-child`.
            let mut is_position_sensitive = false;
            super::selector::visit_selectors(&r.selector, &mut |s| {
                is_position_sensitive |= s
                    .combinators
                    .iter()
                    .any(|c| matches!(c, Combinator::NextSibling | Combinator::SubsequentSibling));
            });
            super::selector::visit_simples(&r.selector, &mut |p| {
                is_position_sensitive |= matches!(
                    p,
                    S::Pseudo(
                        P::FirstChild
                            | P::LastChild
                            | P::OnlyChild
                            | P::Empty
                            | P::NthChild(_, _)
                            | P::FirstOfType
                            | P::LastOfType
                            | P::OnlyOfType
                            | P::NthOfType(_, _)
                    )
                );
            });
            is_position_sensitive
        });
        *self.position_sensitive.borrow_mut() = Some(answer);
        answer
    }

    /// Os COMPOUNDS que contêm `:hover`, um por regra que o usa. É contra estes
    /// que se pergunta "este nó poderia casar uma regra de hover?", e a resposta
    /// é o que separa o `<body>` (ancestral de tudo, casa nada) do `.btn` — sem
    /// essa pergunta, invalidar a cadeia de ancestrais é invalidar a página.
    pub fn hover_compounds(&self) -> Vec<&super::CompoundSelector> {
        let mut out = Vec::new();
        for r in &self.rules {
            super::selector::visit_selectors(&r.selector, &mut |sel| {
                for c in &sel.compounds {
                    if super::selector::compound_has_hover(c) {
                        out.push(c);
                    }
                }
            });
        }
        out
    }

    /// `true` se alguma regra pode tirar um elemento do fluxo
    /// (`position: absolute` ou `fixed`).
    ///
    /// Derivado das regras e cacheado: sem nenhuma delas — e sem `style=""`
    /// inline com `position` —, a passada de fora do fluxo não tem o que achar,
    /// e ela percorre a ÁRVORE INTEIRA pedindo o estilo computado de cada nó.
    /// Era 78% de um frame de mutação numa página de 3000 elementos que não tem
    /// um único posicionado.
    pub fn has_out_of_flow(&self) -> bool {
        if let Some(cached) = *self.out_of_flow.borrow() {
            return cached;
        }
        use super::props::Decl;
        let out_of_flow = |d: &Decl| {
            matches!(
                d,
                Decl::position(Some(super::Position::Absolute | super::Position::Fixed))
            )
        };
        let answer = self.rules.iter().any(|r| {
            r.decls.normal.iter().any(out_of_flow)
                || r.decls.important.iter().any(out_of_flow)
                // uma pendente com var() pode virar qualquer coisa: conta como
                // possível, que é o lado seguro.
                || r.decls.pending.iter().any(|(prop, _, _)| prop == "position")
        });
        *self.out_of_flow.borrow_mut() = Some(answer);
        answer
    }

    /// `true` se alguma regra CITA esta classe. Ver
    /// [`RuleIndex::mentions_class`](super::ruleindex::RuleIndex::mentions_class).
    pub fn mentions_class(&self, class: &str) -> bool {
        self.ensure_rule_index();
        self.index.borrow().mentions_class(class)
    }

    /// `true` quando alguma regra depende da presença/valor de um atributo.
    pub fn has_attribute_selectors(&self) -> bool {
        self.ensure_rule_index();
        self.index.borrow().has_attribute_selectors()
    }

    /// Versão fina de [`has_attribute_selectors`](Self::has_attribute_selectors), por NOME.
    pub fn mentions_attribute_name(&self, name: &str) -> bool {
        self.ensure_rule_index();
        self.index.borrow().mentions_attribute_name(name)
    }

    /// `true` se alguma regra usa `:has()` — a guarda que amplia a invalidação
    /// estrutural/de atributo além da subárvore do pai. Ver `RuleIndex::has_relational`.
    pub fn has_relational(&self) -> bool {
        self.ensure_rule_index();
        self.index.borrow().has_relational()
    }

    /// `true` se não há nenhuma regra (atalho para o `computed_style` pular a
    /// cascade quando a página não tem `<style>`).
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Bytes ESTIMADOS deste stylesheet: regras de AUTOR (seletores + blocos
    /// de declarações) mais keyframes. Estimativa por estrutura, como todo o
    /// [`crate::metrics::footprint`] — compara páginas, não casa com o RSS.
    /// As regras de UA (lote I) ficam FORA: são a mesma folha partilhada por
    /// thread em todo `Stylesheet::new()`, e contá-las inflava um documento
    /// de uma linha com um piso fixo de ~35 regras.
    pub fn estimated_bytes(&self) -> usize {
        let autor: Vec<&Rule> = self.rules.iter().filter(|r| !r.is_ua).collect();
        let mut total = autor.len() * std::mem::size_of::<Rule>();
        // Blocos de declarações são COMPARTILHADOS entre seletores de uma
        // mesma regra — distintos por PONTEIRO para não contar duas vezes.
        let mut seen: Vec<*const RuleDecls> = Vec::new();
        for r in autor {
            total += r.selector.estimated_bytes();
            let ptr = std::rc::Rc::as_ptr(&r.decls);
            if seen.contains(&ptr) {
                continue;
            }
            seen.push(ptr);
            total += r.decls.estimated_bytes();
        }
        total += self.keyframes.len()
            * (std::mem::size_of::<crate::anim::Keyframes>() + std::mem::size_of::<String>());
        total
    }

    /// Os `@keyframes` de um nome, se existir.
    pub fn keyframes(&self, name: &str) -> Option<&crate::anim::Keyframes> {
        self.keyframes.get(name)
    }

    /// Insere um bloco CSS preservado na posição dos blocos sintácticos anexados.
    /// A operação é deliberadamente transaccional: o AST é actualizado primeiro e
    /// rules, keyframes, layers e índices são reconstruídos a partir da nova fonte.
    /// `index == len` equivale a append; um índice fora do intervalo é rejeitado.
    pub fn insert_rule(&mut self, index: usize, css: &str) -> Result<usize, String> {
        if index > self.syntax.len() {
            return Err(format!("índice de regra fora do intervalo: {index}"));
        }
        let ast = crate::style::syntax::StylesheetAst::parse(css);
        if ast.items.is_empty() {
            return Err("regra CSS vazia".to_string());
        }
        self.syntax.insert(index, ast);
        self.rebuild_from_syntax();
        Ok(index)
    }

    /// Remove um bloco CSS sintáctico e reconstrói a representação semântica.
    /// Devolve `false` quando o índice não existe.
    pub fn delete_rule(&mut self, index: usize) -> bool {
        if index >= self.syntax.len() {
            return false;
        }
        self.syntax.remove(index);
        self.rebuild_from_syntax();
        true
    }

    fn rebuild_from_syntax(&mut self) {
        let blocks: Vec<String> = self.syntax.iter().map(|ast| ast.to_css()).collect();
        let mut rebuilt = Stylesheet::new();
        for css in blocks {
            rebuilt.append_css(&css);
        }
        *self = rebuilt;
    }

    /// Acrescenta as regras de mais um bloco `<style>` (uma página pode ter vários).
    /// EXTRAI os `@keyframes` primeiro (não são regras de seletor), depois as regras.
    pub fn append_css(&mut self, css: &str) {
        // Tokeniza uma única vez. O AST preserva a entrada original para tooling;
        // o IR semântico continua a ser a fonte consumida pela cascade.
        let ast = crate::style::syntax::StylesheetAst::parse(css);
        self.syntax.push(ast.clone());

        // `@keyframes` é um at-rule estrutural: não vira `Rule`, mas os seus stops
        // são baixados directamente para a tabela de animações.
        for item in &ast.items {
            if let crate::style::syntax::AstItem::AtRule {
                name,
                prelude,
                block: Some(block),
                ..
            } = item
            {
                let lower = name.to_ascii_lowercase();
                if lower == "keyframes" || lower == "-webkit-keyframes" {
                    let keyframe_name: String = prelude
                        .iter()
                        .map(crate::style::syntax::ComponentValue::to_css_semantic)
                        .collect();
                    let keyframe_name = keyframe_name.trim();
                    if !keyframe_name.is_empty() {
                        crate::bump!(css_keyframes);
                        self.keyframes.insert(
                            keyframe_name.to_string(),
                            super::parse_keyframe_ast(block),
                        );
                    }
                }
                // `@property --nome { … }` — estrutural como `@keyframes`
                // acima (não vira `Rule`); ver `property.rs` para a razão de
                // o registo viver no `Stylesheet` e não numa `Rule`. FORA do
                // `if` acima — estava dentro por engano (o bloco só corre
                // quando `lower == "keyframes"`, então `@property` nunca
                // chegava a `maybe_register`; `properties_registry()` ficava
                // sempre vazio e os dois testes que o liam falhavam de forma
                // que parecia "o registo não é lido", quando era "nunca foi
                // escrito"). `maybe_register` já filtra por nome internamente.
                super::property::maybe_register(&mut self.properties, &lower, prelude, block);
            }
        }

        // (var()/custom properties agora resolvem POR ELEMENTO na cascade — #1779;
        // o antigo passe textual GLOBAL daqui foi removido.)
        let base = self.rules.len() as u32;
        let parsed_rules = super::rules::parse_rules_ast_with_layers(
            &ast,
            &mut self.layer_names.borrow_mut(),
        );
        for (i, rule) in parsed_rules.into_iter().enumerate() {
            crate::bump!(css_rules);
            self.rules.push(Rule {
                order: base + i as u32,
                ..rule
            });
        }
        // As regras mudaram: o que foi derivado delas não vale mais.
        *self.hover_reach.borrow_mut() = None;
        *self.position_sensitive.borrow_mut() = None;
        *self.out_of_flow.borrow_mut() = None;
    }

    /// Computa o estilo de AUTOR para um elemento, aplicando as regras cujo seletor
    /// casa (decidido pelo `matches` fornecido — o `Dom` passa um que navega a
    /// árvore p/ os combinadores). Retorna um [`DeclBlock`] (normal + important
    /// separados). Dentro de cada camada, ordem de (especificidade, order) crescente.
    /// As regras que casam um nó, JÁ ORDENADAS pela cascade (especificidade,
    /// depois ordem de documento).
    ///
    /// Existe porque o matching acontecia DUAS vezes por nó: uma no passe das
    /// custom properties (que precisa vir antes, para resolver `var()`) e outra
    /// no das declarações — sobre o mesmo conjunto candidato e com a mesma
    /// resposta. E `matches` não é barato: NAVEGA a árvore para os
    /// combinadores. Numa página Bootstrap são 149 candidatas por nó, e metade
    /// do trabalho era repetição exata.
    pub fn matched_for_node(
        &self,
        media_ctx: &super::media::MediaContext,
        node_tag: &str,
        node_id: Option<&str>,
        node_classes: &[&str],
        matches: impl Fn(&ComplexSelector) -> bool,
    ) -> MatchedRules {
        // FAST PATH: só as regras cuja chave-alvo o nó pode satisfazer (índice),
        // em vez de TODAS. O `matches` completo ainda decide.
        let cand = self.candidate_indices(node_tag, node_id, node_classes);
        crate::bump!(rules_considered, cand.len());
        let index = self.index.borrow();
        let mut rules: Vec<(u32, u32, u32, u32, usize)> = cand
            .iter()
            .filter_map(|&i| {
                let r = &self.rules[i];
                // Uma regra com pseudo-elemento NÃO estiliza o elemento: os
                // compounds casam-no, mas as declarações são da caixa gerada.
                // Sem esta linha, `p::before { color:red }` pintava o próprio
                // `<p>` de vermelho — e o `::before` foi durante muito tempo
                // recusado no parse justamente para evitar isso.
                (r.selector.pseudo_element.is_none()
                    && r.media.as_ref().map(|m| m.matches(media_ctx)).unwrap_or(true)
                    && matches(&r.selector))
                .then(|| {
                    (
                        u32::from(!r.is_ua), // origem: UA(0) < autor(1), sempre mais fraca
                        r.layer.unwrap_or(u32::MAX),
                        index.specificity(i),
                        r.order,
                        i,
                    )
                })
            })
            .collect();
        crate::bump!(rules_matched, rules.len());
        rules.sort_by_key(|(o, l, s, r, _)| (*o, *l, *s, *r));
        MatchedRules { rules }
    }

    /// As regras de um PSEUDO-ELEMENTO de um nó, ordenadas pela cascade, e o
    /// `content` vencedor.
    ///
    /// É a imagem no espelho de [`matched_for_node`](Self::matched_for_node): os
    /// mesmos candidatos e o mesmo matcher (os compounds casam o elemento
    /// originante), invertido só o filtro do pseudo-elemento. Reaproveitar o
    /// índice é o que mantém isto barato — e é também o que responde a "o
    /// seletor novo entra no `ruleindex`?": entra pela mesma chave de sempre,
    /// porque `p::before` continua a ter `p` como compound alvo.
    ///
    /// `content` segue a cascade como qualquer declaração: vence o da regra
    /// mais específica que o declara. Uma regra que só declare `color` não
    /// apaga o `content` de outra — é o caso maioritário na folha da Wikipédia,
    /// onde 53 das 100 regras com pseudo-elemento não declaram `content`.
    pub fn matched_for_pseudo(
        &self,
        media_ctx: &super::media::MediaContext,
        node_tag: &str,
        node_id: Option<&str>,
        node_classes: &[&str],
        pe: super::PseudoElement,
        matches: impl Fn(&ComplexSelector) -> bool,
    ) -> (MatchedRules, Option<std::rc::Rc<crate::pseudo::Content>>) {
        let cand = self.candidate_indices(node_tag, node_id, node_classes);
        let index = self.index.borrow();
        let mut rules: Vec<(u32, u32, u32, u32, usize)> = cand
            .iter()
            .filter_map(|&i| {
                let r = &self.rules[i];
                (r.selector.pseudo_element == Some(pe)
                    && r.media.as_ref().map(|m| m.matches(media_ctx)).unwrap_or(true)
                    && matches(&r.selector))
                .then(|| {
                    (
                        u32::from(!r.is_ua),
                        r.layer.unwrap_or(u32::MAX),
                        index.specificity(i),
                        r.order,
                        i,
                    )
                })
            })
            .collect();
        rules.sort_by_key(|(o, l, s, r, _)| (*o, *l, *s, *r));
        let content = rules
            .iter()
            .rev()
            .find_map(|(_, _, _, _, i)| self.rules[*i].content.clone());
        (MatchedRules { rules }, content)
    }

    /// As operações de contador VENCEDORAS entre as regras que casaram.
    ///
    /// Vence a última na ordem da cascata que declara alguma — como qualquer
    /// declaração, e ao contrário do que a intuição de "contador" sugere: dois
    /// `counter-increment` que casem o mesmo elemento não somam, o mais
    /// específico substitui o outro.
    ///
    /// Serve tanto o elemento (via [`matched_for_node`](Self::matched_for_node))
    /// como o pseudo (via [`matched_for_pseudo`](Self::matched_for_pseudo)) — é
    /// a mesma pergunta sobre o mesmo conjunto, e ter duas funções para ela era
    /// o segundo mecanismo que este trabalho existe para não criar.
    pub fn counters_from(
        &self,
        matched: &MatchedRules,
    ) -> Option<std::rc::Rc<crate::counters::Ops>> {
        matched
            .rules
            .iter()
            .rev()
            .find_map(|(_, _, _, _, i)| self.rules[*i].counters.clone())
    }

    /// `true` se alguma regra desta folha declara contadores.
    ///
    /// A guarda que faz a passagem documental custar ZERO numa página sem
    /// contadores — que é o caso de três das quatro folhas do corpus.
    pub fn has_counters(&self) -> bool {
        self.rules.iter().any(|r| r.counters.is_some())
    }

    /// `true` se alguma regra desta folha gera conteúdo. É a guarda que impede
    /// o layout de perguntar por caixas geradas numa página que não tem
    /// nenhuma: a pergunta é por elemento e a resposta seria sempre "não".
    pub fn has_generated_content(&self) -> bool {
        self.ensure_rule_index();
        self.index.borrow().has_pseudo_elements()
    }

    /// PASS A do `var()` por elemento (#1779): as declarações de CUSTOM
    /// PROPERTIES (`--x: v`) das regras que casaram, na ordem da cascade (a
    /// última vence por nome, no consumidor).
    pub fn custom_from(&self, matched: &MatchedRules) -> Vec<(String, String)> {
        if !self.has_custom_rules() {
            return Vec::new();
        }
        matched
            .rules
            .iter()
            .flat_map(|(_, _, _, _, i)| self.rules[*i].decls.custom.iter().cloned())
            .collect()
    }

    /// Custom properties `!important` em ordem de aplicação. A prioridade de
    /// layers é invertida para importantes, tal como nas declarações normais.
    pub fn custom_important_from(&self, matched: &MatchedRules) -> Vec<(String, String)> {
        let mut rules = matched.rules.clone();
        rules.sort_by_key(|k| important_key(*k));
        rules
            .iter()
            .flat_map(|(_, _, _, _, i)| self.rules[*i].decls.custom_important.iter().cloned())
            .collect()
    }

    /// PASS B: as declarações normais e `!important` das regras que casaram, com
    /// as pendentes (`prop: …var(--x)…`) resolvidas na POSIÇÃO da regra contra
    /// as custom props do elemento.
    pub fn declarations_from(
        &self,
        matched: &MatchedRules,
        vars: Option<&std::collections::HashMap<String, String>>,
    ) -> DeclBlock {
        let mut out = DeclBlock::default();
        for (_, _, _, _, i) in &matched.rules {
            let r = &self.rules[*i];
            out.all_initial_normal |= r.decls.all_initial_normal;
            out.all_initial_important |= r.decls.all_initial_important;
            r.decls.apply_normal(&mut out.normal);
            if let Some(v) = vars {
                for (prop, raw, important) in &r.decls.pending {
                    if !important {
                        apply_resolved_decl(&mut out.normal, prop, raw, v);
                    }
                }
            }
        }
        // Ordem invertida p/ `!important` — ver `important_key`.
        let mut important_rules = matched.rules.clone();
        important_rules.sort_by_key(|k| important_key(*k));
        for (_, _, _, _, i) in &important_rules {
            let r = &self.rules[*i];
            r.decls.apply_important(&mut out.important);
            if let Some(v) = vars {
                for (prop, raw, important) in &r.decls.pending {
                    if *important {
                        apply_resolved_decl(&mut out.important, prop, raw, v);
                    }
                }
            }
        }
        super::revert::resolve_reverts(self, matched, &mut out); // lote J — ver revert.rs
        out
    }

    /// Conveniência: computa o estilo para um elemento dado SÓ tag/id/classes (sem
    /// árvore). Casa apenas seletores de UM compound (sem combinadores nem pseudo/
    /// atributo dependentes de posição — esses retornam false aqui). Usado em testes
    /// e onde o contexto de árvore não importa.
    pub fn computed_for(&self, tag: &str, id: Option<&str>, classes: &[&str]) -> DeclBlock {
        let no_attr = |_: &str| None;
        let no_pseudo = |_: &PseudoClass| false;
        // viewport de referência 1280 (helper sem árvore/viewport — testes);
        // sem vars (pendentes com var() não resolvem aqui).
        let ctx = super::media::MediaContext {
            width: 1280.0,
            height: 800.0,
            ..Default::default()
        };
        let matched = self.matched_for_node(&ctx, tag, id, classes, |sel| {
            // só seletores de 1 compound casam sem a árvore.
            sel.compounds.len() == 1
                && compound_matches(&sel.compounds[0], tag, id, classes, &no_attr, &no_pseudo)
        });
        self.declarations_from(&matched, None)
    }
}
