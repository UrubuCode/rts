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

mod arvore;
mod animacao;
mod caches;
mod cascade;
mod consulta;
mod eventos;
mod estilo;
mod formulario;
mod geometria;
mod helpers;
mod invalidacao;
mod matcher;
mod mutacao;
mod no;
mod parser;
mod scroll;
mod serial;
mod travessia;

pub use self::no::{Attr, Node, NodeId, NodeKind};
pub use self::eventos::{RawInputEvent, RawKeyboardEvent};
pub use self::parser::{parse_fragmento, parse_html_to_dom};
use self::matcher::TargetKey;
use self::helpers::{memo_forget, memo_put, nth_casa};
use self::parser::is_void;
use self::helpers::{is_plain_ident, references_self, upsert_css_decl};

/// Índice cru de um nó na arena (`Dom::nodes`). Uso INTERNO ao `dom.rs` — o que
/// cruza a fronteira (TS/ABI) é sempre o `NodeId` VERSIONADO, nunca este índice.
pub type NodeIdx = usize;

/// Opções de um listener, normalizadas no limite Rust/TypeScript.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListenerOptions {
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListenerRecord {
    callback: i64,
    options: ListenerOptions,
}

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
///
/// **Lote L**: `forced_outer_w`/`forced_outer_h`/`shrink_to_fit` entraram para
/// que flex, grid e out-of-flow pudessem participar do cache. Sem eles, um
/// item de flex cujo `flex-grow` mudasse o `main size` bateria na MESMA chave
/// que o desenho antigo (o `node_epoch` sozinho não vê essa mudança — ela vem
/// do IRMÃO, não do próprio nó) e devolveria a geometria de outra largura
/// imposta: a classe silenciosa que `CLAUDE.md` pede para nomear. A posição
/// continua de fora — é a costura/emissão que a desloca, não a chave.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FragmentKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    pub(crate) anim_epoch: u64,
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
    /// CSS externo acrescentado por `addStylesheet`. Fica separado do conteúdo
    /// dos elementos `<style>` para que uma mutação possa reconstruir apenas as
    /// regras embutidas ainda vivas, sem ressuscitar regras removidas.
    external_css: String,
    /// CSS BRUTO efetivo da página — CSS externo mais o conteúdo dos `<style>` vivos.
    /// Guardado para resolver os pseudo-elementos `::-webkit-scrollbar*`.
    /// DERIVADO do HTML, fora do `PartialEq`.
    raw_css: String,
    /// Eventos (#1760, modelo de POLLING — F3): que TIPOS cada nó escuta
    /// (`addEventListener`). Aqui só registramos o tipo p/ saber se um
    /// `dispatchEvent` deve notificar o nó. `NodeIdx → tipos`. DERIVADO, fora do Eq.
    listeners: HashMap<NodeIdx, Vec<String>>,
    /// CALLBACKS por (nó, tipo) — `el.addEventListener('click', fn)` com fn de
    /// verdade. O Dom guarda o WORD/handle i64 da Function OPACO (nunca o invoca —
    /// o rts-dom é headless e livre de runtime; quem invoca é a camada TS via
    /// `dispatch_event_collect`). As opções ficam junto de cada callback porque
    /// callbacks iguais com capture diferente são registos distintos no DOM.
    listener_cbs: HashMap<(NodeIdx, String), Vec<ListenerRecord>>,
    /// Fila de eventos PENDENTES a entregar ao loop TS via `pollEvent`. Cada entrada
    /// é `(nó-alvo a notificar, tipo)` — já expandida pelo bubbling no `dispatch`.
    event_queue: std::collections::VecDeque<(NodeIdx, String)>,
    /// Tipo devolvido pelo último `poll_event`; scratch da ABI, fora do `PartialEq`.
    last_event_type: String,
    /// Tipo devolvido pelo último `poll_raw_event`; scratch da ABI, fora do `PartialEq`.
    last_raw_event_type: String,
    /// Scratch da ÚLTIMA coleta de `dispatch_event_collect`: pares (nó-alvo,
    /// callback-word) na ordem de invocação. A camada TS copia TUDO para arrays
    /// locais ANTES de invocar (um callback pode re-despachar e sobrescrever isto).
    last_dispatch: Vec<(NodeIdx, i64)>,
    /// Opções paralelas dos callbacks coletados: capture e passive. `once` já é
    /// removido do mapa antes da camada TypeScript invocar o callback.
    last_dispatch_capture: Vec<bool>,
    last_dispatch_passive: Vec<bool>,
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
    /// Eventos de teclado crus emitidos pelo backend. O alvo é escolhido pelo DOM
    /// (input focado, body ou documentElement), não pelo backend.
    raw_keyboard_event_queue: std::collections::VecDeque<crate::dom::RawKeyboardEvent>,
    /// Evento de teclado devolvido pelo último `poll_raw_keyboard_event`.
    last_raw_keyboard_event: Option<crate::dom::RawKeyboardEvent>,
    /// Eventos raw de edição/composição emitidos pelo backend. O alvo é capturado
    /// no momento da entrada, como no teclado, e a fachada TS faz o dispatch real.
    raw_input_event_queue: std::collections::VecDeque<crate::dom::RawInputEvent>,
    /// Evento de edição devolvido pelo último `poll_raw_input_event`.
    last_raw_input_event: Option<crate::dom::RawInputEvent>,
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
    /// Offset de scroll da PÁGINA, `(x, y)` em pontos de conteúdo — o mesmo
    /// padrão de `hovered`/`focused_input` acima: estado de DOCUMENTO, não de
    /// backend (`dom/scroll.rs`, finding 3 da auditoria estrutural de
    /// 2026-09-04). `Cell`: lido durante a pintura/hit-test sem `&mut Dom`.
    /// DERIVADO, fora do `PartialEq`.
    scroll: std::cell::Cell<(f32, f32)>,
    /// Offset de scroll POR REGIÃO (`overflow:auto`/`scroll`), por `NodeIdx`.
    /// Só ganha entrada um nó que já rolou uma vez — um container nunca
    /// tocado não paga uma entrada de mapa. `RefCell`: `scroll_of` é `&self`.
    /// DERIVADO, fora do `PartialEq`. Ver `dom/scroll.rs`.
    scroll_regioes: std::cell::RefCell<HashMap<NodeIdx, (f32, f32)>>,
}

// Igualdade estrutural: compara só a árvore (nodes+root). Os índices e a `generation`
// são estado DERIVADO/de-identidade — duas árvores com os mesmos nós são iguais.
impl PartialEq for Dom {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.nodes == other.nodes
    }
}
impl Eq for Dom {}

/// A chave do cache de layout: `(revisão de render, viewport w/h em bits,
/// medidor)`. Ver [`crate::layout::layout_cached`] para por que cada parte
/// entra.
pub(crate) type DisplayKey = (u64, u32, u32, u64);

#[cfg(test)]
mod tests;
