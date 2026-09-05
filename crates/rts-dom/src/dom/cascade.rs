//! A CASCADE: resolver o estilo computado de um nó — herança, origens,
//! `var()`, contadores e os pseudo-elementos.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// Resolve o `ComputedStyle` final de um nó pela cascade da MDN (estágio 1
    /// origem/importância → especificidade → ordem). É o estado que o LAYOUT (em
    /// TS) e o render leem para decidir cor/caixa/tamanho. `None` se o id não
    /// resolve ou não é elemento. (A herança de color/font-size é aplicada por quem
    /// desce a árvore; aqui só o estilo PRÓPRIO do nó.)
    pub fn computed_style(&self, id: NodeId) -> Option<crate::style::ComputedStyle> {
        // A API pública devolve VALOR: quem chama de fora (a ABI, o `getComputedStyle`)
        // quer um dado próprio, e é uma chamada por vez — o `Rc` existe para o
        // caminho interno do layout, que pede o mesmo estilo dezenas de vezes.
        self.computed_style_idx(self.resolve(id)?)
            .map(|rc| (*rc).clone())
    }

    /// Igual a [`computed_style`](Dom::computed_style), mas por `NodeIdx` cru — o
    /// render desce a árvore em índices. `None` se o nó não é elemento.
    ///
    /// Aplica a cascade COMPLETA da MDN, em duas passagens (estágio 1: `!important`
    /// inverte a precedência de origem):
    /// - **Normais**, do mais fraco ao mais forte: `defineStyle` (UA) < `<style>`
    ///   autor < `style=""` inline < override por-nó (`setStyleBatch`).
    /// - **Important**, por cima de tudo, na mesma ordem de origem: `<style>`
    ///   important < inline important < override (tratado como mais forte).
    /// Devolve um `Rc`: ver a nota no campo `computed_memo` — o valor tem 1 KB e
    /// o layout o pede várias vezes por nó. Quem precisa MUTAR faz
    /// `(*rc).clone()`, o que é exatamente o ponto (a cópia passa a ser
    /// explícita e rara em vez de implícita e por acesso).
    pub fn computed_style_idx(
        &self,
        idx: NodeIdx,
    ) -> Option<std::rc::Rc<crate::style::ComputedStyle>> {
        // MEMO por revisão: dentro de um mesmo estado da árvore, a cascade de um nó
        // é determinística — e o layout a consulta várias vezes por nó (medição +
        // pintura). Um clone do ComputedStyle é muito mais barato que re-rodar
        // todas as regras do stylesheet (Bootstrap: ~2700).
        let anim_epoch = self.anim_epoch;
        let style_epoch = crate::style::props::style_epoch();
        let (vw, vh) = self.viewport.get();
        let vp_key = (vw.to_bits(), vh.to_bits());
        if self.memo_revision.get() != anim_epoch
            || self.memo_style_epoch.get() != style_epoch
            || self.memo_viewport.get() != vp_key
        {
            self.computed_memo.borrow_mut().clear();
            self.memo_revision.set(anim_epoch);
            self.memo_style_epoch.set(style_epoch);
            self.memo_viewport.set(vp_key);
        }
        crate::bump!(computed_calls);
        if let Some(Some(hit)) = self.computed_memo.borrow().get(idx) {
            crate::bump!(computed_memo_hits);
            return Some(std::rc::Rc::clone(hit));
        }
        // O estilo COM animação = a BASE (cascade sem anim, memoizada por revisão
        // estrutural via `base_style_idx`) + a camada de `anim_override` por cima. Não
        // re-roda a cascade a cada frame de animação: só clona a base cacheada e
        // sobrepõe o override interpolado — o que torna o RELAYOUT durante animação
        // barato (era o gargalo restante depois de acelerar o `advance`).
        let base = self.base_style_idx(idx)?;
        // SEM animação, o computado É a base: compartilha o mesmo `Rc` em vez de
        // materializar uma segunda cópia de 1 KB por nó. Só quem anima paga a
        // cópia, que é quando ela é de fato necessária (o override interpolado
        // muda a cada frame).
        let computed = match self.anim_override.get(&idx) {
            None => base,
            Some(anim) => {
                let mut c = (*base).clone();
                c.merge_over(anim);
                std::rc::Rc::new(c)
            }
        };
        memo_put(
            &mut self.computed_memo.borrow_mut(),
            idx,
            self.nodes.len(),
            &computed,
        );
        Some(computed)
    }

    /// A CAIXA GERADA de um pseudo-elemento deste nó, ou `None` quando a
    /// cascata não manda gerar nenhuma.
    ///
    /// `None` cobre os quatro casos em que não há caixa, e são todos da spec:
    /// nenhuma regra `::before`/`::after` casa; nenhuma delas declara `content`;
    /// o `content` vencedor é `none`/`normal`; ou o pseudo tem `display:none`.
    ///
    /// O estilo é o do elemento originante HERDADO e depois sobreposto pelas
    /// declarações do pseudo — herdar do elemento e não da raiz é o que faz um
    /// `::before` sem `color` sair da cor do texto à volta, como no browser.
    pub fn pseudo_box(
        &self,
        idx: NodeIdx,
        pe: crate::style::PseudoElement,
    ) -> Option<crate::pseudo::PseudoBox> {
        if !self.stylesheet.has_generated_content() {
            return None;
        }
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return None;
        };
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let (matched, content) = self.stylesheet.matched_for_pseudo(
            &self.media_context(),
            tag,
            self.nodes[idx].attr("id"),
            &classes,
            pe,
            |sel| self.matches_complex(idx, sel),
        );
        let content = content?;
        let contadores = self.document_counters();
        let texto = crate::pseudo::texto_de(
            &content,
            &|nome: &str| self.nodes[idx].attr(nome).map(str::to_string),
            contadores.get(&(idx, pe)),
        )?;
        // O `direction` do ORIGINANTE, já resolvido (herança incluída) — o
        // pseudo herda dele daqui a pouco, e uma `margin-inline-*` que o
        // pseudo declare tem de ver o MESMO valor, não o inicial.
        let pai = self.computed_style_idx(idx);
        let direction = pai.as_deref().and_then(|p| p.direction);
        let decls = self.stylesheet.declarations_from(&matched, None, direction);
        // Herda do originante e só depois aplica o que o pseudo declara — a
        // ordem inversa perderia a herança para qualquer propriedade que o
        // pseudo não declare.
        let mut css = crate::style::ComputedStyle::default();
        if let Some(pai) = pai {
            css.inherit_from(&pai);
        }
        css.merge_over(&decls.normal);
        css.merge_over(&decls.important);
        if css.effective_display() == Some(crate::style::DisplayKind::None) {
            return None;
        }
        Some(crate::pseudo::PseudoBox { texto, css })
    }

    /// A COR do `::marker` deste `<li>` (lote O), se alguma regra `::marker`
    /// a declara — `None` quando não há nenhuma, e quem chama fica com o
    /// `color` herdado do próprio `<li>` (o que `listitem::emit_marker` já
    /// fazia sozinho antes deste lote).
    ///
    /// Só a COR: o `font-size` do marcador não é lido daqui de propósito — ele
    /// mudaria a MEDIDA da linha (`ctx.measurer`), e essa medida é decidida em
    /// `layout/linha.rs`/`layout/runs.rs`, fora deste lote (lote S). Aplicar
    /// só a cor é seguro porque pintar não muda geometria nenhuma.
    pub fn marker_color(
        &self,
        idx: NodeIdx,
        herdado: &crate::style::ComputedStyle,
    ) -> Option<u32> {
        if !self.stylesheet.has_generated_content() {
            return None;
        }
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return None;
        };
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let (matched, _content) = self.stylesheet.matched_for_pseudo(
            &self.media_context(),
            tag,
            self.nodes[idx].attr("id"),
            &classes,
            crate::style::PseudoElement::Marker,
            |sel| self.matches_complex(idx, sel),
        );
        if matched.is_empty() {
            return None;
        }
        let decls = self.stylesheet.declarations_from(&matched, None, herdado.direction);
        let mut css = herdado.clone();
        css.merge_over(&decls.normal);
        css.merge_over(&decls.important);
        css.color
    }

    /// A tabela de CONTADORES do documento, calculada uma vez por revisão.
    ///
    /// Numa página que não declare `counter-reset`/`counter-increment` isto é
    /// uma tabela vazia e a travessia nem corre — a guarda é a mesma ideia do
    /// `has_generated_content()` que abre o `pseudo_box`, e pela mesma razão:
    /// três das quatro folhas do corpus não têm contador nenhum.
    fn document_counters(&self) -> std::rc::Rc<crate::counters::Tabela> {
        let chave = (self.revision, crate::style::props::style_epoch());
        if self.counter_memo_revision.get() == chave {
            if let Some(t) = self.counter_memo.borrow().as_ref() {
                return std::rc::Rc::clone(t);
            }
        }
        let tabela = if self.stylesheet.has_counters() {
            crate::counters::calcula(self, &|idx, pe| self.counter_ops(idx, pe))
        } else {
            crate::counters::Tabela::default()
        };
        let tabela = std::rc::Rc::new(tabela);
        *self.counter_memo.borrow_mut() = Some(std::rc::Rc::clone(&tabela));
        self.counter_memo_revision.set(chave);
        tabela
    }

    /// As operações de contador de um elemento (`pe: None`) ou de um dos seus
    /// pseudo-elementos, já resolvidas pela cascata.
    ///
    /// O `style=""` inline NÃO é consultado: `counter-increment` num atributo de
    /// estilo não aparece em nenhuma das quatro folhas do corpus, e lê-lo
    /// exigiria parsear o atributo por nó nesta passagem — o custo por elemento
    /// que a guarda de `has_counters` existe para evitar. Fica dito por ser um
    /// corte e não um esquecimento.
    fn counter_ops(
        &self,
        idx: NodeIdx,
        pe: Option<crate::style::PseudoElement>,
    ) -> Option<std::rc::Rc<crate::counters::Ops>> {
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return None;
        };
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let media_ctx = self.media_context();
        let id_attr = self.nodes[idx].attr("id");
        let matched = match pe {
            None => self
                .stylesheet
                .matched_for_node(&media_ctx, tag, id_attr, &classes, |sel| {
                    self.matches_complex(idx, sel)
                }),
            Some(pe) => {
                self.stylesheet
                    .matched_for_pseudo(&media_ctx, tag, id_attr, &classes, pe, |sel| {
                        self.matches_complex(idx, sel)
                    })
                    .0
            }
        };
        self.stylesheet.counters_from(&matched)
    }

    /// Núcleo da cascade — computa o ALVO-BASE de um nó (SEM a camada de animação; o
    /// override interpolado é sobreposto por quem consome, em `computed_style_idx`).
    /// Chamado via `base_style_idx` (memoizado por revisão estrutural).
    pub(in crate::dom) fn computed_style_idx_inner(&self, idx: NodeIdx) -> Option<crate::style::ComputedStyle> {
        use crate::style;
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.as_str(),
            _ => return None,
        };
        crate::bump!(cascade_runs);
        let _phase = crate::metrics::phases::scope("cascade");
        // id/classes só são materializados quando há regras de autor para testar.
        // Em páginas sem `<style>`, o layout ainda computa cada nó, mas não precisa
        // alocar strings que nunca serão consultadas pelo RuleIndex.
        let node_id: Option<String> = if self.stylesheet.is_empty() {
            None
        } else {
            self.nodes[idx].attr("id").map(str::to_string)
        };
        let node_classes: Vec<String> = if self.stylesheet.is_empty() {
            Vec::new()
        } else {
            self.nodes[idx]
                .attr("class")
                .map(|c| c.split_whitespace().map(str::to_string).collect())
                .unwrap_or_default()
        };
        let class_refs: Vec<&str> = node_classes.iter().map(String::as_str).collect();
        // `style=""` inline (normal + important + customs/pendentes).
        let inline = self.nodes[idx]
            .attr("style")
            .map(style::parse_inline_block)
            .unwrap_or_default();

        // ── CUSTOM PROPERTIES do elemento (#1779, PASS A): as declarações `--x:`
        // das regras que casam + as do style="" + a HERANÇA do pai (o computed do
        // pai já carrega o mapa dele — CoW: sem declaração própria, compartilha o
        // Arc). Precisam vir ANTES porque os valores com var() dependem delas.
        let parent_css_for_vars = self
            .element_parent_idx(idx)
            .and_then(|p| self.base_style_idx(p));
        // O `direction` HERDADO, cedo — pela mesma razão que `parent_font`
        // (abaixo) é lido cedo: uma `margin-inline-*`/`padding-inline-*`/
        // `border-inline-*` deste elemento (`style::logical`) só resolve o
        // lado físico depois de saber o `direction`, e a herança OFICIAL
        // (`css.inherit_from`, mais abaixo) só corre depois de as
        // declarações próprias serem aplicadas. É só o FALLBACK: se este
        // elemento também declarar `direction` (mesma regra ou outra mais
        // específica), essa vitória normal da cascade continua a valer —
        // `apply_resolved_decl` só usa isto quando o campo ainda está por
        // declarar (`.or`). `direction_herdada::para_logicas` nega-o quando
        // o pai é uma LINHA de flex — ver o cabeçalho lá para o porquê.
        let parent_direction = direction_herdada::para_logicas(parent_css_for_vars.as_deref());
        // As regras que casam este nó, casadas UMA vez e usadas nos DOIS passes
        // (custom properties e declarações). Antes cada passe refazia o
        // matching completo — e o matching navega a árvore.
        let matched = if self.stylesheet.is_empty() {
            style::MatchedRules::default()
        } else {
            self.stylesheet.matched_for_node(
                &self.media_context(),
                &tag,
                node_id.as_deref(),
                &class_refs,
                |sel| self.matches_complex(idx, sel),
            )
        };
        let own_customs: Vec<(String, String)> = if self.stylesheet.is_empty() {
            inline.custom.clone()
        } else {
            let mut v = self.stylesheet.custom_from(&matched);
            v.extend(inline.custom.iter().cloned());
            v
        };
        let own_customs_important: Vec<(String, String)> = if self.stylesheet.is_empty() {
            inline.custom_important.clone()
        } else {
            let mut v = self.stylesheet.custom_important_from(&matched);
            v.extend(inline.custom_important.iter().cloned());
            v
        };
        let parent_vars = parent_css_for_vars
            .as_ref()
            .and_then(|p| p.custom_props.clone());
        // `@property … inherits: false` (lote P, §5.P item 4): tira do mapa
        // herdado ANTES de aplicar as declarações próprias — sem isto o filho
        // herdaria o valor do pai como qualquer custom property comum, que é
        // exatamente o que `inherits:false` existe para recusar. Gate no
        // `is_empty()`: uma página sem `@property` não paga a filtragem.
        let props_registry = self.stylesheet.properties_registry();
        let parent_vars = if props_registry.is_empty() {
            parent_vars
        } else {
            parent_vars.map(|arc| {
                std::sync::Arc::new(
                    arc.iter()
                        .filter(|(k, _)| props_registry.inherits(k))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<std::collections::HashMap<_, _>>(),
                )
            })
        };
        let vars_arc: Option<std::sync::Arc<std::collections::HashMap<String, String>>> =
            match (
                parent_vars,
                own_customs.is_empty() && own_customs_important.is_empty(),
            ) {
                (p, true) => p, // só herda: compartilha o Arc (O(1))
                (p, false) => {
                    crate::bump!(custom_maps_built);
                    let mut m = p.map(|a| (*a).clone()).unwrap_or_default();
                    // Normais entram primeiro; as importantes vencem por nome.
                    for (k, v) in own_customs
                        .into_iter()
                        .chain(own_customs_important.into_iter())
                    {
                        // AUTO-REFERÊNCIA DIRETA (`--c: ...var(--c)...`): a declaração é
                        // guaranteed-invalid (spec) — o Chrome a DESCARTA e mantém a
                        // anterior válida. Se já há um valor para `k` e a nova declaração
                        // se auto-referencia, ignora a nova. Sem valor anterior, insere
                        // e o consumidor corta o ciclo.
                        if references_self(&k, &v) && m.contains_key(&k) {
                            continue;
                        }
                        m.insert(k, v);
                    }
                    Some(std::sync::Arc::new(m))
                }
            };
        // `@property … initial-value` (item 4): um `var(--x)` sem declaração
        // alcançável usa o inicial registado em vez do fallback vazio de
        // sempre. `seed_defaults` só ACRESCENTA nomes ausentes — uma
        // declaração real, própria ou herdada, nunca é sobrescrita.
        let vars_arc = if props_registry.is_empty() {
            vars_arc
        } else {
            let mut m = vars_arc.as_deref().cloned().unwrap_or_default();
            props_registry.seed_defaults(&mut m);
            Some(std::sync::Arc::new(m))
        };
        let empty_vars = std::collections::HashMap::new();
        let vars_ref: &std::collections::HashMap<String, String> =
            vars_arc.as_deref().unwrap_or(&empty_vars);

        // Stylesheet de autor resolvido para este nó (normal + important separados;
        // PASS B — as declarações com var() resolvem na posição da regra, contra
        // as vars acima). O matcher navega a árvore via `matches_complex`.
        let author = if self.stylesheet.is_empty() {
            style::DeclBlock::default()
        } else {
            self.stylesheet
                .declarations_from(&matched, Some(vars_ref), parent_direction)
        };
        let override_node = self.style_overrides.get(&idx);

        // ── Passe 1: NORMAIS (fraco → forte) ────────────────────────────────────
        let mut css = style::lookup_style(&tag).unwrap_or_default(); // UA/defineStyle
        if author.all_initial_normal {
            css = style::ComputedStyle::default();
        }
        css.merge_over(&author.normal); // <style> autor
        if inline.all_initial_normal {
            css = style::ComputedStyle::default();
        }
        css.merge_over(&inline.normal); // style="" inline
        for (prop, raw, important) in &inline.pending {
            if !important {
                let dir = css.direction.or(parent_direction);
                crate::style::stylesheet::apply_resolved_decl(&mut css, prop, raw, vars_ref, dir);
            }
        }
        if let Some(ov) = override_node {
            css.merge_over(ov); // override por-nó (setStyleBatch)
        }
        // ── Passe 2: IMPORTANT (vencem qualquer normal) ─────────────────────────
        if author.all_initial_important {
            css = style::ComputedStyle::default();
        }
        css.merge_over(&author.important); // <style> !important
        if inline.all_initial_important {
            css = style::ComputedStyle::default();
        }
        css.merge_over(&inline.important); // inline !important
        for (prop, raw, important) in &inline.pending {
            if *important {
                let dir = css.direction.or(parent_direction);
                crate::style::stylesheet::apply_resolved_decl(&mut css, prop, raw, vars_ref, dir);
            }
        }
        // o mapa de vars entra no computado (os FILHOS herdam daqui).
        css.custom_props = vars_arc;

        // ── FONT-SIZE resolve CEDO (aqui na cascade, não no layout): a base de
        // `em`/`%` de font-size é o font do PAI (já computado em Px pela recursão
        // abaixo) e `rem`/`vw`/`vh` usam root/viewport — assim a HERANÇA desce
        // sempre o VALOR (Px), nunca a forma (um `2em` herdado re-multiplicaria a
        // cada nível). É o que permite `calc(1.375rem + 1.5vw)` no font-size (a
        // tipografia fluida do h1 do Bootstrap).
        let parent_css = parent_css_for_vars;
        // Perguntado UMA vez: decide a base do `rem` na resolução abaixo e a
        // escrita da base logo a seguir, e as duas têm de concordar.
        let e_raiz = matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "html");
        if let Some(d) = css.font_size {
            let parent_font = parent_css
                .as_ref()
                .and_then(|p| match p.font_size {
                    Some(style::Dimension::Px(v)) => Some(v),
                    _ => None,
                })
                .unwrap_or(crate::layout::DEFAULT_FONT_SIZE);
            let (vw, vh) = self.viewport.get();
            let rctx = style::ResolveCtx {
                parent_content_w: parent_font, // `%` de font-size = % do font do PAI
                node_font_size: parent_font,   // `em` de font-size = × font do PAI
                // A RAIZ não tem raiz. Enquanto o `<html>` está a ser resolvido
                // ainda não existe base de `rem` deste documento — e o
                // thread-local ainda carrega a do documento ANTERIOR, porque só
                // é reescrito umas linhas abaixo. Um `html{font-size:2rem}`
                // resolvia contra o `html{font-size:10px}` da página de antes e
                // respondia 20px; agora resolve contra o inicial.
                //
                // É o que o Blink faz, e lá é estrutural em vez de condicional:
                // `ElementResolveContext` só guarda o estilo da raiz `if
                // (element != root_element)`, e o `CSSToLengthConversionData::
                // FontSizes` trata esse nulo com regra própria. O valor deles
                // pertence ao DOCUMENTO e é passado por parâmetro; o nosso é um
                // thread-local, e é essa diferença que abre a janela que esta
                // linha fecha.
                //
                // ⚠️ Fecha a FUGA, não o estado global: os 15 sítios do
                // `layout/` continuam a ler o thread-local, e continuam certos
                // porque a raiz já foi resolvida quando eles correm. Pôr a base
                // no `Dom`, como o Blink, é candidato próprio — toca 11
                // ficheiros do `layout/`.
                root_font_size: if e_raiz {
                    crate::layout::DEFAULT_FONT_SIZE
                } else {
                    crate::style::root_font_size()
                },
                viewport_w: vw,
                viewport_h: vh,
            };
            css.font_size = d
                .resolve(&rctx)
                .filter(|v| *v > 0.0)
                .map(style::Dimension::Px);
        }
        // A fonte do `<html>` é a BASE DO `rem` para a árvore inteira — o idioma
        // `html { font-size: 62.5% }` faz `1rem` valer 10px, e sem esta linha
        // ficava nos 16px de default e todo o `rem` da página saía 60% grande
        // demais. Escrito aqui porque a cascade corre de cima para baixo: quando
        // um descendente resolve o seu `rem`, a raiz já passou por aqui.
        if e_raiz {
            // Sem declaração no root, a base VOLTA aos 16px. É o que impede o
            // valor de um documento de sobreviver ao seguinte: o estado é por
            // thread (como o estilo por tag) e um `html { font-size: 10px }` de
            // uma página ficaria a valer na próxima que não declarasse nada.
            style::set_root_font_size(match css.font_size {
                Some(style::Dimension::Px(v)) => v,
                _ => crate::layout::DEFAULT_FONT_SIZE,
            });
        }

        // ── HERANÇA (CSS inherited properties): color/font/text-align/etc. que NÃO
        // foram declaradas neste nó descem do PAI-elemento. É o que faz o texto pegar
        // a cor do body sem cada elemento redeclarar (sem isto, texto fica preto).
        if let Some(parent_css) = &parent_css {
            crate::bump!(inherit_steps);
            css.inherit_from(parent_css);
        } else {
            crate::style::inherit_kw::apply_root_inherit_as_initial(&mut css);
        }

        // A camada de ANIMAÇÃO (o `anim_override` interpolado) NÃO entra aqui: este é
        // o ALVO-BASE. `computed_style_idx` a sobrepõe sobre a base memoizada — assim a
        // cascade (cara) roda só quando a ESTRUTURA muda, não a cada frame de animação.
        Some(css)
    }

    /// O pai de `idx` SE for um elemento (não o #document) — para a herança subir só
    /// pela cadeia de elementos.
    fn element_parent_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let p = self.nodes[idx].parent?;
        matches!(self.nodes[p].kind, NodeKind::Element { .. }).then_some(p)
    }
}
