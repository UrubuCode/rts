//! A ARENA: criar a árvore, empacotar e resolver `NodeId` versionados,
//! empurrar/desanexar nós e manter os índices `#id`/`.classe`.
//!
//! O `struct Dom` em si fica no `mod.rs` — os campos são privados ao módulo
//! `dom`, e é isso que deixa cada irmão daqui lê-los sem uma anotação.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {
    /// Cria uma árvore vazia contendo só o `#document`. Toma a próxima geração.
    pub(in crate::dom) fn new() -> Dom {
        Dom {
            generation: next_gen(),
            nodes: vec![Node {
                kind: NodeKind::Document,
                attrs: Vec::new(),
                parent: None,
                children: Vec::new(),
            }],
            root: 0,
            id_index: HashMap::new(),
            class_index: HashMap::new(),
            style_overrides: HashMap::new(),
            stylesheet: crate::style::Stylesheet::new(),
            external_css: String::new(),
            raw_css: String::new(),
            listeners: HashMap::new(),
            listener_cbs: HashMap::new(),
            last_dispatch: Vec::new(),
            last_dispatch_capture: Vec::new(),
            last_dispatch_passive: Vec::new(),
            raw_event_queue: std::collections::VecDeque::new(),
            raw_keyboard_event_queue: std::collections::VecDeque::new(),
            last_raw_keyboard_event: None,
            hovered: std::cell::Cell::new(None),
            event_queue: std::collections::VecDeque::new(),
            last_event_type: String::new(),
            last_raw_event_type: String::new(),
            active_transitions: HashMap::new(),
            prev_computed: HashMap::new(),
            anim_override: HashMap::new(),
            anim_start: HashMap::new(),
            revision: 0,
            anim_epoch: 0,
            computed_memo: std::cell::RefCell::new(Vec::new()),
            memo_revision: std::cell::Cell::new(0),
            memo_style_epoch: std::cell::Cell::new(crate::style::props::style_epoch()),
            base_memo: std::cell::RefCell::new(Vec::new()),
            base_memo_revision: std::cell::Cell::new(u64::MAX),
            counter_memo: std::cell::RefCell::new(None),
            counter_memo_revision: std::cell::Cell::new((u64::MAX, u64::MAX)),
            base_memo_viewport: std::cell::Cell::new((0, 0)),
            layout_measure_cache: std::cell::RefCell::new(crate::fasthash::FastMap::default()),
            intrinsic_width_cache: std::cell::RefCell::new(crate::fasthash::FastMap::default()),
            layout_epochs: vec![0],
            viewport: std::cell::Cell::new((1280.0, 800.0)),
            memo_viewport: std::cell::Cell::new((1280.0f32.to_bits(), 800.0f32.to_bits())),
            input_values: HashMap::new(),
            image_pixels: HashMap::new(),
            own_pixels: HashMap::new(),
            focused_input: None,
            inline_position: std::cell::Cell::new(false),
            display_cache: std::cell::RefCell::new(None),
            fragment_cache: std::cell::RefCell::new(crate::fasthash::FastMap::default()),
            dirty_children: std::cell::RefCell::new(crate::fasthash::FastMap::default()),
            dirty_self: std::cell::RefCell::new(std::collections::HashSet::new()),
            last_fragment: std::cell::RefCell::new(crate::fasthash::FastMap::default()),
            doc_order: std::cell::RefCell::new((u64::MAX, Vec::new())),
        }
    }


    /// A geração desta árvore (para o render/ABI compor `NodeId` versionados).
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Empacota um índice cru desta árvore num `NodeId` versionado (com a `generation`
    /// da árvore). É como um índice interno vira handle público.
    pub(in crate::dom) fn make_id(&self, idx: NodeIdx) -> NodeId {
        NodeId {
            generation: self.generation,
            idx: idx as u32,
        }
    }

    /// Valida um `NodeId` versionado contra ESTA árvore e devolve o índice cru.
    /// `None` se a `generation` não casa (id de árvore velha) ou o índice é inválido —
    /// é exatamente a guarda que impede aplicar estado a um nó vivo errado.
    pub fn resolve(&self, id: NodeId) -> Option<NodeIdx> {
        let idx = id.idx as usize;
        if id.generation == self.generation && idx < self.nodes.len() {
            Some(idx)
        } else {
            // Distinguir os dois é o que separa "id de uma árvore ANTERIOR"
            // (uso-após-troca, quase sempre um bug do chamador) de "índice fora
            // da arena" (id corrompido ou forjado na travessia da ABI).
            if id.generation != self.generation {
                crate::bump!(resolve_stale);
            } else {
                crate::bump!(resolve_out_of_range);
            }
            None
        }
    }

    /// O `NodeId` versionado da raiz `#document`.
    pub fn root_id(&self) -> NodeId {
        self.make_id(self.root)
    }

    /// Registra um nó nos índices a partir de seus atributos `id`/`class`.
    pub(in crate::dom) fn deindex_node(&mut self, id: NodeIdx) {
        crate::bump!(index_removes);
        let old_id = self.nodes[id].attr("id").map(str::to_owned);
        let old_classes: Vec<String> = self.nodes[id]
            .attr("class")
            .map(|value| value.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();
        if let Some(key) = old_id {
            if let Some(bucket) = self.id_index.get_mut(&key) {
                bucket.retain(|&x| x != id);
                if bucket.is_empty() {
                    self.id_index.remove(&key);
                }
            }
        }
        for key in old_classes {
            if let Some(bucket) = self.class_index.get_mut(&key) {
                bucket.retain(|&x| x != id);
                if bucket.is_empty() {
                    self.class_index.remove(&key);
                }
            }
        }
    }

    pub(in crate::dom) fn remove_index_key(&mut self, key: &str, id: NodeIdx, is_id: bool) {
        let index = if is_id {
            &mut self.id_index
        } else {
            &mut self.class_index
        };
        if let Some(bucket) = index.get_mut(key) {
            bucket.retain(|&x| x != id);
            if bucket.is_empty() {
                index.remove(key);
            }
        }
    }

    pub(in crate::dom) fn index_node(&mut self, id: NodeIdx) {
        // Coleta antes para não emprestar `self.nodes` e os índices juntos.
        let id_attr = self.nodes[id].attr("id").map(str::to_string);
        let classes: Vec<String> = self.nodes[id]
            .attr("class")
            .map(|c| c.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        if let Some(k) = id_attr {
            self.id_index.entry(k).or_default().push(id);
            crate::bump!(index_inserts);
        }
        for c in classes {
            self.class_index.entry(c).or_default().push(id);
            crate::bump!(index_inserts);
        }
    }

    /// Aloca um nó (com seus atributos) como filho de `parent`; devolve o índice.
    pub(in crate::dom) fn push(&mut self, kind: NodeKind, attrs: Vec<Attr>, parent: NodeIdx) -> NodeIdx {
        if let Some(style) = attrs.iter().find(|a| a.name == "style") {
            self.note_inline_position(&style.value);
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            attrs,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.layout_epochs.push(0);
        self.index_node(id);
        self.nodes[parent].children.push(id);
        crate::bump!(nodes_created);
        crate::bump!(tree_links);
        id
    }

    /// Acesso por índice CRU (interno ao render, que percorre a árvore por
    /// índices). A API pública/ABI usa `NodeId` versionado + `resolve`.
    pub fn node(&self, idx: NodeIdx) -> &Node {
        &self.nodes[idx]
    }

    /// `true` se o `NodeId` versionado é válido NESTA árvore (generation casa + índice na
    /// arena). Substitui o antigo `idx < len` que não detectava id de árvore velha.
    pub fn is_valid(&self, id: NodeId) -> bool {
        self.resolve(id).is_some()
    }


    /// Aloca um nó sem pai (usado por create_element / set_text). Índice cru.
    pub(in crate::dom) fn push_detached(&mut self, kind: NodeKind) -> NodeIdx {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            attrs: Vec::new(),
            parent: None,
            children: Vec::new(),
        });
        self.layout_epochs.push(0);
        crate::bump!(nodes_created);
        id
    }

    /// Remove `idx` da lista de filhos do seu pai atual (se houver).
    pub(in crate::dom) fn detach(&mut self, idx: NodeIdx) {
        if let Some(p) = self.nodes[idx].parent.take() {
            crate::bump!(nodes_detached);
            self.nodes[p].children.retain(|&c| c != idx);
        }
    }

    /// `true` se `a` é ancestral de (ou igual a) `b` — guarda contra ciclos.
    pub(in crate::dom) fn is_ancestor(&self, a: NodeIdx, b: NodeIdx) -> bool {
        let mut cur = Some(b);
        while let Some(c) = cur {
            if c == a {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }
}
