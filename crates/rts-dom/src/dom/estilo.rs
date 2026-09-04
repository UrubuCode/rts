//! ESTILO por nó fora da cascade: o stylesheet de autor, o `style=""` inline,
//! os overrides por-nó (`setStyleBatch`) e as propriedades lidas de fora.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {
    /// O ALVO-BASE (cascade sem animação) de um nó, MEMOIZADO por revisão estrutural.
    /// O `advance` consulta isto a cada frame; entre frames de animação (revisão
    /// estrutural estável) é um hit de cache — a cascade não re-roda. `None` p/
    /// não-elemento.
    pub(in crate::dom) fn base_style_idx(
        &self,
        idx: NodeIdx,
    ) -> Option<std::rc::Rc<crate::style::ComputedStyle>> {
        let style_epoch = crate::style::props::style_epoch();
        let (vw, vh) = self.viewport.get();
        let vp_key = (vw.to_bits(), vh.to_bits());
        if self.base_memo_revision.get() != style_epoch || self.base_memo_viewport.get() != vp_key {
            self.base_memo.borrow_mut().clear();
            self.base_memo_revision.set(style_epoch);
            self.base_memo_viewport.set(vp_key);
        }
        crate::bump!(base_calls);
        if let Some(Some(hit)) = self.base_memo.borrow().get(idx) {
            crate::bump!(base_memo_hits);
            return Some(std::rc::Rc::clone(hit));
        }
        let computed = std::rc::Rc::new(self.computed_style_idx_inner(idx)?);
        memo_put(
            &mut self.base_memo.borrow_mut(),
            idx,
            self.nodes.len(),
            &computed,
        );
        Some(computed)
    }

    /// Acrescenta CSS externo ao stylesheet autoral. O conteúdo dos elementos
    /// `<style>` é recolhido dos nós vivos por `rebuild_author_stylesheet`, para
    /// que remoções e substituições não deixem regras antigas na cascade.
    pub fn add_stylesheet(&mut self, css: &str) {
        let _phase = crate::metrics::phases::scope("parse-css");
        crate::bump!(stylesheets_added);
        crate::bump!(css_bytes, css.len());
        self.external_css.push_str(css);
        self.external_css.push('\n');
        self.rebuild_author_stylesheet();
    }

    /// Reconstrói a stylesheet autoral na ordem documental dos `<style>` vivos.
    /// Esta operação é usada depois de mutações que podem inserir, remover ou
    /// substituir CSS. O CSS externo é preservado como a primeira origem autoral.
    pub(in crate::dom) fn rebuild_author_stylesheet(&mut self) {
        let mut embedded = Vec::new();
        self.collect_embedded_css(self.root, &mut embedded);

        let mut stylesheet = crate::style::Stylesheet::new();
        if !self.external_css.is_empty() {
            stylesheet.append_css(&self.external_css);
        }
        for css in &embedded {
            stylesheet.append_css(css);
        }
        self.stylesheet = stylesheet;

        self.raw_css.clear();
        self.raw_css.push_str(&self.external_css);
        for css in embedded {
            self.raw_css.push_str(&css);
            self.raw_css.push('\n');
        }
        self.touch();
    }

    pub(in crate::dom) fn subtree_contains_style(&self, idx: NodeIdx) -> bool {
        if matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "style") {
            return true;
        }
        self.nodes[idx]
            .children
            .iter()
            .copied()
            .any(|child| self.subtree_contains_style(child))
    }

    fn collect_embedded_css(&self, idx: NodeIdx, out: &mut Vec<String>) {
        if let NodeKind::Element { tag } = &self.nodes[idx].kind {
            if tag == "style" {
                let css = self.text_content(self.make_id(idx)).unwrap_or_default();
                out.push(css);
            }
        }
        for &child in &self.nodes[idx].children {
            self.collect_embedded_css(child, out);
        }
    }

    /// O estilo da SCROLLBAR resolvido da página (#1744): combina `scrollbar-width`/
    /// `scrollbar-color` declarados no `<body>`/`<html>` (sintaxe padrão) com os
    /// pseudo-elementos `::-webkit-scrollbar*` do CSS bruto (WebKit). O WebKit vence
    /// o padrão (ordem do Chrome). O backend (egui) lê isto e pinta a barra.
    pub fn scrollbar_style(&self) -> crate::scrollbar::ScrollbarStyle {
        crate::scrollbar::resolve(&self.raw_css)
    }

    /// O stylesheet de autor acumulado (regras dos `<style>`). Exposto p/ inspeção/teste.
    pub fn stylesheet(&self) -> &crate::style::Stylesheet {
        &self.stylesheet
    }

    /// `getComputedStyle(el).<name>` — o valor COMPUTADO (após a cascade completa)
    /// de uma propriedade CSS por nome, no formato do browser. `""` se não definida
    /// ou o nó não é elemento. (#1759)
    pub fn computed_property(&self, id: NodeId, name: &str) -> String {
        // `computed_value` e não `get_property`: o computed NUNCA responde vazio
        // — o que ninguém declarou vale o INICIAL (`float: none`, `color:
        // rgb(0, 0, 0)`). O `get_property` cru continua a servir o
        // `el.style.x`, que TEM de responder vazio fora do `style=""`. A tag vai
        // junto porque o inicial de `display` é o da UA-stylesheet dela.
        let Some(idx) = self.resolve(id) else {
            return String::new();
        };
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => Some(tag.clone()),
            _ => None,
        };
        let Some(style) = self.computed_style(id) else {
            return String::new();
        };

        // Blink serializa `grid-template-columns` a partir do ComputedStyle com
        // acesso ao LayoutObject quando a propriedade depende dos used values. O
        // layout RTS já calcula as mesmas larguras; consulta-se a display list
        // cacheada em vez de duplicar o algoritmo de sizing no formatador de estilo.
        //
        // Mede com o medidor ACTIVO (`layout::medidor_ativo`) pela mesma razão
        // de `bounding_component`: as larguras de coluna dependem de texto
        // medido, e um `getComputedStyle` chamado com janela aberta deve
        // responder com a MESMA geometria que está a ser pintada, não com a
        // aproximação headless.
        if name.trim().eq_ignore_ascii_case("grid-template-columns")
            && style.grid_template_columns.is_some()
        {
            let (viewport_w, viewport_h) = self.viewport.get();
            let tracks_str = crate::layout::medidor_ativo::with_active(|measurer| {
                let context = crate::layout::LayoutCtx {
                    viewport_w,
                    viewport_h,
                    measurer,
                };
                let list = crate::layout::layout_cached(self, &context);
                list.grid_column_tracks.get(&idx).map(|tracks| {
                    tracks
                        .iter()
                        .map(|track| crate::style::fmt_values::fmt_px(*track))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            });
            if let Some(tracks_str) = tracks_str {
                return tracks_str;
            }
        }

        style.computed_value(name, tag.as_deref())
    }

    /// `el.style.<name>` (getPropertyValue) — o valor INLINE da propriedade (só o
    /// `style=""`, sem a cascade), no formato do browser. `""` se ausente.
    pub fn inline_property(&self, id: NodeId, name: &str) -> String {
        let Some(idx) = self.resolve(id) else {
            return String::new();
        };
        let inline = self.nodes[idx]
            .attr("style")
            .map(crate::style::parse_inline_block)
            .unwrap_or_default();
        // o inline (normal+important fundidos) → get_property.
        let mut css = inline.normal.clone();
        css.merge_over(&inline.important);
        css.get_property(name)
    }

    /// `el.style.cssText` (get) — o atributo `style=""` cru (a string inteira).
    pub fn css_text(&self, id: NodeId) -> String {
        self.get_attr(id, "style").unwrap_or("").to_string()
    }

    /// `el.style.cssText = v` (set) — substitui o `style=""` inteiro.
    pub fn set_css_text(&mut self, id: NodeId, text: &str) {
        self.set_attr(id, "style", text);
    }

    /// `el.style.setProperty(name, value)` — define UMA propriedade no `style=""`
    /// inline, preservando as demais. Re-serializa a string `style`. Valor vazio
    /// REMOVE a propriedade (como `removeProperty`).
    pub fn set_style_property(&mut self, id: NodeId, name: &str, value: &str) {
        let cur = self.css_text(id);
        let new = upsert_css_decl(&cur, name.trim(), value.trim());
        self.set_attr(id, "style", &new);
    }

    /// `el.style.removeProperty(name)` — remove a propriedade do `style=""`.
    pub fn remove_style_property(&mut self, id: NodeId, name: &str) {
        let cur = self.css_text(id);
        let new = upsert_css_decl(&cur, name.trim(), ""); // valor vazio = remover
        self.set_attr(id, "style", &new);
    }

    /// Aplica UM slot de estilo OPACO (invariante 4) a UM nó, acumulando no
    /// override por-nó (`setStyle` por-nó / base do `setStyleBatch`). O `(slot,
    /// val)` é interpretado pelo `apply_slot` do `ComputedStyle` (nunca casa string
    /// CSS aqui). Ignora id que não resolve.
    pub fn set_node_style_slot(&mut self, id: NodeId, slot: i64, val: i64) {
        crate::bump!(style_overrides_set);
        let Some(idx) = self.resolve(id) else { return };
        self.touch_subtree(idx);
        self.style_overrides
            .entry(idx)
            .or_default()
            .apply_slot(slot, val);
    }

    /// Aplica um LOTE de triplas `(nodeId, slot, val)` de uma vez (invariante 6:
    /// estilizar N nós por frame não pode ser N×5 FFIs). Cada tripla acumula no
    /// override do seu nó. O `nodes` é uma fatia plana `[id0, slot0, val0, id1,
    /// slot1, val1, …]` (o jeito que o buffer GC chega da ABI). Triplas com id
    /// inválido são ignoradas (robustez).
    pub fn apply_style_batch(&mut self, triples: &[i64]) {
        crate::bump!(style_overrides_set, triples.len() / 3);
        let mut updates = Vec::with_capacity(triples.len() / 3);
        for t in triples.chunks_exact(3) {
            if let Some(node) = NodeId::from_abi(t[0]) {
                if let Some(idx) = self.resolve(node) {
                    updates.push((idx, t[1], t[2]));
                }
            }
        }
        if updates.is_empty() {
            return;
        }
        self.touch_subtrees(updates.iter().map(|(idx, _, _)| *idx));
        for (idx, slot, val) in updates {
            self.style_overrides
                .entry(idx)
                .or_default()
                .apply_slot(slot, val);
        }
    }

    /// Limpa TODOS os overrides por-nó (`setStyleBatch` recomeça do zero). Útil se
    /// o app quer re-estilizar do zero num frame em vez de acumular.
    pub fn clear_style_overrides(&mut self) {
        self.touch();
        self.style_overrides.clear();
    }

    /// O override de estilo POR-NÓ de um nó (`setStyleBatch`), se houver. O render
    /// o mescla como 3ª camada (após tag e `style=""` inline). `None` = sem override.
    pub fn node_style_override(&self, id: NodeId) -> Option<crate::style::ComputedStyle> {
        let idx = self.resolve(id)?;
        self.style_overrides.get(&idx).cloned()
    }

    /// Idem [`node_style_override`], mas por `NodeIdx` cru (o render de texto opera
    /// em índices ao descer a árvore). `None` = sem override.
    pub fn style_override_idx(&self, idx: NodeIdx) -> Option<crate::style::ComputedStyle> {
        self.style_overrides.get(&idx).cloned()
    }

    /// O código de `display` de um nó (do `BlockDef` registrado p/ a tag), ou
    /// `-1` se a tag não tem layout de bloco (inline/desconhecida).
    pub fn display_of(&self, id: NodeId) -> i64 {
        let Some(idx) = self.resolve(id) else {
            return -1;
        };
        match &self.nodes[idx].kind {
            NodeKind::Element { tag } => crate::block::lookup(tag).map(|d| d.display).unwrap_or(-1),
            _ => -1,
        }
    }
}
