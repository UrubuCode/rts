//! As CONSULTAS: `query`/`queryAll`, por subárvore, e o atalho pelos índices
//! `#id`/`.classe` que evita varrer a árvore.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    // ── Query (base do querySelector) ───────────────────────────────────────

    /// Primeiro nó que casa com um seletor SIMPLES: `tag` (`"h1"`), `#id`
    /// (`"#alvo"`) ou `.classe` (`".card"`). `None` se nada casar. É o
    /// `querySelector` de um seletor só.
    ///
    /// `#id`/`.classe` usam os índices como filtro de candidatos; a resposta final
    /// varre em pré-ordem para preservar a ordem documental. Valida que o candidato
    /// ainda está vivo (anexado à raiz), já que mutações podem desligar nós.
    pub fn query(&self, selector: &str) -> Option<NodeId> {
        let sel = selector.trim();
        let idx = self.query_idx(sel)?;
        Some(self.make_id(idx))
    }

    /// Núcleo do `query` em índices crus (interno). O `query` público embrulha o
    /// resultado no `NodeId` versionado.
    fn query_idx(&self, sel: &str) -> Option<NodeIdx> {
        crate::bump!(query_calls);
        let selectors = crate::style::parse_selector_list(sel);
        if selectors.is_empty() {
            return None;
        }
        // Índices servem como filtro rápido, mas a resposta final sempre vem de uma
        // busca em pré-ordem. Isso é necessário porque IDs/classes duplicados e
        // reordenação por appendChild devem seguir a ordem documental do DOM, não a
        // ordem de alocação da arena.
        if let Some(key) = sel.strip_prefix('#') {
            if is_plain_ident(key) {
                let has_candidate = self
                    .id_index
                    .get(key)
                    .map(|v| {
                        v.iter()
                            .any(|&i| self.is_attached(i) && self.nodes[i].attr("id") == Some(key))
                    })
                    .unwrap_or(false);
                return has_candidate
                    .then(|| self.find_idx_pre_order_parsed(self.root, &selectors))
                    .flatten();
            }
        }
        if let Some(cls) = sel.strip_prefix('.') {
            if is_plain_ident(cls) {
                let has_candidate = self
                    .class_index
                    .get(cls)
                    .map(|v| {
                        v.iter().any(|&i| {
                            self.is_attached(i)
                                && self.nodes[i]
                                    .attr("class")
                                    .map(|c| c.split_whitespace().any(|x| x == cls))
                                    .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                return has_candidate
                    .then(|| self.find_idx_pre_order_parsed(self.root, &selectors))
                    .flatten();
            }
        }
        // Caso geral (composto/combinador/atributo/pseudo): pré-ordem + matches.
        self.find_idx_pre_order_parsed(self.root, &selectors)
    }

    /// Pré-ordem buscando o 1º elemento que casa uma lista já parseada de seletores.
    fn find_idx_pre_order_parsed(
        &self,
        idx: NodeIdx,
        selectors: &[crate::style::ComplexSelector],
    ) -> Option<NodeIdx> {
        if idx != self.root && selectors.iter().any(|sel| self.matches_complex(idx, sel)) {
            return Some(idx);
        }
        for &child in &self.nodes[idx].children {
            if let Some(found) = self.find_idx_pre_order_parsed(child, selectors) {
                return Some(found);
            }
        }
        None
    }

    /// `true` se `idx` está conectado à raiz (não foi desligado por uma mutação).
    /// Os índices não são limpos no `remove`/`append`, então uma busca por
    /// índice valida a alcançabilidade aqui (barato: sobe pelos pais).
    fn is_attached(&self, idx: NodeIdx) -> bool {
        let mut cur = Some(idx);
        while let Some(c) = cur {
            if c == self.root {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }


    // ── Query por subárvore + getElementsBy* — #1758 ─────────────────────────────

    /// `element.querySelector(sel)`: o 1º descendente do nó que casa o seletor
    /// (busca SÓ na subárvore, não na árvore toda). `None` se nenhum casa.
    pub fn query_within(&self, root: NodeId, selector: &str) -> Option<NodeId> {
        self.query_all_within(root, selector).into_iter().next()
    }

    /// `element.querySelectorAll(sel)` restrito à subárvore do nó (exclui o próprio).
    pub fn query_all_within(&self, root: NodeId, selector: &str) -> Vec<NodeId> {
        let selectors = crate::style::parse_selector_list(selector.trim());
        if selectors.is_empty() {
            return Vec::new();
        }
        let Some(root_idx) = self.resolve(root) else {
            return Vec::new();
        };
        let keys: Vec<TargetKey> = selectors.iter().map(TargetKey::of).collect();
        let mut out = Vec::new();
        // só os DESCENDENTES (o próprio nó não casa a si mesmo no querySelector).
        for &child in &self.nodes[root_idx].children {
            self.query_all_into(child, &selectors, &keys, &mut out);
        }
        out
    }


    /// Todos os nós que casam um seletor (`querySelectorAll`), em ordem de documento.
    /// O seletor é parseado uma vez por chamada e a travessia preserva a ordem mesmo
    /// após mutações que reordenam a árvore.
    pub fn query_all(&self, selector: &str) -> Vec<NodeId> {
        let selectors = crate::style::parse_selector_list(selector.trim());
        if selectors.is_empty() {
            return Vec::new();
        }
        crate::bump!(query_calls);
        // CHAVE-ALVO por seletor (a tag/classe/id do último compound, que é o
        // que o seletor casa). O teste completo NAVEGA a árvore para os
        // combinadores; comparar a chave primeiro descarta a maioria dos nós com
        // uma comparação de string. É a mesma ideia do `RuleIndex` da cascade,
        // que a consulta não usava: 5007 nós × 14 seletores eram 70 090 chamadas
        // ao matcher completo numa página de 3000 elementos.
        let keys: Vec<TargetKey> = selectors.iter().map(TargetKey::of).collect();
        // Quando TODA chave é `#id` ou `.classe`, os índices dão os candidatos
        // direto e a árvore inteira não precisa ser andada — é o que o browser
        // faz. A ordem sai da numeração documental, porque os índices guardam
        // ordem de ARENA e `querySelectorAll` promete ordem de documento
        // (`appendChild` reordena a segunda sem mexer na primeira).
        // Um seletor que é SÓ a chave (`.card`, `#topo`) já está inteiramente
        // respondido pelo índice: o candidato tem a classe/id por construção, e
        // chamar o matcher para reconfirmar é refazer a pergunta que o bucket
        // respondeu. Com combinador, pseudo ou compound, o matcher decide.
        let exatos = selectors.iter().all(|sel| {
            sel.compounds.len() == 1
                && sel.compounds[0].parts.len() == 1
                && matches!(
                    sel.compounds[0].parts[0],
                    crate::style::SimpleSelector::Class(_) | crate::style::SimpleSelector::Id(_)
                )
        });
        if let Some(candidatos) = self.candidatos_por_indice(&keys) {
            crate::bump!(query_index_hits);
            let positions = self.document_positions();
            // A numeração documental já responde "está na árvore?": um nó
            // inalcançável nunca foi visitado e ficou com `u32::MAX`. É O(1),
            // contra o `is_attached`, que sobe até a raiz por candidato — e são
            // mil candidatos numa consulta de página grande.
            let mut casaram: Vec<NodeIdx> = candidatos
                .into_iter()
                .filter(|&idx| {
                    crate::bump!(query_nodes_visited);
                    positions.1.get(idx).copied().unwrap_or(u32::MAX) != u32::MAX
                        && (exatos || selectors.iter().any(|sel| self.matches_complex(idx, sel)))
                })
                .collect();
            casaram.sort_unstable_by_key(|&idx| positions.1[idx]);
            drop(positions);
            return casaram.into_iter().map(|idx| self.make_id(idx)).collect();
        }
        let mut out = Vec::new();
        self.query_all_into(self.root, &selectors, &keys, &mut out);
        out
    }

    /// Os candidatos de uma lista de seletores a partir dos índices, ou `None`
    /// quando algum seletor não tem chave indexada (tag, universal, atributo) —
    /// nesse caso a varredura em pré-ordem é a única resposta completa.
    fn candidatos_por_indice(&self, keys: &[TargetKey]) -> Option<Vec<NodeIdx>> {
        let mut out: Vec<NodeIdx> = Vec::new();
        for key in keys {
            let bucket = match key {
                TargetKey::Id(v) => self.id_index.get(v),
                TargetKey::Class(v) => self.class_index.get(v),
                TargetKey::Tag(_) | TargetKey::Any => return None,
            };
            if let Some(bucket) = bucket {
                out.extend_from_slice(bucket);
            }
        }
        // Um nó pode entrar por dois seletores da lista (`.a, .b` num nó com as
        // duas): o `querySelectorAll` devolve cada nó UMA vez.
        out.sort_unstable();
        out.dedup();
        Some(out)
    }

    fn query_all_into(
        &self,
        idx: NodeIdx,
        selectors: &[crate::style::ComplexSelector],
        keys: &[TargetKey],
        out: &mut Vec<NodeId>,
    ) {
        crate::bump!(query_nodes_visited);
        if idx != self.root {
            if let NodeKind::Element { tag } = &self.nodes[idx].kind {
                let id = self.nodes[idx].attr("id");
                let class_attr = self.nodes[idx].attr("class");
                let hit = selectors.iter().zip(keys).any(|(sel, key)| {
                    key.can_match(tag, id, class_attr) && self.matches_complex(idx, sel)
                });
                if hit {
                    out.push(self.make_id(idx));
                }
            }
        }
        for &child in &self.nodes[idx].children {
            self.query_all_into(child, selectors, keys, out);
        }
    }
}
