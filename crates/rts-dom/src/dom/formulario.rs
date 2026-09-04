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
        if self.focused_input == id {
            return;
        }
        let previous = self.focused_input;
        if let Some(previous) = previous {
            self.raw_event_queue.push_back((previous, "focusout".to_string()));
            self.raw_event_queue.push_back((previous, "blur".to_string()));
        }
        self.focused_input = id;
        if let Some(current) = id {
            self.raw_event_queue.push_back((current, "focusin".to_string()));
            self.raw_event_queue.push_back((current, "focus".to_string()));
        }
        self.touch();
    }

    /// Anexa `text` (os caracteres digitados no frame) ao input FOCADO. Ignora se
    /// não há foco. Retorna `true` se algo mudou.
    pub fn input_feed_text(&mut self, text: &str) -> bool {
        let Some(id) = self.focused_input else {
            return false;
        };
        self.input_feed_text_at(id, text)
    }

    /// Variante direccionada usada pela fila backend→DOM. O alvo é validado antes
    /// da mutação para que uma mudança de foco durante `keydown` não redireccione
    /// texto já capturado para outro campo.
    pub fn input_feed_text_at(&mut self, id: NodeIdx, text: &str) -> bool {
        if !self.is_text_input_idx(id) || text.is_empty() {
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

    /// `input.value = t` — SUBSTITUI o que la esta, em vez de acrescentar.
    ///
    /// Existe ao lado do `input_feed_text_at` porque as duas operacoes sao
    /// diferentes e uma nao se escreve com a outra: aquela e uma TECLA a chegar
    /// e esta e o programa a decidir o conteudo. Limpar um campo depois de
    /// submeter — `el.value = ""` — nao tem forma nenhuma no vocabulario de
    /// alimentar, e apagar caracter a caracter com o `input_backspace_at` num
    /// laco e uma operacao O(n) a fazer o trabalho de uma atribuicao.
    ///
    /// Aceita a string VAZIA, que e o caso que motiva a funcao — o
    /// `input_feed_text_at` recusa-a de proposito, porque uma tecla vazia nao
    /// existe.
    ///
    /// Os controlos sao filtrados pela mesma razao que la: uma quebra de linha
    /// num `<input>` de uma linha nao tem representacao e sairia como um quadrado.
    pub fn set_input_value(&mut self, id: NodeIdx, text: &str) -> bool {
        if !self.is_text_input_idx(id) {
            return false;
        }
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if self.input_value(id) == clean {
            return false;
        }
        self.input_values.insert(id, clean);
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
        self.input_backspace_at(id)
    }

    /// Variante direccionada do Backspace para o alvo do evento de teclado.
    pub fn input_backspace_at(&mut self, id: NodeIdx) -> bool {
        if !self.is_text_input_idx(id) {
            return false;
        }
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

    /// `prefers-color-scheme` do host (lote P, §5.P item 1). `&self` (Cell) —
    /// mudar isto não é uma mutação estrutural, só o que `@media`/`matchMedia`
    /// respondem: bumpar `touch()` invalida o memo de estilo, exactamente como
    /// mudar o viewport já faz.
    pub fn set_prefers_color_scheme(&mut self, dark: bool) {
        self.prefers_color_scheme.set(if dark {
            crate::style::PrefersColorScheme::Dark
        } else {
            crate::style::PrefersColorScheme::Light
        });
        self.touch();
    }

    pub fn set_prefers_reduced_motion(&mut self, reduce: bool) {
        self.prefers_reduced_motion.set(reduce);
        self.touch();
    }

    /// O [`MediaContext`](crate::style::MediaContext) corrente — viewport +
    /// preferências do host. Construído aqui e não guardado como campo porque
    /// o viewport (`Cell`) já muda por fora do `&mut self`; um `MediaContext`
    /// persistido ficaria stale no mesmo instante em que `set_viewport` corre.
    pub fn media_context(&self) -> crate::style::MediaContext {
        let (w, h) = self.viewport.get();
        crate::style::MediaContext {
            width: w,
            height: h,
            prefers_color_scheme: self.prefers_color_scheme.get(),
            prefers_reduced_motion: self.prefers_reduced_motion.get(),
        }
    }

    /// Igual a [`media_context`](Dom::media_context), mas com a LARGURA
    /// substituída — o layout às vezes mede contra uma largura diferente do
    /// viewport corrente (o `ctx.viewport_w` de uma passada de intrínseco), e
    /// `<picture><source media>` tem de casar ESSA, não a do `Dom`.
    pub fn media_context_at_width(&self, width: f32) -> crate::style::MediaContext {
        let mut ctx = self.media_context();
        ctx.width = width;
        ctx
    }

    /// `window.matchMedia(query).matches` (lote P, §5.P item 2) — o MESMO
    /// avaliador que a cascade usa para `@media`, contra o mesmo
    /// [`media_context`](Dom::media_context). Reparseia a `query` a cada
    /// chamada (como `@supports`/seletor: não há cache de string arbitrária) —
    /// barato: `matchMedia` não corre por nó, corre por chamada de script.
    pub fn media_matches(&self, query: &str) -> bool {
        crate::style::MediaQuery::parse(query).matches(&self.media_context())
    }
}
