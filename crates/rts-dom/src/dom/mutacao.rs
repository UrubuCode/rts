//! MUTAÇÃO da árvore: criar, inserir, mover, clonar, substituir e remover
//! nós, mexer em atributos e texto, e o `innerHTML` de escrita.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    // ── Mutação (base da API DOM do JS) ─────────────────────────────────────

    /// Substitui TODO o conteúdo de um elemento por um único nó de texto (o
    /// equivalente a `element.textContent = txt`). Não faz nada num nó de texto.
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        crate::bump!(set_text);
        let Some(idx) = self.resolve(id) else { return };
        if !matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            return;
        }
        let style_node = matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "style");
        self.touch_render_only(idx);
        // Descarta os filhos atuais (arena não compacta; vira lixo inacessível —
        // ok para o uso atual, a árvore é reconstruída a cada `html()`). Zera o
        // `parent` de cada um para `is_attached`/query não os acharem.
        let old_children = std::mem::take(&mut self.nodes[idx].children);
        for c in old_children {
            self.nodes[c].parent = None;
        }
        let child = self.push_detached(NodeKind::Text(text.to_string()));
        self.nodes[child].parent = Some(idx);
        self.nodes[idx].children.push(child);
        if style_node {
            self.rebuild_author_stylesheet();
        }
    }


    // ── Mutação rica — #1756 ─────────────────────────────────────────────────────

    /// `node.cloneNode(deep)`: duplica o nó. `deep=false` clona só o nó (sem
    /// filhos); `deep=true` clona a subárvore inteira. O clone é SOLTO (sem pai) —
    /// anexe-o com appendChild/insertBefore. Devolve o `NodeId` do clone.
    pub fn clone_node(&mut self, id: NodeId, deep: bool) -> Option<NodeId> {
        crate::bump!(clones);
        let idx = self.resolve(id)?;
        let new_idx = self.clone_subtree(idx, deep);
        Some(self.make_id(new_idx))
    }

    /// Clona um nó (e opcionalmente sua subárvore) DENTRO do mesmo DOM, soltos.
    fn clone_subtree(&mut self, src_idx: NodeIdx, deep: bool) -> NodeIdx {
        let kind = self.nodes[src_idx].kind.clone();
        let attrs = self.nodes[src_idx].attrs.clone();
        let new_idx = self.push_detached(kind);
        self.nodes[new_idx].attrs = attrs;
        // INDEXA o clone (id/class) — senão querySelector('#x')/getElementById não o
        // acham depois de anexado (caminhos que usam só os índices).
        self.index_node(new_idx);
        if deep {
            let children = self.nodes[src_idx].children.clone();
            for c in children {
                let cc = self.clone_subtree(c, true);
                self.nodes[cc].parent = Some(new_idx);
                self.nodes[new_idx].children.push(cc);
            }
        }
        new_idx
    }

    /// `parent.prepend(child)`: insere `child` no INÍCIO dos filhos de `parent`.
    pub fn prepend_child(&mut self, parent: NodeId, child: NodeId) {
        self.touch();
        let first = self.first_child(parent);
        self.insert_before(parent, child, first);
    }

    /// `node.before(other)` / `after`: insere `other` como irmão antes/depois de
    /// `node` (no pai de `node`). `after=true` insere depois.
    pub fn insert_adjacent(&mut self, node: NodeId, other: NodeId, after: bool) {
        self.touch();
        let Some(parent) = self.parent_of(node) else {
            return;
        };
        let reference = if after {
            self.next_sibling(node)
        } else {
            Some(node)
        };
        self.insert_before(parent, other, reference);
    }

    /// `node.replaceWith(other)`: substitui `node` por `other`. ATÔMICO: insere
    /// `other` no lugar e SÓ remove `node` se a inserção funcionou (a guarda de
    /// ciclo pode abortar o insert — aí não destruímos `node`). No-op se `other`
    /// é o próprio `node`.
    pub fn replace_with(&mut self, node: NodeId, other: NodeId) {
        self.touch();
        if node == other {
            return; // substituir por si mesmo é no-op (não remove)
        }
        let Some(parent) = self.parent_of(node) else {
            return;
        };
        self.insert_before(parent, other, Some(node)); // other ANTES de node
        // só remove node se other realmente entrou (insert pode ter abortado por ciclo).
        if self.parent_of(other) == Some(parent) {
            self.remove_node(node);
        }
    }

    /// `parent.replaceChild(new, old)`: substitui o filho `old` por `new`. ATÔMICO:
    /// só remove `old` se `new` foi inserido (guarda de ciclo). No-op se new==old.
    pub fn replace_child(&mut self, parent: NodeId, new_child: NodeId, old_child: NodeId) {
        self.touch();
        if new_child == old_child {
            return;
        }
        if self.parent_of(old_child) != Some(parent) {
            return; // old precisa ser filho de parent
        }
        self.insert_before(parent, new_child, Some(old_child));
        if self.parent_of(new_child) == Some(parent) {
            self.remove_node(old_child);
        }
    }

    /// `parent.removeChild(child)`: remove `child` se for filho de `parent`.
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) {
        self.touch();
        if self.parent_of(child) == Some(parent) {
            self.remove_node(child);
        }
    }

    /// `parent.replaceChildren()`: remove TODOS os filhos de `parent` (a variante
    /// com novos filhos é montada no JS chamando isto + appendChild).
    pub fn clear_children(&mut self, parent: NodeId) {
        self.touch();
        let Some(idx) = self.resolve(parent) else {
            return;
        };
        let children: Vec<NodeIdx> = self.nodes[idx].children.clone();
        let style_affected = children
            .iter()
            .copied()
            .any(|child| self.subtree_contains_style(child));
        for c in children {
            self.detach(c);
        }
        if style_affected {
            self.rebuild_author_stylesheet();
        }
    }

    /// `node.nodeValue`: o texto cru de um nó Text/Comment; `None` para
    /// Element/Document (que têm `nodeValue` null no DOM). Distinto de
    /// `textContent` (que concatena descendentes).
    pub fn node_value(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        match &self.nodes[idx].kind {
            NodeKind::Text(t) | NodeKind::Comment(t) => Some(t.clone()),
            _ => None,
        }
    }

    /// `node.nodeValue = v`: substitui o texto de um nó Text/Comment (no-op em
    /// Element/Document).
    pub fn set_node_value(&mut self, id: NodeId, value: &str) {
        self.touch();
        let Some(idx) = self.resolve(id) else { return };
        let style_affected = self
            .nodes[idx]
            .parent
            .map(|parent| self.subtree_contains_style(parent))
            .unwrap_or(false);
        match &mut self.nodes[idx].kind {
            NodeKind::Text(t) | NodeKind::Comment(t) => *t = value.to_string(),
            _ => {}
        }
        if style_affected {
            self.rebuild_author_stylesheet();
        }
    }

    /// `document.createComment(text)`: cria um nó de comentário solto.
    pub fn create_comment(&mut self, text: &str) -> NodeId {
        let idx = self.push_detached(NodeKind::Comment(text.to_string()));
        self.make_id(idx)
    }

    /// `node.normalize()`: funde nós de Texto ADJACENTES num só e remove os de
    /// texto vazio, recursivamente. Mantém a semântica do DOM (não toca elementos).
    pub fn normalize(&mut self, id: NodeId) {
        self.touch();
        let Some(idx) = self.resolve(id) else { return };
        // 1) recursão nos filhos-elemento primeiro.
        let children: Vec<NodeIdx> = self.nodes[idx].children.clone();
        for c in &children {
            if matches!(self.nodes[*c].kind, NodeKind::Element { .. }) {
                let cid = self.make_id(*c);
                self.normalize(cid);
            }
        }
        // 2) funde Text adjacentes + remove vazios nos filhos diretos.
        let mut new_children: Vec<NodeIdx> = Vec::new();
        for c in self.nodes[idx].children.clone() {
            if let NodeKind::Text(t) = &self.nodes[c].kind {
                if t.is_empty() {
                    continue; // remove texto vazio
                }
                // funde com o anterior se também for Text.
                if let Some(&prev) = new_children.last() {
                    if let NodeKind::Text(pt) = &self.nodes[prev].kind {
                        let merged = format!("{pt}{t}");
                        if let NodeKind::Text(pt_mut) = &mut self.nodes[prev].kind {
                            *pt_mut = merged;
                        }
                        continue; // não acrescenta o nó atual (foi fundido)
                    }
                }
            }
            new_children.push(c);
        }
        self.nodes[idx].children = new_children;
    }

    // ── Atributos extra — #1761 ──────────────────────────────────────────────────

    /// `element.removeAttribute(name)`: remove o atributo (no-op se ausente).
    /// Limpa os índices id/class para o nó (a busca revalida, mas evita stale).
    pub fn remove_attr(&mut self, id: NodeId, name: &str) {
        crate::bump!(remove_attr);
        let Some(idx) = self.resolve(id) else { return };
        let name_lc = name.to_ascii_lowercase();
        let Some(old_value) = self.nodes[idx]
            .attrs
            .iter()
            .find(|a| a.name == name_lc)
            .map(|a| a.value.clone())
        else {
            return;
        };
        // Por NOME (lote I) — o booleano grosso invalidava por QUALQUER atributo.
        let affects_parent_selectors = matches!(name_lc.as_str(), "id" | "class")
            || self.stylesheet.mentions_attribute_name(&name_lc);
        let dirty_root = if affects_parent_selectors {
            self.nodes[idx].parent.unwrap_or(idx)
        } else {
            idx
        };
        self.touch_subtree(dirty_root);
        self.nodes[idx].attrs.retain(|a| a.name != name_lc);
        // Limpa somente os buckets que o atributo removido ocupava.
        match name_lc.as_str() {
            "id" => self.remove_index_key(&old_value, idx, true),
            "class" => {
                for class in old_value.split_whitespace() {
                    self.remove_index_key(class, idx, false);
                }
            }
            _ => {}
        }
    }

    /// `element.hasAttribute(name)`: o atributo ESTÁ PRESENTE (mesmo com valor
    /// vazio — `hidden`/`disabled` são booleanos com valor `""`)? Checa a presença
    /// na lista, não o valor (o `getAttribute("").length>0` da fachada errava aqui).
    pub fn has_attr(&self, id: NodeId, name: &str) -> bool {
        let Some(idx) = self.resolve(id) else {
            return false;
        };
        let name_lc = name.to_ascii_lowercase();
        self.nodes[idx].attrs.iter().any(|a| a.name == name_lc)
    }

    /// `element.getAttributeNames()`: os nomes dos atributos, em ordem do HTML.
    pub fn attr_names(&self, id: NodeId) -> Vec<String> {
        let Some(idx) = self.resolve(id) else {
            return Vec::new();
        };
        self.nodes[idx]
            .attrs
            .iter()
            .map(|a| a.name.clone())
            .collect()
    }

    /// Valor do atributo N-ésimo (para `attributes`), por índice. `None` fora do range.
    pub fn attr_value_at(&self, id: NodeId, i: usize) -> Option<String> {
        let idx = self.resolve(id)?;
        self.nodes[idx].attrs.get(i).map(|a| a.value.clone())
    }

    /// Define/atualiza um atributo (`element.setAttribute`). Cria se não existir.
    /// Reindexa `id`/`class` para que mudanças de valor não deixem candidatos stale.
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        crate::bump!(set_attr);
        let Some(idx) = self.resolve(id) else { return };
        let name_lc = name.to_ascii_lowercase();
        if name_lc == "style" {
            self.note_inline_position(value);
        }
        if self.nodes[idx]
            .attrs
            .iter()
            .find(|a| a.name == name_lc)
            .is_some_and(|a| a.value == value)
        {
            return;
        }
        let affects_index = matches!(name_lc.as_str(), "id" | "class");
        // DESCARTE PRECOCE (o que um browser chama de invalidation set): trocar
        // uma classe que NENHUMA regra cita não muda o estilo de nó nenhum, e
        // invalidar por ela é refazer a cascade e o layout da página inteira
        // por nada. É o caso mais comum de app — `el.classList.toggle('x')` —
        // e o Chrome o resolve em 5 µs onde nós gastávamos 2,9 ms numa página
        // de 3000 elementos.
        //
        // A guarda: só vale para `class`, e cai fora se houver `[class*=…]` — por
        // NOME (lote I), não pelo booleano grosso que a UA `[hidden]` sempre liga.
        let style_unaffected = name_lc == "class"
            && !self.stylesheet.mentions_attribute_name("class")
            && self.class_change_is_inert(idx, value);
        if !style_unaffected {
            let affects_parent_selectors =
                affects_index || self.stylesheet.mentions_attribute_name(&name_lc);
            let dirty_root = if affects_parent_selectors {
                self.nodes[idx].parent.unwrap_or(idx)
            } else {
                idx
            };
            self.touch_subtree(dirty_root);
        }
        if affects_index {
            self.deindex_node(idx);
        }
        let node = &mut self.nodes[idx];
        if let Some(a) = node.attrs.iter_mut().find(|a| a.name == name_lc) {
            a.value = value.to_string();
        } else {
            node.attrs.push(Attr {
                name: name_lc,
                value: value.to_string(),
            });
        }
        if affects_index {
            self.index_node(idx);
        }
    }

    /// Cria um elemento SOLTO (sem pai) e devolve seu `NodeId` versionado; ligue-o
    /// com `append_child` (`document.createElement`).
    pub fn create_element(&mut self, tag: &str) -> NodeId {
        let idx = self.push_detached(NodeKind::Element {
            tag: tag.to_ascii_lowercase(),
        });
        self.make_id(idx)
    }

    /// Cria um nó de TEXTO solto com o conteúdo dado (`document.createTextNode`).
    /// Ligue com `append_child`/`insert_before`.
    pub fn create_text_node(&mut self, text: &str) -> NodeId {
        let idx = self.push_detached(NodeKind::Text(text.to_string()));
        self.make_id(idx)
    }

    /// Insere `child` ANTES de `reference` na lista de filhos de `parent`
    /// (`parent.insertBefore(child, reference)`). Se `reference` é `None` ou não é
    /// filho de `parent`, anexa ao fim (semântica do DOM). Move `child` do pai
    /// antigo; ignora ids inválidos/ciclos.
    pub fn insert_before(&mut self, parent: NodeId, child: NodeId, reference: Option<NodeId>) {
        let (Some(parent), Some(child)) = (self.resolve(parent), self.resolve(child)) else {
            return;
        };
        if parent == child || self.is_ancestor(child, parent) {
            return;
        }
        let style_affected = self.subtree_contains_style(child);
        // ref==child é no-op (inserir antes de si mesmo mantém a posição). A spec do
        // DOM trata referenceNode==node como manter no lugar.
        let ref_idx = reference.and_then(|r| self.resolve(r));
        if ref_idx == Some(child) {
            // já garante o parent (caso o nó fosse solto) e mantém a ordem.
            if self.nodes[child].parent != Some(parent) {
                let old_parent = self.nodes[child].parent;
                self.detach(child);
                self.nodes[child].parent = Some(parent);
                self.nodes[parent].children.push(child);
                self.touch_structural(child, old_parent);
                if style_affected {
                    self.rebuild_author_stylesheet();
                }
            }
            return;
        }
        // captura a posição da referência ANTES do detach (o detach pode mexer na
        // lista de filhos do pai se o child já era irmão da referência).
        let old_parent = self.nodes[child].parent;
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        let pos = ref_idx
            .and_then(|r| self.nodes[parent].children.iter().position(|&c| c == r))
            .unwrap_or(self.nodes[parent].children.len());
        self.nodes[parent].children.insert(pos, child);
        self.touch_structural(child, old_parent);
        if style_affected {
            self.rebuild_author_stylesheet();
        }
    }

    /// `node.nodeType` — código numérico do DOM: Element=1, Text=3, Comment=8,
    /// Document=9. `-1` se o id não resolve.
    pub fn node_type(&self, id: NodeId) -> i64 {
        let Some(idx) = self.resolve(id) else {
            return -1;
        };
        match &self.nodes[idx].kind {
            NodeKind::Element { .. } => 1,
            NodeKind::Text(_) => 3,
            NodeKind::Comment(_) => 8,
            NodeKind::Document => 9,
        }
    }

    /// `node.nodeName` — nome do DOM: a TAG (maiúscula no browser; aqui devolvemos
    /// como está, minúscula) para Element; `#text`/`#comment`/`#document` para os
    /// demais. `None` se o id não resolve.
    pub fn node_name(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        Some(match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.clone(),
            NodeKind::Text(_) => "#text".to_string(),
            NodeKind::Comment(_) => "#comment".to_string(),
            NodeKind::Document => "#document".to_string(),
        })
    }

    /// Move `child` para o fim dos filhos de `parent` (`parent.appendChild`).
    /// Remove `child` do pai antigo, se tiver. Ignora ids inválidos ou ciclos.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        let (Some(parent), Some(child)) = (self.resolve(parent), self.resolve(child)) else {
            return;
        };
        if parent == child {
            return;
        }
        if self.is_ancestor(child, parent) {
            return; // evita criar ciclo (child seria ancestral de parent)
        }
        // O pai ANTIGO também muda (perdeu um filho) — capturado antes do detach.
        let old_parent = self.nodes[child].parent;
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
        self.touch_structural(child, old_parent);
        if self.subtree_contains_style(child) {
            self.rebuild_author_stylesheet();
        }
    }

    /// Desliga um nó do pai (`element.remove`). O nó continua na arena (lixo).
    pub fn remove_node(&mut self, id: NodeId) {
        let Some(idx) = self.resolve(id) else { return };
        if idx == self.root {
            return;
        }
        // A subárvore que sai já não é alcançável depois do detach, então o
        // ANTIGO PAI é quem carrega a invalidação (o `touch_subtrees` desce por
        // ele e sobe pelos ancestrais).
        // ANTES do detach: a raiz da invalidação é o nó que SAI (a subárvore
        // dele), e os ancestrais precisam estar alcançáveis para os epochs
        // subirem — depois do detach o nó já não tem pai.
        let parent = self.nodes[idx].parent;
        let style_affected = self.subtree_contains_style(idx);
        self.touch_structural(idx, parent);
        self.detach(idx);
        if style_affected {
            self.rebuild_author_stylesheet();
        }
    }


    /// `element.innerHTML = html` (SET) — parseia o HTML e SUBSTITUI todos os filhos
    /// do nó pela nova subárvore. Reusa o parser (`parse_html_to_dom`); os nós
    /// parseados são COPIADOS para esta arena (re-parentados sob `id`), atualizando
    /// os índices id/class. Não faz nada num nó que não é elemento ou não resolve.
    pub fn set_inner_html(&mut self, id: NodeId, html: &str) {
        crate::bump!(inner_html_sets);
        let _phase = crate::metrics::phases::scope("set-inner-html");
        self.touch();
        let Some(idx) = self.resolve(id) else { return };
        if !matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            return;
        }
        // Descarta os filhos atuais (arena não compacta; viram lixo). Zera o
        // `parent` de cada um — sem isso `is_attached` ainda os vê ligados à raiz
        // e `querySelector` acha nó destacado.
        let old_children = std::mem::take(&mut self.nodes[idx].children);
        for c in old_children {
            self.nodes[c].parent = None;
        }
        // Parseia a nova subárvore numa árvore temporária e copia os filhos do
        // #document dela para baixo de `idx`.
        let sub = parse_fragmento(html);
        let sub_root_children: Vec<NodeIdx> = sub.nodes[sub.root].children.clone();
        for sub_child in sub_root_children {
            self.copy_subtree_into(&sub, sub_child, idx);
        }
        self.rebuild_author_stylesheet();
    }

    /// Copia recursivamente o nó `src_idx` da árvore `src` para dentro desta arena,
    /// como filho de `dst_parent`. Novos `NodeIdx`, índices id/class atualizados.
    fn copy_subtree_into(&mut self, src: &Dom, src_idx: NodeIdx, dst_parent: NodeIdx) -> NodeIdx {
        let src_node = &src.nodes[src_idx];
        let new_idx = self.push(src_node.kind.clone(), src_node.attrs.clone(), dst_parent);
        let src_children: Vec<NodeIdx> = src_node.children.clone();
        for c in src_children {
            self.copy_subtree_into(src, c, new_idx);
        }
        new_idx
    }
}
