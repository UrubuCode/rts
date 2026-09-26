//! The geometry ASKED of a paint list after layout: `Geometry` (rects per
//! node, the scroll regions) and the per-box and per-node
//! rect queries.
//!
//! The hit-test does not read it: it walks the fragments itself (`hit.rs`),
//! because a rect per node is exactly what cannot tell a wrapped link's two
//! lines from the gap between them.

use crate::boxes::BoxId;
use crate::dom::NodeIdx;
use crate::paint::list::{DisplayList, Rect, ScrollRegion};
use crate::paint::pieces::Piece;

/// A geometria de uma passada de layout, já com as subárvores reusadas somadas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub rects: crate::fasthash::FastMap<NodeIdx, Rect>,
    pub scroll_regions: Vec<ScrollRegion>,
}

impl DisplayList {
    /// A geometria COMPLETA desta lista: os retângulos próprios mais os das
    /// subárvores reusadas, já deslocados. Built on every call.
    ///
    /// **The memo is the `Dom`'s, not the list's** (PQ-C4): the repeated
    /// reader is `bounding_component` in a read loop, and it reads
    /// `Dom::geometry_cached`, which keeps ONE geometry beside the ONE cached
    /// list. The list used to carry a `RefCell` for it, which made paint's
    /// type name query's.
    pub fn geometry_now(&self) -> Geometry {
        // The rects are layout's fold (`layout/fragment/known_rects.rs`, PQ-C3),
        // the same one the out-of-flow pass reads mid-layout.
        let mut g = Geometry {
            rects: crate::layout::fragment::known_rects::known_rects(self),
            scroll_regions: self.scroll_regions.clone(),
        };
        // The reused subtrees' scroll regions, in the order they always came.
        add_scroll_regions(&self.pieces, 0.0, 0.0, &mut g.scroll_regions);
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

/// The scroll regions of the subtrees `pieces` reuses, offset, into `out`:
/// each subtree's subtrees' first, then its own — the order `geometry_now`
/// always produced them in.
fn add_scroll_regions(pieces: &[Piece], dx: f32, dy: f32, out: &mut Vec<ScrollRegion>) {
    for c in crate::paint::pieces::children(pieces) {
        let (dx, dy) = (dx + c.dx, dy + c.dy);
        let moved = dx != 0.0 || dy != 0.0;
        add_scroll_regions(&c.fragment.pieces, dx, dy, out);
        for region in c.fragment.scroll_regions.iter() {
            let mut region = *region;
            if moved {
                region.visible.x += dx;
                region.visible.y += dy;
            }
            out.push(region);
        }
    }
}
