//! Campos de FORMULÁRIO: `<input>`/`<textarea>` editável, foco, e os pixels
//! que um `<img>`/`<canvas>` carrega.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    // ── FORMULÁRIO: input editável (mini-browser) ────────────────────────────────

    /// O texto a EXIBIR num `<input>`: o valor editado (`input_values`), senão o
    /// atributo `value=`, senão `""`. É o que o layout pinta dentro da caixa.
    pub fn input_value(&self, id: NodeIdx) -> String {
        if let Some(v) = self.input_values.get(&id) {
            return v.clone();
        }
        self.node(id).attr("value").unwrap_or("").to_string()
    }

    /// `true` se o input está vazio (nada digitado e sem `value=`) — o layout então
    /// mostra o `placeholder` em cor apagada, como o browser.
    pub fn input_is_empty(&self, id: NodeIdx) -> bool {
        self.input_value(id).is_empty()
    }

    /// Qual input tem o foco agora (recebe teclas).
    pub fn focused_input(&self) -> Option<NodeIdx> {
        self.focused_input
    }

    /// Dá o foco a `id` (ou tira o foco, com `None`). O caller (loop TS) passa o
    /// input sob o cursor após um clique. Bumpa a revisão (o cursor a pintar muda).
    pub fn focus_input(&mut self, id: Option<NodeIdx>) {
        if self.focused_input != id {
            self.focused_input = id;
            self.touch();
        }
    }

    /// Anexa `text` (os caracteres digitados no frame) ao input FOCADO. Ignora se
    /// não há foco. Retorna `true` se algo mudou.
    pub fn input_feed_text(&mut self, text: &str) -> bool {
        let Some(id) = self.focused_input else {
            return false;
        };
        if text.is_empty() {
            return false;
        }
        // filtra controles (o backend já separa Enter/Backspace; aqui só texto real).
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if clean.is_empty() {
            return false;
        }
        let cur = self.input_value(id);
        self.input_values.insert(id, cur + &clean);
        self.touch();
        true
    }

    /// Associa a um `<img>` os pixels RGBA já decodificados (handle do Buffer +
    /// offset + w + h). O browser chama após baixar+decodificar. Bumpa a revisão.
    /// Os PIXELS de um nó, guardados no PRÓPRIO documento.
    ///
    /// Existe ao lado do [`set_image`](Dom::set_image) — que aponta para um
    /// buffer de FORA por handle — porque um `<canvas>` desenhado pelo programa
    /// não tem dono externo: quem pintou foi o próprio programa, e o desenho
    /// precisa sobreviver à chamada que o produziu. É a mesma razão de o texto
    /// de um nó ser copiado para dentro da árvore em vez de referenciado.
    ///
    /// RGBA8, `w * h * 4` bytes. Um buffer CURTO é recusado: lido como imagem,
    /// ele é uma leitura fora dos limites dentro do backend.
    pub fn set_pixel_data(&mut self, id: NodeId, bytes: Vec<u8>, w: u32, h: u32) {
        let Some(idx) = self.resolve(id) else { return };
        if bytes.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        self.own_pixels.insert(idx, (std::rc::Rc::new(bytes), w, h));
        self.touch_render_only(idx);
    }

    /// Os pixels próprios de um nó, se ele tem.
    pub fn pixel_data_of(&self, idx: NodeIdx) -> Option<(std::rc::Rc<Vec<u8>>, u32, u32)> {
        self.own_pixels
            .get(&idx)
            .map(|(b, w, h)| (std::rc::Rc::clone(b), *w, *h))
    }

    pub fn set_image(&mut self, id: NodeId, handle: u64, off: u32, w: u32, h: u32) {
        if let Some(idx) = self.resolve(id) {
            self.image_pixels.insert(idx, (handle, off, w, h));
            self.touch();
        }
    }

    /// Os pixels da imagem de um nó (handle, offset, w, h), se já setados.
    pub fn image_of(&self, idx: NodeIdx) -> Option<(u64, u32, u32, u32)> {
        self.image_pixels.get(&idx).copied()
    }

    /// `true` se o `NodeIdx` cru é um `<input>`/`<textarea>` (para o hit-test de foco).
    pub fn is_text_input_idx(&self, idx: NodeIdx) -> bool {
        matches!(&self.nodes.get(idx).map(|n| &n.kind),
            Some(NodeKind::Element { tag }) if matches!(tag.as_str(), "input" | "textarea"))
    }

    /// O `NodeId` público (com generation) de um `NodeIdx` cru — para o hit-test
    /// devolver ao TS um id estável.
    pub fn id_of_idx(&self, idx: NodeIdx) -> NodeId {
        self.make_id(idx)
    }

    /// Apaga o último caractere do input focado (Backspace). Retorna `true` se mudou.
    pub fn input_backspace(&mut self) -> bool {
        let Some(id) = self.focused_input else {
            return false;
        };
        let mut cur = self.input_value(id);
        if cur.pop().is_none() {
            return false;
        }
        self.input_values.insert(id, cur);
        self.touch();
        true
    }

    /// Informa o VIEWPORT da passada de layout (base de `vw`/`vh` no computed).
    /// `&self` (Cell) — o layout roda sobre `&Dom`; o memo de estilo invalida
    /// sozinho quando o viewport muda (compara em `computed_style_idx`).
    pub fn set_viewport(&self, w: f32, h: f32) {
        self.viewport.set((w, h));
    }
}
