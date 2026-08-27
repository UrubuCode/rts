//! Eventos: `addEventListener`, `dispatchEvent` com bubbling, a fila de
//! polling e os eventos crus do backend.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

/// Evento de teclado bruto produzido por um backend. O DOM escolhe o alvo antes
/// de o evento atravessar a fronteira TypeScript; `key_code` usa a tabela neutra
/// de `rts-input` (A-Z 100..125, dígitos 130..139, F1..F12 140..151).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawKeyboardEvent {
    pub target: NodeIdx,
    pub key_code: i64,
    pub pressed: bool,
    pub repeat: bool,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
}

impl Dom {

    // ── Eventos (#1760) — modelo de polling + bubbling headless ──────────────────
    // O motor não guarda callbacks de fn de forma confiável (limite #195), então o
    // Rust registra só QUE TIPO cada nó escuta; os callbacks vivem no TS. O
    // `dispatchEvent` enfileira (nó, tipo) já expandido pelo BUBBLING (alvo → pais
    // que escutam), e o loop TS consome via `poll_event` e chama o handler certo.

    /// `element.addEventListener(type, handler)`: registra que o nó escuta `type`.
    /// O handler real é guardado no lado Rust como word opaco. Sem callback, mantém
    /// a API de polling legada; com callback, as opções definem a propagação.
    pub fn add_event_listener(&mut self, id: NodeId, event_type: &str) {
        crate::bump!(listeners_added);
        let Some(idx) = self.resolve(id) else { return };
        let types = self.listeners.entry(idx).or_default();
        let t = event_type.to_string();
        if !types.contains(&t) {
            types.push(t);
        }
    }

    /// Regista um callback com as opções DOM. A identidade de um listener é
    /// `(target, type, callback, capture)`; `once` e `passive` não criam duplicatas.
    pub fn add_event_listener_cb_with_options(
        &mut self,
        id: NodeId,
        event_type: &str,
        cb: i64,
        options: ListenerOptions,
    ) {
        let Some(idx) = self.resolve(id) else { return };
        self.add_event_listener(id, event_type);
        let cbs = self
            .listener_cbs
            .entry((idx, event_type.to_string()))
            .or_default();
        if !cbs.iter().any(|record| record.callback == cb && record.options.capture == options.capture) {
            cbs.push(ListenerRecord { callback: cb, options });
        }
    }

    /// Compatibilidade com a ABI antiga: listener bubble, persistente e activo.
    pub fn add_event_listener_cb(&mut self, id: NodeId, event_type: &str, cb: i64) {
        self.add_event_listener_cb_with_options(id, event_type, cb, ListenerOptions::default());
    }

    /// Remove um callback específico. `capture` participa na identidade do listener.
    pub fn remove_event_listener_cb(
        &mut self,
        id: NodeId,
        event_type: &str,
        cb: i64,
        capture: bool,
    ) {
        crate::bump!(listeners_removed);
        let Some(idx) = self.resolve(id) else { return };
        let key = (idx, event_type.to_string());
        if let Some(cbs) = self.listener_cbs.get_mut(&key) {
            cbs.retain(|record| !(record.callback == cb && record.options.capture == capture));
            if cbs.is_empty() {
                self.listener_cbs.remove(&key);
            }
        }
    }

    /// `element.removeEventListener(type)`: remove todos os callbacks desse tipo.
    pub fn remove_event_listener(&mut self, id: NodeId, event_type: &str) {
        crate::bump!(listeners_removed);
        let Some(idx) = self.resolve(id) else { return };
        if let Some(types) = self.listeners.get_mut(&idx) {
            types.retain(|x| x != event_type);
        }
        self.listener_cbs.remove(&(idx, event_type.to_string()));
    }

    /// `true` se o nó escuta o tipo de evento dado (case-sensitive).
    pub fn has_listener(&self, id: NodeId, event_type: &str) -> bool {
        let Some(idx) = self.resolve(id) else {
            return false;
        };
        self.listeners
            .get(&idx)
            .map(|v| v.iter().any(|x| x == event_type))
            .unwrap_or(false)
    }

    /// `element.dispatchEvent(type, bubbles)`: dispara um evento no nó-alvo. Sempre
    /// notifica o ALVO; se `bubbles`, sobe pelos ancestrais que escutam o tipo (fiel
    /// ao DOM: `focus`/`blur`/`new Event(t)` não borbulham). Para cada nó na cadeia
    /// que escuta, enfileira `(nó, tipo)` para o loop TS via `poll_event`. Devolve
    /// quantos listeners foram enfileirados. Tipo CASE-SENSITIVE.
    pub fn dispatch_event(&mut self, target: NodeId, event_type: &str, bubbles: bool) -> i64 {
        crate::bump!(dispatches);
        let mut count = 0;
        let mut cur = Some(target);
        let mut first = true;
        while let Some(node) = cur {
            let Some(idx) = self.resolve(node) else { break };
            if self
                .listeners
                .get(&idx)
                .map(|v| v.iter().any(|x| x == event_type))
                .unwrap_or(false)
            {
                crate::bump!(dispatch_targets);
                self.event_queue.push_back((idx, event_type.to_string()));
                count += 1;
            }
            // sem bubbling: só o alvo (primeira iteração) é notificado.
            if !bubbles && first {
                break;
            }
            first = false;
            cur = self.parent_of(node);
        }
        count
    }

    /// `poll_event`: remove e devolve o próximo evento pendente `(NodeId, tipo)`, ou
    /// `None` se a fila está vazia. O loop TS chama em laço por frame e despacha o
    /// callback certo (que vive no TS, indexado por nó+tipo). O NodeId é versionado.
    pub fn poll_event(&mut self) -> Option<(NodeId, String)> {
        let event = self.event_queue.pop_front();
        if let Some((_, ref event_type)) = event {
            self.last_event_type = event_type.clone();
        } else {
            self.last_event_type.clear();
        }
        event.map(|(idx, _)| (self.make_id(idx), self.last_event_type.clone()))
    }

    /// Metadata do callback colectado na última dispatch.
    pub fn last_dispatch_capture_at(&self, index: usize) -> bool {
        self.last_dispatch_capture.get(index).copied().unwrap_or(false)
    }

    pub fn last_dispatch_passive_at(&self, index: usize) -> bool {
        self.last_dispatch_passive.get(index).copied().unwrap_or(false)
    }

    /// Tipo devolvido pelo último `poll_event`.
    pub fn poll_event_type(&self) -> &str {
        &self.last_event_type
    }

    /// `dispatchEvent` com COLETA de callbacks: mesmo caminhamento (alvo → bubbling
    /// pelos ancestrais), mas além de enfileirar no polling, coleta em
    /// `last_dispatch` os pares (nó-que-escuta, callback-word) na ordem de invocação
    /// DOM (alvo primeiro, depois os ancestrais). O rts-dom NUNCA invoca — a camada
    /// TS lê via [`Dom::last_dispatch_len`]/[`Dom::last_dispatch_at`], COPIA tudo e
    /// só então invoca (um callback pode re-despachar e sobrescrever o scratch).
    /// Devolve quantos callbacks foram coletados.
    pub fn dispatch_event_collect(
        &mut self,
        target: NodeId,
        event_type: &str,
        bubbles: bool,
    ) -> i64 {
        self.last_dispatch.clear();
        self.last_dispatch_capture.clear();
        self.last_dispatch_passive.clear();

        let mut path = Vec::new();
        let mut cur = Some(target);
        while let Some(node) = cur {
            let Some(idx) = self.resolve(node) else { break };
            path.push(idx);
            cur = self.parent_of(node);
        }
        let key_type = event_type.to_string();
        let mut once = Vec::new();
        let mut collect = |idx: NodeIdx, records: &[ListenerRecord]| {
            for record in records {
                self.last_dispatch.push((idx, record.callback));
                self.last_dispatch_capture.push(record.options.capture);
                self.last_dispatch_passive.push(record.options.passive);
                if record.options.once {
                    once.push((idx, record.callback, record.options.capture));
                }
            }
        };

        // Capture: raiz → pai do target. No target, capture e bubble seguem a
        // ordem de registo, como no DOM.
        if bubbles {
            let mut i = path.len();
            while i > 1 {
                i -= 1;
                if let Some(records) = self.listener_cbs.get(&(path[i], key_type.clone())) {
                    let capture: Vec<ListenerRecord> = records
                        .iter()
                        .copied()
                        .filter(|record| record.options.capture)
                        .collect();
                    collect(path[i], &capture);
                }
            }
        }
        if let Some(&idx) = path.first() {
            if let Some(records) = self.listener_cbs.get(&(idx, key_type.clone())) {
                let records: Vec<ListenerRecord> = records.iter().copied().collect();
                collect(idx, &records);
            }
        }
        if bubbles {
            let mut i = 1;
            while i < path.len() {
                if let Some(records) = self.listener_cbs.get(&(path[i], key_type.clone())) {
                    let bubble: Vec<ListenerRecord> = records
                        .iter()
                        .copied()
                        .filter(|record| !record.options.capture)
                        .collect();
                    collect(path[i], &bubble);
                }
                i += 1;
            }
        }

        // `once` é removido antes da invocação JS. Se o callback re-despachar o
        // mesmo evento, não volta a ser colectado.
        for (idx, callback, capture) in once {
            self.remove_event_listener_cb(self.make_id(idx), event_type, callback, capture);
        }

        // Mantém o contrato do polling também (contadores/fila do modelo #1760):
        // um app antigo que só usa pumpEvents continua vendo o evento.
        self.dispatch_event(target, event_type, bubbles);
        crate::bump!(callbacks_collected, self.last_dispatch.len());
        self.last_dispatch.len() as i64
    }

    /// BACKEND → DOM: informa o nó SOB O CURSOR neste frame (`None` = ponteiro
    /// fora do conteúdo). O estado alimenta o `:hover` vivo da cascade. GUARDA DE
    /// PERF (handoff #1793): só invalida os caches (touch → re-cascade/re-layout)
    /// quando o hovered realmente MUDA **e** o stylesheet tem alguma regra
    /// `:hover` — mover o mouse numa página sem :hover custa zero.
    pub fn set_hovered(&mut self, idx: Option<NodeIdx>) {
        let previous = self.hovered.get();
        if previous == idx {
            return;
        }
        self.hovered.set(idx);
        // O ALCANCE de `:hover` é derivado das regras UMA vez (cacheado no
        // stylesheet). Antes, cada movimento do mouse varria todas as regras
        // para responder "há alguma :hover?" — 2643 delas numa página Bootstrap,
        // por frame, antes mesmo de decidir o que invalidar.
        let reach = self.stylesheet.hover_reach();
        if reach == crate::style::HoverReach::None {
            return;
        }
        // `:hover` casa o nó sob o cursor E seus ancestrais (`pseudo_matches`),
        // então a mudança é a diferença entre as duas CADEIAS. Invalidar a
        // página inteira era o que se fazia; a cadeia é o conjunto de nós cujo
        // estilo pode ter mudado, e ela tem a profundidade da árvore, não o
        // tamanho dela.
        if reach == crate::style::HoverReach::Siblings {
            // `.a:hover + .b` alcança FORA da subárvore de quem casa, e uma
            // invalidação por subárvore não a cobre. Fallback declarado — pagar
            // o global aqui é o preço de não responder errado.
            self.touch();
            return;
        }
        // Da cadeia, só entram os nós que PODERIAM casar uma regra de hover. O
        // `<body>` é ancestral de tudo e não casa `.btn:hover`; deixá-lo entrar
        // faria a subárvore suja ser a página, que é de onde se estava partindo.
        let hover_compounds = self.stylesheet.hover_compounds();
        let mut roots: Vec<NodeIdx> = Vec::new();
        for start in [previous, idx].into_iter().flatten() {
            let mut cur = Some(start);
            while let Some(node) = cur {
                if node != self.root
                    && !roots.contains(&node)
                    && self.could_match_hover(node, &hover_compounds)
                {
                    roots.push(node);
                }
                cur = self.nodes[node].parent;
            }
        }
        if !roots.is_empty() {
            // Subárvore e não só o nó: uma propriedade HERDADA declarada num
            // `:hover` (o caso comum é `color`) desce para os filhos, que não
            // casam regra nenhuma e mudam mesmo assim.
            self.touch_subtrees(roots);
        }
    }

    /// BACKEND → DOM: enfileira uma transição de teclado. O target é o input
    /// focado; sem foco, usa `body` e finalmente `documentElement`.
    pub fn push_raw_keyboard_event(
        &mut self,
        key_code: i64,
        pressed: bool,
        repeat: bool,
        ctrl_key: bool,
        shift_key: bool,
        alt_key: bool,
        meta_key: bool,
    ) {
        let target = self
            .focused_input
            .or_else(|| self.query_idx("body"))
            .or_else(|| self.document_element().map(|id| id.idx as usize))
            .unwrap_or(self.root);
        self.raw_keyboard_event_queue.push_back(RawKeyboardEvent {
            target,
            key_code,
            pressed,
            repeat,
            ctrl_key,
            shift_key,
            alt_key,
            meta_key,
        });
    }

    /// Retira o próximo evento de teclado bruto produzido pelo backend.
    pub fn poll_raw_keyboard_event(&mut self) -> Option<RawKeyboardEvent> {
        let event = self.raw_keyboard_event_queue.pop_front();
        self.last_raw_keyboard_event = event;
        event
    }

    /// Consulta o evento de teclado devolvido pelo último polling.
    pub fn last_raw_keyboard_event(&self) -> Option<&RawKeyboardEvent> {
        self.last_raw_keyboard_event.as_ref()
    }

    /// BACKEND → DOM: empurra um evento CRU (`(nó, tipo)`) vindo do hit-test do
    /// mouse. Nenhuma expansão aqui — a fachada TS drena com [`Dom::poll_raw_event`]
    /// e faz o dispatch completo (bubbling + callbacks). `idx` é o `NodeIdx` cru
    /// que o backend tem em mãos (chave de `node_rects`).
    pub fn push_raw_event(&mut self, idx: NodeIdx, event_type: &str) {
        self.raw_event_queue
            .push_back((idx, event_type.to_string()));
    }

    /// Próximo evento CRU do backend `(NodeId versionado, tipo)`, ou `None`.
    pub fn poll_raw_event(&mut self) -> Option<(NodeId, String)> {
        let event = self.raw_event_queue.pop_front();
        if let Some((_, ref event_type)) = event {
            self.last_raw_event_type = event_type.clone();
        } else {
            self.last_raw_event_type.clear();
        }
        event.map(|(idx, _)| (self.make_id(idx), self.last_raw_event_type.clone()))
    }

    /// Tipo devolvido pelo último `poll_raw_event`.
    pub fn poll_raw_event_type(&self) -> &str {
        &self.last_raw_event_type
    }

    /// Nº de callbacks coletados pelo último [`Dom::dispatch_event_collect`].
    pub fn last_dispatch_len(&self) -> i64 {
        self.last_dispatch.len() as i64
    }

    /// O i-ésimo par coletado: `(NodeId versionado do nó que escuta, callback-word)`.
    /// `None` se fora do range.
    pub fn last_dispatch_at(&self, i: usize) -> Option<(NodeId, i64)> {
        self.last_dispatch
            .get(i)
            .map(|&(idx, cb)| (self.make_id(idx), cb))
    }
}
