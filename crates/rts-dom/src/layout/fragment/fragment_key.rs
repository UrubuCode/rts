//! The FRAGMENT CACHE KEY: `KeyBase` (the part shared by every child of one
//! container) and `fragment_key` (the per-child key built from it).
//!
//! Split out of `fragment.rs`, which was past the 500-line ceiling. Kept
//! apart from the reuse/stitch algorithm that CONSUMES this key (`stitch`
//! and `layout_block_reusing`, still in `fragment.rs`) rather than from the
//! `Fragment`/`ChildRef` types (`types.rs`) or the emit/build entry point,
//! because the key's own header already explains why its two pieces belong
//! together: `KeyBase` amortizes what does not vary per child, and
//! `fragment_key` is the one-line composition a caller reaches for. Grouping
//! by "output types" instead was rejected — `KeyBase`/`fragment_key` compute
//! a value, they do not represent the layout result the way `Fragment` and
//! `ChildRef` do, so filing them beside those types would mix a lookup
//! question with a storage question.

use super::*;

/// A chave do fragmento de uma caixa com certas constraints. Extraída porque o laço
/// do fluxo vertical CONSULTA o cache antes de classificar o filho: um fragmento
/// só existe para bloco-normal, então encontrá-lo já responde o que a
/// classificação responderia — e a classificação custa estilo computado,
/// `block::lookup` e a margem resolvida, mil vezes por frame.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn fragment_key(
    dom: &Dom,
    id: NodeIdx,
    caixa: crate::boxes::BoxId,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    shrink_to_fit: bool,
    ctx: &LayoutCtx,
) -> crate::dom::FragmentKey {
    KeyBase::new(dom, avail_w, avail_h, ctx).key(
        dom,
        id,
        caixa,
        forced_outer_w,
        forced_outer_h,
        shrink_to_fit,
    )
}

/// A parte da chave de fragmento que NÃO varia entre os filhos de um container:
/// identidade da árvore, epochs globais, viewport, medidor e as constraints.
///
/// Montar a chave inteira por filho relia um `thread_local` (o epoch de estilo)
/// e refazia as conversões mil vezes por container — o laço do fluxo vertical
/// pergunta o mesmo a cada iteração e só o nó muda.
#[derive(Clone, Copy)]
pub(in crate::layout) struct KeyBase {
    tree: u64,
    style_epoch: u64,
    anim_epoch: u64,
    avail_w: u32,
    avail_h: Option<u32>,
    viewport_w: u32,
    viewport_h: u32,
    measurer: u64,
}

impl KeyBase {
    pub(in crate::layout) fn new(
        dom: &Dom,
        avail_w: f32,
        avail_h: Option<f32>,
        ctx: &LayoutCtx,
    ) -> KeyBase {
        KeyBase {
            tree: dom.cache_identity(),
            style_epoch: crate::style::props::style_epoch(),
            anim_epoch: dom.anim_epoch(),
            avail_w: avail_w.to_bits(),
            avail_h: avail_h.map(f32::to_bits),
            viewport_w: ctx.viewport_w.to_bits(),
            viewport_h: ctx.viewport_h.to_bits(),
            measurer: ctx.measurer.identity(),
        }
    }

    pub(in crate::layout) fn key(
        &self,
        dom: &Dom,
        id: NodeIdx,
        caixa: crate::boxes::BoxId,
        forced_outer_w: Option<f32>,
        forced_outer_h: Option<f32>,
        shrink_to_fit: bool,
    ) -> crate::dom::FragmentKey {
        crate::dom::FragmentKey {
            tree: self.tree,
            node_epoch: dom.layout_epoch(id),
            style_epoch: self.style_epoch,
            anim_epoch: self.anim_epoch,
            target: super::caixa_cache_target(dom, id, caixa),
            avail_w: self.avail_w,
            avail_h: self.avail_h,
            forced_outer_w: forced_outer_w.map(f32::to_bits),
            forced_outer_h: forced_outer_h.map(f32::to_bits),
            shrink_to_fit,
            viewport_w: self.viewport_w,
            viewport_h: self.viewport_h,
            measurer: self.measurer,
        }
    }
}
