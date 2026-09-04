//! Freelist de índices da arena — recicla o slot de um nó DESLIGADO sem
//! wrapper vivo do lado TS, para que inserir/remover N vezes não faça a
//! arena crescer sem limite (PLAN §4.M, finding 2 da auditoria estrutural:
//! "o ciclo de vida do nó não tem via de saída").
//!
//! **Quem decide reciclar é a FACHADA TS, nunca o Rust sozinho** — o Rust não
//! sabe se `__wrappers` ainda tem uma entrada apontando para o nó; só o TS lê
//! esse mapa. `release_subtree` é a via explícita que a fachada chama em
//! `removeChild`/`remove()` quando o nó removido (e cada descendente, um a
//! um) não tem wrapper vivo. Rejeitada: contagem de referências do lado do
//! bridge (`retain`/`release` por wrapper criado/coletado) — exigiria uma
//! chamada extra em CADA leitura que devolve um `NodeId` ao TS (toda travessia:
//! `firstChild`, `querySelector`, `childNodes[i]`...), uma superfície muito
//! maior do que uma chamada a mais no caminho de remoção que já existe.
//!
//! A GERAÇÃO passa a ser POR NÓ, não por árvore: reciclar `idx` incrementa só
//! `node_generation[idx]`; a geração DA ÁRVORE (`Dom::generation`, tomada uma
//! vez por `Dom::new`/re-parse) continua a ser o valor inicial de todo nó
//! recém-alocado, e é o que ainda distingue um `NodeId` de uma árvore anterior
//! de um `idx` reciclado NA MESMA árvore — os dois casos que `resolve` tem de
//! separar. `to_abi`/`from_abi` já empacotam `(generation << 32) | idx` sem
//! assumir que a geração é uniforme entre nós, então nada nessa fronteira
//! muda: um `NodeId` velho de um `idx` reciclado carrega a geração de ANTES e
//! resolve a `None`, tal como um `NodeId` de uma árvore anterior.

use super::*;

impl Dom {
    /// Aloca um índice de nó: reusa um slot da freelist se houver, senão
    /// cresce a arena. O conteúdo devolvido é um placeholder morto — quem
    /// chama substitui `self.nodes[idx]` imediatamente.
    pub(in crate::dom) fn alloc_slot(&mut self) -> NodeIdx {
        if let Some(idx) = self.free_list.pop() {
            crate::bump!(nodes_recycled_reused);
            idx
        } else {
            let idx = self.nodes.len();
            self.nodes.push(Node {
                kind: NodeKind::Comment(String::new()),
                attrs: Vec::new(),
                parent: None,
                children: Vec::new(),
            });
            self.node_generation.push(self.generation);
            self.layout_epochs.push(0);
            idx
        }
    }

    /// Recicla `idx` para reuso: geração local incrementada (qualquer
    /// `NodeId` velho desse `idx` passa a resolver a `None`), estado
    /// por-nó de todos os mapas laterais purgado, e o índice devolvido à
    /// freelist. Só sobre um nó já DESANEXADO — reciclar um nó ainda
    /// alcançável corromperia a árvore.
    fn recycle(&mut self, idx: NodeIdx) {
        debug_assert!(
            self.nodes[idx].parent.is_none(),
            "recycle de nó ainda anexado"
        );
        self.deindex_node(idx);
        self.purge_node_state(idx);
        self.nodes[idx] = Node {
            kind: NodeKind::Comment(String::new()),
            attrs: Vec::new(),
            parent: None,
            children: Vec::new(),
        };
        // `wrapping_add` + `max(1)`: 0 nunca é uma geração válida (reservado
        // como "nunca alocado"), então um overflow teórico não faz um `NodeId`
        // velho voltar a resolver por acidente.
        self.node_generation[idx] = self.node_generation[idx].wrapping_add(1).max(1);
        self.layout_epochs[idx] = self.layout_epochs[idx].wrapping_add(1);
        self.free_list.push(idx);
        crate::bump!(nodes_recycled);
    }

    /// Remove toda entrada de `idx` dos mapas laterais indexados por `NodeIdx`
    /// — sem isto, o PRÓXIMO nó a ocupar este slot herdaria o estilo, os
    /// listeners, o valor de input ou a transição do nó ANTERIOR: o mesmo
    /// bug de identidade que a `generation` existe para impedir, só que por
    /// um caminho que a `generation` sozinha não fecha (estes mapas não são
    /// consultados através de um `NodeId`, são indexados pelo `idx` cru).
    fn purge_node_state(&mut self, idx: NodeIdx) {
        self.style_overrides.remove(&idx);
        self.listeners.remove(&idx);
        self.listener_cbs.retain(|(node, _), _| *node != idx);
        self.input_values.remove(&idx);
        self.image_pixels.remove(&idx);
        self.own_pixels.remove(&idx);
        self.scroll_regioes.borrow_mut().remove(&idx);
        self.active_transitions.remove(&idx);
        self.prev_computed.remove(&idx);
        self.anim_override.remove(&idx);
        self.anim_start.remove(&idx);
        self.dirty_self.borrow_mut().remove(&idx);
        self.dirty_children.borrow_mut().remove(&idx);
        self.last_fragment.borrow_mut().remove(&idx);
        if self.hovered.get() == Some(idx) {
            self.hovered.set(None);
        }
        if self.focused_input == Some(idx) {
            self.focused_input = None;
        }
    }

    /// `releaseSubtree(node)` — chamado pela fachada quando `node` (já
    /// desanexado pelo `remove()`/`removeChild` que a precedeu) e a sua
    /// subárvore não têm wrapper vivo do lado TS. Recicla cada nó da
    /// subárvore recursivamente. Sem efeito se `id` não resolve ou o nó
    /// AINDA está anexado (ou é a raiz) — evita reciclar por engano algo que
    /// voltou a entrar na árvore entre o `remove()` e esta chamada, ou a
    /// raiz `#document`, que não tem via de saída.
    pub fn release_subtree(&mut self, id: NodeId) {
        let Some(idx) = self.resolve(id) else {
            return;
        };
        if idx == self.root || self.nodes[idx].parent.is_some() {
            return;
        }
        self.release_subtree_idx(idx);
    }

    fn release_subtree_idx(&mut self, idx: NodeIdx) {
        let children = std::mem::take(&mut self.nodes[idx].children);
        for c in children {
            self.release_subtree_idx(c);
        }
        // Descendentes ainda carregam `parent = Some(idx-do-pai-dentro-da-
        // MESMA subárvore)` — só a RAIZ da subárvore chega aqui desanexada
        // (`release_subtree` já checou); um filho nunca foi desligado do seu
        // pai porque a subárvore inteira sai JUNTA. `recycle`'s guarda
        // continua correta para o caso perigoso (um nó preso à ÁRVORE VIVA,
        // pinado por `release_subtree_de_no_ainda_anexado_e_um_no_op`) — o
        // que muda aqui é só que "anexado a um nó desta MESMA chamada, que
        // também está a sair" não é esse caso. Desligar explicitamente aqui
        // (em vez de afrouxar a guarda do `recycle`) mantém `recycle` livre
        // de saber sobre a recursão de quem o chama.
        self.nodes[idx].parent = None;
        self.recycle(idx);
    }
}
