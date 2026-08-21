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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::html::{Token, tokenize};

mod animacao;
mod caches;
mod consulta;
mod eventos;
mod formulario;
mod geometria;
mod helpers;
mod invalidacao;
mod matcher;
mod no;
mod parser;
mod serial;
mod travessia;

pub use self::no::{Attr, Node, NodeId, NodeKind};
pub use self::parser::{parse_fragmento, parse_html_to_dom};
use self::matcher::TargetKey;
use self::helpers::{memo_forget, memo_put, nth_casa};
use self::parser::is_void;
use self::helpers::{is_plain_ident, references_self, upsert_css_decl};

/// Índice cru de um nó na arena (`Dom::nodes`). Uso INTERNO ao `dom.rs` — o que
/// cruza a fronteira (TS/ABI) é sempre o `NodeId` VERSIONADO, nunca este índice.
pub type NodeIdx = usize;

/// Chave de uma medição de layout descartável. O cache guarda apenas `(outer_w,
/// outer_h)`, nunca itens de pintura; por isso a posição `(x,y)` não participa.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct LayoutMeasureKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    pub(crate) node: NodeIdx,
    pub(crate) avail_w: u32,
    pub(crate) avail_h: Option<u32>,
    pub(crate) forced_outer_w: Option<u32>,
    pub(crate) forced_outer_h: Option<u32>,
    pub(crate) shrink_to_fit: bool,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    pub(crate) measurer: u64,
}

/// A chave de um FRAGMENTO de layout: o desenho de uma subárvore posta com
/// certas constraints.
///
/// É a `LayoutMeasureKey` sem a posição — porque a posição é justamente o que se
/// corrige ao reusar (o desenho é o mesmo, deslocado). Os campos são os mesmos
/// pela mesma razão: cada um protege uma dependência do resultado. `node_epoch`
/// cobre mudanças na subárvore, `style_epoch` as globais de estilo, o viewport e
/// o medidor cobrem o resto do ambiente.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FragmentKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    pub(crate) anim_epoch: u64,
    pub(crate) node: NodeIdx,
    pub(crate) avail_w: u32,
    pub(crate) avail_h: Option<u32>,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    pub(crate) measurer: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct IntrinsicWidthKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    pub(crate) node: NodeIdx,
    pub(crate) font_size: u32,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    pub(crate) measurer: u64,
}

/// Contador global de gerações de árvore. Cada `Dom` novo (parse ou vazio) toma a
/// próxima geração; assim duas árvores nunca colidem e um `NodeId` de uma árvore
/// velha é detectável como stale na árvore atual.
static NEXT_GEN: AtomicU32 = AtomicU32::new(1);

fn next_gen() -> u32 {
    NEXT_GEN.fetch_add(1, Ordering::Relaxed)
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
    /// Índice `valor-de-id → [NodeIdx]`. Mantém todos os candidatos para que a
    /// consulta possa escolher o primeiro em ordem documental, como no browser.
    id_index: HashMap<String, Vec<NodeIdx>>,
    /// Índice `classe → nós que a têm` (usado como filtro; a ordem final vem da árvore).
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
    /// O nó SOB O CURSOR (hit-test do backend, por frame) — o estado do `:hover`
    /// vivo. `None` = ponteiro fora do conteúdo. Cell: mutável sem `&mut` (o
    /// matcher lê durante a cascade). Setar via [`Dom::set_hovered`] (que só
    /// invalida caches quando MUDA e quando o stylesheet tem regra `:hover`).
    hovered: std::cell::Cell<Option<NodeIdx>>,
    /// Eventos CRUS vindos do BACKEND (hit-test do mouse): `(nó-alvo, tipo)` SEM
    /// expansão de bubbling/listeners — o backend só empurra "clicou no nó X"; a
    /// fachada TS drena via `pumpEventCallbacks` e faz o dispatch completo
    /// (bubbling + callbacks + fila de polling). Padrão 1-frame-latency do
    /// north-star §3.
    raw_event_queue: std::collections::VecDeque<(NodeIdx, String)>,
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
    /// O valor é `Rc` e não `ComputedStyle`: um `ComputedStyle` tem 1000 bytes
    /// (medido por `metrics::footprint::type_sizes`), e um hit de memo devolvia
    /// uma CÓPIA deles. Num relayout de 3000 elementos isso eram ~12 MB de
    /// memcpy por frame com 100% de acerto de cache — o cache funcionando e
    /// custando. Com `Rc`, um hit é um incremento de contador.
    /// Indexado pelo índice da arena, e não um `HashMap`: a arena é DENSA, e
    /// o layout pede o estilo ~2,4 vezes por nó por passada — eram 12 000
    /// lookups com SipHash por frame numa página de 3000 elementos, para
    /// responder o que um índice de vetor responde sem hash nenhum. `None` =
    /// não memoizado; o vetor cresce sob demanda até `nodes.len()`.
    computed_memo: std::cell::RefCell<Vec<Option<std::rc::Rc<crate::style::ComputedStyle>>>>,
    /// O epoch de animação em que o `computed_memo` foi preenchido. Mutações locais
    /// de conteúdo não invalidam esse cache; animações continuam invalidando o estilo
    /// final interpolado.
    memo_revision: std::cell::Cell<u64>,
    /// O epoch global de estilos em que o `computed_memo` foi preenchido.
    memo_style_epoch: std::cell::Cell<u64>,
    /// Memo do ALVO-BASE (cascade SEM a camada de animação, `with_anim=false`) que o
    /// `advance` consulta por nó a cada frame. Invalida por epoch global de estilo,
    /// viewport ou dirty bit local — então frames de animação que só bumpam
    /// `anim_epoch` o REUSAM, tornando o `advance` barato.
    base_memo: std::cell::RefCell<Vec<Option<std::rc::Rc<crate::style::ComputedStyle>>>>,
    /// A revisão estrutural em que o `base_memo` foi preenchido.
    base_memo_revision: std::cell::Cell<u64>,
    base_memo_viewport: std::cell::Cell<(u32, u32)>,
    /// Memo da tabela de CONTADORES do documento (ver [`crate::counters`]).
    ///
    /// É por DOCUMENTO e não por nó, porque a resposta de um nó depende de tudo
    /// o que veio antes dele em ordem documental — memoizar por nó guardaria n
    /// cópias de uma travessia que se faz uma vez. Invalidado pelas mesmas duas
    /// chaves que o `base_memo`: a revisão estrutural (a árvore mudou de ordem)
    /// e o epoch de estilo (as regras mudaram).
    ///
    /// `None` = ainda não calculada nesta revisão; uma página sem contadores
    /// calcula uma tabela VAZIA e volta a acertar o memo, em vez de refazer a
    /// pergunta por cada pseudo-elemento.
    counter_memo: std::cell::RefCell<Option<std::rc::Rc<crate::counters::Tabela>>>,
    counter_memo_revision: std::cell::Cell<(u64, u64)>,
    /// Cache derivado de medições de bloco feitas em listas descartáveis durante
    /// flex/grid/inline-block/out-of-flow. É limpo em qualquer mutação visual para
    /// não reutilizar tamanho sob estilo ou conteúdo stale.
    layout_measure_cache:
        std::cell::RefCell<crate::fasthash::FastMap<LayoutMeasureKey, (f32, f32)>>,
    /// Cache derivado de largura intrínseca (max-content), usada pelos pré-passos
    /// de shrink-to-fit/flex/grid. A chave inclui o contexto tipográfico completo.
    intrinsic_width_cache: std::cell::RefCell<crate::fasthash::FastMap<IntrinsicWidthKey, f32>>,
    /// Epoch local de geometria. Filhos e ancestrais são incrementados quando uma
    /// mutação pode alterar seu tamanho; irmãos independentes mantêm o valor.
    layout_epochs: Vec<u64>,
    /// O VIEWPORT corrente (w, h) — setado pelo layout no início da passada
    /// ([`set_viewport`](Dom::set_viewport)); a base de `vw`/`vh` na cascade
    /// (font-size fluido/calc) e do `@media` futuro. Default 1280×800 (headless).
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
    /// Pixels que o PRÓPRIO documento guarda (o `<canvas>` que um programa
    /// pintou). Distinto do `image_pixels`, que aponta para um buffer de fora.
    own_pixels: HashMap<NodeIdx, (std::rc::Rc<Vec<u8>>, u32, u32)>,
    /// A posição de cada nó em ORDEM DOCUMENTAL (pré-ordem), numerada sob
    /// demanda e reusada enquanto a árvore não muda. É o que permite responder
    /// uma consulta a partir dos ÍNDICES `#id`/`.classe` — que dão os candidatos
    /// em ordem de arena — sem perder a ordem que o `querySelectorAll` promete.
    /// `u64` = a revisão em que foi numerada.
    doc_order: std::cell::RefCell<(u64, Vec<u32>)>,
    /// Por ANCESTRAL, quais filhos DIRETOS têm sujeira abaixo — anotado durante
    /// a subida que a invalidação já faz. É o que permite refazer só o ramo que
    /// mudou em vez de percorrer o container inteiro.
    dirty_children: std::cell::RefCell<crate::fasthash::FastMap<NodeIdx, Vec<NodeIdx>>>,
    /// Os nós que foram ALVO DIRETO de uma invalidação. O desenho deles não pode
    /// ser aproveitado do anterior; o dos ancestrais pode.
    dirty_self: std::cell::RefCell<std::collections::HashSet<NodeIdx>>,
    /// O ÚLTIMO fragmento de cada nó, com a chave que o validava — a pergunta
    /// "o que este nó desenhou da última vez?", que o cache por chave não responde.
    last_fragment: std::cell::RefCell<
        crate::fasthash::FastMap<NodeIdx, (FragmentKey, std::rc::Rc<crate::layout::Fragment>)>,
    >,
    /// FRAGMENTOS de layout por subárvore — o desenho de um bloco com certas
    /// constraints, guardado em coordenadas relativas à origem em que foi posto.
    /// É o que torna o layout INCREMENTAL: mudar uma folha invalida o epoch dela
    /// e dos ancestrais, e todo irmão intacto reusa o fragmento em vez de
    /// recalcular cascade, medição de texto e box model.
    fragment_cache: std::cell::RefCell<
        crate::fasthash::FastMap<FragmentKey, std::rc::Rc<crate::layout::Fragment>>,
    >,
    /// A ÚLTIMA `DisplayList` calculada, com a chave que a validou — o layout
    /// inteiro reusado enquanto nada que o afete mudar (ver
    /// [`crate::layout::layout_cached`]). Um só slot: o padrão é reperguntar
    /// pelo MESMO estado (uma consulta de geometria atrás da outra, um frame
    /// atrás do outro), não alternar entre viewports.
    display_cache:
        std::cell::RefCell<Option<(DisplayKey, std::rc::Rc<crate::layout::DisplayList>)>>,
    /// Algum `style=""` inline desta árvore menciona `position`.
    ///
    /// Junto com [`Stylesheet::has_out_of_flow`](crate::style::Stylesheet::has_out_of_flow),
    /// responde "esta página PODE ter elemento fora do fluxo?" — e quando a
    /// resposta é não, a passada que procura por eles (uma varredura da árvore
    /// inteira pedindo o estilo computado de cada nó) não precisa acontecer.
    inline_position: std::cell::Cell<bool>,
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
            raw_event_queue: std::collections::VecDeque::new(),
            hovered: std::cell::Cell::new(None),
            event_queue: std::collections::VecDeque::new(),
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

    /// O ALVO-BASE (cascade sem animação) de um nó, MEMOIZADO por revisão estrutural.
    /// O `advance` consulta isto a cada frame; entre frames de animação (revisão
    /// estrutural estável) é um hit de cache — a cascade não re-roda. `None` p/
    /// não-elemento.
    fn base_style_idx(&self, idx: NodeIdx) -> Option<std::rc::Rc<crate::style::ComputedStyle>> {
        let style_epoch = crate::style::props::style_epoch();
        let (vw, vh) = self.viewport.get();
        let vp_key = (vw.to_bits(), vh.to_bits());
        if self.base_memo_revision.get() != style_epoch || self.base_memo_viewport.get() != vp_key {
            self.base_memo.borrow_mut().clear();
            self.base_memo_revision.set(style_epoch);
            self.base_memo_viewport.set(vp_key);
        }
        crate::bump!(base_calls);
        if let Some(Some(hit)) = self.base_memo.borrow().get(idx) {
            crate::bump!(base_memo_hits);
            return Some(std::rc::Rc::clone(hit));
        }
        let computed = std::rc::Rc::new(self.computed_style_idx_inner(idx)?);
        memo_put(
            &mut self.base_memo.borrow_mut(),
            idx,
            self.nodes.len(),
            &computed,
        );
        Some(computed)
    }

    /// Acrescenta o conteúdo de um `<style>` ao stylesheet de autor da página
    /// (chamado pelo parser ao encontrar um `RawElement` de `style`). Vários
    /// `<style>` acumulam, com as regras posteriores desempatando por cima.
    pub fn add_stylesheet(&mut self, css: &str) {
        let _phase = crate::metrics::phases::scope("parse-css");
        crate::bump!(stylesheets_added);
        crate::bump!(css_bytes, css.len());
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
    fn deindex_node(&mut self, id: NodeIdx) {
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

    fn remove_index_key(&mut self, key: &str, id: NodeIdx, is_id: bool) {
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

    fn index_node(&mut self, id: NodeIdx) {
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
    fn push(&mut self, kind: NodeKind, attrs: Vec<Attr>, parent: NodeIdx) -> NodeIdx {
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

    // ── Mutação (base da API DOM do JS) ─────────────────────────────────────

    /// Substitui TODO o conteúdo de um elemento por um único nó de texto (o
    /// equivalente a `element.textContent = txt`). Não faz nada num nó de texto.
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        crate::bump!(set_text);
        let Some(idx) = self.resolve(id) else { return };
        if !matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            return;
        }
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
    }

    /// Resolve o `ComputedStyle` final de um nó pela cascade da MDN (estágio 1
    /// origem/importância → especificidade → ordem). É o estado que o LAYOUT (em
    /// TS) e o render leem para decidir cor/caixa/tamanho. `None` se o id não
    /// resolve ou não é elemento. (A herança de color/font-size é aplicada por quem
    /// desce a árvore; aqui só o estilo PRÓPRIO do nó.)
    pub fn computed_style(&self, id: NodeId) -> Option<crate::style::ComputedStyle> {
        // A API pública devolve VALOR: quem chama de fora (a ABI, o `getComputedStyle`)
        // quer um dado próprio, e é uma chamada por vez — o `Rc` existe para o
        // caminho interno do layout, que pede o mesmo estilo dezenas de vezes.
        self.computed_style_idx(self.resolve(id)?)
            .map(|rc| (*rc).clone())
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
    /// Devolve um `Rc`: ver a nota no campo `computed_memo` — o valor tem 1 KB e
    /// o layout o pede várias vezes por nó. Quem precisa MUTAR faz
    /// `(*rc).clone()`, o que é exatamente o ponto (a cópia passa a ser
    /// explícita e rara em vez de implícita e por acesso).
    pub fn computed_style_idx(
        &self,
        idx: NodeIdx,
    ) -> Option<std::rc::Rc<crate::style::ComputedStyle>> {
        // MEMO por revisão: dentro de um mesmo estado da árvore, a cascade de um nó
        // é determinística — e o layout a consulta várias vezes por nó (medição +
        // pintura). Um clone do ComputedStyle é muito mais barato que re-rodar
        // todas as regras do stylesheet (Bootstrap: ~2700).
        let anim_epoch = self.anim_epoch;
        let style_epoch = crate::style::props::style_epoch();
        let (vw, vh) = self.viewport.get();
        let vp_key = (vw.to_bits(), vh.to_bits());
        if self.memo_revision.get() != anim_epoch
            || self.memo_style_epoch.get() != style_epoch
            || self.memo_viewport.get() != vp_key
        {
            self.computed_memo.borrow_mut().clear();
            self.memo_revision.set(anim_epoch);
            self.memo_style_epoch.set(style_epoch);
            self.memo_viewport.set(vp_key);
        }
        crate::bump!(computed_calls);
        if let Some(Some(hit)) = self.computed_memo.borrow().get(idx) {
            crate::bump!(computed_memo_hits);
            return Some(std::rc::Rc::clone(hit));
        }
        // O estilo COM animação = a BASE (cascade sem anim, memoizada por revisão
        // estrutural via `base_style_idx`) + a camada de `anim_override` por cima. Não
        // re-roda a cascade a cada frame de animação: só clona a base cacheada e
        // sobrepõe o override interpolado — o que torna o RELAYOUT durante animação
        // barato (era o gargalo restante depois de acelerar o `advance`).
        let base = self.base_style_idx(idx)?;
        // SEM animação, o computado É a base: compartilha o mesmo `Rc` em vez de
        // materializar uma segunda cópia de 1 KB por nó. Só quem anima paga a
        // cópia, que é quando ela é de fato necessária (o override interpolado
        // muda a cada frame).
        let computed = match self.anim_override.get(&idx) {
            None => base,
            Some(anim) => {
                let mut c = (*base).clone();
                c.merge_over(anim);
                std::rc::Rc::new(c)
            }
        };
        memo_put(
            &mut self.computed_memo.borrow_mut(),
            idx,
            self.nodes.len(),
            &computed,
        );
        Some(computed)
    }

    /// A CAIXA GERADA de um pseudo-elemento deste nó, ou `None` quando a
    /// cascata não manda gerar nenhuma.
    ///
    /// `None` cobre os quatro casos em que não há caixa, e são todos da spec:
    /// nenhuma regra `::before`/`::after` casa; nenhuma delas declara `content`;
    /// o `content` vencedor é `none`/`normal`; ou o pseudo tem `display:none`.
    ///
    /// O estilo é o do elemento originante HERDADO e depois sobreposto pelas
    /// declarações do pseudo — herdar do elemento e não da raiz é o que faz um
    /// `::before` sem `color` sair da cor do texto à volta, como no browser.
    pub fn pseudo_box(
        &self,
        idx: NodeIdx,
        pe: crate::style::PseudoElement,
    ) -> Option<crate::pseudo::PseudoBox> {
        if !self.stylesheet.has_generated_content() {
            return None;
        }
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return None;
        };
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let (matched, content) = self.stylesheet.matched_for_pseudo(
            self.viewport.get().0,
            tag,
            self.nodes[idx].attr("id"),
            &classes,
            pe,
            |sel| self.matches_complex(idx, sel),
        );
        let content = content?;
        let contadores = self.document_counters();
        let texto = crate::pseudo::texto_de(
            &content,
            &|nome: &str| self.nodes[idx].attr(nome).map(str::to_string),
            contadores.get(&(idx, pe)),
        )?;
        let decls = self.stylesheet.declarations_from(&matched, None);
        // Herda do originante e só depois aplica o que o pseudo declara — a
        // ordem inversa perderia a herança para qualquer propriedade que o
        // pseudo não declare.
        let mut css = crate::style::ComputedStyle::default();
        if let Some(pai) = self.computed_style_idx(idx) {
            css.inherit_from(&pai);
        }
        css.merge_over(&decls.normal);
        css.merge_over(&decls.important);
        if css.effective_display() == Some(crate::style::DisplayKind::None) {
            return None;
        }
        Some(crate::pseudo::PseudoBox { texto, css })
    }

    /// A tabela de CONTADORES do documento, calculada uma vez por revisão.
    ///
    /// Numa página que não declare `counter-reset`/`counter-increment` isto é
    /// uma tabela vazia e a travessia nem corre — a guarda é a mesma ideia do
    /// `has_generated_content()` que abre o `pseudo_box`, e pela mesma razão:
    /// três das quatro folhas do corpus não têm contador nenhum.
    fn document_counters(&self) -> std::rc::Rc<crate::counters::Tabela> {
        let chave = (self.revision, crate::style::props::style_epoch());
        if self.counter_memo_revision.get() == chave {
            if let Some(t) = self.counter_memo.borrow().as_ref() {
                return std::rc::Rc::clone(t);
            }
        }
        let tabela = if self.stylesheet.has_counters() {
            crate::counters::calcula(self, &|idx, pe| self.counter_ops(idx, pe))
        } else {
            crate::counters::Tabela::default()
        };
        let tabela = std::rc::Rc::new(tabela);
        *self.counter_memo.borrow_mut() = Some(std::rc::Rc::clone(&tabela));
        self.counter_memo_revision.set(chave);
        tabela
    }

    /// As operações de contador de um elemento (`pe: None`) ou de um dos seus
    /// pseudo-elementos, já resolvidas pela cascata.
    ///
    /// O `style=""` inline NÃO é consultado: `counter-increment` num atributo de
    /// estilo não aparece em nenhuma das quatro folhas do corpus, e lê-lo
    /// exigiria parsear o atributo por nó nesta passagem — o custo por elemento
    /// que a guarda de `has_counters` existe para evitar. Fica dito por ser um
    /// corte e não um esquecimento.
    fn counter_ops(
        &self,
        idx: NodeIdx,
        pe: Option<crate::style::PseudoElement>,
    ) -> Option<std::rc::Rc<crate::counters::Ops>> {
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return None;
        };
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let vw = self.viewport.get().0;
        let id_attr = self.nodes[idx].attr("id");
        let matched = match pe {
            None => self
                .stylesheet
                .matched_for_node(vw, tag, id_attr, &classes, |sel| {
                    self.matches_complex(idx, sel)
                }),
            Some(pe) => {
                self.stylesheet
                    .matched_for_pseudo(vw, tag, id_attr, &classes, pe, |sel| {
                        self.matches_complex(idx, sel)
                    })
                    .0
            }
        };
        self.stylesheet.counters_from(&matched)
    }

    /// Núcleo da cascade — computa o ALVO-BASE de um nó (SEM a camada de animação; o
    /// override interpolado é sobreposto por quem consome, em `computed_style_idx`).
    /// Chamado via `base_style_idx` (memoizado por revisão estrutural).
    fn computed_style_idx_inner(&self, idx: NodeIdx) -> Option<crate::style::ComputedStyle> {
        use crate::style;
        let tag = match &self.nodes[idx].kind {
            NodeKind::Element { tag } => tag.as_str(),
            _ => return None,
        };
        crate::bump!(cascade_runs);
        let _phase = crate::metrics::phases::scope("cascade");
        // id/classes só são materializados quando há regras de autor para testar.
        // Em páginas sem `<style>`, o layout ainda computa cada nó, mas não precisa
        // alocar strings que nunca serão consultadas pelo RuleIndex.
        let node_id: Option<String> = if self.stylesheet.is_empty() {
            None
        } else {
            self.nodes[idx].attr("id").map(str::to_string)
        };
        let node_classes: Vec<String> = if self.stylesheet.is_empty() {
            Vec::new()
        } else {
            self.nodes[idx]
                .attr("class")
                .map(|c| c.split_whitespace().map(str::to_string).collect())
                .unwrap_or_default()
        };
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
        // As regras que casam este nó, casadas UMA vez e usadas nos DOIS passes
        // (custom properties e declarações). Antes cada passe refazia o
        // matching completo — e o matching navega a árvore.
        let matched = if self.stylesheet.is_empty() {
            style::MatchedRules::default()
        } else {
            self.stylesheet.matched_for_node(
                self.viewport.get().0,
                &tag,
                node_id.as_deref(),
                &class_refs,
                |sel| self.matches_complex(idx, sel),
            )
        };
        let own_customs: Vec<(String, String)> = if self.stylesheet.is_empty() {
            inline.custom.clone()
        } else {
            let mut v = self.stylesheet.custom_from(&matched);
            v.extend(inline.custom.iter().cloned());
            v
        };
        let parent_vars = parent_css_for_vars
            .as_ref()
            .and_then(|p| p.custom_props.clone());
        let vars_arc: Option<std::sync::Arc<std::collections::HashMap<String, String>>> =
            match (parent_vars, own_customs.is_empty()) {
                (p, true) => p, // só herda: compartilha o Arc (O(1))
                (p, false) => {
                    crate::bump!(custom_maps_built);
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
            self.stylesheet.declarations_from(&matched, Some(vars_ref))
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
                root_font_size: crate::style::root_font_size(),
                viewport_w: vw,
                viewport_h: vh,
            };
            css.font_size = d
                .resolve(&rctx)
                .filter(|v| *v > 0.0)
                .map(style::Dimension::Px);
        }
        // A fonte do `<html>` é a BASE DO `rem` para a árvore inteira — o idioma
        // `html { font-size: 62.5% }` faz `1rem` valer 10px, e sem esta linha
        // ficava nos 16px de default e todo o `rem` da página saía 60% grande
        // demais. Escrito aqui porque a cascade corre de cima para baixo: quando
        // um descendente resolve o seu `rem`, a raiz já passou por aqui.
        if matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "html") {
            // Sem declaração no root, a base VOLTA aos 16px. É o que impede o
            // valor de um documento de sobreviver ao seguinte: o estado é por
            // thread (como o estilo por tag) e um `html { font-size: 10px }` de
            // uma página ficaria a valer na próxima que não declarasse nada.
            style::set_root_font_size(match css.font_size {
                Some(style::Dimension::Px(v)) => v,
                _ => crate::layout::DEFAULT_FONT_SIZE,
            });
        }

        // ── HERANÇA (CSS inherited properties): color/font/text-align/etc. que NÃO
        // foram declaradas neste nó descem do PAI-elemento. É o que faz o texto pegar
        // a cor do body sem cada elemento redeclarar (sem isto, texto fica preto).
        if let Some(parent_css) = &parent_css {
            crate::bump!(inherit_steps);
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
        // `computed_value` e não `get_property`: o computed NUNCA responde vazio
        // — o que ninguém declarou vale o INICIAL (`float: none`, `color:
        // rgb(0, 0, 0)`). O `get_property` cru continua a servir o
        // `el.style.x`, que TEM de responder vazio fora do `style=""`. A tag vai
        // junto porque o inicial de `display` é o da UA-stylesheet dela.
        let tag = self
            .resolve(id)
            .and_then(|idx| match &self.nodes[idx].kind {
                NodeKind::Element { tag } => Some(tag.clone()),
                _ => None,
            });
        self.computed_style(id)
            .map(|c| c.computed_value(name, tag.as_deref()))
            .unwrap_or_default()
    }

    /// `el.style.<name>` (getPropertyValue) — o valor INLINE da propriedade (só o
    /// `style=""`, sem a cascade), no formato do browser. `""` se ausente.
    pub fn inline_property(&self, id: NodeId, name: &str) -> String {
        let Some(idx) = self.resolve(id) else {
            return String::new();
        };
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
        self.set_attr(id, "style", text);
    }

    /// `el.style.setProperty(name, value)` — define UMA propriedade no `style=""`
    /// inline, preservando as demais. Re-serializa a string `style`. Valor vazio
    /// REMOVE a propriedade (como `removeProperty`).
    pub fn set_style_property(&mut self, id: NodeId, name: &str, value: &str) {
        let cur = self.css_text(id);
        let new = upsert_css_decl(&cur, name.trim(), value.trim());
        self.set_attr(id, "style", &new);
    }

    /// `el.style.removeProperty(name)` — remove a propriedade do `style=""`.
    pub fn remove_style_property(&mut self, id: NodeId, name: &str) {
        let cur = self.css_text(id);
        let new = upsert_css_decl(&cur, name.trim(), ""); // valor vazio = remover
        self.set_attr(id, "style", &new);
    }

    /// Aplica UM slot de estilo OPACO (invariante 4) a UM nó, acumulando no
    /// override por-nó (`setStyle` por-nó / base do `setStyleBatch`). O `(slot,
    /// val)` é interpretado pelo `apply_slot` do `ComputedStyle` (nunca casa string
    /// CSS aqui). Ignora id que não resolve.
    pub fn set_node_style_slot(&mut self, id: NodeId, slot: i64, val: i64) {
        crate::bump!(style_overrides_set);
        let Some(idx) = self.resolve(id) else { return };
        self.touch_subtree(idx);
        self.style_overrides
            .entry(idx)
            .or_default()
            .apply_slot(slot, val);
    }

    /// Aplica um LOTE de triplas `(nodeId, slot, val)` de uma vez (invariante 6:
    /// estilizar N nós por frame não pode ser N×5 FFIs). Cada tripla acumula no
    /// override do seu nó. O `nodes` é uma fatia plana `[id0, slot0, val0, id1,
    /// slot1, val1, …]` (o jeito que o buffer GC chega da ABI). Triplas com id
    /// inválido são ignoradas (robustez).
    pub fn apply_style_batch(&mut self, triples: &[i64]) {
        crate::bump!(style_overrides_set, triples.len() / 3);
        let mut updates = Vec::with_capacity(triples.len() / 3);
        for t in triples.chunks_exact(3) {
            if let Some(node) = NodeId::from_abi(t[0]) {
                if let Some(idx) = self.resolve(node) {
                    updates.push((idx, t[1], t[2]));
                }
            }
        }
        if updates.is_empty() {
            return;
        }
        self.touch_subtrees(updates.iter().map(|(idx, _, _)| *idx));
        for (idx, slot, val) in updates {
            self.style_overrides
                .entry(idx)
                .or_default()
                .apply_slot(slot, val);
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
        let Some(idx) = self.resolve(id) else {
            return -1;
        };
        match &self.nodes[idx].kind {
            NodeKind::Element { tag } => crate::block::lookup(tag).map(|d| d.display).unwrap_or(-1),
            _ => -1,
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
        for c in children {
            self.detach(c);
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
        let affects_parent_selectors =
            matches!(name_lc.as_str(), "id" | "class") || self.stylesheet.has_attribute_selectors();
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
        // A guarda: só vale para `class`, e cai fora se houver seletor de
        // ATRIBUTO no stylesheet (um `[class*=…]` reage a qualquer classe).
        let style_unaffected = name_lc == "class"
            && !self.stylesheet.has_attribute_selectors()
            && self.class_change_is_inert(idx, value);
        if !style_unaffected {
            let affects_parent_selectors =
                affects_index || self.stylesheet.has_attribute_selectors();
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
        self.touch_structural(idx, parent);
        self.detach(idx);
    }

    /// Aloca um nó sem pai (usado por create_element / set_text). Índice cru.
    fn push_detached(&mut self, kind: NodeKind) -> NodeIdx {
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
    fn detach(&mut self, idx: NodeIdx) {
        if let Some(p) = self.nodes[idx].parent.take() {
            crate::bump!(nodes_detached);
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

/// A chave do cache de layout: `(revisão de render, viewport w/h em bits,
/// medidor)`. Ver [`crate::layout::layout_cached`] para por que cada parte
/// entra.
pub(crate) type DisplayKey = (u64, u32, u32, u64);

#[cfg(test)]
mod tests;
