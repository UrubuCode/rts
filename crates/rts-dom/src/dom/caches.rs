//! Os CACHES derivados: memos de estilo, fragmentos, `DisplayList`, medições
//! de bloco e larguras intrínsecas, mais o que os mede para as métricas.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

/// O último desenho de cada caixa, agrupado pelo NÓ que a gera e depois pelo
/// ordinal dela entre as caixas desse nó (`BoxCacheTarget` desdobrado).
///
/// Agrupado por nó e não chaveado pelo par porque quem apaga só sabe o nó: o
/// `recycle` devolve um slot à freelist sem árvore de caixas à mão, e com a
/// chave plana a única saída era varrer o mapa inteiro por nó reciclado —
/// quadrático ao remover uma lista — ou esvaziá-lo, que desligava a costura da
/// página toda. `Vec` e não mapa no segundo nível: quase todo nó tem UMA caixa.
pub(in crate::dom) type UltimosFragmentos = crate::fasthash::FastMap<
    NodeIdx,
    Vec<(u32, FragmentKey, std::rc::Rc<crate::layout::Fragment>)>,
>;

impl Dom {

    /// Quantos nós têm estilo memoizado (as duas camadas somadas). É a contagem
    /// que o `HashMap` dava de graça e um vetor esparso não dá — usada só por
    /// métricas e pelo atalho da construção pura, ambos fora do caminho quente
    /// de um layout.
    pub(in crate::dom) fn memo_entries(&self) -> usize {
        self.computed_memo
            .borrow()
            .iter()
            .filter(|s| s.is_some())
            .count()
            + self
                .base_memo
                .borrow()
                .iter()
                .filter(|s| s.is_some())
                .count()
    }

    /// A posição de `idx` em ordem documental, renumerando se a árvore mudou.
    /// O(n) na primeira consulta depois de uma mutação, O(1) nas seguintes.
    pub(in crate::dom) fn document_positions(&self) -> std::cell::Ref<'_, (u64, Vec<u32>)> {
        if self.doc_order.borrow().0 != self.revision {
            let mut order = vec![u32::MAX; self.nodes.len()];
            let mut next = 0u32;
            let mut stack = vec![self.root];
            while let Some(node) = stack.pop() {
                order[node] = next;
                next += 1;
                // pilha: empilha ao contrário para visitar os filhos na ordem.
                stack.extend(self.nodes[node].children.iter().rev().copied());
            }
            *self.doc_order.borrow_mut() = (self.revision, order);
        }
        self.doc_order.borrow()
    }


    pub(crate) fn last_fragment_of(
        &self,
        target: BoxCacheTarget,
    ) -> Option<(FragmentKey, std::rc::Rc<crate::layout::Fragment>)> {
        let mapa = self.last_fragment.borrow();
        let (_, key, fragment) = mapa
            .get(&target.node)?
            .iter()
            .find(|(ordinal, _, _)| *ordinal == target.ordinal)?;
        Some((*key, std::rc::Rc::clone(fragment)))
    }

    /// Esquece o último desenho de todas as caixas de `node` — o que o
    /// `recycle` precisa ao devolver o slot à freelist.
    pub(in crate::dom) fn forget_last_fragments(&self, node: NodeIdx) {
        self.last_fragment.borrow_mut().remove(&node);
    }

    /// Puts `value` in the computed-style memo of `idx` and answers what was
    /// there. The rotated frame (`layout/block/rotated.rs`) installs the
    /// frame's view of a vertical subtree's styles for the length of its
    /// layout and puts the originals back after, which is how the hundred
    /// physical call sites read logical sides without changing.
    pub(crate) fn swap_computed_memo(
        &self,
        idx: NodeIdx,
        value: Option<std::rc::Rc<crate::style::ComputedStyle>>,
    ) -> Option<std::rc::Rc<crate::style::ComputedStyle>> {
        let mut memo = self.computed_memo.borrow_mut();
        if idx >= memo.len() {
            memo.resize(self.nodes.len().max(idx + 1), None);
        }
        std::mem::replace(&mut memo[idx], value)
    }

    pub(crate) fn fragment_get(
        &self,
        key: FragmentKey,
    ) -> Option<std::rc::Rc<crate::layout::Fragment>> {
        self.fragment_cache.borrow().get(&key).cloned()
    }

    pub(crate) fn fragment_put(
        &self,
        key: FragmentKey,
        fragment: std::rc::Rc<crate::layout::Fragment>,
    ) {
        {
            let mut mapa = self.last_fragment.borrow_mut();
            let caixas = mapa.entry(key.target.node).or_default();
            let novo = (key.target.ordinal, key, std::rc::Rc::clone(&fragment));
            match caixas.iter_mut().find(|(ordinal, _, _)| *ordinal == key.target.ordinal) {
                Some(slot) => *slot = novo,
                None => caixas.push(novo),
            }
        }
        let mut cache = self.fragment_cache.borrow_mut();
        // Teto igual ao dos outros caches de layout: uma página que rola muito
        // acumula fragmentos de caixas que já saíram de cena, e o epoch na chave
        // impede que um stale seja SERVIDO, não que ele ocupe memória.
        if cache.len() >= 4096 && !cache.contains_key(&key) {
            if let Some(old) = cache.keys().next().copied() {
                cache.remove(&old);
            }
        }
        cache.insert(key, fragment);
    }

    /// Esquece todos os fragmentos. Existe para o TESTE de equivalência poder
    /// recalcular do zero e comparar com o que o reuso devolveu: sem isso, o
    /// teste compararia o cache consigo mesmo, que é o mesmo que não testar.
    pub fn clear_fragment_cache(&self) {
        self.fragment_cache.borrow_mut().clear();
        self.last_fragment.borrow_mut().clear();
    }

    pub(crate) fn display_cache_get(
        &self,
        key: DisplayKey,
    ) -> Option<std::rc::Rc<crate::paint::DisplayList>> {
        let cache = self.display_cache.borrow();
        let (k, list, _) = cache.as_ref()?;
        (*k == key).then(|| std::rc::Rc::clone(list))
    }

    pub(crate) fn display_cache_put(
        &self,
        key: DisplayKey,
        list: &std::rc::Rc<crate::paint::DisplayList>,
    ) {
        *self.display_cache.borrow_mut() =
            Some((key, std::rc::Rc::clone(list), std::cell::OnceCell::new()));
    }

    /// The geometry of `layout::layout_cached(self, ctx)`, built ONCE per
    /// cached list (PQ-C4, F4 of the phase-C plan).
    ///
    /// The ruler is the 13.7 ms-per-call class of 2026-09-16: a read loop of
    /// `bounding_component` with no mutation in between must do one layout
    /// and one geometry build, and this is the memo that makes the second
    /// half true. It sits in the same slot as the list, so a new list (any
    /// key change) starts with an empty cell and a stale geometry cannot be
    /// read against it.
    pub fn geometry_cached(
        &self,
        ctx: &crate::layout::LayoutCtx,
    ) -> std::rc::Rc<crate::query::Geometry> {
        let list = crate::layout::layout_cached(self, ctx);
        let cache = self.display_cache.borrow();
        match cache.as_ref() {
            // The slot holds THIS list (`layout_cached` just put or hit it).
            Some((_, cached, geometry)) if std::rc::Rc::ptr_eq(cached, &list) => {
                std::rc::Rc::clone(geometry.get_or_init(|| std::rc::Rc::new(list.geometry_now())))
            }
            _ => std::rc::Rc::new(list.geometry_now()),
        }
    }


    /// Os dois índices de consulta, para a AUDITORIA (`metrics::audit`) poder
    /// confrontá-los com a árvore. Só de leitura, e `pub(crate)`: um índice que
    /// saísse do crate viraria uma segunda fonte de verdade sobre quem tem qual
    /// classe.
    pub(crate) fn debug_indices(
        &self,
    ) -> (
        &HashMap<String, Vec<NodeIdx>>,
        &HashMap<String, Vec<NodeIdx>>,
    ) {
        (&self.id_index, &self.class_index)
    }

    /// Todo estado DERIVADO indexado por nó, como `(nome do mapa, índice)`. A
    /// auditoria só precisa saber que existe uma entrada para um nó — não o que
    /// há nela — e enumerar os mapas AQUI (uma vez, ao lado dos campos) é o que
    /// impede que um mapa novo passe a vazar sem ninguém notar: quem esquecer de
    /// acrescentá-lo aqui não ganha auditoria, mas quem o remover não deixa a
    /// auditoria mentindo sobre um campo que já não existe.
    pub(crate) fn derived_node_state(&self) -> Vec<(&'static str, NodeIdx)> {
        let mut out = Vec::new();
        let mut push = |label: &'static str, it: &mut dyn Iterator<Item = NodeIdx>| {
            for idx in it {
                out.push((label, idx));
            }
        };
        push("style_overrides", &mut self.style_overrides.keys().copied());
        push("listeners", &mut self.listeners.keys().copied());
        push(
            "listener_cbs",
            &mut self.listener_cbs.keys().map(|(idx, _)| *idx),
        );
        push("input_values", &mut self.input_values.keys().copied());
        push("image_pixels", &mut self.image_pixels.keys().copied());
        push(
            "active_transitions",
            &mut self.active_transitions.keys().copied(),
        );
        push("anim_override", &mut self.anim_override.keys().copied());
        push("prev_computed", &mut self.prev_computed.keys().copied());
        push("focused_input", &mut self.focused_input.into_iter());
        push("scroll_regioes", &mut self.scroll_regioes.borrow().keys().copied());
        out
    }

    /// O tamanho da tabela de epochs de layout — deve andar junto com a arena, e
    /// a auditoria compara os dois.
    pub(crate) fn layout_epoch_len(&self) -> usize {
        self.layout_epochs.len()
    }

    /// `(entradas nos memos de estilo, entradas nos caches de layout)` — o
    /// estado DERIVADO que cresce sem a árvore crescer. Enumerado aqui, ao lado
    /// dos campos, pela mesma razão do [`derived_node_state`](Self::derived_node_state).
    pub(crate) fn derived_cache_sizes(&self) -> (usize, usize) {
        (
            self.memo_entries(),
            self.layout_measure_cache.borrow().len() + self.intrinsic_width_cache.borrow().len(),
        )
    }

    /// Bytes estimados dos caches de layout: chave + valor por entrada, vezes a
    /// CAPACIDADE do mapa (um `HashMap` reserva além do que usa, e ignorar isso
    /// subestima justamente a área que enche).
    pub(crate) fn layout_cache_bytes(&self) -> usize {
        let measure = self.layout_measure_cache.borrow();
        let intrinsic = self.intrinsic_width_cache.borrow();
        measure.capacity()
            * (std::mem::size_of::<LayoutMeasureKey>() + std::mem::size_of::<(f32, f32)>())
            + intrinsic.capacity()
                * (std::mem::size_of::<IntrinsicWidthKey>() + std::mem::size_of::<f32>())
    }

    /// Bytes estimados do stylesheet de autor mais o CSS bruto guardado para os
    /// pseudo-elementos de scrollbar. Numa página com o Bootstrap inteiro num
    /// `<style>`, esta é a maior área do `Dom` — e ela não some quando a árvore
    /// muda, o que é o motivo de estar separada da árvore no relatório.
    pub(crate) fn stylesheet_bytes(&self) -> usize {
        self.raw_css.capacity() + self.stylesheet.estimated_bytes()
    }

    // The two measure caches are keyed by PHYSICAL constraints, and inside a
    // rotated frame (`layout/block/rotated.rs`) the same node answers with
    // the frame's styles: neither reads nor writes there. A frame is only
    // entered at a writing-mode boundary, so this is one thread-local read
    // where none exists.
    pub(crate) fn layout_measure_get(&self, key: LayoutMeasureKey) -> Option<(f32, f32)> {
        if crate::layout::in_rotated_frame() {
            return None;
        }
        self.layout_measure_cache.borrow().get(&key).copied()
    }

    pub(crate) fn layout_measure_put(&self, key: LayoutMeasureKey, value: (f32, f32)) {
        if crate::layout::in_rotated_frame() {
            return;
        }
        let mut cache = self.layout_measure_cache.borrow_mut();
        if cache.len() >= 4096 && !cache.contains_key(&key) {
            crate::bump!(measure_cache_evictions);
            if let Some(old_key) = cache.keys().next().copied() {
                cache.remove(&old_key);
            }
        }
        cache.insert(key, value);
    }

    pub(crate) fn intrinsic_width_get(&self, key: IntrinsicWidthKey) -> Option<f32> {
        if crate::layout::in_rotated_frame() {
            return None;
        }
        self.intrinsic_width_cache.borrow().get(&key).copied()
    }

    pub(crate) fn intrinsic_width_put(&self, key: IntrinsicWidthKey, value: f32) {
        if crate::layout::in_rotated_frame() {
            return;
        }
        let mut cache = self.intrinsic_width_cache.borrow_mut();
        if cache.len() >= 4096 && !cache.contains_key(&key) {
            crate::bump!(intrinsic_cache_evictions);
            if let Some(old_key) = cache.keys().next().copied() {
                cache.remove(&old_key);
            }
        }
        cache.insert(key, value);
    }

    /// A revisão de RENDER desta árvore: muda sempre que árvore/estilo/animação
    /// mudam — inclui o epoch GLOBAL de estilo por-tag (`defineStyle`/`defineBlock`,
    /// que vivem fora do `Dom`). É a chave de cache de layout do backend e da ABI:
    /// mesma revisão + mesmo viewport ⇒ a DisplayList anterior ainda vale.
    /// O epoch de ANIMAÇÃO — entra nas chaves de cache de layout, porque um
    /// frame de interpolação muda o estilo sem mudar a estrutura.
    pub(crate) fn anim_epoch(&self) -> u64 {
        self.anim_epoch
    }

    pub fn render_revision(&self) -> u64 {
        self.revision
            .wrapping_add(self.anim_epoch)
            .wrapping_add(crate::style::props::style_epoch())
    }

    /// Identidade da instância da árvore para caches do backend. Combina a geração
    /// com o endereço da instância: clones independentes e novas árvores nunca
    /// compartilham uma `DisplayList` por acidente.
    pub fn cache_identity(&self) -> u64 {
        let address = self as *const Dom as usize as u64;
        address.rotate_left(17) ^ self.generation as u64
    }
}
