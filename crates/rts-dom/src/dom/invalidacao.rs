//! INVALIDAÇÃO: os `touch_*`, os dirty bits por subárvore e o epoch de
//! geometria — quem decide o que deixa de valer quando algo muda.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// Marca uma mudança global de estilo/estrutura: invalida os memos de todos os
    /// nós. É o fallback seguro para mudanças de stylesheet, estrutura ou regras que
    /// podem atravessar a árvore.
    pub(in crate::dom) fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        crate::bump!(touch_global);
        crate::bump!(memo_cleared_entries, self.memo_entries());
        self.computed_memo.borrow_mut().clear();
        self.base_memo.borrow_mut().clear();
        // Os FRAGMENTOS são chaveados por epoch de nó, e o `touch()` global é
        // exatamente o caso em que não se sabe QUAIS nós mudaram (stylesheet
        // novo, mutação sem alvo). Esvaziar é o fallback seguro; as invalidações
        // com alvo bumpam epoch e preservam o resto do cache.
        self.fragment_cache.borrow_mut().clear();
        self.last_fragment.borrow_mut().clear();
        // Sem alvo não há "quais filhos": marcar todos seria o mesmo que não
        // marcar nenhum.
        self.dirty_children.borrow_mut().clear();
        self.dirty_self.borrow_mut().clear();
        self.layout_measure_cache.borrow_mut().clear();
        self.intrinsic_width_cache.borrow_mut().clear();
    }

    /// Marca uma mudança que altera pixels/geometria, mas não o estilo computado.
    /// O cache de cascade pode continuar válido para o próximo layout.
    /// Mudança que altera PIXELS/geometria mas não o estilo computado — trocar
    /// o texto de um elemento é o caso.
    ///
    /// `node` é obrigatório porque o reuso de fragmentos exige saber ONDE a
    /// geometria mudou: sem bumpar o epoch da subárvore (e dos ancestrais, que
    /// podem mudar de tamanho), um fragmento com o texto ANTIGO continuaria
    /// casando a chave. Foi assim que o teste de equivalência pegou este caminho
    /// na primeira mutação.
    pub(in crate::dom) fn touch_render_only(&mut self, node: NodeIdx) {
        self.revision = self.revision.wrapping_add(1);
        crate::bump!(touch_render_only);
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            self.layout_epochs[n] = self.layout_epochs[n].wrapping_add(1);
            stack.extend(self.nodes[n].children.iter().copied());
        }
        self.mark_self_dirty(node);
        let mut filho = node;
        let mut ancestor = self.nodes[node].parent;
        while let Some(n) = ancestor {
            self.layout_epochs[n] = self.layout_epochs[n].wrapping_add(1);
            self.mark_dirty_child(n, filho);
            filho = n;
            ancestor = self.nodes[n].parent;
        }
        self.layout_measure_cache.borrow_mut().clear();
        self.intrinsic_width_cache.borrow_mut().clear();
    }

    /// Invalida estilo apenas no nó e em seus descendentes. É seguro para `style=""`
    /// e overrides locais porque a mudança pode afetar propriedades herdadas.
    pub(in crate::dom) fn touch_subtree(&mut self, idx: NodeIdx) {
        self.revision = self.revision.wrapping_add(1);
        crate::bump!(touch_subtree_calls);
        let mut affected = HashSet::new();
        let mut stack = vec![idx];
        while let Some(node) = stack.pop() {
            if affected.insert(node) {
                stack.extend(self.nodes[node].children.iter().copied());
            }
        }
        crate::bump!(touch_subtree_nodes, affected.len());
        let mut computed = self.computed_memo.borrow_mut();
        let mut base = self.base_memo.borrow_mut();
        for &node in &affected {
            memo_forget(&mut computed, node);
            memo_forget(&mut base, node);
            self.layout_epochs[node] = self.layout_epochs[node].wrapping_add(1);
        }
        // Sobe anotando por onde passou: cada ancestral fica sabendo qual filho
        // dele tem sujeira abaixo.
        self.mark_self_dirty(idx);
        let mut filho = idx;
        let mut ancestor = self.nodes[idx].parent;
        while let Some(node) = ancestor {
            self.layout_epochs[node] = self.layout_epochs[node].wrapping_add(1);
            self.mark_dirty_child(node, filho);
            filho = node;
            ancestor = self.nodes[node].parent;
        }
    }

    /// Invalida várias subárvores em uma única revisão. É o equivalente local ao
    /// batching de invalidation sets dos browsers: sobreposições são deduplicadas e
    /// os caches só sofrem uma operação de escrita por lote.
    pub(in crate::dom) fn touch_subtrees<I>(&mut self, roots: I)
    where
        I: IntoIterator<Item = NodeIdx>,
    {
        self.revision = self.revision.wrapping_add(1);
        crate::bump!(touch_subtree_calls);
        let roots: Vec<NodeIdx> = roots.into_iter().collect();
        let mut affected = HashSet::new();
        let mut stack = roots.clone();
        while let Some(node) = stack.pop() {
            if affected.insert(node) {
                stack.extend(self.nodes[node].children.iter().copied());
            }
        }
        let mut ancestors = HashSet::new();
        for root in roots {
            self.mark_self_dirty(root);
            let mut filho = root;
            let mut ancestor = self.nodes[root].parent;
            while let Some(node) = ancestor {
                ancestors.insert(node);
                self.mark_dirty_child(node, filho);
                filho = node;
                ancestor = self.nodes[node].parent;
            }
        }
        crate::bump!(touch_subtree_nodes, affected.len());
        let mut computed = self.computed_memo.borrow_mut();
        let mut base = self.base_memo.borrow_mut();
        for node in affected {
            memo_forget(&mut computed, node);
            memo_forget(&mut base, node);
            self.layout_epochs[node] = self.layout_epochs[node].wrapping_add(1);
        }
        for node in ancestors {
            self.layout_epochs[node] = self.layout_epochs[node].wrapping_add(1);
        }
    }

    /// Invalidação de uma mudança de ESTRUTURA (inserir, mover, remover).
    ///
    /// Um `appendChild` chamava o `touch()` global: o memo de estilo de TODOS os
    /// nós ia fora, e o próximo layout re-cascadeava a página. Montar uma lista
    /// de 4000 itens lendo o layout de vez em quando custava 82 120 cascades
    /// completas — quadrático, medido.
    ///
    /// Sem seletor sensível a POSIÇÃO no stylesheet, inserir um nó não muda o
    /// estilo de nenhum outro: basta invalidar a subárvore que entrou/saiu e os
    /// ancestrais (que podem mudar de tamanho), que é o que `touch_subtrees`
    /// faz. Com `:nth-child`/`:first-child`/`:empty`/`+`/`~`, os irmãos mudam
    /// de verdade e o global é a resposta certa — a guarda está em
    /// [`Stylesheet::position_sensitive`](crate::style::Stylesheet::position_sensitive).
    /// `moved` é o nó que entrou/saiu (a subárvore dele é o que muda de estilo);
    /// `former_parent` é o pai anterior, quando houve um, para os epochs de
    /// layout dele subirem também.
    ///
    /// Escrito sem `HashSet`, ao contrário do `touch_subtrees`: com UMA raiz não
    /// há sobreposição a deduplicar numa árvore acíclica, e a alocação por
    /// chamada aparece — um `append` × 4000 são 4000 chamadas destas. A primeira
    /// versão reusava `touch_subtrees(pai)` e ficou 118× MAIS LENTA na remoção:
    /// varrer a subárvore do PAI por nó removido é quadrático, e é o pai que tem
    /// 2000 filhos, não o nó que saiu.
    pub(in crate::dom) fn touch_structural(&mut self, moved: NodeIdx, former_parent: Option<NodeIdx>) {
        if self.stylesheet.position_sensitive() {
            self.touch();
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        crate::bump!(touch_subtree_calls);
        // CONSTRUÇÃO PURA (montar a árvore antes de ler qualquer estilo): não há
        // memo nem cache preenchido, então não há o que invalidar — nem os
        // epochs, que só existem para invalidar CHAVES já guardadas. Sem este
        // atalho, um `append` × 4000 pagava a varredura e dois `borrow_mut` por
        // nó, e ficava 40% mais lento do que o `touch()` global que substituiu.
        // O(1): um vetor de memo VAZIO é "nunca houve layout", que é o caso da
        // construção pura. Contar os slots preenchidos é O(n) e estava sendo
        // pago POR MUTAÇÃO — foi o que deixou a remoção de 2000 nós 2,8× mais
        // lenta assim que o layout passou a preencher os memos.
        if self.computed_memo.borrow().is_empty()
            && self.base_memo.borrow().is_empty()
            && self.layout_measure_cache.borrow().is_empty()
            && self.intrinsic_width_cache.borrow().is_empty()
        {
            return;
        }
        let mut computed = self.computed_memo.borrow_mut();
        let mut base = self.base_memo.borrow_mut();
        let mut stack = vec![moved];
        let mut visited = 0u64;
        while let Some(node) = stack.pop() {
            visited += 1;
            memo_forget(&mut computed, node);
            memo_forget(&mut base, node);
            self.layout_epochs[node] = self.layout_epochs[node].wrapping_add(1);
            stack.extend(self.nodes[node].children.iter().copied());
        }
        crate::bump!(touch_subtree_nodes, visited);
        // Sem a feature `metrics` o `bump!` some e a contagem fica sem leitor.
        let _ = visited;
        drop(computed);
        drop(base);
        // Ancestrais: só o EPOCH — o estilo deles não mudou, o tamanho pode ter.
        self.mark_self_dirty(moved);
        for start in [self.nodes[moved].parent, former_parent]
            .into_iter()
            .flatten()
        {
            let mut filho = moved;
            let mut cur = Some(start);
            while let Some(node) = cur {
                self.layout_epochs[node] = self.layout_epochs[node].wrapping_add(1);
                self.mark_dirty_child(node, filho);
                filho = node;
                cur = self.nodes[node].parent;
            }
        }
    }


    fn mark_dirty_child(&self, pai: NodeIdx, filho: NodeIdx) {
        let mut map = self.dirty_children.borrow_mut();
        let entry = map.entry(pai).or_default();
        // Lista curta: o caso que interessa é um ou dois filhos sujos. Acima de
        // um punhado, refazer o container sai mais barato do que comparar.
        if entry.len() < 8 && !entry.contains(&filho) {
            entry.push(filho);
        } else if entry.len() >= 8 {
            entry.push(filho); // marca "muitos"; o consumidor olha o tamanho
        }
    }

    fn mark_self_dirty(&self, node: NodeIdx) {
        self.dirty_self.borrow_mut().insert(node);
    }

    pub(crate) fn dirty_children_of(&self, pai: NodeIdx) -> Option<Vec<NodeIdx>> {
        let map = self.dirty_children.borrow();
        let list = map.get(&pai)?;
        (list.len() <= 8).then(|| list.clone())
    }

    pub(crate) fn is_self_dirty(&self, node: NodeIdx) -> bool {
        self.dirty_self.borrow().contains(&node)
    }

    /// Esquece as marcas — por passada de layout, que é quem as consome.
    pub(crate) fn clear_dirty(&self) {
        self.dirty_children.borrow_mut().clear();
        self.dirty_self.borrow_mut().clear();
    }


    /// `true` se esta árvore PODE ter algum elemento fora do fluxo. Falso
    /// negativo é impossível: qualquer `position` inline liga a flag e qualquer
    /// regra com `absolute`/`fixed` (ou uma pendente com `var()`) liga a do
    /// stylesheet.
    pub(crate) fn may_have_out_of_flow(&self) -> bool {
        self.inline_position.get() || self.stylesheet.has_out_of_flow()
    }

    /// Anota que um `style=""` menciona `position` — chamado no parse e em toda
    /// escrita de atributo.
    pub(in crate::dom) fn note_inline_position(&self, value: &str) {
        if value.contains("position") {
            self.inline_position.set(true);
        }
    }

    pub(crate) fn layout_epoch(&self, idx: NodeIdx) -> u64 {
        self.layout_epochs[idx]
    }


    /// Bumpa SÓ o epoch de animação (invalida o layout p/ re-pintar a interpolação),
    /// sem tocar a revisão estrutural — o `advance` chama isto por frame no lugar de
    /// `touch()`, para o `base_memo` sobreviver ao frame.
    pub(in crate::dom) fn touch_anim(&mut self) {
        self.anim_epoch = self.anim_epoch.wrapping_add(1);
        crate::bump!(touch_anim);
        self.layout_measure_cache.borrow_mut().clear();
        self.intrinsic_width_cache.borrow_mut().clear();
    }
}
