//! `Dom` — árvore de elementos RETIDA (retained-mode), a fonte da verdade do
//! conteúdo de uma janela.
//!
//! Diferente do caminho immediate-mode de `html.rs` (que re-parseia a string a
//! cada frame para uma fila PLANA de `WidgetCmd`), aqui o HTML é parseado UMA vez
//! para uma **árvore de nós persistente**. Essa árvore:
//!
//! - tem hierarquia real (cada nó conhece pai e filhos), não pares Begin/End;
//! - dá a cada nó um `NodeId` ESTÁVEL — o que o JS vai usar depois para
//!   referenciar e MUTAR um elemento (`getElementById`/`setText`/`append`…);
//! - é o que o render passa a percorrer (Fatia 2), em vez da fila.
//!
//! ## Por que arena (`Vec<Node>` + índice como `NodeId`)
//!
//! Um DOM é um grafo mutável com referências cruzadas (pai↔filho). Em Rust isso
//! com `Rc<RefCell<…>>` vira um inferno de borrows/ciclos. A arena resolve:
//! cada nó vive num `Vec`, o `NodeId` é só o índice — `Copy`, estável, trivial
//! de guardar do lado do JS, e a mutação é um `self.nodes[id]` sem brigar com o
//! borrow-checker. É o padrão consagrado (`indextree`/o DOM do servo etc).
//!
//! ## Como SABER se a árvore está correta (o ponto desta fatia)
//!
//! `Node`/`NodeKind`/`Dom` derivam `Debug` + `PartialEq`, e `Dom::dump()`
//! serializa a árvore indentada (estilo devtools). Com isso a verificação é um
//! teste unitário determinístico (`cargo test -p rts-egui`), SEM abrir janela:
//! compara-se o `dump()` (ou a estrutura) com o esperado. Ver `#[cfg(test)]`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::html::{tokenize, Token};

/// Índice cru de um nó na arena (`Dom::nodes`). Uso INTERNO ao `dom.rs` — o que
/// cruza a fronteira (TS/ABI) é sempre o `NodeId` VERSIONADO, nunca este índice.
pub type NodeIdx = usize;

/// Contador global de gerações de árvore. Cada `Dom` novo (parse ou vazio) toma a
/// próxima geração; assim duas árvores nunca colidem e um `NodeId` de uma árvore
/// velha é detectável como stale na árvore atual.
static NEXT_GEN: AtomicU32 = AtomicU32::new(1);

fn next_gen() -> u32 {
    NEXT_GEN.fetch_add(1, Ordering::Relaxed)
}

/// Identificador VERSIONADO e estável de um nó: `{ generation, idx }` (invariante 2 do
/// roadmap — sem `generation`, um índice reciclado após re-parse aplica estado a um nó
/// vivo errado, um bug de SEGURANÇA DE MEMÓRIA). É o handle que o lado JS guarda.
///
/// `generation` é a geração da ÁRVORE dona do nó; `idx` é a posição na arena. Um acesso
/// só é válido se a `generation` do id casa com a `generation` da árvore atual (`Dom::generation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId {
    pub generation: u32,
    pub idx: u32,
}

impl NodeId {
    /// Empacota num `i64` opaco para a ABI: `(generation << 32) | idx`. Sempre ≥ 0
    /// (generation começa em 1, então o bit de sinal nunca acende). `-1` é a sentinela
    /// de "nó nenhum" (invariante 3), distinta de qualquer id real.
    pub fn to_abi(self) -> i64 {
        (((self.generation as u64) << 32) | (self.idx as u64)) as i64
    }

    /// Desempacota o `i64` da ABI. `None` para a sentinela `-1` ou valores
    /// negativos (id inválido vindo do TS).
    pub fn from_abi(v: i64) -> Option<NodeId> {
        if v < 0 {
            return None;
        }
        let u = v as u64;
        Some(NodeId { generation: (u >> 32) as u32, idx: (u & 0xFFFF_FFFF) as u32 })
    }
}

/// O tipo de um nó da árvore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A raiz sintética (`#document`) que contém os nós de topo.
    Document,
    /// Um elemento com nome de tag em minúsculas (`h1`, `p`, `div`, `b`, `i`,
    /// e também tags desconhecidas como `span`/`code` — preservadas como nós,
    /// não descartadas: um DOM fiel mantém o elemento).
    Element { tag: String },
    /// Um nó de texto (folha). Entidades já vêm decodificadas.
    Text(String),
    /// Um nó de COMENTÁRIO (`<!-- ... -->`). Um DOM fiel preserva comentários como
    /// nós (nodeType 8); o render os ignora. O conteúdo é o texto entre os
    /// delimitadores. (Antes eram descartados no parse.)
    Comment(String),
}

/// Um par atributo→valor de um elemento (`class="card"`). Lista ordenada (não
/// mapa) para preservar a ordem do HTML — importante para `style` e para a
/// futura cascata de CSS, onde a ordem de declaração desempata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// Nome em minúsculas (`class`, `id`, `href`, `style`…).
    pub name: String,
    /// Valor já com entidades decodificadas; `""` para atributo sem valor.
    pub value: String,
}

/// Um nó da árvore: seu tipo + atributos + os elos de parentesco (índices na
/// arena).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    /// Atributos do elemento (vazio para Document/Text). É a base de qualquer
    /// seletor além da tag (`.classe`, `#id`) e de `<a href>` — o pré-requisito
    /// de um motor de CSS.
    pub attrs: Vec<Attr>,
    /// `None` apenas para a raiz `Document`. Índice cru (interno à arena).
    pub parent: Option<NodeIdx>,
    /// Filhos em ordem de documento. Índices crus (internos à arena).
    pub children: Vec<NodeIdx>,
}

impl Node {
    /// Valor do atributo `name` (case-insensitive), se presente.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .map(|a| a.value.as_str())
    }
}

/// A árvore inteira: arena de nós + a raiz + índices de busca O(1).
///
/// **DOM otimizado:** como somos donos da arena, mantemos índices `id → NodeId`
/// e `classe → [NodeId]` atualizados na construção e na mutação. Assim
/// `query("#alvo")`/`query(".card")` é O(1) em vez de varrer a árvore (o que um
/// `querySelector` genérico não consegue). Query por tag segue pré-ordem O(n)
/// (pra respeitar a ordem de documento; um índice por tag viria depois, se valer).
#[derive(Debug, Clone)]
pub struct Dom {
    /// Geração desta árvore (invariante 2). Todo `NodeId` que sai daqui carrega
    /// esta `generation`; um id com `generation` diferente é stale (de uma árvore anterior).
    generation: u32,
    /// Arena: `nodes[idx]` é o nó de índice cru `idx`.
    pub nodes: Vec<Node>,
    /// A raiz sintética `#document` (índice cru — sempre 0).
    pub root: NodeIdx,
    /// Índice `valor-de-id → NodeIdx` (último a registrar vence, como no browser).
    id_index: HashMap<String, NodeIdx>,
    /// Índice `classe → nós que a têm` (em ordem de inserção).
    class_index: HashMap<String, Vec<NodeIdx>>,
    /// Override de estilo POR-NÓ (`setStyleBatch`) — a 3ª e mais forte fonte de
    /// estilo (precedência: tag < `style=""` inline < override por-nó). Mapa à
    /// parte (não no `Node`) porque `ComputedStyle` tem `f32` (não-`Eq`) e o `Node`
    /// precisa de `Eq` p/ o diff de árvore. Estado DERIVADO: não entra no
    /// `PartialEq` do `Dom` (que compara só `nodes`+`root`).
    style_overrides: HashMap<NodeIdx, crate::style::ComputedStyle>,
    /// Stylesheet de AUTOR acumulado de todos os `<style>` da página (regras com
    /// seletor tag/`.class`/`#id`, resolvidas por especificidade). Camada entre o
    /// `defineStyle` (por-tag, mais fraco) e o `style=""` inline. Estado DERIVADO
    /// do HTML — não entra no `PartialEq` do `Dom`.
    stylesheet: crate::style::Stylesheet,
    /// CSS BRUTO acumulado dos `<style>` — guardado para resolver os pseudo-elementos
    /// `::-webkit-scrollbar*` (o stylesheet parseado não modela pseudo-elementos).
    /// DERIVADO do HTML, fora do `PartialEq`.
    raw_css: String,
    /// Eventos (#1760, modelo de POLLING — F3): que TIPOS cada nó escuta
    /// (`addEventListener`). Aqui só registramos o tipo p/ saber se um
    /// `dispatchEvent` deve notificar o nó. `NodeIdx → tipos`. DERIVADO, fora do Eq.
    listeners: HashMap<NodeIdx, Vec<String>>,
    /// CALLBACKS por (nó, tipo) — `el.addEventListener('click', fn)` com fn de
    /// verdade. O Dom guarda o WORD/handle i64 da Function OPACO (nunca o invoca —
    /// o rts-dom é headless e livre de runtime; quem invoca é a camada TS via
    /// `dispatch_event_collect`). Guardar fn-handles ficou confiável com o motor
    /// novo (Entry::Function com keep_alive; o antigo limite #195 caiu).
    listener_cbs: HashMap<(NodeIdx, String), Vec<i64>>,
    /// Fila de eventos PENDENTES a entregar ao loop TS via `pollEvent`. Cada entrada
    /// é `(nó-alvo a notificar, tipo)` — já expandida pelo bubbling no `dispatch`.
    event_queue: std::collections::VecDeque<(NodeIdx, String)>,
    /// Scratch da ÚLTIMA coleta de `dispatch_event_collect`: pares (nó-alvo,
    /// callback-word) na ordem de invocação. A camada TS copia TUDO para um array
    /// local ANTES de invocar (um callback pode re-despachar e sobrescrever isto).
    last_dispatch: Vec<(NodeIdx, i64)>,
    // ── Animação (#1776) — LOOP INTERNO ao DOM; o egui só passa o tempo ───────────
    /// As transições EM CURSO, por nó. O `Dom` é dono do loop: `advance(now_ms)`
    /// detecta mudanças de estilo, inicia/atualiza transições e grava o estilo
    /// interpolado em `style_overrides` (a camada mais forte). O backend (egui) só
    /// passa o `now_ms` ao pedir o layout — continua BURRO (lê a DisplayList e pinta).
    active_transitions: HashMap<NodeIdx, crate::anim::ActiveTransition>,
    /// O estilo computado de cada nó NO FRAME ANTERIOR (sem o override de animação),
    /// para detectar mudanças que disparam uma transição. DERIVADO, fora do Eq.
    prev_computed: HashMap<NodeIdx, crate::style::ComputedStyle>,
    /// O estilo INTERPOLADO atual de cada nó animando (camada que o `computed_style`
    /// aplica POR ÚLTIMO, acima de tudo). Escrito por `advance`, lido pelo layout.
    anim_override: HashMap<NodeIdx, crate::style::ComputedStyle>,
    /// Instante (ms) em que a `animation` (@keyframes) de cada nó começou. Mantido
    /// enquanto o nó tem a mesma animação; reinicia se o nome muda.
    anim_start: HashMap<NodeIdx, (String, f32)>,
    /// REVISÃO de render: contador bumpado por TODA mutação que afeta o que se
    /// desenha (árvore/atributos/texto/estilo/animação — via `touch()`). É a chave
    /// dos caches de layout (a DisplayList do backend e o GEOM_CACHE da ABI):
    /// mesma [`render_revision`](Dom::render_revision) + mesmo viewport ⇒ o layout
    /// anterior ainda vale (era o motivo de a página Bootstrap re-rodar a cascade
    /// de ~2700 regras a CADA frame/clique). DERIVADO, fora do `PartialEq`.
    /// ⚠️ Método mutador novo que afete o render DEVE chamar `self.touch()`.
    revision: u64,
    /// EPOCH de animação — bumpado a CADA frame de `advance` que interpola (o único
    /// `touch` por-frame). Entra no `render_revision` (o layout re-pinta a
    /// interpolação) mas NÃO na `revision` estrutural — assim o `base_memo` (alvo-base
    /// da cascade, que não depende da camada de animação) sobrevive entre frames de
    /// animação, e o `advance` deixa de re-rodar a cascade de todos os nós por frame.
    anim_epoch: u64,
    /// Memo do estilo COMPUTADO por nó DENTRO de uma revisão: a cascade completa
    /// (todas as regras × seletores) rodava várias vezes POR NÓ num único layout
    /// (pré-pass de medição + pintura + intrínsecas). Invalidado quando a revisão
    /// muda. `RefCell` porque `computed_style_idx` é `&self` (chamado do layout).
    computed_memo: std::cell::RefCell<HashMap<NodeIdx, crate::style::ComputedStyle>>,
    /// A revisão de render em que o `computed_memo` foi preenchido.
    memo_revision: std::cell::Cell<u64>,
    /// Memo do ALVO-BASE (cascade SEM a camada de animação, `with_anim=false`) que o
    /// `advance` consulta por nó a cada frame. Invalida só pela revisão ESTRUTURAL
    /// (`revision`+viewport+style_epoch, SEM o `anim_epoch`) — então frames de
    /// animação (que só bumpam `anim_epoch`) o REUSAM, tornando o `advance` barato.
    base_memo: std::cell::RefCell<HashMap<NodeIdx, crate::style::ComputedStyle>>,
    /// A revisão estrutural em que o `base_memo` foi preenchido.
    base_memo_revision: std::cell::Cell<u64>,
    base_memo_viewport: std::cell::Cell<(u32, u32)>,
    /// O VIEWPORT corrente (w, h) — setado pelo layout no início da passada
    /// ([`set_viewport`](Dom::set_viewport)); a base de `vw`/`vh` na cascade
    /// (font-size fluido) e do `@media` futuro. Default 1280×800 (headless).
    viewport: std::cell::Cell<(f32, f32)>,
    /// O viewport com que o `computed_memo` foi preenchido (o computed depende
    /// dele via vw/vh) — muda → memo invalida.
    memo_viewport: std::cell::Cell<(u32, u32)>,
    // ── Campos de FORMULÁRIO (input editável) — F: mini-browser ───────────────────
    /// Texto CORRENTE de cada `<input>`/`<textarea>` editável. O valor INICIAL vem do
    /// atributo `value=`; toda digitação (append/backspace via `input_feed_*`) grava
    /// AQUI, não no atributo. O layout lê daqui (fallback: atributo `value`, senão o
    /// `placeholder`). DERIVADO — fora do `PartialEq`.
    input_values: HashMap<NodeIdx, String>,
    /// Imagem decodificada de cada `<img>` (pixels RGBA já prontos): `(handle do
    /// Buffer, offset dos pixels, w, h)`. Setado pelo browser via `setImage` depois
    /// de baixar+decodificar; o layout emite `DisplayItem::Image`. DERIVADO, fora do Eq.
    image_pixels: HashMap<NodeIdx, (u64, u32, u32, u32)>,
    /// Qual `<input>` tem o FOCO (recebe as teclas). `None` = nenhum. Setado por
    /// `focus_input` (o loop TS chama após um clique dentro da caixa de um input).
    /// DERIVADO, fora do `PartialEq`.
    focused_input: Option<NodeIdx>,
}

// Igualdade estrutural: compara só a árvore (nodes+root). Os índices e a `generation`
// são estado DERIVADO/de-identidade — duas árvores com os mesmos nós são iguais.
impl PartialEq for Dom {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.nodes == other.nodes
    }
}
impl Eq for Dom {}

impl Dom {
    /// Cria uma árvore vazia contendo só o `#document`. Toma a próxima geração.
    fn new() -> Dom {
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
            raw_css: String::new(),
            listeners: HashMap::new(),
            listener_cbs: HashMap::new(),
            last_dispatch: Vec::new(),
            event_queue: std::collections::VecDeque::new(),
            active_transitions: HashMap::new(),
            prev_computed: HashMap::new(),
            anim_override: HashMap::new(),
            anim_start: HashMap::new(),
            revision: 0,
            anim_epoch: 0,
            computed_memo: std::cell::RefCell::new(HashMap::new()),
            memo_revision: std::cell::Cell::new(0),
            base_memo: std::cell::RefCell::new(HashMap::new()),
            base_memo_revision: std::cell::Cell::new(u64::MAX),
            base_memo_viewport: std::cell::Cell::new((0, 0)),
            viewport: std::cell::Cell::new((1280.0, 800.0)),
            memo_viewport: std::cell::Cell::new((1280.0f32.to_bits(), 800.0f32.to_bits())),
            input_values: HashMap::new(),
            image_pixels: HashMap::new(),
            focused_input: None,
        }
    }

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
        let Some(id) = self.focused_input else { return false };
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
        let Some(id) = self.focused_input else { return false };
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

    /// Marca que ALGO que afeta o render mudou (bumpa a revisão). Chamado por todo
    /// método mutador de árvore/atributo/texto/estilo/animação. Barato (u64 += 1);
    /// um bump espúrio (mutação que falhou a validação) só invalida cache à toa.
    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// A revisão de RENDER desta árvore: muda sempre que árvore/estilo/animação
    /// mudam — inclui o epoch GLOBAL de estilo por-tag (`defineStyle`/`defineBlock`,
    /// que vivem fora do `Dom`). É a chave de cache de layout do backend e da ABI:
    /// mesma revisão + mesmo viewport ⇒ a DisplayList anterior ainda vale.
    pub fn render_revision(&self) -> u64 {
        self.revision
            .wrapping_add(self.anim_epoch)
            .wrapping_add(crate::style::props::style_epoch())
    }

    /// A revisão ESTRUTURAL: muda com árvore/atributo/estilo/viewport (o que altera o
    /// ALVO-BASE da cascade), mas NÃO com a interpolação de animação (`anim_epoch`).
    /// É a chave do `base_memo` — o que o `advance` reusa entre frames de animação.
    fn struct_revision(&self) -> u64 {
        self.revision.wrapping_add(crate::style::props::style_epoch())
    }

    /// Bumpa SÓ o epoch de animação (invalida o layout p/ re-pintar a interpolação),
    /// sem tocar a revisão estrutural — o `advance` chama isto por frame no lugar de
    /// `touch()`, para o `base_memo` sobreviver ao frame.
    fn touch_anim(&mut self) {
        self.anim_epoch = self.anim_epoch.wrapping_add(1);
    }

    /// O ALVO-BASE (cascade sem animação) de um nó, MEMOIZADO por revisão estrutural.
    /// O `advance` consulta isto a cada frame; entre frames de animação (revisão
    /// estrutural estável) é um hit de cache — a cascade não re-roda. `None` p/
    /// não-elemento.
    fn base_style_idx(&self, idx: NodeIdx) -> Option<crate::style::ComputedStyle> {
        let rev = self.struct_revision();
        let (vw, vh) = self.viewport.get();
        let vp_key = (vw.to_bits(), vh.to_bits());
        if self.base_memo_revision.get() != rev || self.base_memo_viewport.get() != vp_key {
            self.base_memo.borrow_mut().clear();
            self.base_memo_revision.set(rev);
            self.base_memo_viewport.set(vp_key);
        }
        if let Some(hit) = self.base_memo.borrow().get(&idx) {
            return Some(hit.clone());
        }
        let computed = self.computed_style_idx_inner(idx)?;
        self.base_memo.borrow_mut().insert(idx, computed.clone());
        Some(computed)
    }

    /// Acrescenta o conteúdo de um `<style>` ao stylesheet de autor da página
    /// (chamado pelo parser ao encontrar um `RawElement` de `style`). Vários
    /// `<style>` acumulam, com as regras posteriores desempatando por cima.
    pub fn add_stylesheet(&mut self, css: &str) {
        self.touch();
        self.stylesheet.append_css(css);
        // guarda o bruto p/ os pseudo-elementos ::-webkit-scrollbar* (#1744).
        self.raw_css.push_str(css);
        self.raw_css.push('\n');
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

    /// A geração desta árvore (para o render/ABI compor `NodeId` versionados).
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Empacota um índice cru desta árvore num `NodeId` versionado (com a `generation`
    /// da árvore). É como um índice interno vira handle público.
    fn make_id(&self, idx: NodeIdx) -> NodeId {
        NodeId { generation: self.generation, idx: idx as u32 }
    }

    /// Valida um `NodeId` versionado contra ESTA árvore e devolve o índice cru.
    /// `None` se a `generation` não casa (id de árvore velha) ou o índice é inválido —
    /// é exatamente a guarda que impede aplicar estado a um nó vivo errado.
    pub fn resolve(&self, id: NodeId) -> Option<NodeIdx> {
        let idx = id.idx as usize;
        if id.generation == self.generation && idx < self.nodes.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// O `NodeId` versionado da raiz `#document`.
    pub fn root_id(&self) -> NodeId {
        self.make_id(self.root)
    }

    /// Registra um nó nos índices a partir de seus atributos `id`/`class`.
    fn index_node(&mut self, id: NodeIdx) {
        // Coleta antes para não emprestar `self.nodes` e os índices juntos.
        let id_attr = self.nodes[id].attr("id").map(str::to_string);
        let classes: Vec<String> = self.nodes[id]
            .attr("class")
            .map(|c| c.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        if let Some(k) = id_attr {
            self.id_index.insert(k, id);
        }
        for c in classes {
            self.class_index.entry(c).or_default().push(id);
        }
    }

    /// Aloca um nó (com seus atributos) como filho de `parent`; devolve o índice.
    fn push(&mut self, kind: NodeKind, attrs: Vec<Attr>, parent: NodeIdx) -> NodeIdx {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            attrs,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.index_node(id);
        self.nodes[parent].children.push(id);
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

    // ── Query (base do querySelector) ───────────────────────────────────────

    /// Primeiro nó que casa com um seletor SIMPLES: `tag` (`"h1"`), `#id`
    /// (`"#alvo"`) ou `.classe` (`".card"`). `None` se nada casar. É o
    /// `querySelector` de um seletor só.
    ///
    /// `#id`/`.classe` usam os índices O(1); `tag` varre em pré-ordem (ordem de
    /// documento). Valida que o hit do índice ainda está vivo (anexado à raiz),
    /// já que mutações podem ter desligado o nó sem limpar o índice.
    pub fn query(&self, selector: &str) -> Option<NodeId> {
        let sel = selector.trim();
        let idx = self.query_idx(sel)?;
        Some(self.make_id(idx))
    }

    /// Núcleo do `query` em índices crus (interno). O `query` público embrulha o
    /// resultado no `NodeId` versionado.
    fn query_idx(&self, sel: &str) -> Option<NodeIdx> {
        // Atalho por ÍNDICE p/ seletores simples PUROS (`#id`/`.classe`) — O(1).
        if let Some(key) = sel.strip_prefix('#') {
            if is_plain_ident(key) {
                return self
                    .id_index
                    .get(key)
                    .copied()
                    .filter(|&i| self.is_attached(i) && self.nodes[i].attr("id") == Some(key));
            }
        }
        if let Some(cls) = sel.strip_prefix('.') {
            if is_plain_ident(cls) {
                return self.class_index.get(cls)?.iter().copied().find(|&i| {
                    self.is_attached(i)
                        && self.nodes[i]
                            .attr("class")
                            .map(|c| c.split_whitespace().any(|x| x == cls))
                            .unwrap_or(false)
                });
            }
        }
        // Caso geral (composto/combinador/atributo/pseudo): pré-ordem + matches.
        self.find_idx_pre_order(self.root, sel)
    }

    /// Pré-ordem buscando o 1º elemento que casa o seletor completo.
    fn find_idx_pre_order(&self, idx: NodeIdx, sel: &str) -> Option<NodeIdx> {
        if idx != self.root && self.matches(idx, sel) {
            return Some(idx);
        }
        for &child in &self.nodes[idx].children {
            if let Some(found) = self.find_idx_pre_order(child, sel) {
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

    fn find_pre_order(&self, idx: NodeIdx, m: &dyn Fn(&Node) -> bool) -> Option<NodeIdx> {
        let node = &self.nodes[idx];
        if idx != self.root && m(node) {
            return Some(idx);
        }
        for &child in &node.children {
            if let Some(hit) = self.find_pre_order(child, m) {
                return Some(hit);
            }
        }
        None
    }

    // ── Mutação (base da API DOM do JS) ─────────────────────────────────────

    /// Substitui TODO o conteúdo de um elemento por um único nó de texto (o
    /// equivalente a `element.textContent = txt`). Não faz nada num nó de texto.
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        self.touch();
        let Some(idx) = self.resolve(id) else { return };
        if !matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            return;
        }
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
    }

    /// Resolve o `ComputedStyle` final de um nó pela cascade da MDN (estágio 1
    /// origem/importância → especificidade → ordem). É o estado que o LAYOUT (em
    /// TS) e o render leem para decidir cor/caixa/tamanho. `None` se o id não
    /// resolve ou não é elemento. (A herança de color/font-size é aplicada por quem
    /// desce a árvore; aqui só o estilo PRÓPRIO do nó.)
    pub fn computed_style(&self, id: NodeId) -> Option<crate::style::ComputedStyle> {
        self.computed_style_idx(self.resolve(id)?)
    }

    /// Igual a [`computed_style`](Dom::computed_style), mas por `NodeIdx` cru — o
    /// render desce a árvore em índices. `None` se o nó não é elemento.
    ///
    /// Aplica a cascade COMPLETA da MDN, em duas passagens (estágio 1: `!important`
    /// inverte a precedência de origem):
    /// - **Normais**, do mais fraco ao mais forte: `defineStyle` (UA) < `<style>`
    ///   autor < `style=""` inline < override por-nó (`setStyleBatch`).
    /// - **Important**, por cima de tudo, na mesma ordem de origem: `<style>`
    ///   important < inline important < override (tratado como mais forte).
    pub fn computed_style_idx(&self, idx: NodeIdx) -> Option<crate::style::ComputedStyle> {
        // MEMO por revisão: dentro de um mesmo estado da árvore, a cascade de um nó
        // é determinística — e o layout a consulta várias vezes por nó (medição +
        // pintura). Um clone do ComputedStyle é muito mais barato que re-rodar
        // todas as regras do stylesheet (Bootstrap: ~2700).
        let rev = self.render_revision();
        let (vw, vh) = self.viewport.get();
        let vp_key = (vw.to_bits(), vh.to_bits());
        if self.memo_revision.get() != rev || self.memo_viewport.get() != vp_key {
            self.computed_memo.borrow_mut().clear();
            self.memo_revision.set(rev);
            self.memo_viewport.set(vp_key);
        }
        if let Some(hit) = self.computed_memo.borrow().get(&idx) {
            return Some(hit.clone());
        }
        // O estilo COM animação = a BASE (cascade sem anim, memoizada por revisão
        // estrutural via `base_style_idx`) + a camada de `anim_override` por cima. Não
        // re-roda a cascade a cada frame de animação: só clona a base cacheada e
        // sobrepõe o override interpolado — o que torna o RELAYOUT durante animação
        // barato (era o gargalo restante depois de acelerar o `advance`).
        let mut computed = self.base_style_idx(idx)?;
        if let Some(anim) = self.anim_override.get(&idx) {
            computed.merge_over(anim);
        }
        self.computed_memo.borrow_mut().insert(idx, computed.clone());
        Some(computed)
    }

    /// Núcleo da cascade — computa o ALVO-BASE de um nó (SEM a camada de animação; o
    /// override interpolado é sobreposto por quem consome, em `computed_style_idx`).
    /// Chamado via `base_style_idx` (memoizado por revisão estrutural).
    fn computed_style_idx_inner(&self, idx: NodeIdx) -> Option<crate::style::ComputedStyle> {
        use crate::style;
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.clone(),
            _ => return None,
        };
        // id/classes do nó — a CHAVE do índice de regras da cascade (só as regras
        // cujo alvo o nó pode satisfazer são testadas, não todas). Materializados em
        // String/Vec para não conflitar com o borrow de `self` nos closures abaixo.
        let node_id: Option<String> = self.nodes[idx].attr("id").map(str::to_string);
        let node_classes: Vec<String> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        let class_refs: Vec<&str> = node_classes.iter().map(String::as_str).collect();
        // `style=""` inline (normal + important + customs/pendentes).
        let inline = self.nodes[idx]
            .attr("style")
            .map(style::parse_inline_block)
            .unwrap_or_default();

        // ── CUSTOM PROPERTIES do elemento (#1779, PASS A): as declarações `--x:`
        // das regras que casam + as do style="" + a HERANÇA do pai (o computed do
        // pai já carrega o mapa dele — CoW: sem declaração própria, compartilha o
        // Arc). Precisam vir ANTES porque os valores com var() dependem delas.
        let parent_css_for_vars = self
            .element_parent_idx(idx)
            .and_then(|p| self.base_style_idx(p));
        let own_customs: Vec<(String, String)> = if self.stylesheet.is_empty() {
            inline.custom.clone()
        } else {
            let mut v = self.stylesheet.custom_for_node(
                self.viewport.get().0,
                &tag,
                node_id.as_deref(),
                &class_refs,
                |sel| self.matches_complex(idx, sel),
            );
            v.extend(inline.custom.iter().cloned());
            v
        };
        let parent_vars = parent_css_for_vars.as_ref().and_then(|p| p.custom_props.clone());
        let vars_arc: Option<std::sync::Arc<std::collections::HashMap<String, String>>> =
            match (parent_vars, own_customs.is_empty()) {
                (p, true) => p, // só herda: compartilha o Arc (O(1))
                (p, false) => {
                    let mut m = p.map(|a| (*a).clone()).unwrap_or_default();
                    // o valor de uma custom pode conter var() de OUTRA — a
                    // substituição recursiva do consumidor resolve; guarda cru.
                    for (k, v) in own_customs {
                        // AUTO-REFERÊNCIA DIRETA (`--c: ...var(--c)...`): a declaração é
                        // guaranteed-invalid (spec) — o Chrome a DESCARTA e mantém a
                        // anterior válida. Se já há um valor para `k` (ex. o oklch real do
                        // bloco de tema) e a nova declaração se auto-referencia, ignora a
                        // nova. Sem valor anterior, insere (o consumidor corta o ciclo).
                        if references_self(&k, &v) && m.contains_key(&k) {
                            continue;
                        }
                        m.insert(k, v);
                    }
                    Some(std::sync::Arc::new(m))
                }
            };
        let empty_vars = std::collections::HashMap::new();
        let vars_ref: &std::collections::HashMap<String, String> =
            vars_arc.as_deref().unwrap_or(&empty_vars);

        // Stylesheet de autor resolvido para este nó (normal + important separados;
        // PASS B — as declarações com var() resolvem na posição da regra, contra
        // as vars acima). O matcher navega a árvore via `matches_complex`.
        let author = if self.stylesheet.is_empty() {
            style::DeclBlock::default()
        } else {
            self.stylesheet.computed_for_node(
                self.viewport.get().0,
                &tag,
                node_id.as_deref(),
                &class_refs,
                Some(vars_ref),
                |sel| self.matches_complex(idx, sel),
            )
        };
        let override_node = self.style_overrides.get(&idx);

        // ── Passe 1: NORMAIS (fraco → forte) ────────────────────────────────────
        let mut css = style::lookup_style(&tag).unwrap_or_default(); // UA/defineStyle
        css.merge_over(&author.normal); // <style> autor
        css.merge_over(&inline.normal); // style="" inline
        for (prop, raw, important) in &inline.pending {
            if !important {
                crate::style::stylesheet::apply_resolved_decl(&mut css, prop, raw, vars_ref);
            }
        }
        if let Some(ov) = override_node {
            css.merge_over(ov); // override por-nó (setStyleBatch)
        }
        // ── Passe 2: IMPORTANT (vencem qualquer normal) ─────────────────────────
        css.merge_over(&author.important); // <style> !important
        css.merge_over(&inline.important); // inline !important
        for (prop, raw, important) in &inline.pending {
            if *important {
                crate::style::stylesheet::apply_resolved_decl(&mut css, prop, raw, vars_ref);
            }
        }
        // o mapa de vars entra no computado (os FILHOS herdam daqui).
        css.custom_props = vars_arc;

        // ── FONT-SIZE resolve CEDO (aqui na cascade, não no layout): a base de
        // `em`/`%` de font-size é o font do PAI (já computado em Px pela recursão
        // abaixo) e `rem`/`vw`/`vh` usam root/viewport — assim a HERANÇA desce
        // sempre o VALOR (Px), nunca a forma (um `2em` herdado re-multiplicaria a
        // cada nível). É o que permite `calc(1.375rem + 1.5vw)` no font-size (a
        // tipografia fluida do h1 do Bootstrap).
        let parent_css = parent_css_for_vars;
        if let Some(d) = css.font_size {
            let parent_font = parent_css
                .as_ref()
                .and_then(|p| match p.font_size {
                    Some(style::Dimension::Px(v)) => Some(v),
                    _ => None,
                })
                .unwrap_or(crate::layout::DEFAULT_FONT_SIZE);
            let (vw, vh) = self.viewport.get();
            let rctx = style::ResolveCtx {
                parent_content_w: parent_font, // `%` de font-size = % do font do PAI
                node_font_size: parent_font,   // `em` de font-size = × font do PAI
                root_font_size: crate::layout::DEFAULT_FONT_SIZE,
                viewport_w: vw,
                viewport_h: vh,
            };
            css.font_size = d.resolve(&rctx).filter(|v| *v > 0.0).map(style::Dimension::Px);
        }

        // ── HERANÇA (CSS inherited properties): color/font/text-align/etc. que NÃO
        // foram declaradas neste nó descem do PAI-elemento. É o que faz o texto pegar
        // a cor do body sem cada elemento redeclarar (sem isto, texto fica preto).
        if let Some(parent_css) = &parent_css {
            css.inherit_from(parent_css);
        }

        // A camada de ANIMAÇÃO (o `anim_override` interpolado) NÃO entra aqui: este é
        // o ALVO-BASE. `computed_style_idx` a sobrepõe sobre a base memoizada — assim a
        // cascade (cara) roda só quando a ESTRUTURA muda, não a cada frame de animação.
        Some(css)
    }

    /// O pai de `idx` SE for um elemento (não o #document) — para a herança subir só
    /// pela cadeia de elementos.
    fn element_parent_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let p = self.nodes[idx].parent?;
        matches!(self.nodes[p].kind, NodeKind::Element { .. }).then_some(p)
    }

    /// `getComputedStyle(el).<name>` — o valor COMPUTADO (após a cascade completa)
    /// de uma propriedade CSS por nome, no formato do browser. `""` se não definida
    /// ou o nó não é elemento. (#1759)
    pub fn computed_property(&self, id: NodeId, name: &str) -> String {
        self.computed_style(id).map(|c| c.get_property(name)).unwrap_or_default()
    }

    /// `el.style.<name>` (getPropertyValue) — o valor INLINE da propriedade (só o
    /// `style=""`, sem a cascade), no formato do browser. `""` se ausente.
    pub fn inline_property(&self, id: NodeId, name: &str) -> String {
        let Some(idx) = self.resolve(id) else { return String::new() };
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
        self.touch();
        self.set_attr(id, "style", text);
    }

    /// `el.style.setProperty(name, value)` — define UMA propriedade no `style=""`
    /// inline, preservando as demais. Re-serializa a string `style`. Valor vazio
    /// REMOVE a propriedade (como `removeProperty`).
    pub fn set_style_property(&mut self, id: NodeId, name: &str, value: &str) {
        self.touch();
        let cur = self.css_text(id);
        let new = upsert_css_decl(&cur, name.trim(), value.trim());
        self.set_attr(id, "style", &new);
    }

    /// `el.style.removeProperty(name)` — remove a propriedade do `style=""`.
    pub fn remove_style_property(&mut self, id: NodeId, name: &str) {
        self.touch();
        let cur = self.css_text(id);
        let new = upsert_css_decl(&cur, name.trim(), ""); // valor vazio = remover
        self.set_attr(id, "style", &new);
    }

    // ── Eventos (#1760) — modelo de polling + bubbling headless ──────────────────
    // O motor não guarda callbacks de fn de forma confiável (limite #195), então o
    // Rust registra só QUE TIPO cada nó escuta; os callbacks vivem no TS. O
    // `dispatchEvent` enfileira (nó, tipo) já expandido pelo BUBBLING (alvo → pais
    // que escutam), e o loop TS consome via `poll_event` e chama o handler certo.

    /// `element.addEventListener(type, handler)`: registra que o nó escuta `type`.
    /// (O handler real é guardado no lado TS, indexado por (nó, tipo).) Idempotente:
    /// não duplica o mesmo tipo. O tipo é CASE-SENSITIVE (spec DOM: `click`≠`CLICK`).
    pub fn add_event_listener(&mut self, id: NodeId, event_type: &str) {
        let Some(idx) = self.resolve(id) else { return };
        let types = self.listeners.entry(idx).or_default();
        let t = event_type.to_string();
        if !types.contains(&t) {
            types.push(t);
        }
    }

    /// `element.addEventListener(type, fn)` com CALLBACK: registra o tipo (como
    /// acima) e guarda o word/handle i64 da Function, opaco. Duplicatas do MESMO
    /// callback são ignoradas (spec DOM: registrar o mesmo par duas vezes é no-op).
    pub fn add_event_listener_cb(&mut self, id: NodeId, event_type: &str, cb: i64) {
        let Some(idx) = self.resolve(id) else { return };
        self.add_event_listener(id, event_type);
        let cbs = self
            .listener_cbs
            .entry((idx, event_type.to_string()))
            .or_default();
        if !cbs.contains(&cb) {
            cbs.push(cb);
        }
    }

    /// `element.removeEventListener(type)`: para de escutar `type` neste nó.
    /// (Remove também os callbacks registrados do tipo.)
    pub fn remove_event_listener(&mut self, id: NodeId, event_type: &str) {
        let Some(idx) = self.resolve(id) else { return };
        if let Some(types) = self.listeners.get_mut(&idx) {
            types.retain(|x| x != event_type);
        }
        self.listener_cbs.remove(&(idx, event_type.to_string()));
    }

    /// `true` se o nó escuta o tipo de evento dado (case-sensitive).
    pub fn has_listener(&self, id: NodeId, event_type: &str) -> bool {
        let Some(idx) = self.resolve(id) else { return false };
        self.listeners.get(&idx).map(|v| v.iter().any(|x| x == event_type)).unwrap_or(false)
    }

    /// `element.dispatchEvent(type, bubbles)`: dispara um evento no nó-alvo. Sempre
    /// notifica o ALVO; se `bubbles`, sobe pelos ancestrais que escutam o tipo (fiel
    /// ao DOM: `focus`/`blur`/`new Event(t)` não borbulham). Para cada nó na cadeia
    /// que escuta, enfileira `(nó, tipo)` para o loop TS via `poll_event`. Devolve
    /// quantos listeners foram enfileirados. Tipo CASE-SENSITIVE.
    pub fn dispatch_event(&mut self, target: NodeId, event_type: &str, bubbles: bool) -> i64 {
        let mut count = 0;
        let mut cur = Some(target);
        let mut first = true;
        while let Some(node) = cur {
            let Some(idx) = self.resolve(node) else { break };
            if self.listeners.get(&idx).map(|v| v.iter().any(|x| x == event_type)).unwrap_or(false) {
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
        self.event_queue.pop_front().map(|(idx, t)| (self.make_id(idx), t))
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
        let mut cur = Some(target);
        let mut first = true;
        while let Some(node) = cur {
            let Some(idx) = self.resolve(node) else { break };
            if let Some(cbs) = self.listener_cbs.get(&(idx, event_type.to_string())) {
                for &cb in cbs {
                    self.last_dispatch.push((idx, cb));
                }
            }
            if !bubbles && first {
                break;
            }
            first = false;
            cur = self.parent_of(node);
        }
        // Mantém o contrato do polling também (contadores/fila do modelo #1760):
        // um app antigo que só usa pumpEvents continua vendo o evento.
        self.dispatch_event(target, event_type, bubbles);
        self.last_dispatch.len() as i64
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

    // ── Animação (#1776) — o LOOP INTERNO ao DOM ─────────────────────────────────

    /// `advance(now_ms)` — avança TODAS as animações para o instante `now_ms` (ms do
    /// relógio do backend). É o LOOP INTERNO: o `Dom` é dono do tempo; o egui só
    /// chama isto ao pedir o render, passando o tempo do frame, e continua BURRO.
    ///
    /// Para cada elemento: computa o estilo-ALVO base (sem animação); se mudou desde
    /// o frame anterior E o nó tem `transition`, INICIA uma transição (captura o
    /// estilo anterior como `from`); grava o estilo interpolado em `anim_override`
    /// (a camada que o layout/render vê). Transições terminadas são removidas.
    /// Devolve `true` se há QUALQUER animação ativa (o backend deve continuar
    /// repintando — pedir o próximo frame).
    pub fn advance(&mut self, now_ms: f32) -> bool {
        // todos os elementos da árvore (a animação só vale p/ elementos).
        let mut elements = Vec::new();
        self.collect_all_element_idxs(self.root, &mut elements);

        let mut any_active = false;
        // `changed` = o estilo VISÍVEL de algum nó mudou neste tick (override
        // inserido OU removido — remover também muda o render: do interpolado
        // para o alvo final). Dirige o `touch()` que invalida os caches de layout;
        // `any_active` sozinho não cobre o frame em que a animação TERMINA.
        let mut changed = false;
        for idx in elements {
            // o ALVO base deste frame (cascade sem a camada de animação) — MEMOIZADO
            // por revisão estrutural, então entre frames de animação é hit de cache.
            let Some(target) = self.base_style_idx(idx) else { continue };

            // ── @keyframes ANIMATION (#1776 fase 2): roda sozinha no tempo ──────────
            if let Some(anim) = &target.animation {
                // tempo de início: novo nó/nome reinicia; mesmo nome mantém.
                let start = match self.anim_start.get(&idx) {
                    Some((n, s)) if *n == anim.name => *s,
                    _ => {
                        self.anim_start.insert(idx, (anim.name.clone(), now_ms));
                        now_ms
                    }
                };
                match anim.progress(now_ms - start) {
                    Some(t) => {
                        // acha os @keyframes do nome e interpola sobre o estilo base.
                        if let Some(kf) = self.stylesheet.keyframes(&anim.name) {
                            let styled = kf.at(t, &target);
                            self.anim_override.insert(idx, styled);
                            any_active = true;
                            changed = true;
                        }
                    }
                    None => {
                        // animação terminou (iterações esgotadas) → fica no estado final.
                        if self.anim_override.remove(&idx).is_some() {
                            changed = true;
                        }
                    }
                }
                self.prev_computed.insert(idx, target.clone());
                continue; // animation tem prioridade sobre transition neste nó
            } else {
                self.anim_start.remove(&idx);
            }

            // ── TRANSITION (fase 1): anima mudanças de estilo ───────────────────────
            let prev = self.prev_computed.get(&idx).cloned();
            if let (Some(prev_style), Some(spec)) = (&prev, target.transition) {
                if prev_style.differs_animated(&target) {
                    let from = self
                        .anim_override
                        .get(&idx)
                        .cloned()
                        .unwrap_or_else(|| prev_style.clone());
                    self.active_transitions.insert(
                        idx,
                        crate::anim::ActiveTransition { from, start_ms: now_ms, spec },
                    );
                }
            }
            self.prev_computed.insert(idx, target.clone());

            if let Some(active) = self.active_transitions.get(&idx).cloned() {
                let interp = active.current(&target, now_ms);
                self.anim_override.insert(idx, interp);
                changed = true;
                if active.done(now_ms) {
                    self.active_transitions.remove(&idx);
                    self.anim_override.remove(&idx);
                } else {
                    any_active = true;
                }
            }
        }
        if changed {
            // o estilo visível mudou neste tick → invalida os caches de LAYOUT (o
            // layout re-pinta a interpolação). Usa `touch_anim` (só `anim_epoch`), NÃO
            // `touch()`: a ESTRUTURA/cascade-base não mudou, então o `base_memo`
            // sobrevive e o próximo `advance` não re-roda a cascade de todos os nós.
            self.touch_anim();
        }
        any_active
    }

    /// Coleta os NodeIdx de todos os ELEMENTOS da árvore (pré-ordem).
    fn collect_all_element_idxs(&self, idx: NodeIdx, out: &mut Vec<NodeIdx>) {
        if idx != self.root && matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            out.push(idx);
        }
        for &child in &self.nodes[idx].children {
            self.collect_all_element_idxs(child, out);
        }
    }

    /// `true` se a tag do nó é texto-cru não-renderável (`<style>`/`<script>`): o
    /// render deve PULAR (o conteúdo é CSS/JS, não conteúdo de página). O CSS já foi
    /// absorvido pelo stylesheet no parse.
    pub fn is_raw_text_element(&self, idx: NodeIdx) -> bool {
        matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "style" || tag == "script")
    }

    /// Aplica UM slot de estilo OPACO (invariante 4) a UM nó, acumulando no
    /// override por-nó (`setStyle` por-nó / base do `setStyleBatch`). O `(slot,
    /// val)` é interpretado pelo `apply_slot` do `ComputedStyle` (nunca casa string
    /// CSS aqui). Ignora id que não resolve.
    pub fn set_node_style_slot(&mut self, id: NodeId, slot: i64, val: i64) {
        self.touch();
        if let Some(idx) = self.resolve(id) {
            self.style_overrides.entry(idx).or_default().apply_slot(slot, val);
        }
    }

    /// Aplica um LOTE de triplas `(nodeId, slot, val)` de uma vez (invariante 6:
    /// estilizar N nós por frame não pode ser N×5 FFIs). Cada tripla acumula no
    /// override do seu nó. O `nodes` é uma fatia plana `[id0, slot0, val0, id1,
    /// slot1, val1, …]` (o jeito que o buffer GC chega da ABI). Triplas com id
    /// inválido são ignoradas (robustez).
    pub fn apply_style_batch(&mut self, triples: &[i64]) {
        self.touch();
        for t in triples.chunks_exact(3) {
            if let Some(node) = NodeId::from_abi(t[0]) {
                self.set_node_style_slot(node, t[1], t[2]);
            }
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
        let Some(idx) = self.resolve(id) else { return -1 };
        match &self.nodes[idx].kind {
            NodeKind::Element { tag } => {
                crate::block::lookup(tag).map(|d| d.display).unwrap_or(-1)
            }
            _ => -1,
        }
    }

    /// Concatena o texto de TODOS os descendentes de `id`, em ordem de documento
    /// (`element.textContent` getter). `None` se o id não resolve nesta árvore.
    /// Num nó de texto, retorna o próprio texto.
    pub fn text_content(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        let mut out = String::new();
        self.collect_text_into(idx, &mut out);
        Some(out)
    }

    fn collect_text_into(&self, idx: NodeIdx, out: &mut String) {
        match &self.nodes[idx].kind {
            NodeKind::Text(t) => out.push_str(t),
            _ => {
                for &child in &self.nodes[idx].children {
                    self.collect_text_into(child, out);
                }
            }
        }
    }

    /// Nome da tag de um elemento em minúsculas (`element.tagName`, mas o browser
    /// devolve em CAIXA ALTA para HTML — a fachada TS faz o upper). `None` se não
    /// resolve ou não é elemento.
    pub fn tag_name(&self, id: NodeId) -> Option<&str> {
        let idx = self.resolve(id)?;
        match &self.nodes[idx].kind {
            NodeKind::Element { tag } => Some(tag.as_str()),
            _ => None,
        }
    }

    /// Valor de um atributo (`element.getAttribute`). `None` se o id não resolve
    /// ou o atributo não existe.
    pub fn get_attr(&self, id: NodeId, name: &str) -> Option<&str> {
        let idx = self.resolve(id)?;
        self.nodes[idx].attr(name)
    }

    /// Os filhos ELEMENTO de um nó (`element.children` — exclui nós de texto), em
    /// ordem. Vazio se o id não resolve.
    pub fn child_elements(&self, id: NodeId) -> Vec<NodeId> {
        let Some(idx) = self.resolve(id) else { return Vec::new() };
        self.nodes[idx]
            .children
            .iter()
            .filter(|&&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
            .map(|&c| self.make_id(c))
            .collect()
    }

    /// TODOS os filhos de um nó (`node.childNodes` — inclui nós de TEXTO), em
    /// ordem de documento. Vazio se o id não resolve. (`child_elements` filtra só
    /// elementos; este é o `childNodes` cru do DOM.)
    pub fn child_nodes(&self, id: NodeId) -> Vec<NodeId> {
        let Some(idx) = self.resolve(id) else { return Vec::new() };
        self.nodes[idx].children.iter().map(|&c| self.make_id(c)).collect()
    }

    // ── Traversal POR ELEMENTO (pula nós de texto/comentário) — #1757 ────────────
    // `*ElementChild`/`*ElementSibling`/`parentElement` são as variantes "só
    // elemento" das de cima. O JS usa muito mais estas (ignora whitespace/texto).

    /// `element.firstElementChild`: o 1º filho que é ELEMENTO (pula Text/Comment).
    pub fn first_element_child(&self, id: NodeId) -> Option<NodeId> {
        self.child_elements(id).first().copied()
    }

    /// `element.lastElementChild`: o último filho-elemento.
    pub fn last_element_child(&self, id: NodeId) -> Option<NodeId> {
        self.child_elements(id).last().copied()
    }

    /// `element.nextElementSibling`: o próximo irmão que é ELEMENTO (pula texto).
    pub fn next_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.element_sibling(id, 1)
    }

    /// `element.previousElementSibling`: o irmão-elemento anterior.
    pub fn previous_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.element_sibling(id, -1)
    }

    /// Caminha irmão-a-irmão na direção `delta` até achar um ELEMENTO (ou acabar).
    fn element_sibling(&self, id: NodeId, delta: isize) -> Option<NodeId> {
        let mut cur = id;
        loop {
            cur = self.sibling(cur, delta)?;
            let idx = self.resolve(cur)?;
            if matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
                return Some(cur);
            }
        }
    }

    /// `element.parentElement`: o pai SE for um elemento; `None` se o pai é o
    /// `#document` (a raiz não é um elemento) ou não há pai.
    pub fn parent_element(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.parent_of(id)?;
        let pidx = self.resolve(parent)?;
        matches!(self.nodes[pidx].kind, NodeKind::Element { .. }).then_some(parent)
    }

    /// `element.matches(sel)`: o nó casa o seletor SIMPLES (tag/`#id`/`.classe`)?
    /// Reusa o matcher de `querySelector` (mesma sintaxe). Combinadores → #1752.
    pub fn matches_selector(&self, id: NodeId, sel: &str) -> bool {
        self.resolve(id).map(|i| self.matches(i, sel.trim())).unwrap_or(false)
    }

    /// `element.closest(sel)`: sobe pela cadeia de ancestrais (incluindo o próprio
    /// nó) e devolve o PRIMEIRO que casa o seletor; `None` se nenhum casa.
    pub fn closest(&self, id: NodeId, sel: &str) -> Option<NodeId> {
        let sel = sel.trim();
        let mut cur = Some(id);
        while let Some(node) = cur {
            let idx = self.resolve(node)?;
            if matches!(self.nodes[idx].kind, NodeKind::Element { .. }) && self.matches(idx, sel) {
                return Some(node);
            }
            cur = self.parent_of(node);
        }
        None
    }

    // ── Query por subárvore + getElementsBy* — #1758 ─────────────────────────────

    /// `element.querySelector(sel)`: o 1º descendente do nó que casa o seletor
    /// (busca SÓ na subárvore, não na árvore toda). `None` se nenhum casa.
    pub fn query_within(&self, root: NodeId, selector: &str) -> Option<NodeId> {
        self.query_all_within(root, selector).into_iter().next()
    }

    /// `element.querySelectorAll(sel)` restrito à subárvore do nó (exclui o próprio).
    pub fn query_all_within(&self, root: NodeId, selector: &str) -> Vec<NodeId> {
        let sel = selector.trim();
        let Some(root_idx) = self.resolve(root) else { return Vec::new() };
        let mut out = Vec::new();
        // só os DESCENDENTES (o próprio nó não casa a si mesmo no querySelector).
        for &child in &self.nodes[root_idx].children {
            self.query_all_into(child, sel, &mut out);
        }
        out
    }

    /// `getElementsByTagName(tag)`: todos os descendentes da árvore com a tag.
    /// (`"*"` casa qualquer elemento.) Reusa o matcher de `query_all`.
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<NodeId> {
        let tag = tag.trim();
        if tag == "*" {
            // todos os elementos em ordem de documento.
            let mut out = Vec::new();
            self.collect_all_elements(self.root, &mut out);
            return out;
        }
        self.query_all(tag)
    }

    fn collect_all_elements(&self, idx: NodeIdx, out: &mut Vec<NodeId>) {
        if idx != self.root && matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            out.push(self.make_id(idx));
        }
        for &child in &self.nodes[idx].children {
            self.collect_all_elements(child, out);
        }
    }

    /// `getElementsByClassName(names)`: todos os elementos que têm TODAS as classes
    /// dadas (separadas por espaço — semântica AND da MDN). Um único token reusa o
    /// caminho de `.classe`; múltiplos filtram por interseção.
    pub fn get_elements_by_class_name(&self, names: &str) -> Vec<NodeId> {
        let wanted: Vec<&str> = names.split_whitespace().collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        // varre todos os elementos, mantendo os que têm TODAS as classes pedidas.
        let mut out = Vec::new();
        self.collect_by_classes(self.root, &wanted, &mut out);
        out
    }

    fn collect_by_classes(&self, idx: NodeIdx, wanted: &[&str], out: &mut Vec<NodeId>) {
        if idx != self.root {
            if let Some(class_attr) = self.nodes[idx].attr("class") {
                let have: Vec<&str> = class_attr.split_whitespace().collect();
                if wanted.iter().all(|w| have.contains(w)) {
                    out.push(self.make_id(idx));
                }
            }
        }
        for &child in &self.nodes[idx].children {
            self.collect_by_classes(child, wanted, out);
        }
    }

    /// `getElementsByName(name)`: todos os elementos cujo atributo `name` é igual.
    /// Nome vazio → lista vazia (consistente com getElementsByClassName).
    pub fn get_elements_by_name(&self, name: &str) -> Vec<NodeId> {
        if name.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.collect_by_name(self.root, name, &mut out);
        out
    }

    fn collect_by_name(&self, idx: NodeIdx, name: &str, out: &mut Vec<NodeId>) {
        if idx != self.root && self.nodes[idx].attr("name") == Some(name) {
            out.push(self.make_id(idx));
        }
        for &child in &self.nodes[idx].children {
            self.collect_by_name(child, name, out);
        }
    }

    // ── Mutação rica — #1756 ─────────────────────────────────────────────────────

    /// `node.cloneNode(deep)`: duplica o nó. `deep=false` clona só o nó (sem
    /// filhos); `deep=true` clona a subárvore inteira. O clone é SOLTO (sem pai) —
    /// anexe-o com appendChild/insertBefore. Devolve o `NodeId` do clone.
    pub fn clone_node(&mut self, id: NodeId, deep: bool) -> Option<NodeId> {
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
        let Some(parent) = self.parent_of(node) else { return };
        let reference = if after { self.next_sibling(node) } else { Some(node) };
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
        let Some(parent) = self.parent_of(node) else { return };
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
        let Some(idx) = self.resolve(parent) else { return };
        let children: Vec<NodeIdx> = self.nodes[idx].children.clone();
        for c in children {
            self.detach(c);
        }
    }

    // ── Node utils — #1762 ───────────────────────────────────────────────────────

    /// `node.contains(other)`: `other` é o próprio nó OU um descendente dele?
    /// (Reusa a guarda de ciclo `is_ancestor`, que é exatamente esta relação.)
    pub fn contains(&self, node: NodeId, other: NodeId) -> bool {
        let (Some(a), Some(b)) = (self.resolve(node), self.resolve(other)) else { return false };
        a == b || self.is_ancestor(a, b)
    }

    /// `node.hasChildNodes()`: tem ao menos um filho (de qualquer tipo)?
    pub fn has_child_nodes(&self, id: NodeId) -> bool {
        self.resolve(id).map(|i| !self.nodes[i].children.is_empty()).unwrap_or(false)
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
        match &mut self.nodes[idx].kind {
            NodeKind::Text(t) | NodeKind::Comment(t) => *t = value.to_string(),
            _ => {}
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
        self.touch();
        let Some(idx) = self.resolve(id) else { return };
        let name_lc = name.to_ascii_lowercase();
        self.nodes[idx].attrs.retain(|a| a.name != name_lc);
        // limpa o índice correspondente (entradas stale são toleradas, mas
        // remover ajuda a manter o índice enxuto).
        match name_lc.as_str() {
            "id" => self.id_index.retain(|_, &mut v| v != idx),
            "class" => {
                for v in self.class_index.values_mut() {
                    v.retain(|&x| x != idx);
                }
            }
            _ => {}
        }
    }

    /// `element.hasAttribute(name)`: o atributo ESTÁ PRESENTE (mesmo com valor
    /// vazio — `hidden`/`disabled` são booleanos com valor `""`)? Checa a presença
    /// na lista, não o valor (o `getAttribute("").length>0` da fachada errava aqui).
    pub fn has_attr(&self, id: NodeId, name: &str) -> bool {
        let Some(idx) = self.resolve(id) else { return false };
        let name_lc = name.to_ascii_lowercase();
        self.nodes[idx].attrs.iter().any(|a| a.name == name_lc)
    }

    /// `element.getAttributeNames()`: os nomes dos atributos, em ordem do HTML.
    pub fn attr_names(&self, id: NodeId) -> Vec<String> {
        let Some(idx) = self.resolve(id) else { return Vec::new() };
        self.nodes[idx].attrs.iter().map(|a| a.name.clone()).collect()
    }

    /// Valor do atributo N-ésimo (para `attributes`), por índice. `None` fora do range.
    pub fn attr_value_at(&self, id: NodeId, i: usize) -> Option<String> {
        let idx = self.resolve(id)?;
        self.nodes[idx].attrs.get(i).map(|a| a.value.clone())
    }

    // ── Navegação do DOM (parentNode / first|lastChild / next|previousSibling) ───
    // O `parent`/`children` da arena já têm tudo; aqui só expomos no vocabulário do
    // DOM. `None`/`-1` na fronteira ABI quando não há (raiz não tem pai; primeiro
    // filho não tem irmão anterior; etc.).

    /// `node.parentNode`: o pai, ou `None` para a raiz `#document` (ou id inválido).
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        self.nodes[idx].parent.map(|p| self.make_id(p))
    }

    /// `node.firstChild`: o PRIMEIRO filho (qualquer tipo, inclui Text), ou `None`.
    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        self.nodes[idx].children.first().map(|&c| self.make_id(c))
    }

    /// `node.lastChild`: o ÚLTIMO filho (qualquer tipo), ou `None`.
    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        self.nodes[idx].children.last().map(|&c| self.make_id(c))
    }

    /// `node.nextSibling`: o próximo irmão na lista de filhos do pai, ou `None` se
    /// é o último (ou não tem pai / id inválido).
    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.sibling(id, 1)
    }

    /// `node.previousSibling`: o irmão anterior, ou `None` se é o primeiro.
    pub fn previous_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.sibling(id, -1)
    }

    /// Irmão a `delta` posições (`+1` próximo, `-1` anterior). Acha a posição do nó
    /// na lista de filhos do pai e desloca; `None` se sai dos limites.
    fn sibling(&self, id: NodeId, delta: isize) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        let parent = self.nodes[idx].parent?;
        let sibs = &self.nodes[parent].children;
        let pos = sibs.iter().position(|&c| c == idx)?;
        let target = pos as isize + delta;
        if target < 0 || target as usize >= sibs.len() {
            return None;
        }
        Some(self.make_id(sibs[target as usize]))
    }

    /// Todos os nós que casam um seletor simples (`querySelectorAll`), em ordem de
    /// documento. `tag` varre pré-ordem; `#id`/`.classe` usam os índices.
    pub fn query_all(&self, selector: &str) -> Vec<NodeId> {
        let sel = selector.trim();
        let mut out = Vec::new();
        self.query_all_into(self.root, sel, &mut out);
        out
    }

    fn query_all_into(&self, idx: NodeIdx, sel: &str, out: &mut Vec<NodeId>) {
        if idx != self.root && self.matches(idx, sel) {
            out.push(self.make_id(idx));
        }
        for &child in &self.nodes[idx].children {
            self.query_all_into(child, sel, out);
        }
    }

    /// `true` se o nó `idx` casa o seletor `sel` (string). Aceita uma LISTA separada
    /// por vírgula (`p, a` casa se QUALQUER um casar). Cada item é um seletor
    /// COMPLEXO (compostos + combinadores + atributo + pseudo). Item inválido é
    /// ignorado; lista toda inválida → false. (#1752)
    fn matches(&self, idx: NodeIdx, sel: &str) -> bool {
        crate::style::parse_selector_list(sel)
            .iter()
            .any(|complex| self.matches_complex(idx, complex))
    }

    /// Casa um [`ComplexSelector`] contra o nó `idx`, navegando a árvore para os
    /// combinadores. O ÚLTIMO compound casa `idx`; os anteriores casam ancestrais/
    /// irmãos conforme o combinador (matching da direita p/ a esquerda).
    fn matches_complex(&self, idx: NodeIdx, sel: &crate::style::ComplexSelector) -> bool {
        let n = sel.compounds.len();
        if !self.compound_matches_idx(idx, &sel.compounds[n - 1]) {
            return false;
        }
        if n == 1 {
            return true;
        }
        self.match_combinators(idx, sel, n - 1)
    }

    /// Tenta casar os compounds [0..=i-1] contra o contexto (ancestrais/irmãos) de
    /// `idx`, dado que `compounds[i]` já casou `idx`. Backtracking p/ descendente e
    /// irmão-geral (que têm múltiplos candidatos).
    fn match_combinators(&self, idx: NodeIdx, sel: &crate::style::ComplexSelector, i: usize) -> bool {
        if i == 0 {
            return true;
        }
        let combinator = sel.combinators[i - 1];
        let prev = &sel.compounds[i - 1];
        use crate::style::Combinator;
        match combinator {
            Combinator::Child => match self.parent_element_idx(idx) {
                Some(p) if self.compound_matches_idx(p, prev) => self.match_combinators(p, sel, i - 1),
                _ => false,
            },
            Combinator::Descendant => {
                let mut cur = self.parent_element_idx(idx);
                while let Some(a) = cur {
                    if self.compound_matches_idx(a, prev) && self.match_combinators(a, sel, i - 1) {
                        return true;
                    }
                    cur = self.parent_element_idx(a);
                }
                false
            }
            Combinator::NextSibling => match self.prev_element_sibling_idx(idx) {
                Some(s) if self.compound_matches_idx(s, prev) => self.match_combinators(s, sel, i - 1),
                _ => false,
            },
            Combinator::SubsequentSibling => {
                let mut cur = self.prev_element_sibling_idx(idx);
                while let Some(s) = cur {
                    if self.compound_matches_idx(s, prev) && self.match_combinators(s, sel, i - 1) {
                        return true;
                    }
                    cur = self.prev_element_sibling_idx(s);
                }
                false
            }
        }
    }

    /// `true` se o COMPOUND casa o elemento `idx` (tag/id/classe/atributo/pseudo).
    fn compound_matches_idx(&self, idx: NodeIdx, compound: &crate::style::CompoundSelector) -> bool {
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.as_str(),
            _ => return false,
        };
        let id = self.nodes[idx].attr("id");
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let attr = |name: &str| self.nodes[idx].attr(name).map(str::to_string);
        let pseudo = |pc: &crate::style::PseudoClass| self.pseudo_matches(idx, pc);
        crate::style::compound_matches(compound, tag, id, &classes, &attr, &pseudo)
    }

    /// Resolve uma pseudo-classe contra o nó (posição entre irmãos / atributo de estado).
    fn pseudo_matches(&self, idx: NodeIdx, pc: &crate::style::PseudoClass) -> bool {
        use crate::style::PseudoClass as P;
        match pc {
            // `:root` = o elemento raiz do documento (o `<html>`). Num DOM headless de
            // FRAGMENTO (sem <html>), casa só se for o ÚNICO elemento top-level — senão
            // 0 (fiel ao browser, que tem exatamente 1 root).
            P::Root => {
                self.nodes[idx].parent == Some(self.root)
                    && self.nodes[self.root]
                        .children
                        .iter()
                        .filter(|&&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
                        .count()
                        == 1
            }
            P::Empty => !self.nodes[idx].children.iter().any(|&c| {
                matches!(self.nodes[c].kind, NodeKind::Element { .. })
                    || matches!(&self.nodes[c].kind, NodeKind::Text(t) if !t.trim().is_empty())
            }),
            P::FirstChild => self.element_index_among_siblings(idx) == Some(0),
            P::LastChild => self.element_siblings(idx).last() == Some(&idx),
            P::OnlyChild => self.element_siblings(idx).len() == 1,
            P::NthChild(a, b) => match self.element_index_among_siblings(idx) {
                Some(zero_based) => {
                    let n = zero_based as i32 + 1; // 1-based
                    if *a == 0 {
                        n == *b
                    } else {
                        let k = (n - b) / a;
                        k >= 0 && a * k + b == n
                    }
                }
                None => false,
            },
            // estado → presença de atributo (DOM headless, sem UI viva).
            P::Checked => {
                self.nodes[idx].attr("checked").is_some() || self.nodes[idx].attr("selected").is_some()
            }
            P::Disabled => self.nodes[idx].attr("disabled").is_some(),
            P::Required => self.nodes[idx].attr("required").is_some(),
            P::Enabled => {
                let is_form = matches!(&self.nodes[idx].kind,
                    NodeKind::Element { tag } if matches!(tag.as_str(),
                        "input" | "button" | "select" | "textarea" | "option" | "fieldset"));
                is_form && self.nodes[idx].attr("disabled").is_none()
            }
        }
    }

    /// Os irmãos-ELEMENTO de `idx` (incluindo ele), em ordem.
    fn element_siblings(&self, idx: NodeIdx) -> Vec<NodeIdx> {
        let Some(parent) = self.nodes[idx].parent else { return vec![idx] };
        self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
            .collect()
    }

    /// Índice (0-based) de `idx` entre seus irmãos-elemento, ou `None`.
    fn element_index_among_siblings(&self, idx: NodeIdx) -> Option<usize> {
        self.element_siblings(idx).iter().position(|&c| c == idx)
    }

    /// O pai de `idx` SE for elemento (não o #document), em índice cru.
    fn parent_element_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let p = self.nodes[idx].parent?;
        matches!(self.nodes[p].kind, NodeKind::Element { .. }).then_some(p)
    }

    /// O irmão-elemento imediatamente anterior a `idx`, em índice cru.
    fn prev_element_sibling_idx(&self, idx: NodeIdx) -> Option<NodeIdx> {
        let sibs = self.element_siblings(idx);
        let pos = sibs.iter().position(|&c| c == idx)?;
        (pos > 0).then(|| sibs[pos - 1])
    }

    /// Define/atualiza um atributo (`element.setAttribute`). Cria se não existir.
    /// Mantém os índices `id`/`class` em dia (adiciona a nova entrada; entradas
    /// antigas viram stale mas a busca valida alcançabilidade/valor).
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        self.touch();
        let Some(idx) = self.resolve(id) else { return };
        let name_lc = name.to_ascii_lowercase();
        let node = &mut self.nodes[idx];
        if let Some(a) = node.attrs.iter_mut().find(|a| a.name == name_lc) {
            a.value = value.to_string();
        } else {
            node.attrs.push(Attr { name: name_lc.clone(), value: value.to_string() });
        }
        // Atualiza índices se o atributo afeta busca.
        match name_lc.as_str() {
            "id" => {
                self.id_index.insert(value.to_string(), idx);
            }
            "class" => {
                for c in value.split_whitespace() {
                    let v = self.class_index.entry(c.to_string()).or_default();
                    if !v.contains(&idx) {
                        v.push(idx);
                    }
                }
            }
            _ => {}
        }
    }

    /// Cria um elemento SOLTO (sem pai) e devolve seu `NodeId` versionado; ligue-o
    /// com `append_child` (`document.createElement`).
    pub fn create_element(&mut self, tag: &str) -> NodeId {
        let idx = self.push_detached(NodeKind::Element { tag: tag.to_ascii_lowercase() });
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
        self.touch();
        let (Some(parent), Some(child)) = (self.resolve(parent), self.resolve(child)) else {
            return;
        };
        if parent == child || self.is_ancestor(child, parent) {
            return;
        }
        // ref==child é no-op (inserir antes de si mesmo mantém a posição). A spec do
        // DOM trata referenceNode==node como manter no lugar.
        let ref_idx = reference.and_then(|r| self.resolve(r));
        if ref_idx == Some(child) {
            // já garante o parent (caso o nó fosse solto) e mantém a ordem.
            if self.nodes[child].parent != Some(parent) {
                self.detach(child);
                self.nodes[child].parent = Some(parent);
                self.nodes[parent].children.push(child);
            }
            return;
        }
        // captura a posição da referência ANTES do detach (o detach pode mexer na
        // lista de filhos do pai se o child já era irmão da referência).
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        let pos = ref_idx
            .and_then(|r| self.nodes[parent].children.iter().position(|&c| c == r))
            .unwrap_or(self.nodes[parent].children.len());
        self.nodes[parent].children.insert(pos, child);
    }

    /// `node.nodeType` — código numérico do DOM: Element=1, Text=3, Comment=8,
    /// Document=9. `-1` se o id não resolve.
    pub fn node_type(&self, id: NodeId) -> i64 {
        let Some(idx) = self.resolve(id) else { return -1 };
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
        self.touch();
        let (Some(parent), Some(child)) = (self.resolve(parent), self.resolve(child)) else {
            return;
        };
        if parent == child {
            return;
        }
        if self.is_ancestor(child, parent) {
            return; // evita criar ciclo (child seria ancestral de parent)
        }
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    /// Desliga um nó do pai (`element.remove`). O nó continua na arena (lixo).
    pub fn remove_node(&mut self, id: NodeId) {
        self.touch();
        if let Some(idx) = self.resolve(id) {
            if idx != self.root {
                self.detach(idx);
            }
        }
    }

    /// Aloca um nó sem pai (usado por create_element / set_text). Índice cru.
    fn push_detached(&mut self, kind: NodeKind) -> NodeIdx {
        let id = self.nodes.len();
        self.nodes.push(Node { kind, attrs: Vec::new(), parent: None, children: Vec::new() });
        id
    }

    /// Remove `idx` da lista de filhos do seu pai atual (se houver).
    fn detach(&mut self, idx: NodeIdx) {
        if let Some(p) = self.nodes[idx].parent.take() {
            self.nodes[p].children.retain(|&c| c != idx);
        }
    }

    /// `true` se `a` é ancestral de (ou igual a) `b` — guarda contra ciclos.
    fn is_ancestor(&self, a: NodeIdx, b: NodeIdx) -> bool {
        let mut cur = Some(b);
        while let Some(c) = cur {
            if c == a {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }

    /// `element.innerHTML` (GET) — serializa os FILHOS do nó como string HTML
    /// válida (o inverso do parser: `<tag attrs>filhos</tag>`, texto com entidades
    /// re-encodadas, `<!-- -->` para comentário, void tags sem fechar). `None` se o
    /// id não resolve. Round-trip com `set_inner_html` é estável para o subset.
    pub fn inner_html(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        let mut out = String::new();
        for &child in &self.nodes[idx].children {
            self.serialize_node(child, &mut out);
        }
        Some(out)
    }

    /// `element.outerHTML` (GET) — como [`inner_html`](Dom::inner_html) mas inclui o
    /// PRÓPRIO elemento (a tag de abertura+fechamento ao redor dos filhos).
    pub fn outer_html(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        let mut out = String::new();
        self.serialize_node(idx, &mut out);
        Some(out)
    }

    /// Serializa UM nó como HTML (recursivo). Element → `<tag a="v">filhos</tag>`
    /// (void → `<tag>` sem fechar); Text → texto com entidades re-encodadas;
    /// Comment → `<!-- ... -->`; Document → só os filhos.
    fn serialize_node(&self, idx: NodeIdx, out: &mut String) {
        match &self.nodes[idx].kind {
            NodeKind::Document => {
                for &c in &self.nodes[idx].children {
                    self.serialize_node(c, out);
                }
            }
            NodeKind::Text(t) => out.push_str(&crate::html::encode_text_entities(t)),
            NodeKind::Comment(c) => {
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->");
            }
            NodeKind::Element { tag } => {
                out.push('<');
                out.push_str(tag);
                for a in &self.nodes[idx].attrs {
                    out.push(' ');
                    out.push_str(&a.name);
                    out.push_str("=\"");
                    out.push_str(&crate::html::encode_attr_entities(&a.value));
                    out.push('"');
                }
                out.push('>');
                if is_void(tag) {
                    return; // void: sem filhos, sem fechamento.
                }
                for &c in &self.nodes[idx].children {
                    self.serialize_node(c, out);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
    }

    /// `element.innerHTML = html` (SET) — parseia o HTML e SUBSTITUI todos os filhos
    /// do nó pela nova subárvore. Reusa o parser (`parse_html_to_dom`); os nós
    /// parseados são COPIADOS para esta arena (re-parentados sob `id`), atualizando
    /// os índices id/class. Não faz nada num nó que não é elemento ou não resolve.
    pub fn set_inner_html(&mut self, id: NodeId, html: &str) {
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
        let sub = parse_html_to_dom(html);
        let sub_root_children: Vec<NodeIdx> = sub.nodes[sub.root].children.clone();
        for sub_child in sub_root_children {
            self.copy_subtree_into(&sub, sub_child, idx);
        }
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

    /// Serializa a árvore indentada (estilo devtools) — a forma legível de
    /// inspecionar/verificar o que foi gerado. Elemento vira `<tag>`; texto vira
    /// a string entre aspas; cada nível adiciona 2 espaços.
    ///
    /// Exemplo de saída:
    /// ```text
    /// #document
    ///   <h1>
    ///     "Titulo"
    ///   <p>
    ///     "antes "
    ///     <b>
    ///       "forte"
    /// ```
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_node(self.root, 0, &mut out);
        out
    }

    fn dump_node(&self, idx: NodeIdx, depth: usize, out: &mut String) {
        let node = &self.nodes[idx];
        for _ in 0..depth {
            out.push_str("  ");
        }
        match &node.kind {
            NodeKind::Document => out.push_str("#document"),
            NodeKind::Element { tag } => {
                out.push('<');
                out.push_str(tag);
                for a in &node.attrs {
                    out.push(' ');
                    out.push_str(&a.name);
                    out.push_str("=\"");
                    out.push_str(&a.value);
                    out.push('"');
                }
                out.push('>');
            }
            NodeKind::Text(t) => {
                out.push('"');
                out.push_str(t);
                out.push('"');
            }
            NodeKind::Comment(c) => {
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->");
            }
        }
        out.push('\n');
        for &child in &node.children {
            self.dump_node(child, depth + 1, out);
        }
    }
}

/// Tags VAZIAS (void) da spec HTML — não têm fechamento nem filhos, logo NUNCA
/// empilham como "elemento aberto". Lista COMPLETA do HTML5 (whatwg
/// §void-elements): antes faltavam `area/base/col/embed/source/track/wbr`, e um
/// `<source>` dentro de `<video>` empilhava sem nunca fechar — o resto do
/// documento inteiro virava descendente dele.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input"
            | "link" | "meta" | "source" | "track" | "wbr"
    )
}

/// Elementos de BLOCO cuja tag de ABERTURA fecha implicitamente um `<p>` aberto
/// (HTML5, tag omission do `p`: developer.mozilla.org/docs/Web/HTML/Element/p).
/// É a regra de fim-omitido que MAIS aparece em páginas reais — `<p>texto<div>`
/// põe o `div` como IRMÃO do `p`, nunca filho. Tabela como DADOS (uma lista num
/// único lugar), não um emaranhado de `if`s.
fn closes_open_p(tag: &str) -> bool {
    matches!(
        tag,
        "address" | "article" | "aside" | "blockquote" | "details" | "div" | "dl"
            | "fieldset" | "figcaption" | "figure" | "footer" | "form"
            | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "header" | "hgroup" | "hr"
            | "main" | "menu" | "nav" | "ol" | "p" | "pre" | "section" | "table" | "ul"
    )
}

/// `true` se a ABERTURA de `new_tag` fecha implicitamente `open_tag` quando este
/// é o TOPO da pilha de abertos (subconjunto das regras de tag-omission do HTML5
/// que mais dói em páginas reais: `<li>` sem `</li>`, `<p>` sem `</p>`, células
/// de tabela). IMPORTANTE: o chamador só aplica isto ao TOPO da pilha, em loop —
/// nunca fechamos "através" de um container (um `<li>` novo NÃO fecha o `<li>`
/// de um `<ul>` ancestral: se o topo é `ul`, nada casa e nada fecha).
fn implicitly_closes(new_tag: &str, open_tag: &str) -> bool {
    let same_kind = match new_tag {
        // um <li> novo termina o <li> corrente (viram irmãos, não aninhados).
        "li" => open_tag == "li",
        // <dt>/<dd> terminam o <dt>/<dd> corrente (termo/definição irmãos).
        "dt" | "dd" => matches!(open_tag, "dt" | "dd"),
        // <option> termina o <option> corrente.
        "option" => open_tag == "option",
        // um <tr> novo termina a célula aberta E o <tr> corrente: o loop do
        // chamador fecha o td/th que estiver no topo e depois o tr exposto.
        "tr" => matches!(open_tag, "td" | "th" | "tr"),
        // uma célula nova termina a célula corrente (mas NÃO o tr — a nova
        // célula nasce dentro da mesma linha).
        "td" | "th" => matches!(open_tag, "td" | "th"),
        _ => false,
    };
    // Regra dos blocos: a abertura de qualquer elemento de bloco fecha um <p>
    // aberto (inclui `<p>` novo fechando `<p>` corrente — p está na tabela).
    same_kind || (open_tag == "p" && closes_open_p(new_tag))
}

// A herança de CSS (`inherit_from`) e o gatilho de transição (`differs_animated`)
// são GERADOS pela tabela de propriedades `css_props!` em `style/props.rs` — a
// lista de campos herdáveis/animáveis vive SÓ lá (as versões locais campo-a-campo
// que moravam aqui dessincronizavam da interpolação do anim.rs).

/// `true` se `s` é um identificador CSS PURO (letra/dígito/`-`/`_`), sem
/// combinadores/compostos/atributo/pseudo — habilita o atalho por índice no query.
/// `true` se o valor de uma custom property `name` referencia a SI MESMA via
/// `var(--name...)` (auto-referência direta) — a declaração é guaranteed-invalid na
/// spec (o Chrome a descarta). Ex.: `--color-base: hsl(var(--color-base))`.
fn references_self(name: &str, value: &str) -> bool {
    // procura `var(` seguido (após espaços) do próprio nome.
    let mut rest = value;
    while let Some(at) = rest.find("var(") {
        let after = rest[at + 4..].trim_start();
        if after.starts_with(name) {
            // confirma que é o nome COMPLETO (próximo char é ',', ')' ou espaço).
            let tail = &after[name.len()..];
            if tail.is_empty() || tail.starts_with([',', ')', ' ']) {
                return true;
            }
        }
        rest = &rest[at + 4..];
    }
    false
}

fn is_plain_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Insere/atualiza/remove uma declaração `name: value` numa string de `style=""`,
/// preservando as outras declarações e a ordem. `value` vazio REMOVE a declaração.
/// É o motor de `element.style.setProperty`/`removeProperty` (#1759).
fn upsert_css_decl(css_text: &str, name: &str, value: &str) -> String {
    let name_lc = name.to_ascii_lowercase();
    let mut decls: Vec<(String, String)> = Vec::new();
    let mut replaced = false;
    // parseia as declarações atuais (split por ';', cada uma `prop: val`).
    for part in css_text.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((p, v)) = part.split_once(':') else { continue };
        let p = p.trim().to_ascii_lowercase();
        if p == name_lc {
            if !value.is_empty() {
                // PRESERVA o `!important` que a declaração antiga tinha (o novo valor
                // só substitui o valor, não a prioridade — fiel ao CSSOM setProperty).
                let had_important = v.to_ascii_lowercase().contains("!important");
                let new_v = if had_important && !value.to_ascii_lowercase().contains("!important") {
                    format!("{value} !important")
                } else {
                    value.to_string()
                };
                decls.push((p, new_v));
            }
            replaced = true;
        } else {
            decls.push((p, v.trim().to_string()));
        }
    }
    // não existia e tem valor → adiciona ao fim.
    if !replaced && !value.is_empty() {
        decls.push((name_lc, value.to_string()));
    }
    decls
        .iter()
        .map(|(p, v)| format!("{p}: {v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Parseia a parte crua de atributos de uma tag (`class='card' id="x" checked`)
/// em pares `Attr`. Tolerante: aceita aspas simples/duplas ou sem aspas, e
/// atributo sem valor (`checked` → value vazio). Nomes em minúsculas; valores
/// com entidades decodificadas. Não é conforme à spec — cobre o uso comum.
fn parse_attrs(raw: &str) -> Vec<Attr> {
    let mut attrs = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Pula espaços entre atributos.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Lê o nome até `=`, espaço ou fim.
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        if i == name_start {
            break; // nada de nome — acabou.
        }
        let name = raw[name_start..i].to_ascii_lowercase();
        // Pula espaços antes de um possível `=`.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && bytes[i] == b'=' {
            i += 1; // consome `=`
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                // Valor entre aspas: lê até a aspa de fechamento igual.
                let quote = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let v = raw[v_start..i].to_string();
                if i < bytes.len() {
                    i += 1; // consome a aspa de fechamento
                }
                v
            } else {
                // Valor sem aspas: lê até o próximo espaço.
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                raw[v_start..i].to_string()
            }
        } else {
            String::new() // atributo booleano (sem `=valor`).
        };
        attrs.push(Attr {
            name,
            value: crate::html::decode_entities(&value),
        });
    }
    attrs
}

/// Parseia HTML para uma árvore retida. Reusa o tokenizador de `html.rs`; a
/// diferença é a etapa sintática: aqui mantém-se uma PILHA de "elemento aberto"
/// e cada nó nasce filho do topo da pilha.
///
/// - Tag de abertura → primeiro aplica o AUTO-FECHAMENTO IMPLÍCITO do HTML5
///   (`implicitly_closes`: `<li>` fecha `<li>`, bloco fecha `<p>`, `<tr>` fecha
///   `td/th`+`tr`…), depois cria `Element` filho do topo e empurra na pilha
///   (salvo void, que não empurra).
/// - Tag de fechamento → faz pop até casar o nome (tolerante a aninhamento
///   malformado; um `</x>` sem `<x>` aberto é ignorado).
/// - Texto → vira nó `Text` filho do topo (whitespace puro entre tags é
///   descartado, como no caminho immediate-mode, para a árvore não encher de
///   nós de espaço irrelevantes).
pub fn parse_html_to_dom(html: &str) -> Dom {
    // Instala a UA-stylesheet (defaults de display/margem das tags HTML) na primeira
    // vez — em Rust, como DADOS (tabela em block.rs), rodando só quando há DOM. NÃO é
    // mais um prelude `.ts` (isso quebrava todo programa: o `ua.ts` chamava `dom.*`
    // no top-level e `dom` é unbound sem `import "rts:dom"`). Idempotente.
    crate::block::install_ua_defaults();
    let mut dom = Dom::new();
    // Pilha de (índice cru aberto, nome da tag). Começa na raiz Document.
    let mut open: Vec<(NodeIdx, String)> = vec![(dom.root, String::new())];

    for tok in tokenize(html) {
        match tok {
            Token::Tag { name, attrs_raw, close } => {
                if close {
                    // Pop até encontrar a tag de nome igual (tolerante).
                    if let Some(pos) = open.iter().rposition(|(_, n)| *n == name) {
                        // Fecha esse nível e quaisquer filhos mal-fechados acima.
                        open.truncate(pos);
                    }
                    // `</x>` órfão (sem abertura): ignora, não mexe na pilha.
                } else {
                    // AUTO-FECHAMENTO IMPLÍCITO (HTML5 tag omission, subconjunto):
                    // antes de abrir, fecha a(s) tag(s) do TOPO da pilha que a
                    // nova tag termina. Em loop porque `<tr>` pode ter que fechar
                    // a célula (`td`/`th`) E o `tr` empilhados; só o topo é
                    // inspecionado a cada passo — nunca um ancestral através de
                    // um container (ver `implicitly_closes`). O guard `> 1`
                    // preserva a raiz `#document`.
                    while open.len() > 1 && implicitly_closes(&name, &open.last().unwrap().1) {
                        open.pop();
                    }
                    let parent = open.last().unwrap().0;
                    let attrs = parse_attrs(&attrs_raw);
                    let id = dom.push(NodeKind::Element { tag: name.clone() }, attrs, parent);
                    if !is_void(&name) {
                        open.push((id, name));
                    }
                }
            }
            Token::Text(text) => {
                if text.trim().is_empty() {
                    continue; // whitespace puro entre tags — descarta.
                }
                let parent = open.last().unwrap().0;
                dom.push(NodeKind::Text(text), Vec::new(), parent);
            }
            Token::Comment(content) => {
                // DOM fiel preserva comentários como nós (nodeType 8); o render os
                // ignora. Conteúdo cru (sem decodificar entidades).
                let parent = open.last().unwrap().0;
                dom.push(NodeKind::Comment(content), Vec::new(), parent);
            }
            Token::RawElement { tag, attrs, content } => {
                // `<style>`/`<script>`: DOM fiel preserva o ELEMENTO (com o texto cru
                // como filho), mas o conteúdo NÃO é HTML. Para `<style>`, o CSS
                // alimenta o stylesheet de autor (a cascade de `computed_style`).
                // Para `<script>`, só preserva o nó (não executamos JS). O render
                // ignora ambos (sem `BlockDef`/inline para essas tags). Os atributos
                // da abertura são preservados (`<script src>`/`<style media>`).
                if tag == "style" {
                    dom.add_stylesheet(&content);
                }
                let parent = open.last().unwrap().0;
                let parsed = parse_attrs(&attrs);
                let el = dom.push(NodeKind::Element { tag }, parsed, parent);
                if !content.is_empty() {
                    dom.push(NodeKind::Text(content), Vec::new(), el);
                }
            }
        }
    }
    dom
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: nome de tag de um nó Element por índice cru (panica se não for
    /// elemento) — só para deixar os asserts curtos.
    fn tag(dom: &Dom, idx: NodeIdx) -> &str {
        match &dom.node(idx).kind {
            NodeKind::Element { tag } => tag,
            other => panic!("esperava Element, achei {other:?}"),
        }
    }

    /// Helper: resolve um `NodeId` versionado da API pública para o índice cru
    /// usado nos asserts de `children`/`parent`.
    fn idx(dom: &Dom, id: NodeId) -> NodeIdx {
        dom.resolve(id).expect("NodeId deveria resolver nesta árvore")
    }

    #[test]
    fn query_por_tag_id_classe() {
        let dom = parse_html_to_dom(
            "<div class='card'><span id='alvo'>x</span><b class='hl a'>y</b></div>",
        );
        // tag
        let span = dom.query("span").unwrap();
        assert_eq!(tag(&dom, idx(&dom, span)), "span");
        // #id
        assert_eq!(dom.query("#alvo"), Some(span));
        // .classe (mesmo dentro de class multi-valor "hl a")
        let b = dom.query(".hl").unwrap();
        assert_eq!(tag(&dom, idx(&dom, b)), "b");
        assert_eq!(dom.query(".a"), Some(b));
        // sem match
        assert_eq!(dom.query("#naoexiste"), None);
        assert_eq!(dom.query(".naoexiste"), None);
    }

    #[test]
    fn navegacao_dom() {
        // parentNode / first|lastChild / next|previousSibling sobre <div><a/><b/><i/></div>
        let dom = parse_html_to_dom("<div><a>1</a><b>2</b><i>3</i></div>");
        let div = dom.query("div").unwrap();
        let a = dom.query("a").unwrap();
        let b = dom.query("b").unwrap();
        let i = dom.query("i").unwrap();
        // parentNode
        assert_eq!(dom.parent_of(a), Some(div));
        assert_eq!(dom.parent_of(div).map(|p| idx(&dom, p)), Some(dom.root)); // pai do div = #document
        // first/lastChild do div
        assert_eq!(dom.first_child(div), Some(a));
        assert_eq!(dom.last_child(div), Some(i));
        // siblings
        assert_eq!(dom.next_sibling(a), Some(b));
        assert_eq!(dom.next_sibling(b), Some(i));
        assert_eq!(dom.next_sibling(i), None); // último
        assert_eq!(dom.previous_sibling(i), Some(b));
        assert_eq!(dom.previous_sibling(a), None); // primeiro
    }

    #[test]
    fn create_text_e_insert_before() {
        let mut dom = parse_html_to_dom("<ul><li>b</li></ul>");
        let ul = dom.query("ul").unwrap();
        let li_b = dom.query("li").unwrap();
        // createElement + insertBefore(novo, li_b) → novo vira PRIMEIRO filho.
        let li_a = dom.create_element("li");
        dom.insert_before(ul, li_a, Some(li_b));
        assert_eq!(dom.first_child(ul), Some(li_a));
        assert_eq!(dom.next_sibling(li_a), Some(li_b));
        // createTextNode + appendChild dentro do li_a.
        let txt = dom.create_text_node("a");
        dom.append_child(li_a, txt);
        assert_eq!(dom.node_type(txt), 3); // Text
        assert_eq!(dom.first_child(li_a), Some(txt));
        // insert_before com reference None → anexa ao fim.
        let li_c = dom.create_element("li");
        dom.insert_before(ul, li_c, None);
        assert_eq!(dom.last_child(ul), Some(li_c));
    }

    #[test]
    fn style_override_por_no_e_batch() {
        use crate::style::{SLOT_BG, SLOT_COLOR};
        let mut dom = parse_html_to_dom("<div><p id='a'>x</p><p id='b'>y</p></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        // setNodeStyleSlot (1 nó, 1 slot): cor vermelha no #a.
        dom.set_node_style_slot(a, SLOT_COLOR, 0xFF0000FF);
        assert_eq!(dom.computed_style(a).unwrap().color, Some(0xFF0000FF));
        assert_eq!(dom.computed_style(b).unwrap().color, None); // #b intacto
        // batch: triplas planas [id, slot, val] — bg em ambos + cor no #b.
        let triples = vec![
            a.to_abi(), SLOT_BG, 0x111111FF,
            b.to_abi(), SLOT_BG, 0x222222FF,
            b.to_abi(), SLOT_COLOR, 0x00FF00FF,
        ];
        dom.apply_style_batch(&triples);
        assert_eq!(dom.computed_style(a).unwrap().bg, Some(0x111111FF));
        assert_eq!(dom.computed_style(b).unwrap().bg, Some(0x222222FF));
        assert_eq!(dom.computed_style(b).unwrap().color, Some(0x00FF00FF));
        // o override VENCE o estilo inline:
        let mut dom2 = parse_html_to_dom("<p id='c' style='color:#0000ff'>z</p>");
        let c = dom2.query("#c").unwrap();
        assert_eq!(dom2.computed_style(c).unwrap().color, Some(0x0000FFFF)); // inline
        dom2.set_node_style_slot(c, SLOT_COLOR, 0xFF0000FF);
        assert_eq!(dom2.computed_style(c).unwrap().color, Some(0xFF0000FF)); // override vence
    }

    #[test]
    fn style_tag_alimenta_cascade() {
        // <style> com tag/.class/#id alimenta o computed_style por especificidade.
        let dom = parse_html_to_dom(
            "<style>p { color:#ff0000; font-size:14 } .hl { color:#00ff00 } #x { color:#0000ff }</style>\
             <p>normal</p><p class='hl'>destaque</p><p id='x' class='hl'>id</p>",
        );
        let ps = dom.query_all("p");
        assert_eq!(ps.len(), 3);
        // <p> normal: regra de tag.
        let s0 = dom.computed_style(ps[0]).unwrap();
        assert_eq!(s0.color, Some(0xFF0000FF));
        assert_eq!(s0.font_size, Some(crate::style::Dimension::Px(14.0)));
        // <p class="hl">: classe vence a tag na cor; font-size herda da tag.
        let s1 = dom.computed_style(ps[1]).unwrap();
        assert_eq!(s1.color, Some(0x00FF00FF));
        assert_eq!(s1.font_size, Some(crate::style::Dimension::Px(14.0)));
        // <p id="x" class="hl">: id vence tudo.
        let s2 = dom.computed_style(ps[2]).unwrap();
        assert_eq!(s2.color, Some(0x0000FFFF));
    }

    #[test]
    fn style_tag_precede_inline_e_preserva_no() {
        // precedência: <style> autor < style="" inline.
        let dom = parse_html_to_dom(
            "<style>.c { color:#ff0000; padding:10 }</style>\
             <div class='c' style='color:#0000ff'>x</div>",
        );
        let div = dom.query(".c").unwrap();
        let s = dom.computed_style(div).unwrap();
        assert_eq!(s.color, Some(0x0000FFFF)); // inline vence o <style>
        assert_eq!(s.padding.top, crate::style::Side::px_len(10.0)); // padding só o <style> define
        // o <style> também vira NÓ no DOM (fiel), com o CSS como texto cru filho.
        let st = dom.query("style").unwrap();
        assert_eq!(dom.node_type(st), 1); // Element
        let kids = dom.child_nodes(st);
        assert_eq!(kids.len(), 1);
        assert_eq!(dom.node_type(kids[0]), 3); // Text (o CSS cru)
    }

    #[test]
    fn important_inverte_precedencia_de_origem() {
        // MDN estágio 1: `<style>` com `!important` vence o `style=""` inline NORMAL
        // (normalmente o inline venceria o autor; o `!important` inverte isso).
        let dom = parse_html_to_dom(
            "<style>.c { color:#ff0000 !important }</style>\
             <div class='c' style='color:#0000ff'>x</div>",
        );
        let div = dom.query(".c").unwrap();
        assert_eq!(dom.computed_style(div).unwrap().color, Some(0xFF0000FF)); // important vence
        // mas inline `!important` vence o autor `!important` (mesma camada, inline
        // é origem mais forte que o `<style>`):
        let dom2 = parse_html_to_dom(
            "<style>.c { color:#ff0000 !important }</style>\
             <div class='c' style='color:#0000ff !important'>x</div>",
        );
        let div2 = dom2.query(".c").unwrap();
        assert_eq!(dom2.computed_style(div2).unwrap().color, Some(0x0000FFFF)); // inline important
    }

    #[test]
    fn style_tag_conteudo_nao_vira_html() {
        // CSS com `{`, `>` em `a > b` não deve criar tags-fantasma na árvore.
        let dom = parse_html_to_dom("<style>a > b { color:red } p { color:blue }</style><p>oi</p>");
        // o `<b>` do combinador NÃO vira nó na árvore (ficou dentro do raw-text).
        assert!(dom.query("b").is_none());
        assert!(dom.query("p").is_some());
        // o combinador `a > b` é cortado (não suportado); mas `p { }` simples passa.
        assert!(!dom.stylesheet().is_empty());
        assert_eq!(dom.computed_style(dom.query("p").unwrap()).unwrap().color, Some(0x0000FFFF));
    }

    #[test]
    fn inner_html_get_serializa() {
        // innerHTML (get): reconstrói o HTML dos filhos.
        let dom = parse_html_to_dom("<div><p class='x'>oi <b>forte</b></p></div>");
        let div = dom.query("div").unwrap();
        assert_eq!(dom.inner_html(div).unwrap(), "<p class=\"x\">oi <b>forte</b></p>");
        // outerHTML inclui o próprio div.
        assert_eq!(dom.outer_html(div).unwrap(), "<div><p class=\"x\">oi <b>forte</b></p></div>");
        // entidades re-encodadas no texto.
        let d2 = parse_html_to_dom("<p>a &lt; b &amp; c</p>");
        let p = d2.query("p").unwrap();
        assert_eq!(d2.inner_html(p).unwrap(), "a &lt; b &amp; c");
    }

    #[test]
    fn inner_html_set_substitui() {
        // innerHTML (set): parseia e troca os filhos.
        let mut dom = parse_html_to_dom("<div><span>velho</span></div>");
        let div = dom.query("div").unwrap();
        dom.set_inner_html(div, "<p>novo</p><b>!</b>");
        // os filhos novos estão lá; o velho sumiu.
        assert_eq!(dom.inner_html(div).unwrap(), "<p>novo</p><b>!</b>");
        // a árvore real reflete (query acha o <p> novo).
        let p = dom.query("p").unwrap();
        assert_eq!(dom.text_content(p).unwrap(), "novo");
        assert!(dom.query("span").is_none()); // o velho foi descartado
    }

    #[test]
    fn listener_cb_registra_e_coleta_com_bubbling() {
        // addEventListener(type, fn): o Dom guarda o word opaco; dispatch_event_collect
        // devolve os pares (nó, cb) na ordem DOM (alvo → ancestrais).
        let mut dom = parse_html_to_dom("<div id=pai><button id=b>x</button></div>");
        let pai = dom.query("#pai").unwrap();
        let b = dom.query("#b").unwrap();
        dom.add_event_listener_cb(b, "click", 111);
        dom.add_event_listener_cb(pai, "click", 222);
        // duplicata do MESMO cb é no-op (spec DOM).
        dom.add_event_listener_cb(b, "click", 111);
        assert_eq!(dom.dispatch_event_collect(b, "click", true), 2);
        assert_eq!(dom.last_dispatch_at(0).unwrap().1, 111); // alvo primeiro
        assert_eq!(dom.last_dispatch_at(1).unwrap().1, 222); // depois o pai
        // sem bubbling: só o alvo.
        assert_eq!(dom.dispatch_event_collect(b, "click", false), 1);
        // removeEventListener limpa os callbacks do tipo.
        dom.remove_event_listener(b, "click");
        assert_eq!(dom.dispatch_event_collect(b, "click", false), 0);
    }

    #[test]
    fn inner_html_set_desanexa_dos_indices() {
        // Regressão: o atalho O(1) por índice (`.classe`/`#id`) não pode achar nó
        // descartado pelo set_inner_html/set_text (o `parent` dos filhos velhos é
        // zerado para o `is_attached` do índice falhar).
        let mut dom = parse_html_to_dom("<div id=a><span class=x id=velho>old</span></div>");
        let div = dom.query("#a").unwrap();
        dom.set_inner_html(div, "<b>new</b>");
        assert!(dom.query(".x").is_none());
        assert!(dom.query("#velho").is_none());
        // set_text idem.
        let mut d2 = parse_html_to_dom("<div id=a><span class=x>old</span></div>");
        let div2 = d2.query("#a").unwrap();
        d2.set_text(div2, "txt");
        assert!(d2.query(".x").is_none());
    }

    #[test]
    fn inner_html_round_trip() {
        // parse → serialize → parse estável (subset).
        let html = "<section id=\"s\"><h2>T</h2><p>texto <i>it</i> fim</p></section>";
        let dom = parse_html_to_dom(html);
        let sec = dom.query("#s").unwrap();
        let serial = dom.outer_html(sec).unwrap();
        assert_eq!(serial, html);
    }

    #[test]
    fn revision_bumpa_na_mutacao_e_nao_na_leitura() {
        // O contrato dos caches de layout (backend + GEOM_CACHE): a revisão muda a
        // cada MUTAÇÃO que afeta render, e NÃO muda em leituras — inclusive o
        // computed_style memoizado (que preenche o memo mas não altera o estado).
        let mut dom = parse_html_to_dom("<div id='a' style='color:#fff'>x</div>");
        let r0 = dom.render_revision();
        // leituras não bumpam (o memo do computed usa interior mutability).
        let a = dom.query("#a").unwrap();
        let _ = dom.computed_style(a);
        let _ = dom.computed_style(a); // 2ª leitura = hit do memo
        assert_eq!(dom.render_revision(), r0, "leitura não muda a revisão");
        // mutações bumpam — e o computed reflete a mudança (memo invalidado).
        dom.set_attr(a, "style", "color:#ff0000");
        assert_ne!(dom.render_revision(), r0, "set_attr bumpa");
        let css = dom.computed_style(a).unwrap();
        assert_eq!(css.color, Some(0xFF0000FF), "memo invalidado pela revisão");
        let r1 = dom.render_revision();
        dom.set_text(a, "novo");
        assert_ne!(dom.render_revision(), r1, "set_text bumpa");
        // defineStyle (estado global por-tag, fora do Dom) também invalida.
        let r2 = dom.render_revision();
        crate::style::define_style("tag_rev_teste", crate::style::SLOT_COLOR, 0x11223344);
        assert_ne!(dom.render_revision(), r2, "defineStyle bumpa o epoch global");
    }

    #[test]
    fn var_por_elemento_na_cascade() {
        // #1779: .btn usa var(--btn-bg); cada VARIANTE redefine a var NO SELETOR
        // do componente — cada botao pega a SUA cor. (O antigo mapa global dava a
        // mesma cor a todos: a ultima declaracao do arquivo vencia.)
        let dom = parse_html_to_dom(
            "<html><head><style>               :root { --btn-bg: #000000 }               .btn { background: var(--btn-bg) }               .btn-primary { --btn-bg: #0000ff }               .btn-danger { --btn-bg: #ff0000 }             </style></head><body>             <div id=\"a\" class=\"btn btn-primary\">a</div>             <div id=\"b\" class=\"btn btn-danger\">b</div>             <div id=\"c\" class=\"btn\">c</div>             </body></html>",
        );
        let bg = |sel: &str| {
            let n = dom.query(sel).unwrap();
            dom.computed_property(n, "background-color")
        };
        assert_eq!(bg("#a"), "rgb(0, 0, 255)", "btn-primary redefine a var");
        assert_eq!(bg("#b"), "rgb(255, 0, 0)", "btn-danger redefine a var");
        assert_eq!(bg("#c"), "rgb(0, 0, 0)", "sem variante: o :root vale");
    }

    #[test]
    fn var_heranca_fallback_e_aninhado() {
        // heranca: o filho usa a var declarada no ANCESTRAL; fallback quando
        // ausente; var aninhada (--a referencia --b).
        let dom = parse_html_to_dom(
            "<style>               #pai { --c: #00ff00; --a: var(--b); --b: #112233 }               span { color: var(--c) }               em { color: var(--a) }               p { color: var(--nada, #123456) }             </style>             <div id=\"pai\"><span id=\"f\">x</span><em id=\"e\">y</em></div>             <p id=\"p\">z</p>",
        );
        let color = |sel: &str| {
            let n = dom.query(sel).unwrap();
            dom.computed_property(n, "color")
        };
        assert_eq!(color("#f"), "rgb(0, 255, 0)", "var herdada do pai");
        assert_eq!(color("#e"), "rgb(17, 34, 51)", "var aninhada resolve");
        assert_eq!(color("#p"), "rgb(18, 52, 86)", "fallback quando ausente");
    }

    #[test]
    fn transition_interpola_no_tempo() {
        // transition: o DOM é dono do loop — advance(now) interpola a mudança (#1776).
        let mut dom = parse_html_to_dom(
            "<div id=\"box\" style=\"background:#000000;transition:0.5s linear\">x</div>",
        );
        let box_id = dom.query("#box").unwrap();
        let bi = dom.resolve(box_id).unwrap();
        // frame 0: estabelece o baseline (background preto). Sem animação ainda.
        assert!(!dom.advance(0.0));
        // o JS muda o background para branco (via setStyleProp → atributo style).
        dom.set_style_property(box_id, "background", "white");
        // frame em t=0: detecta a mudança, inicia a transição. Ainda preto (t=0).
        assert!(dom.advance(0.0)); // há animação ativa
        let at0 = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(at0, 0x000000FF, "início = preto");
        // metade do tempo (250ms de 500): cinza (#808080).
        dom.advance(250.0);
        let mid = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(mid, 0x808080FF, "meio = cinza, got 0x{mid:08X}");
        // fim (500ms): branco, e a animação encerra.
        let still = dom.advance(500.0);
        let end = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(end, 0xFFFFFFFF, "fim = branco");
        assert!(!still, "animação encerrou");
    }

    #[test]
    fn keyframes_anima_no_tempo() {
        // @keyframes roda SOZINHA no tempo (sem gatilho) — fase 2 do #1776.
        let mut dom = parse_html_to_dom(
            "<style>@keyframes pulse{0%{background:#000000}50%{background:#ff0000}100%{background:#000000}}\
             #box{animation:pulse 1s linear}</style><div id=\"box\">x</div>",
        );
        let bi = dom.resolve(dom.query("#box").unwrap()).unwrap();
        // t=0: começa no 0% (preto). advance estabelece o start.
        dom.advance(0.0);
        assert_eq!(dom.computed_style_idx(bi).unwrap().bg, Some(0x000000FF), "0%");
        // t=250ms (25% da animação): entre 0% e 50% → metade do caminho preto→vermelho.
        dom.advance(250.0);
        let q = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(q, 0x800000FF, "25% = meio de preto→vermelho, got 0x{q:08X}");
        // t=500ms (50%): vermelho puro.
        dom.advance(500.0);
        assert_eq!(dom.computed_style_idx(bi).unwrap().bg, Some(0xFF0000FF), "50%");
        // t=750ms (75%): meio de vermelho→preto de volta.
        dom.advance(750.0);
        assert_eq!(dom.computed_style_idx(bi).unwrap().bg, Some(0x800000FF), "75%");
        // a animação fica ativa (retorna true durante o curso).
        assert!(dom.advance(400.0));
    }

    #[test]
    fn keyframes_from_to_e_iteracoes() {
        // sintaxe from/to + iterações finitas (termina no estado final).
        let mut dom = parse_html_to_dom(
            "<style>@keyframes grow{from{width:100px}to{width:300px}}#b{animation:grow 1s linear 1}</style><div id=\"b\">x</div>",
        );
        let bi = dom.resolve(dom.query("#b").unwrap()).unwrap();
        dom.advance(0.0);
        assert_eq!(dom.computed_style_idx(bi).unwrap().width, Some(crate::style::Dimension::Px(100.0)), "from");
        dom.advance(500.0);
        assert_eq!(dom.computed_style_idx(bi).unwrap().width, Some(crate::style::Dimension::Px(200.0)), "meio");
        // depois de 1 iteração (1s), a animação encerra (não retorna ativa).
        let active = dom.advance(1100.0);
        assert!(!active, "1 iteração terminou");
    }

    #[test]
    fn seletor_composto() {
        let dom = parse_html_to_dom("<p class=\"card big\" id=\"x\">a</p><p class=\"card\">b</p><div class=\"card\">c</div>");
        assert_eq!(dom.query_all("p.card").len(), 2);
        assert_eq!(dom.query_all(".card.big").len(), 1);
        assert_eq!(dom.query_all("p.card#x").len(), 1);
        assert_eq!(dom.query_all("div.card").len(), 1);
    }

    #[test]
    fn seletor_combinadores() {
        let dom = parse_html_to_dom(
            "<div id=\"root\"><section><p class=\"a\">1</p></section><p class=\"b\">2</p><span>3</span><p class=\"c\">4</p></div>",
        );
        assert_eq!(dom.query_all("#root p").len(), 3); // descendente
        assert_eq!(dom.query_all("#root > p").len(), 2); // filho direto
        assert_eq!(dom.query_all("p.b + span").len(), 1); // irmão imediato
        assert_eq!(dom.query_all("p.b ~ p").len(), 1); // irmão geral
        assert_eq!(dom.query_all("section p.a").len(), 1);
    }

    #[test]
    fn seletor_atributo() {
        let dom = parse_html_to_dom(
            "<a href=\"https://x.com/page\">1</a><a href=\"http://y.org\">2</a><input type=\"text\"><input disabled>",
        );
        assert_eq!(dom.query_all("[href]").len(), 2);
        assert_eq!(dom.query_all("[disabled]").len(), 1);
        assert_eq!(dom.query_all("[type=text]").len(), 1);
        assert_eq!(dom.query_all("[href^=https]").len(), 1);
        assert_eq!(dom.query_all("[href$=.org]").len(), 1);
        assert_eq!(dom.query_all("[href*=x.com]").len(), 1);
    }

    #[test]
    fn seletor_pseudo_estrutural() {
        let dom = parse_html_to_dom("<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul><div></div>");
        assert_eq!(dom.query_all("li:first-child").len(), 1);
        assert_eq!(dom.query_all("li:last-child").len(), 1);
        assert_eq!(dom.query_all("li:nth-child(2)").len(), 1);
        assert_eq!(dom.query_all("li:nth-child(odd)").len(), 2);
        assert_eq!(dom.query_all("li:nth-child(even)").len(), 2);
        assert_eq!(dom.query_all("li:nth-child(2n+1)").len(), 2);
        assert_eq!(dom.query_all("div:empty").len(), 1);
        assert_eq!(dom.query_all("ul:only-child").len(), 0);
    }

    #[test]
    fn seletor_pseudo_estado() {
        // :checked/:disabled/:required mapeiam para presença de atributo (#1752).
        let dom = parse_html_to_dom(
            "<input type=\"checkbox\" checked><input type=\"checkbox\"><input required><button disabled>x</button>",
        );
        assert_eq!(dom.query_all("input:checked").len(), 1);
        assert_eq!(dom.query_all(":disabled").len(), 1); // o button
        assert_eq!(dom.query_all("input:required").len(), 1);
        assert_eq!(dom.query_all("input:enabled").len(), 3); // os 3 inputs (nenhum disabled)
    }

    #[test]
    fn seletor_lista_e_invalidos() {
        // bugs da verificação adversarial corrigidos (#1752).
        let dom = parse_html_to_dom("<div><p>1</p><a>2</a><span>3</span></div>");
        // lista por vírgula: p, a → casa qualquer um.
        assert_eq!(dom.query_all("p, a").len(), 2);
        assert_eq!(dom.query_all("p, a, span").len(), 3);
        // combinador duplo `>>` → inválido, casa 0.
        assert_eq!(dom.query_all("div >> p").len(), 0);
        // universal no meio `p*` → inválido.
        assert_eq!(dom.query_all("p*").len(), 0);
    }

    #[test]
    fn seletor_root_em_fragmento() {
        // :root num fragmento com VÁRIOS top-level → casa 0 (não há <html> único).
        let dom = parse_html_to_dom("<div id=\"a\">x</div><div id=\"b\">y</div>");
        assert_eq!(dom.query_all(":root").len(), 0);
        // com UM só top-level, :root casa esse 1.
        let dom2 = parse_html_to_dom("<html><body>x</body></html>");
        assert_eq!(dom2.query_all(":root").len(), 1);
    }

    #[test]
    fn seletor_atributo_com_colchete_no_valor() {
        // [data-x="a]b"] — o `]` literal no valor aspado não fecha o seletor.
        let dom = parse_html_to_dom("<div data-x=\"a]b\">x</div>");
        assert_eq!(dom.query_all("[data-x=\"a]b\"]").len(), 1);
    }

    #[test]
    fn cascade_com_seletor_composto() {
        let dom = parse_html_to_dom(
            "<style>p { color:#000000 } p.hi { color:#ff0000 } div > p.hi { color:#00ff00 }</style>\
             <div><p class=\"hi\">x</p></div>",
        );
        let p = dom.query("p.hi").unwrap();
        assert_eq!(dom.computed_style(p).unwrap().color, Some(0x00FF00FF));
    }

    #[test]
    fn traversal_por_elemento() {
        // firstElementChild/nextElementSibling pulam texto e comentário (#1757).
        let dom = parse_html_to_dom("<div id=\"a\"><!--c--><p class=\"x\">um</p>txt<span>dois</span></div>");
        let div = dom.query("#a").unwrap();
        let p = dom.first_element_child(div).unwrap();
        // node_name no Rust é minúsculo (a fachada TS faz toUpperCase).
        assert_eq!(dom.node_name(p).as_deref(), Some("p")); // pulou o comentário
        let span = dom.next_element_sibling(p).unwrap();
        assert_eq!(dom.node_name(span).as_deref(), Some("span")); // pulou o texto "txt"
        assert_eq!(dom.last_element_child(div), Some(span));
        assert_eq!(dom.parent_element(p), Some(div));
        // matches/closest com seletor simples.
        assert!(dom.matches_selector(p, ".x"));
        assert!(!dom.matches_selector(p, ".y"));
        assert_eq!(dom.closest(p, "#a"), Some(div));
        assert_eq!(dom.closest(p, "p"), Some(p)); // o próprio nó conta
    }

    #[test]
    fn node_utils_contains_e_nodevalue() {
        let dom = parse_html_to_dom("<div id=\"a\"><p>oi</p></div>");
        let div = dom.query("#a").unwrap();
        let p = dom.query("p").unwrap();
        assert!(dom.contains(div, p)); // div contém p
        assert!(dom.contains(div, div)); // contém a si mesmo
        assert!(!dom.contains(p, div)); // p NÃO contém o div
        assert!(dom.has_child_nodes(div));
        // nodeValue: só Text/Comment; Element = None.
        let txt = dom.first_child(p).unwrap();
        assert_eq!(dom.node_value(txt).as_deref(), Some("oi"));
        assert_eq!(dom.node_value(div), None);
    }

    #[test]
    fn normalize_funde_textos_adjacentes() {
        let mut dom = parse_html_to_dom("<div id=\"a\"></div>");
        let div = dom.query("#a").unwrap();
        for s in ["a", "b", "", "c"] {
            let t = dom.create_text_node(s);
            dom.append_child(div, t);
        }
        assert_eq!(dom.child_nodes(div).len(), 4);
        dom.normalize(div);
        let kids = dom.child_nodes(div);
        assert_eq!(kids.len(), 1, "4 textos (1 vazio) → 1 fundido");
        assert_eq!(dom.node_value(kids[0]).as_deref(), Some("abc"));
    }

    #[test]
    fn atributos_remove_has_e_names() {
        let mut dom = parse_html_to_dom("<div id=\"a\" class=\"c\" hidden>x</div>");
        let div = dom.query("#a").unwrap();
        // hidden é booleano (valor "") mas PRESENTE — has_attr o detecta.
        assert!(dom.has_attr(div, "hidden"));
        assert!(!dom.has_attr(div, "title"));
        assert_eq!(dom.attr_names(div), vec!["id", "class", "hidden"]);
        dom.remove_attr(div, "hidden");
        assert!(!dom.has_attr(div, "hidden"));
        assert_eq!(dom.attr_names(div), vec!["id", "class"]);
    }

    #[test]
    fn query_por_subarvore() {
        // querySelector restrito à subárvore (#1758): o <p> dentro de #b não deve
        // ser achado pela busca dentro de #a.
        let dom = parse_html_to_dom("<div id=\"a\"><p class=\"x\">in-a</p></div><div id=\"b\"><p class=\"x\">in-b</p></div>");
        let a = dom.query("#a").unwrap();
        let found = dom.query_within(a, ".x").unwrap();
        assert_eq!(dom.text_content(found).as_deref(), Some("in-a")); // só o de dentro de #a
        assert_eq!(dom.query_all_within(a, ".x").len(), 1); // não vê o de #b
        // mas a busca global vê os dois.
        assert_eq!(dom.query_all(".x").len(), 2);
    }

    #[test]
    fn get_elements_by() {
        let dom = parse_html_to_dom(
            "<div class=\"card\"><p class=\"card\">x</p></div><span name=\"f\">y</span><span name=\"f\">z</span>",
        );
        assert_eq!(dom.get_elements_by_class_name("card").len(), 2); // div + p
        assert_eq!(dom.get_elements_by_tag_name("span").len(), 2);
        assert_eq!(dom.get_elements_by_name("f").len(), 2); // os 2 spans
        // '*' = todos os elementos.
        assert_eq!(dom.get_elements_by_tag_name("*").len(), 4); // div,p,span,span
    }

    #[test]
    fn clone_node_deep_e_shallow() {
        let mut dom = parse_html_to_dom("<div id=\"a\"><p>oi</p><span>tchau</span></div>");
        let a = dom.query("#a").unwrap();
        // shallow: clone sem filhos.
        let shallow = dom.clone_node(a, false).unwrap();
        assert_eq!(dom.child_nodes(shallow).len(), 0);
        assert_eq!(dom.node_name(shallow).as_deref(), Some("div"));
        // deep: com a subárvore.
        let deep = dom.clone_node(a, true).unwrap();
        assert_eq!(dom.child_elements(deep).len(), 2); // p + span
        assert_eq!(dom.text_content(deep).as_deref(), Some("oitchau"));
        // o clone é SOLTO (sem pai).
        assert_eq!(dom.parent_of(deep), None);
    }

    #[test]
    fn mutacao_rica() {
        let mut dom = parse_html_to_dom("<div id=\"a\"><p id=\"p\">x</p></div>");
        let a = dom.query("#a").unwrap();
        let p = dom.query("#p").unwrap();
        // prepend: novo elemento no início.
        let h = dom.create_element("h1");
        dom.prepend_child(a, h);
        assert_eq!(dom.first_element_child(a), Some(h)); // h1 antes do p
        // before/after: irmão de p.
        let b = dom.create_element("b");
        dom.insert_adjacent(p, b, false); // b antes de p
        let i = dom.create_element("i");
        dom.insert_adjacent(p, i, true); // i depois de p
        // ordem: h1, b, p, i.
        let kids = dom.child_elements(a);
        let names: Vec<String> = kids.iter().map(|&k| dom.node_name(k).unwrap()).collect();
        assert_eq!(names, vec!["h1", "b", "p", "i"]);
        // replaceWith: troca p por um span.
        let s = dom.create_element("span");
        dom.replace_with(p, s);
        assert!(dom.query("#p").is_none()); // p saiu
        // clearChildren: esvazia.
        dom.clear_children(a);
        assert_eq!(dom.child_nodes(a).len(), 0);
    }

    #[test]
    fn matcher_universal_e_multi_classe() {
        // BUG (verificação adversarial): "*" não casava; multi-classe não tokenizava.
        let dom = parse_html_to_dom("<div class=\"a b\"><p class=\"a\">x</p></div>");
        // "*" casa todos os elementos (div + p).
        assert_eq!(dom.query_all("*").len(), 2);
        // multi-classe = AND: só o div tem 'a' E 'b'.
        assert_eq!(dom.get_elements_by_class_name("a b").len(), 1);
        assert_eq!(dom.get_elements_by_class_name("a").len(), 2); // div e p têm 'a'
        // ordem dos tokens não importa.
        assert_eq!(dom.get_elements_by_class_name("b a").len(), 1);
    }

    #[test]
    fn clone_indexado_e_achavel() {
        // BUG: o clone não entrava nos índices id/class → querySelector não achava.
        let mut dom = parse_html_to_dom("<div id=\"src\" class=\"card\">x</div>");
        let src = dom.query("#src").unwrap();
        let clone = dom.clone_node(src, true).unwrap();
        // muda o id do clone e anexa à raiz.
        dom.set_attr(clone, "id", "copy");
        // anexa à própria raiz #document.
        dom.append_child(dom.root_id(), clone);
        // agora querySelector acha o clone pela classe (índice) e pelo novo id.
        assert!(dom.query(".card").is_some());
        assert_eq!(dom.get_elements_by_class_name("card").len(), 2); // original + clone
    }

    #[test]
    fn replace_with_atomico_nao_destroi_em_ciclo() {
        // BUG CRITICAL: replaceWith(node, ancestral) destruía node sem inserir.
        let mut dom = parse_html_to_dom("<div id=\"out\"><div id=\"in\"><p id=\"p\">x</p></div></div>");
        let out = dom.query("#out").unwrap();
        let p = dom.query("#p").unwrap();
        // tentar substituir p por 'out' (ancestral de p) — guarda de ciclo aborta o
        // insert; p NÃO deve ser destruído.
        dom.replace_with(p, out);
        assert!(dom.query("#p").is_some(), "p preservado quando a inserção aborta");
        // replaceWith por si mesmo é no-op (não remove).
        dom.replace_with(p, p);
        assert!(dom.query("#p").is_some());
        // caso normal: substitui p por um span novo.
        let s = dom.create_element("span");
        dom.set_attr(s, "id", "s");
        dom.replace_with(p, s);
        assert!(dom.query("#p").is_none());
        assert!(dom.query("#s").is_some());
    }

    #[test]
    fn after_com_proximo_irmao_mantem_ordem() {
        // BUG: after(other) com other já sendo o próximo irmão jogava other pro fim.
        let mut dom = parse_html_to_dom("<div id=\"a\"><b id=\"b\">1</b><i id=\"i\">2</i><u id=\"u\">3</u></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        let i = dom.query("#i").unwrap();
        // b.after(i) — i JÁ é o próximo irmão de b; deve manter a ordem b,i,u.
        dom.insert_adjacent(b, i, true);
        let names: Vec<String> = dom.child_elements(a).iter().map(|&k| dom.node_name(k).unwrap()).collect();
        assert_eq!(names, vec!["b", "i", "u"]); // ordem preservada, i não foi pro fim
    }

    #[test]
    fn computed_property_formato_browser() {
        // getComputedStyle por nome, formato do browser (#1759).
        let dom = parse_html_to_dom(
            "<style>#a{color:#ff0000;background:rgba(0,0,255,0.5);font-size:18px;padding:10px}</style><div id=\"a\">x</div>",
        );
        let a = dom.query("#a").unwrap();
        assert_eq!(dom.computed_property(a, "color"), "rgb(255, 0, 0)");
        // alpha a 2 casas — VALIDADO no Chrome (#..80 / rgba(.5) → "0.5", não 0.501961).
        assert_eq!(dom.computed_property(a, "background-color"), "rgba(0, 0, 255, 0.5)");
        assert_eq!(dom.computed_property(a, "font-size"), "18px");
        assert_eq!(dom.computed_property(a, "padding-top"), "10px");
        assert_eq!(dom.computed_property(a, "margin-top"), ""); // não definido
    }

    #[test]
    fn style_set_get_remove_property() {
        // el.style.setProperty/getPropertyValue/removeProperty + cssText (#1759).
        let mut dom = parse_html_to_dom("<div id=\"a\" style=\"color: red; padding: 5px\">x</div>");
        let a = dom.query("#a").unwrap();
        // get inline.
        assert_eq!(dom.inline_property(a, "color"), "rgb(255, 0, 0)");
        // set nova prop preserva as outras.
        dom.set_style_property(a, "font-size", "20px");
        assert_eq!(dom.inline_property(a, "font-size"), "20px");
        assert_eq!(dom.inline_property(a, "color"), "rgb(255, 0, 0)"); // mantida
        // atualizar prop existente.
        dom.set_style_property(a, "color", "blue");
        assert_eq!(dom.inline_property(a, "color"), "rgb(0, 0, 255)");
        // remover.
        dom.remove_style_property(a, "padding");
        assert_eq!(dom.inline_property(a, "padding-top"), "");
        // cssText reflete o estado.
        assert!(dom.css_text(a).contains("color: blue"));
        assert!(dom.css_text(a).contains("font-size: 20px"));
        assert!(!dom.css_text(a).contains("padding"));
    }

    #[test]
    fn upsert_preserva_important() {
        // editar uma prop com !important NÃO perde a prioridade (verificação adversarial).
        let r = upsert_css_decl("color: red !important; margin: 0", "color", "blue");
        assert!(r.contains("color: blue !important"), "got: {r}");
        assert!(r.contains("margin: 0"));
        // prop sem important continua sem.
        let r2 = upsert_css_decl("color: red; margin: 0", "color", "blue");
        assert!(!r2.contains("!important"), "got: {r2}");
    }

    #[test]
    fn display_keyword_valido() {
        // FlexWrap → "flex" (não "flexwrap" inválido); flex-wrap é prop separada.
        let dom = parse_html_to_dom("<style>#a{display:flex;flex-wrap:wrap}</style><div id=\"a\">x</div>");
        let a = dom.query("#a").unwrap();
        assert_eq!(dom.computed_property(a, "display"), "flex");
    }

    #[test]
    fn css_text_set_substitui_tudo() {
        let mut dom = parse_html_to_dom("<div id=\"a\" style=\"color: red\">x</div>");
        let a = dom.query("#a").unwrap();
        dom.set_css_text(a, "background: green; margin: 4px");
        assert_eq!(dom.inline_property(a, "color"), ""); // o color sumiu
        assert_eq!(dom.inline_property(a, "background-color"), "rgb(0, 128, 0)");
    }

    #[test]
    fn eventos_add_dispatch_poll() {
        // addEventListener marca o nó; dispatchEvent enfileira; poll consome (#1760).
        let mut dom = parse_html_to_dom("<div id=\"a\"><button id=\"b\">x</button></div>");
        let b = dom.query("#b").unwrap();
        dom.add_event_listener(b, "click");
        assert!(dom.has_listener(b, "click"));
        assert!(!dom.has_listener(b, "mousedown"));
        // dispatch no botão → 1 listener (só o botão escuta).
        assert_eq!(dom.dispatch_event(b, "click", true), 1);
        let (target, t) = dom.poll_event().unwrap();
        assert_eq!(target, b);
        assert_eq!(t, "click");
        assert!(dom.poll_event().is_none()); // fila esvaziou
    }

    #[test]
    fn eventos_bubbling() {
        // dispatch no filho borbulha para o pai que também escuta (#1760).
        let mut dom = parse_html_to_dom("<div id=\"a\"><button id=\"b\">x</button></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        dom.add_event_listener(a, "click"); // o PAI escuta
        dom.add_event_listener(b, "click"); // o filho também
        // dispatch no filho: notifica filho E pai (bubbling) → 2.
        assert_eq!(dom.dispatch_event(b, "click", true), 2);
        // ordem: alvo primeiro, depois o ancestral (target → bubble).
        let (first, _) = dom.poll_event().unwrap();
        let (second, _) = dom.poll_event().unwrap();
        assert_eq!(first, b);
        assert_eq!(second, a);
    }

    #[test]
    fn eventos_arvore_profunda_e_filtro_por_tipo() {
        // VALIDADO no Chrome: árvore de 6 níveis, bubbling seletivo + filtro de tipo.
        let mut dom = parse_html_to_dom(
            "<section id=\"sec\"><article id=\"art\"><div id=\"box\"><p id=\"par\"><a id=\"link\">t</a></p></div></article></section>",
        );
        let sec = dom.query("#sec").unwrap();
        let box_ = dom.query("#box").unwrap();
        let link = dom.query("#link").unwrap();
        dom.add_event_listener(sec, "click");
        dom.add_event_listener(box_, "click");
        dom.add_event_listener(link, "click");
        dom.add_event_listener(box_, "mouseover");
        // click no link borbulha por link→box→sec (pula art/par que não escutam).
        assert_eq!(dom.dispatch_event(link, "click", true), 3);
        let chain: Vec<NodeId> = std::iter::from_fn(|| dom.poll_event().map(|(n, _)| n)).collect();
        assert_eq!(chain, vec![link, box_, sec]); // ordem target→bubble
        // mouseover no link: só o box escuta esse TIPO (apesar do bubbling).
        assert_eq!(dom.dispatch_event(link, "mouseover", true), 1);
        assert_eq!(dom.poll_event().unwrap().0, box_);
        // remove o do box → re-dispatch click enfileira só link+sec.
        dom.remove_event_listener(box_, "click");
        assert_eq!(dom.dispatch_event(link, "click", true), 2);
    }

    #[test]
    fn eventos_no_solto_sem_bubbling() {
        // dispatch num nó SOLTO (sem pai): só ele, sem bubbling.
        let mut dom = parse_html_to_dom("<div></div>");
        let solto = dom.create_element("button");
        dom.add_event_listener(solto, "click");
        assert_eq!(dom.dispatch_event(solto, "click", true), 1); // só ele, não tem pai
    }

    #[test]
    fn eventos_bubbles_false_so_o_alvo() {
        // bubbles=false: só o alvo é notificado, mesmo com o pai escutando (#1760).
        let mut dom = parse_html_to_dom("<div id=\"a\"><button id=\"b\">x</button></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        dom.add_event_listener(a, "focus");
        dom.add_event_listener(b, "focus");
        // focus não borbulha (bubbles=false): só o botão, não o pai.
        assert_eq!(dom.dispatch_event(b, "focus", false), 1);
        assert_eq!(dom.poll_event().unwrap().0, b);
        assert!(dom.poll_event().is_none());
    }

    #[test]
    fn eventos_tipo_case_sensitive() {
        // tipos de evento são CASE-SENSITIVE (spec DOM: click ≠ CLICK).
        let mut dom = parse_html_to_dom("<div id=\"a\">x</div>");
        let a = dom.query("#a").unwrap();
        dom.add_event_listener(a, "click");
        assert!(dom.has_listener(a, "click"));
        assert!(!dom.has_listener(a, "CLICK")); // case diferente não casa
        assert_eq!(dom.dispatch_event(a, "Click", true), 0); // não dispara
        assert_eq!(dom.dispatch_event(a, "click", true), 1); // o exato dispara
    }

    #[test]
    fn eventos_remove_e_sem_listener() {
        let mut dom = parse_html_to_dom("<div id=\"a\">x</div>");
        let a = dom.query("#a").unwrap();
        dom.add_event_listener(a, "click");
        dom.remove_event_listener(a, "click");
        assert!(!dom.has_listener(a, "click"));
        // dispatch sem ninguém escutando → 0 enfileirados.
        assert_eq!(dom.dispatch_event(a, "click", true), 0);
        assert!(dom.poll_event().is_none());
    }

    #[test]
    fn create_comment_node() {
        let mut dom = parse_html_to_dom("<div></div>");
        let c = dom.create_comment("nota");
        assert_eq!(dom.node_type(c), 8);
        assert_eq!(dom.node_value(c).as_deref(), Some("nota"));
    }

    #[test]
    fn parser_preserva_comentarios() {
        // DOM fiel: <!-- --> vira nó Comment (nodeType 8), não é descartado.
        let dom = parse_html_to_dom("<div><!-- nota --><p>oi</p></div>");
        let div = dom.query("div").unwrap();
        let kids = dom.child_nodes(div); // childNodes inclui o comentário
        assert_eq!(kids.len(), 2); // Comment + <p>
        assert_eq!(dom.node_type(kids[0]), 8); // Comment
        assert_eq!(dom.node_name(kids[0]).as_deref(), Some("#comment"));
        assert_eq!(dom.node_type(kids[1]), 1); // <p>
        // o `>` DENTRO do comentário não encerra cedo:
        let dom2 = parse_html_to_dom("<!-- a > b --><span>x</span>");
        let span = dom2.query("span").unwrap();
        assert_eq!(dom2.node_type(span), 1); // span foi parseado corretamente
    }

    #[test]
    fn node_type_e_name() {
        let mut dom = parse_html_to_dom("<p>oi</p>");
        let p = dom.query("p").unwrap();
        let txt = dom.first_child(p).unwrap();
        assert_eq!(dom.node_type(p), 1); // Element
        assert_eq!(dom.node_name(p).as_deref(), Some("p"));
        assert_eq!(dom.node_type(txt), 3); // Text
        assert_eq!(dom.node_name(txt).as_deref(), Some("#text"));
        let c = dom.create_text_node("x");
        assert_eq!(dom.node_type(c), 3);
    }

    #[test]
    fn set_text_substitui_conteudo() {
        let mut dom = parse_html_to_dom("<p>antes <b>x</b></p>");
        let p = dom.query("p").unwrap();
        dom.set_text(p, "depois");
        let p = idx(&dom, p);
        assert_eq!(dom.node(p).children.len(), 1);
        assert_eq!(dom.node(dom.node(p).children[0]).kind, NodeKind::Text("depois".into()));
    }

    #[test]
    fn set_attr_cria_e_atualiza() {
        let mut dom = parse_html_to_dom("<div>x</div>");
        let div = dom.query("div").unwrap();
        dom.set_attr(div, "class", "card");
        let d = idx(&dom, div);
        assert_eq!(dom.node(d).attr("class"), Some("card"));
        dom.set_attr(div, "class", "card ativo"); // atualiza, não duplica
        assert_eq!(dom.node(d).attr("class"), Some("card ativo"));
        assert_eq!(dom.node(d).attrs.len(), 1);
    }

    #[test]
    fn create_e_append_child() {
        let mut dom = parse_html_to_dom("<ul></ul>");
        let ul = dom.query("ul").unwrap();
        let li = dom.create_element("li");
        dom.set_text(li, "novo item");
        dom.append_child(ul, li);
        let (ul, li) = (idx(&dom, ul), idx(&dom, li));
        assert_eq!(dom.node(ul).children, vec![li]);
        assert_eq!(dom.node(li).parent, Some(ul));
        assert_eq!(tag(&dom, li), "li");
    }

    #[test]
    fn append_move_de_pai_e_remove() {
        let mut dom = parse_html_to_dom("<div><span>x</span></div><section></section>");
        let div = dom.query("div").unwrap();
        let span = dom.query("span").unwrap();
        let section = dom.query("section").unwrap();
        // move o span do div para o section
        dom.append_child(section, span);
        let (di, si, se) = (idx(&dom, div), idx(&dom, span), idx(&dom, section));
        assert!(dom.node(di).children.is_empty());
        assert_eq!(dom.node(se).children, vec![si]);
        assert_eq!(dom.node(si).parent, Some(se));
        // remove o span de vez
        dom.remove_node(span);
        assert!(dom.node(se).children.is_empty());
        assert_eq!(dom.node(si).parent, None);
    }

    #[test]
    fn append_nao_cria_ciclo() {
        let mut dom = parse_html_to_dom("<div><span>x</span></div>");
        let div = dom.query("div").unwrap();
        let span = dom.query("span").unwrap();
        // tentar pôr o div (ancestral) dentro do span deve ser ignorado.
        dom.append_child(span, div);
        let (di, si) = (idx(&dom, div), idx(&dom, span));
        assert_eq!(dom.node(di).parent, Some(dom.root)); // intacto
        assert!(dom.node(si).children.contains(&di) == false);
    }

    #[test]
    fn nodeid_versionado_stale_apos_reparse() {
        // INVARIANTE 2: um NodeId de uma árvore anterior NÃO resolve na nova.
        let dom1 = parse_html_to_dom("<div id='x'>a</div>");
        let id_velho = dom1.query("#x").unwrap();
        let dom2 = parse_html_to_dom("<div id='x'>b</div>");
        // mesmo seletor, árvore nova → gen diferente.
        let id_novo = dom2.query("#x").unwrap();
        assert_ne!(id_velho.generation, id_novo.generation);
        // o id velho é stale na árvore nova: resolve → None (não aplica a nó errado).
        assert_eq!(dom2.resolve(id_velho), None);
        assert!(dom2.resolve(id_novo).is_some());
    }

    #[test]
    fn nodeid_abi_roundtrip() {
        let id = NodeId { generation: 7, idx: 42 };
        let v = id.to_abi();
        assert!(v >= 0);
        assert_eq!(NodeId::from_abi(v), Some(id));
        // sentinela -1 e negativos → None.
        assert_eq!(NodeId::from_abi(-1), None);
        assert_eq!(NodeId::from_abi(-999), None);
    }

    #[test]
    fn atributos_class_id_href_preservados() {
        let dom = parse_html_to_dom(
            "<div class='card' id=\"alvo\"><a href='https://x'>l</a></div>",
        );
        let div = dom.node(dom.root).children[0];
        assert_eq!(dom.node(div).attr("class"), Some("card"));
        assert_eq!(dom.node(div).attr("id"), Some("alvo"));
        assert_eq!(dom.node(div).attr("naoexiste"), None);
        let a = dom.node(div).children[0];
        assert_eq!(tag(&dom, a), "a");
        assert_eq!(dom.node(a).attr("href"), Some("https://x"));
    }

    #[test]
    fn atributos_variantes_aspas_e_booleano() {
        // aspas duplas, simples, sem aspas, e atributo sem valor.
        let dom = parse_html_to_dom("<input type=text value='oi' disabled checked=\"x\">");
        let inp = dom.node(dom.root).children[0];
        assert_eq!(dom.node(inp).attr("type"), Some("text"));   // sem aspas
        assert_eq!(dom.node(inp).attr("value"), Some("oi"));    // aspas simples
        assert_eq!(dom.node(inp).attr("disabled"), Some(""));   // booleano
        assert_eq!(dom.node(inp).attr("checked"), Some("x"));   // aspas duplas
        // `input` é void: não empilha, não tem filhos.
        assert!(dom.node(inp).children.is_empty());
    }

    #[test]
    fn valor_de_atributo_decodifica_entidades() {
        let dom = parse_html_to_dom("<a title='Tom &amp; Jerry'>x</a>");
        let a = dom.node(dom.root).children[0];
        assert_eq!(dom.node(a).attr("title"), Some("Tom & Jerry"));
    }

    #[test]
    fn dump_mostra_atributos() {
        let dom = parse_html_to_dom("<div class='card' id='x'>oi</div>");
        let esperado = "\
#document
  <div class=\"card\" id=\"x\">
    \"oi\"
";
        assert_eq!(dom.dump(), esperado);
    }

    #[test]
    fn arvore_simples_heading_e_paragrafo() {
        let dom = parse_html_to_dom("<h1>Titulo</h1><p>Corpo</p>");
        // Document tem 2 filhos de topo: h1 e p.
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 2);
        assert_eq!(tag(&dom, top[0]), "h1");
        assert_eq!(tag(&dom, top[1]), "p");
        // h1 tem um único filho de texto "Titulo".
        let h1_kids = &dom.node(top[0]).children;
        assert_eq!(h1_kids.len(), 1);
        assert_eq!(dom.node(h1_kids[0]).kind, NodeKind::Text("Titulo".into()));
    }

    #[test]
    fn inline_aninhado_vira_subarvore() {
        // <b> com <i> dentro precisa virar b → i → texto (aninhamento real).
        let dom = parse_html_to_dom("<p>a <b>forte <i>e it</i></b> z</p>");
        let p = dom.node(dom.root).children[0];
        assert_eq!(tag(&dom, p), "p");
        let pk = &dom.node(p).children;
        // p: "a ", <b>, " z"
        assert_eq!(pk.len(), 3);
        assert_eq!(dom.node(pk[0]).kind, NodeKind::Text("a ".into()));
        assert_eq!(tag(&dom, pk[1]), "b");
        assert_eq!(dom.node(pk[2]).kind, NodeKind::Text(" z".into()));
        // <b>: "forte ", <i>
        let bk = &dom.node(pk[1]).children;
        assert_eq!(bk.len(), 2);
        assert_eq!(dom.node(bk[0]).kind, NodeKind::Text("forte ".into()));
        assert_eq!(tag(&dom, bk[1]), "i");
        // <i>: "e it"
        assert_eq!(dom.node(bk[1]).children.len(), 1);
    }

    #[test]
    fn cada_no_conhece_o_pai() {
        let dom = parse_html_to_dom("<p><b>x</b></p>");
        let p = dom.node(dom.root).children[0];
        let b = dom.node(p).children[0];
        let x = dom.node(b).children[0];
        assert_eq!(dom.node(p).parent, Some(dom.root));
        assert_eq!(dom.node(b).parent, Some(p));
        assert_eq!(dom.node(x).parent, Some(b));
    }

    #[test]
    fn tag_desconhecida_e_preservada_como_no() {
        // No caminho de fila <span> some; na árvore ele PERSISTE como elemento.
        let dom = parse_html_to_dom("<p>oi <span>spn</span> tchau</p>");
        let p = dom.node(dom.root).children[0];
        let pk = &dom.node(p).children;
        assert_eq!(pk.len(), 3);
        assert_eq!(tag(&dom, pk[1]), "span");
        assert_eq!(dom.node(pk[1]).children.len(), 1);
    }

    #[test]
    fn entidades_decodificadas() {
        let dom = parse_html_to_dom("<p>a &lt; b &amp; c &gt; d</p>");
        let p = dom.node(dom.root).children[0];
        let txt = dom.node(dom.node(p).children[0]).kind.clone();
        assert_eq!(txt, NodeKind::Text("a < b & c > d".into()));
    }

    #[test]
    fn fechamento_orfao_nao_quebra() {
        // </div> sem abertura é ignorado; texto ao redor preservado.
        let dom = parse_html_to_dom("</div><p>ok</p>");
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 1);
        assert_eq!(tag(&dom, top[0]), "p");
    }

    #[test]
    fn void_tag_nao_empilha() {
        // <br> não tem fechamento; o <p> seguinte deve ser irmão, não filho.
        let dom = parse_html_to_dom("<br><p>depois</p>");
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 2);
        assert_eq!(tag(&dom, top[0]), "br");
        assert_eq!(tag(&dom, top[1]), "p");
        assert!(dom.node(top[0]).children.is_empty());
    }

    #[test]
    fn dump_legivel_para_inspecao() {
        let dom = parse_html_to_dom("<h1>Oi</h1><p>antes <b>forte</b></p>");
        let esperado = "\
#document
  <h1>
    \"Oi\"
  <p>
    \"antes \"
    <b>
      \"forte\"
";
        assert_eq!(dom.dump(), esperado);
    }

    // ── Parser para páginas REAIS: DOCTYPE, `>` em atributo, void tags, ──────────
    // ── auto-fechamento implícito (HTML5 tag omission) ───────────────────────────

    #[test]
    fn doctype_nao_vira_elemento() {
        // Antes, `<!DOCTYPE html>` virava `Element { tag: "!doctype" }` que
        // EMPILHAVA na pilha de abertos (a "tag" nunca fecha) — o documento
        // INTEIRO aninhava como filho dele. Agora o tokenizador ignora `<!…>`
        // (não modelamos DocumentType, nodeType 10 fora do escopo) e `html`
        // é filho direto do #document.
        let dom = parse_html_to_dom("<!DOCTYPE html><html><body><p>x</p></body></html>");
        let html_el = dom.query("html").unwrap();
        assert_eq!(dom.parent_of(html_el).map(|p| idx(&dom, p)), Some(dom.root));
        // único elemento de topo — nada de "!doctype" fantasma na raiz.
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 1);
        assert_eq!(tag(&dom, top[0]), "html");
        // body é filho do html, e o texto chega intacto.
        let body = dom.query("body").unwrap();
        assert_eq!(dom.parent_of(body), Some(html_el));
        assert_eq!(dom.text_content(dom.query("p").unwrap()).unwrap(), "x");
    }

    #[test]
    fn atributo_com_maior_que_no_valor() {
        // `<div title="a>b">`: o `>` dentro do valor com aspas não termina a
        // tag — antes o tokenizador cortava no primeiro `>` cru, o atributo
        // vinha truncado (`title="a`) e `b">` vazava como texto.
        let dom = parse_html_to_dom(r#"<div title="a>b">x</div>"#);
        let div = dom.query("div").unwrap();
        let n = dom.node(idx(&dom, div));
        assert_eq!(n.attr("title"), Some("a>b"));
        assert_eq!(dom.text_content(div).unwrap(), "x");
        // um único elemento de topo (nenhuma tag-fantasma criada pela quebra).
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 1);
    }

    #[test]
    fn void_tags_completas_nao_empilham() {
        // `source`/`track` (e as demais void novas: area/base/col/embed/wbr)
        // não têm fechamento — se empilhassem, o `</video>` não casaria e o
        // `<p>` seguinte viraria DESCENDENTE do video em vez de irmão.
        let dom = parse_html_to_dom("<video><source src=\"a.mp4\"><track></video><p>x</p>");
        let video = dom.query("video").unwrap();
        let p = dom.query("p").unwrap();
        let source = dom.query("source").unwrap();
        let track = dom.query("track").unwrap();
        // source/track são filhos do video (não empilham nem engolem irmãos)…
        assert_eq!(dom.parent_of(source), Some(video));
        assert_eq!(dom.parent_of(track), Some(video));
        // …e p é IRMÃO do video (filho do #document), não descendente.
        assert_eq!(dom.parent_of(p).map(|x| idx(&dom, x)), Some(dom.root));
        assert_eq!(dom.next_sibling(video), Some(p));
    }

    #[test]
    fn li_fecha_li_implicito() {
        // HTML5 tag omission: um `<li>` novo fecha o `<li>` corrente — os dois
        // viram IRMÃOS dentro do `<ul>` (antes o segundo aninhava no primeiro).
        let dom = parse_html_to_dom("<ul><li>a<li>b</ul>");
        let ul = dom.query("ul").unwrap();
        let kids = dom.child_nodes(ul);
        assert_eq!(kids.len(), 2);
        assert_eq!(tag(&dom, idx(&dom, kids[0])), "li");
        assert_eq!(tag(&dom, idx(&dom, kids[1])), "li");
        assert_eq!(dom.text_content(kids[0]).unwrap(), "a");
        assert_eq!(dom.text_content(kids[1]).unwrap(), "b");
    }

    #[test]
    fn dt_dd_e_option_implicitos() {
        // <dt>/<dd> fecham o dt/dd corrente (termo e definição são irmãos)…
        let dom = parse_html_to_dom("<dl><dt>t<dd>d</dl>");
        let dl = dom.query("dl").unwrap();
        let kids = dom.child_nodes(dl);
        assert_eq!(kids.len(), 2);
        assert_eq!(tag(&dom, idx(&dom, kids[0])), "dt");
        assert_eq!(tag(&dom, idx(&dom, kids[1])), "dd");
        // …e <option> fecha o option corrente.
        let dom2 = parse_html_to_dom("<select><option>1<option>2</select>");
        let sel = dom2.query("select").unwrap();
        let opts = dom2.child_nodes(sel);
        assert_eq!(opts.len(), 2);
        assert_eq!(dom2.text_content(opts[0]).unwrap(), "1");
        assert_eq!(dom2.text_content(opts[1]).unwrap(), "2");
    }

    #[test]
    fn p_fecha_p_e_bloco_fecha_p() {
        // `<p>a<p>b`: p nunca aninha em p — o segundo fecha o primeiro (irmãos).
        let dom = parse_html_to_dom("<p>a<p>b");
        let p1 = dom.query("p").unwrap();
        let p2 = dom.next_sibling(p1).expect("segundo <p> deveria ser irmão");
        assert_eq!(dom.text_content(p1).unwrap(), "a");
        assert_eq!(dom.text_content(p2).unwrap(), "b");
        // `<p>texto<div>`: a regra do HTML5 que MAIS aparece em páginas reais —
        // a abertura de um elemento de bloco fecha o <p> aberto.
        let dom2 = parse_html_to_dom("<p>texto<div>x</div>");
        let p = dom2.query("p").unwrap();
        let div = dom2.query("div").unwrap();
        assert_eq!(dom2.parent_of(div).map(|x| idx(&dom2, x)), Some(dom2.root));
        assert_eq!(dom2.next_sibling(p), Some(div));
        assert_eq!(dom2.text_content(p).unwrap(), "texto"); // o "x" NÃO entrou no p
    }

    #[test]
    fn tabela_com_td_tr_implicitos() {
        // `<td>` fecha a célula corrente; `<tr>` fecha a célula E o tr do topo.
        // (Divergência consciente da spec: não sintetizamos `<tbody>` — os tr
        // ficam filhos diretos do table.)
        let dom = parse_html_to_dom("<table><tr><td>a<td>b<tr><td>c</table>");
        let table = dom.query("table").unwrap();
        let trs = dom.child_nodes(table);
        assert_eq!(trs.len(), 2);
        assert_eq!(tag(&dom, idx(&dom, trs[0])), "tr");
        assert_eq!(tag(&dom, idx(&dom, trs[1])), "tr");
        let tds1 = dom.child_nodes(trs[0]);
        assert_eq!(tds1.len(), 2);
        assert_eq!(dom.text_content(tds1[0]).unwrap(), "a");
        assert_eq!(dom.text_content(tds1[1]).unwrap(), "b");
        let tds2 = dom.child_nodes(trs[1]);
        assert_eq!(tds2.len(), 1);
        assert_eq!(dom.text_content(tds2[0]).unwrap(), "c");
    }

    #[test]
    fn li_novo_nao_fecha_li_de_lista_ancestral() {
        // O fechamento implícito só olha o TOPO da pilha: o `<li>` de uma
        // sublista NÃO fecha o `<li>` do `<ul>` ancestral (o topo ali é `ul`,
        // nada casa). Fechar "através" do container colapsaria a sublista.
        let dom = parse_html_to_dom("<ul><li>a<ul><li>b</li></ul></li></ul>");
        let outer_ul = dom.query("ul").unwrap();
        let outer_kids = dom.child_nodes(outer_ul);
        assert_eq!(outer_kids.len(), 1); // só o li "a…"
        let li_a = outer_kids[0];
        assert_eq!(tag(&dom, idx(&dom, li_a)), "li");
        // dentro do li: o texto "a" + a sublista com o li "b".
        let inner_ul = dom
            .child_nodes(li_a)
            .into_iter()
            .find(|&k| dom.node_type(k) == 1)
            .expect("sublista deveria estar DENTRO do li externo");
        assert_eq!(tag(&dom, idx(&dom, inner_ul)), "ul");
        let inner_kids = dom.child_nodes(inner_ul);
        assert_eq!(inner_kids.len(), 1);
        assert_eq!(dom.text_content(inner_kids[0]).unwrap(), "b");
    }

    #[test]
    fn pagina_real_bootstrap_cover() {
        // Valida contra uma página REAL (Bootstrap 5.3 "cover": `<!doctype html>`
        // minúsculo, tags multi-linha, `<meta/>`/`<link/>` autofecháveis, <svg>,
        // <style> longo): `html` deve ser filho DIRETO do #document — o doctype
        // não pode virar elemento que aninha o documento — e head/body filhos
        // de html. O corpus vive em `examples/` (ainda não versionado); se
        // ausente (ex.: CI antes do corpus entrar), o teste é um no-op EXPLÍCITO.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/bootstrap-5.3.8-examples/cover/index.html"
        );
        let Ok(src) = std::fs::read_to_string(path) else {
            eprintln!("corpus real ausente ({path}) — validação da página pulada");
            return;
        };
        let dom = parse_html_to_dom(&src);
        let html_el = dom.query("html").unwrap();
        assert_eq!(dom.parent_of(html_el).map(|p| idx(&dom, p)), Some(dom.root));
        // único elemento de topo (sem "!doctype" fantasma).
        let top: Vec<_> = dom
            .node(dom.root)
            .children
            .iter()
            .filter(|&&c| matches!(dom.node(c).kind, NodeKind::Element { .. }))
            .collect();
        assert_eq!(top.len(), 1);
        // head e body são filhos de html.
        let head = dom.query("head").unwrap();
        let body = dom.query("body").unwrap();
        assert_eq!(dom.parent_of(head), Some(html_el));
        assert_eq!(dom.parent_of(body), Some(html_el));
        // conteúdo real chegou: o h1 do template.
        let h1 = dom.query("h1").unwrap();
        assert_eq!(dom.text_content(h1).unwrap(), "Cover your page.");
    }
}
