//! The geometry ASKED of a paint list after layout: `Geometry` (rects per
//! node, the hit order, the scroll regions) and the per-box and per-node
//! rect queries.
//!
//! Moved from `layout/display.rs` and `layout/pieces.rs` (`collect`) on 2026-09-25 (PQ-A3); nothing in it changed.

use crate::boxes::{BoxId, BoxTree};
use crate::dom::NodeIdx;
use crate::layout::Fragment;
use crate::paint::list::{DisplayList, Rect, ScrollRegion};
use crate::paint::pieces::Piece;

/// A geometria de uma passada de layout, já com as subárvores reusadas somadas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub rects: crate::fasthash::FastMap<NodeIdx, Rect>,
    pub hit_order: Vec<(NodeIdx, Rect)>, // each box's own rect: `pieces::collect`
    pub scroll_regions: Vec<ScrollRegion>,
}

impl DisplayList {
    /// A geometria COMPLETA desta lista: os retângulos próprios mais os das
    /// subárvores reusadas, já deslocados. Construída na primeira consulta e
    /// guardada — o layout mesmo só a pede quando há elemento fora do fluxo.
    pub fn geometry(&self) -> std::rc::Rc<Geometry> {
        if let Some(g) = self.geometry_cache.borrow().as_ref() {
            return std::rc::Rc::clone(g);
        }
        let g = std::rc::Rc::new(self.geometry_now());
        *self.geometry_cache.borrow_mut() = Some(std::rc::Rc::clone(&g));
        g
    }

    /// A geometria SEM cachear — para uso DURANTE a montagem da lista, quando
    /// ainda vão entrar itens (a passada de fora do fluxo). Cachear ali deixaria
    /// o hit-test lendo uma geometria anterior aos `position:absolute`, que foi
    /// exatamente o que um teste de `z-index` acusou.
    pub fn geometry_now(&self) -> Geometry {
        // Agrega `box_rects` (por CAIXA) em `rects` (por NÓ) — o limite de
        // agregação que o §6 do desenho da árvore de caixas pede. Enquanto a
        // árvore for o espelho, cada nó tem no máximo uma caixa e isto é uma
        // cópia; deixa de ser quando um nó vier a ter várias.
        let mut rects: crate::fasthash::FastMap<NodeIdx, Rect> = crate::fasthash::FastMap::default();
        for (box_id, rect) in self.box_rects.unions() {
            if let Some(node) = self.tree.node_of(box_id) {
                match rects.get_mut(&node) {
                    Some(existing) => *existing = existing.union(rect),
                    None => {
                        rects.insert(node, rect);
                    }
                }
            }
        }
        let mut g = Geometry {
            rects,
            hit_order: Vec::new(),
            scroll_regions: self.scroll_regions.clone(),
        };
        // The hit order is the geometry marks in paint order, a reused
        // subtree's entering where its `Child` stands — `pieces::collect`.
        collect(&self.tree, &self.pieces, 0.0, 0.0, &|b| self.box_rects.union(b), &mut g);
        g
    }

    /// Retângulo de uma caixa concreta da BoxTree, inclusive quando ela foi
    /// emitida dentro de um fragmento reutilizado. Esta é a consulta interna
    /// para código de layout; a API de DOM continua a usar [`Self::rect_of`],
    /// pois caixas anônimas não possuem `NodeIdx` para expor.
    pub(crate) fn rect_of_box(&self, box_id: BoxId) -> Option<Rect> {
        self.box_rects
            .union(box_id)
            .or_else(|| crate::paint::pieces::rect_in_children(&self.pieces, box_id, 0.0, 0.0))
    }

    /// O retângulo de um NÓ: a união dos retângulos das caixas que ele
    /// gerou. Par de `rect_of`, sem passar por `Geometry` nem pelo cache —
    /// para um chamador que já tem um `NodeIdx` isolado e não quer montar a
    /// geometria completa da lista.
    ///
    /// Enquanto a árvore for o espelho (BT-1 fase 1), `boxes_of` devolve no
    /// máximo uma caixa, então isto é exatamente o retângulo dela, ou `None`
    /// para um nó que não gerou caixa nenhuma (texto, `display:none`) ou
    /// ainda não foi layoutado.
    pub fn rect_of_node(&self, node: NodeIdx) -> Option<Rect> {
        let mut acc: Option<Rect> = None;
        for &box_id in self.tree.boxes_of(node) {
            if let Some(rect) = self.rect_of_box(box_id) {
                acc = Some(match acc {
                    Some(a) => a.union(rect),
                    None => rect,
                });
            }
        }
        acc
    }
}

/// The hit order and the geometry of the subtrees `pieces` reuses, into `out`.
///
/// ONE walk in paint order: a box's hit-test entry is its [`Piece::Rect`], and a
/// subtree's entries come in where its `Child` stands — which is what `hit_at`
/// reconstructed by counting. The `Geometry` answers by NODE, so a box is
/// translated here, and an anonymous box (no node) does not enter: the bridge
/// promises element boxes, and a box the document does not have is not
/// reachable by any `NodeId`.
///
/// **Each entry carries its BOX's rect** (`rect_of`, already offset), not the
/// node's. A split inline's fragment after the block marks its box after the
/// block's; with the node's union — which spans the block — it won a click on
/// the block that Blink gives the block (`rect_cliente.rs`). That only held
/// while every fragment box was written at the first line, marking them all
/// before the block — true of the per-node union, false once each fragment
/// records its own rects (BT-2c).
pub(crate) fn collect(
    tree: &BoxTree,
    pieces: &[Piece],
    dx: f32,
    dy: f32,
    rect_of: &dyn Fn(BoxId) -> Option<Rect>,
    out: &mut Geometry,
) {
    for piece in pieces {
        match piece {
            Piece::Rect(box_id) => out.hit_order.extend(tree.node_of(*box_id).zip(rect_of(*box_id))),
            Piece::Child(c) => collect_fragment(tree, &c.fragment, dx + c.dx, dy + c.dy, out),
            Piece::Item(_) => {}
        }
    }
}

/// A reused fragment's rects (several boxes of one node unite, which is what
/// `getBoundingClientRect` asks), then its pieces, then its scroll regions —
/// the order `geometry_now` always produced them in.
fn collect_fragment(tree: &BoxTree, fragment: &Fragment, dx: f32, dy: f32, out: &mut Geometry) {
    let moved = dx != 0.0 || dy != 0.0;
    let mut by_box: crate::fasthash::FastMap<BoxId, Rect> = crate::fasthash::FastMap::default();
    for (box_id, rect) in fragment.rects.iter() {
        let mut rect = *rect;
        if moved {
            rect.x += dx;
            rect.y += dy;
        }
        by_box.entry(*box_id).and_modify(|r| *r = r.union(rect)).or_insert(rect);
        let Some(node) = tree.node_of(*box_id) else { continue };
        out.rects.entry(node).and_modify(|r| *r = r.union(rect)).or_insert(rect);
    }
    collect(tree, &fragment.pieces, dx, dy, &|b| by_box.get(&b).copied(), out);
    for region in fragment.scroll_regions.iter() {
        let mut region = *region;
        if moved {
            region.visible.x += dx;
            region.visible.y += dy;
        }
        out.scroll_regions.push(region);
    }
}