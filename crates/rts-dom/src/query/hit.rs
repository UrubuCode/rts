//! HIT-TEST: which node is under a point of a laid-out paint list.
//!
//! Moved on 2026-09-25 (PQ-A3): `DisplayList::hit_test` from `layout/display.rs`, `Dom::hit_test_clickable` from `dom/geometria.rs`. Nothing in it changed.

use crate::dom::{Dom, NodeIdx};
use crate::paint::list::{DisplayList, Rect};

impl DisplayList {
    /// HIT-TEST: o nó sob o ponto `(x, y)` em COORDENADAS DE CONTEÚDO (o backend
    /// converte tela→conteúdo somando o offset de scroll antes de chamar). Quando a
    /// lista foi produzida por `layout_document`, a ordem de pintura respeita
    /// ancestrais/descendentes, irmãos e `z-index`; o ÚLTIMO retângulo que contém o
    /// ponto é o elemento visualmente no topo. Listas antigas sem `hit_order` usam o
    /// fallback por menor área para manter compatibilidade.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeIdx> {
        let g = self.geometry();
        if !g.hit_order.is_empty() {
            let hit = |r: &Rect| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
            return g.hit_order.iter().rev().find(|(_, r)| hit(r)).map(|&(idx, _)| idx);
        }
        let mut best: Option<(NodeIdx, f32)> = None;
        for (&idx, r) in &g.rects {
            if x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h {
                let area = r.w * r.h;
                if best.map(|(_, a)| area < a).unwrap_or(true) {
                    best = Some((idx, area));
                }
            }
        }
        best.map(|(idx, _)| idx)
    }
}

impl Dom {
    /// Igual a [`crate::paint::DisplayList::hit_test`], mas `pointer-events:
    /// none` fica TRANSPARENTE ao clique — a espessura de `hit_order` sob o
    /// nó de topo é revisitada até achar um nó cujo computado não seja
    /// `none` (herda, como a spec pede, e o `ComputedStyle` de cada nó já
    /// reflete a herança — não há necessidade de subir a árvore aqui).
    ///
    /// Vive no `Dom` e não em `DisplayList::hit_test` porque só o `Dom` tem a
    /// cascade; a lista de exibição só tem retângulos e índices. Quem decide
    /// clique (`rts-egui`) chamava `DisplayList::hit_test` direto — passar a
    /// chamar este em vez daquele é a mudança mínima que fecha o gap.
    pub fn hit_test_clickable(&self, list: &crate::paint::DisplayList, x: f32, y: f32) -> Option<NodeIdx> {
        let g = list.geometry();
        g.hit_order.iter().rev().find(|&&(idx, r)| {
            let dentro = x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
            dentro
                && !matches!(
                    self.computed_style_idx(idx).and_then(|s| s.pointer_events),
                    Some(crate::style::vocab::PointerEvents::None)
                )
        })
        .map(|&(idx, _)| idx)
    }
}
