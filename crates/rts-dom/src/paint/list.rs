//! A LISTA DE DESENHO, the list: `Rect`, `ScrollRegion`, `DisplayList` and the
//! traversals that paint it.
//!
//! Moved from `layout/display.rs` on 2026-09-25 (PQ-A2); nothing in it changed.

use crate::boxes::{BoxId, BoxTree};
use crate::dom::NodeIdx;
use crate::layout::fragment::items::translate_item;
use crate::query::Geometry;
use crate::paint::item::DisplayItem;
use crate::paint::pieces::Piece;

/// Um retângulo em coordenadas de conteúdo (a origem é o canto da área de render;
/// o backend soma seu próprio offset de tela ao pintar). Unidade: pontos (f32).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// The bounding rectangle of `self` and `other`.
    ///
    /// The ONE union arithmetic: a box's fragments (`box_fragments.rs`), a
    /// node's boxes and a reused subtree's rects all fold through it. It has
    /// no sentinel to skip: a reserved placeholder is FLAGGED in its entry and
    /// replaced by the first real write, so "empty box at the origin" and "no
    /// box yet" cannot be confused — by construction, not by a special case.
    pub fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Rect::new(x, y, right - x, bottom - y)
    }
}

/// Um CONTAINER ROLÁVEL interno (uma `<div>` com `overflow:auto/scroll` e tamanho
/// definido): o conteúdo é maior que a caixa, então o backend recorta no `visible`,
/// rola por um offset próprio e mostra barra(s) dentro dela. Produzido pelo layout,
/// consumido pelo backend. Distinto do scroll da PÁGINA (que é a viewport inteira).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollRegion {
    /// Qual nó é o container (chave do offset por-região no backend).
    pub node_idx: NodeIdx,
    /// Rect VISÍVEL (content-box do container, coords de conteúdo da página).
    pub visible: Rect,
    /// Largura REAL do conteúdo (pode exceder `visible.w` → rola em X).
    pub content_w: f32,
    /// Altura REAL do conteúdo (pode exceder `visible.h` → rola em Y).
    pub content_h: f32,
    /// overflow de cada eixo (auto/scroll rolam; hidden corta; visible não recorta).
    pub overflow_x: crate::scrollbar::Overflow,
    pub overflow_y: crate::scrollbar::Overflow,
}

/// A saída do layout: a lista plana de itens de pintura, em z-order. É o ÚNICO
/// que o backend de render consome. Sem nenhuma referência à árvore — o layout já
/// consumiu a topologia (herança/cascade/box model) ao produzir esta lista.
#[derive(Clone, Default, Debug)]
pub struct DisplayList {
    /// The box tree this list was laid out against, memoised on the `Dom`
    /// (`Dom::box_tree`).
    ///
    /// This is what the flow sequence, the fragment cache's address and the
    /// public geometry read the boxes from — the fix for invariant I1 in
    /// `docs/ui/html-engine/box-tree.md` §7. `Rc` and not a borrow:
    /// this list can outlive the layout pass that built it (it is cached),
    /// and cloning the `Rc` guarantees it is always read against the SAME
    /// tree it was built against, never half of one tree and half of
    /// another because the memo on the `Dom` moved on in between.
    ///
    /// `BoxTree` derives `Default`, so `Rc<BoxTree>` does too (an empty tree):
    /// the five `DisplayList::default()` call sites compile unchanged.
    pub tree: std::rc::Rc<BoxTree>,
    /// The output in paint order: own items, subtrees reused by REFERENCE, and
    /// the geometry marks the hit order is read from — one sequence
    /// (`pieces.rs` says what it replaced and why).
    ///
    /// The subtrees are what make the output a TREE: a frame that touches one
    /// leaf does not rebuild the page's 30 000 items, it points at the fragments
    /// that already existed. Whoever paints walks it ([`Self::walk`]); whoever
    /// must mutate or compare flattens it ([`Self::materialize`]).
    pub pieces: Vec<Piece>,
    /// A cor do CANVAS — o fundo do `<body>`/`<html>` propagado, e BRANCO
    /// quando nenhum dos dois define um. Vive aqui e não como item da lista
    /// porque é a cor de LIMPEZA do backend; sem ela o que aparecia por trás de
    /// uma página era a cor padrão dele (quase preta), e uma página real cujo
    /// estilo mora num `<link>` externo saía texto preto sobre preto.
    pub canvas_background: u32,
    /// Altura total ocupada pelo conteúdo (para o backend dimensionar o scroll).
    pub content_height: f32,
    /// Geometria por CAIXA (border-box, em coordenadas de conteúdo) — a base
    /// de `element.getBoundingClientRect()`/`offsetWidth`/etc. Preenchido
    /// durante o layout: cada bloco regista o retângulo da sua caixa (margin
    /// EXCLUÍDA — border-box, como o `getBoundingClientRect` do browser);
    /// nós de texto não entram.
    ///
    /// Chaveado por `BoxId` e não por `NodeIdx` — a mudança central do lote
    /// BT-1. Enquanto a árvore for o espelho (uma caixa por elemento) isto é
    /// indistinguível da forma antiga: é o que faz este lote não mudar
    /// resposta nenhuma. `rect_of_node` e `geometry_now` são as vistas
    /// agregadas por nó, para quando um nó vier a ter mais do que uma caixa.
    pub box_rects: crate::layout::BoxRects,
    pub static_anchors: Vec<(BoxId, f32, f32)>, // static positions, `layout/inline/static_anchor.rs`
    /// Tracks de coluna de grids explícitos, já resolvidas em px pelo layout. O
    /// `computedProperty` usa esta fonte de used values sem duplicar `resolve_tracks`.
    pub grid_column_tracks: crate::fasthash::FastMap<NodeIdx, Vec<f32>>,
    /// Containers roláveis internos (divs com `overflow`) — o backend gerencia o
    /// offset de cada região e recorta. Vazio quando a página não tem scroll interno.
    pub scroll_regions: Vec<ScrollRegion>,
    /// A geometria completa, montada sob demanda a partir da árvore. Não entra
    /// no `PartialEq` nem no `Clone` lógico: é derivada.
    pub(crate) geometry_cache: std::cell::RefCell<Option<std::rc::Rc<Geometry>>>,
}

/// Equal when everything a repaint or a hit-test could observe is equal.
///
/// `tree` and `geometry_cache` are excluded on purpose — both, the comment on
/// `geometry_cache` already said before `tree` existed, are DERIVED from the
/// same document that produced `box_rects` and the pieces. Comparing `tree`
/// would add nothing and would force `BoxTree`/`LayoutBox` to carry `PartialEq`
/// for no other reason.
impl PartialEq for DisplayList {
    fn eq(&self, other: &Self) -> bool {
        self.pieces == other.pieces
            && self.canvas_background == other.canvas_background
            && self.content_height == other.content_height
            && self.box_rects == other.box_rects
            && self.grid_column_tracks == other.grid_column_tracks
            && self.scroll_regions == other.scroll_regions
    }
}

impl DisplayList {
    /// An empty list that already carries the document box tree.
    ///
    /// **Use this and not `default()` for any list layout writes into.** The
    /// translation from node to box happens through `tree`, so a list built
    /// with `default()` has an EMPTY tree, `boxes_of` answers nothing, and
    /// every rectangle written into it is silently dropped. That is not a
    /// hypothetical: it is what 322 tests failed with before this existed.
    ///
    /// `default()` stays for the callers that never receive geometry — a probe,
    /// a test that only reads items.
    pub fn for_dom(dom: &crate::dom::Dom) -> Self {
        DisplayList {
            tree: dom.box_tree(),
            ..Default::default()
        }
    }

    /// Paints `item` over everything emitted so far — the one write most of
    /// layout does. An item that must go BEHIND what is already there is an
    /// insert into `pieces` at a position remembered before it (`pieces.rs`).
    pub fn push_item(&mut self, item: DisplayItem) {
        self.pieces.push(Piece::Item(item));
    }

    /// Todos os itens a pintar, em z-order, cada um com o deslocamento a somar.
    ///
    /// Anda a ÁRVORE de fragmentos: um item de uma subárvore reusada sai daqui
    /// sem nunca ter sido copiado. Quem pinta já somava uma origem, então somar
    /// mais um deslocamento é grátis — foi o que permitiu a saída deixar de ser
    /// uma lista plana refeita por frame.
    pub fn walk(&self, mut f: impl FnMut(&DisplayItem, f32, f32)) {
        crate::paint::pieces::walk(&self.pieces, 0.0, 0.0, &mut f);
    }

    /// A lista PLANA. Para quem precisa MUTAR itens (o `transform` do CSS, o
    /// offset de scroll no `BeginClip`) ou comparar duas listas.
    pub fn materialized(&self) -> Vec<DisplayItem> {
        let mut out = Vec::with_capacity(self.total_items());
        self.walk(|item, dx, dy| {
            let mut item = item.clone();
            if dx != 0.0 || dy != 0.0 {
                translate_item(&mut item, dx, dy);
            }
            out.push(item);
        });
        out
    }

    /// Achata esta lista em itens próprios, esquecendo a árvore — and the
    /// geometry of the subtrees it reused, as it always did.
    pub fn materialize(&mut self) {
        crate::paint::pieces::flatten_from(&mut self.pieces, 0);
    }

    /// Quantos itens esta lista pinta ao todo.
    pub fn total_items(&self) -> usize {
        crate::paint::pieces::count_items(&self.pieces)
    }

}
