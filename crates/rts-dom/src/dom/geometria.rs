//! `getBoundingClientRect` — a geometria pedida de fora, singular e em lote.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// `getBoundingClientRect(el)[componente]` — 0=x, 1=y, 2=largura, 3=altura.
    ///
    /// Mede com o medidor ACTIVO da thread (`layout::medidor_ativo`): quando
    /// há uma janela real aberta, é o mesmo `TextMeasurer` de fontes reais que
    /// `render_dom` usa para pintar — a mesma geometria que o JS lê é a que sai
    /// na tela. Sem janela (headless), `with_active` cai sozinho no medidor
    /// APROXIMADO: não há fonte real, e devolver zero seria pior do que a
    /// aproximação que o layout headless já usa em todo o resto.
    ///
    /// **Reuses `layout::layout_cached` instead of running a fresh
    /// `layout_document`/`bounding_rect` on every call** (BR, 2026-09-16). The
    /// defect this closes was measured at 13.7 ms per call on the Wikipedia
    /// fixture (16 813 elements): a page — or a layout library — that calls
    /// `getBoundingClientRect` in a loop pays the whole document's layout once
    /// PER CALL, and libraries do call it in loops.
    ///
    /// `layout_cached` is not a new cache invented for this: it is the SAME
    /// single-slot memo `rts-egui`'s paint path already reuses every frame
    /// (`crates/rts-egui/src/frame/render/mod.rs`), keyed by
    /// `(dom.render_revision(), viewport_w, viewport_h, measurer.identity())`.
    /// **What that key SEES:** `render_revision()` folds in `revision`
    /// (bumped by every `touch*` — `touch`, `touch_subtree`, `touch_structural`,
    /// `touch_attr` — regardless of WHICH node changed), `anim_epoch` (bumped
    /// per animation frame) and the global per-tag `style_epoch`. Because
    /// `revision` is a single document-wide counter and not a per-node epoch,
    /// a change to a SIBLING of the node being measured still invalidates the
    /// cache — this is exactly the gap `box-tree.md` §7 (I7) calls out for the
    /// per-node `FragmentKey`/`node_epoch` caches used INSIDE one layout pass;
    /// it does not apply here because this key never narrows to a node.
    /// **What it does NOT see:** it does not distinguish an edit to a distant,
    /// unrelated node from one to the queried node itself — any mutation
    /// anywhere pays a full relayout on the next query, same as a real
    /// browser's forced synchronous layout. The win is specifically the
    /// read-loop with no mutation in between, which is the shape both the
    /// defect above and `bounding_components_many` describe.
    pub fn bounding_component(&self, id: NodeId, which: i64) -> f32 {
        let Some(idx) = self.resolve(id) else {
            return 0.0;
        };
        let (vw, vh) = self.viewport.get();
        crate::layout::measure::active_measurer::with_active(|measurer| {
            let ctx = crate::layout::LayoutCtx {
                viewport_w: vw,
                viewport_h: vh,
                measurer,
            };
            // The list and its geometry, both memoised on the `Dom` (PQ-C4):
            // `rect_of` alone would rebuild the geometry on every call.
            let list = crate::layout::layout_cached(self, &ctx);
            let geometry = self.geometry_cached(&ctx);
            let Some(rect) = list.rect_of_in(&geometry, idx) else {
                return 0.0;
            };
            match which {
                0 => rect.x,
                1 => rect.y,
                2 => rect.w,
                _ => rect.h,
            }
        })
    }

    /// As quatro componentes da caixa de MUITOS nós de uma vez, na ordem
    /// `x, y, w, h` por nó pedido.
    ///
    /// Existia porque `bounding_component` fazia um `layout_document` INTEIRO
    /// por chamada, e isso é linear no documento: medido a 13,7 ms por chamada
    /// na Wikipédia (16 813 elementos). O extrator de paridade pede quatro
    /// componentes por elemento, ou seja ~67 mil layouts completos do mesmo
    /// documento imutável — os 9m21s que a extração de paridade custava eram
    /// isto, e não o layout, que precisa de correr uma vez. Com o lote: 9,7s, e
    /// o dump a sair byte a byte igual ao de antes.
    ///
    /// **Já não é a única forma de evitar o problema.** Desde BR (2026-09-16),
    /// `bounding_component` também passou a reusar `layout::layout_cached` —
    /// então um chamador que peça uma componente de cada vez, num laço, sem
    /// mutar o documento entre chamadas, já não paga N layouts completos
    /// também pela via singular. Esta função em lote continua a existir porque
    /// uma passada que resolve TODOS os `NodeId`s de uma vez evita reentrar no
    /// `with_active`/`layout_cached` uma vez por elemento — mais barato ainda
    /// quando o chamador já sabe de antemão quais nós quer.
    ///
    /// Um id que não resolve responde `0.0` nas quatro, que é exatamente o que
    /// `bounding_component` responde no mesmo caso.
    pub fn bounding_components_many(&self, ids: &[NodeId]) -> Vec<f32> {
        let (vw, vh) = self.viewport.get();
        crate::layout::measure::active_measurer::with_active(|measurer| {
            let ctx = crate::layout::LayoutCtx {
                viewport_w: vw,
                viewport_h: vh,
                measurer,
            };
            let list = crate::layout::layout_cached(self, &ctx);
            let geometry = self.geometry_cached(&ctx);
            let mut out = Vec::with_capacity(ids.len() * 4);
            for &id in ids {
                // `rect_of` e NAO `rect_of_node`, e a diferenca importa: o
                // primeiro le a `Geometry`, que agrega a lista de topo MAIS a
                // geometria que vive dentro de cada fragmento em cache; o
                // segundo le so os rectangulos escritos directamente nesta
                // lista. Numa pagina com layout incremental a maior parte da
                // geometria esta nos fragmentos, e ler so a lista devolvia
                // zero — apanhado pelo teste que pina que a via em lote
                // responde o mesmo que a singular.
                //
                // A agregacao por caixa acontece na mesma, uma camada abaixo:
                // e `collect_geometry` que une as caixas de um no ao montar a
                // `Geometry`.
                match self.resolve(id).and_then(|idx| list.rect_of_in(&geometry, idx)) {
                    Some(r) => out.extend_from_slice(&[r.x, r.y, r.w, r.h]),
                    None => out.extend_from_slice(&[0.0, 0.0, 0.0, 0.0]),
                }
            }
            out
        })
    }
}
