//! `position: relative` — desloca a PINTURA sem tirar o elemento do fluxo.
//!
//! CSS 2.1 §9.4.3: os offsets de um `relative` movem a caixa já layoutada —
//! e o que a segue (`getBoundingClientRect`) — sem reservar nem libertar
//! espaço nenhum: o irmão seguinte fica exactamente onde ficaria se não
//! houvesse deslocamento. É por isso que este módulo corre DEPOIS de
//! `block.rs` já ter medido e posicionado a caixa e os filhos na posição
//! NATURAL — nada aqui volta a medir, só translada o que já existe.
//!
//! O mecanismo de "deslocar uma subárvore já pintada, in-place" já existe
//! para `transform` (`block.rs`, atalho `so_translate`) e é reusado aqui para
//! a metade da PINTURA (`pieces::shift_from`). A diferença, e a razão
//! de não bastar chamar essa função: `transform` nunca toca `list.box_rects`
//! — é visual, não move o `getBoundingClientRect` (decisão já tomada nesse
//! módulo) — mas o offset de `relative` TEM de mover, porque é exactamente o
//! que o Chrome mede em `claude-position-relative.esperado.json`.
//!
//! `list.box_rects` é um mapa achatado por CAIXA, não uma fatia por posição
//! como `list.pieces`. Per the box-tree invariant I2
//! (`docs/ui/html-engine/box-tree.md` §7), the only way to know which entries
//! belong to this subtree is walking the box tree from `id` — not the DOM —
//! so an anonymous box, which has no `NodeIdx`, is still found and shifted.
//! Cost is O(subtree size), paid only on `relative` nodes with a non-zero
//! offset.
//!
//! Alternativa rejeitada: deslocar a caixa na MEDIÇÃO (somar o offset a
//! `x`/`y` antes de layoutar `id`), como o `absolute` faz contra o seu
//! containing block. Isso desloca a pintura correctamente, mas desloca
//! também o CURSOR que o pai devolve ao irmão seguinte — exactamente o que a
//! fixture pina que NÃO deve acontecer (`#seguinte` fica onde ficaria sem o
//! `#relativo` deslocado). Deslocar DEPOIS, só a pintura e a geometria já
//! produzidas, é o que mantém o espaço reservado no fluxo intacto.

use super::*;
use crate::boxes::{BoxId, BoxTree};

/// Aplica o deslocamento de `position:relative` a um bloco já layoutado.
/// `box_start` é o mesmo marcador que `block.rs` usa para o `transform` — o
/// início, em `list.pieces`, da pintura desta caixa e dos seus descendentes.
/// Sem efeito quando `css.position` não é `Relative`, ou quando os quatro
/// insets resolvem a deslocamento nulo (não vale andar a subárvore à toa).
///
/// `id` is already a `BoxId` — the caller resolves it through
/// `list.tree.boxes_of(node)` before calling in, which is what lets this
/// function walk the box tree instead of the DOM (invariant I2).
/// `pub(crate)`, not `pub(in crate::layout)`: `table/relativo.rs` calls this
/// too, for the row/row-group boxes `layout_block` never sees (see its own
/// header for why). Reusing this function rather than writing a second
/// offset routine is the point — a table-internal box's offset is computed
/// and applied exactly like any other block's.
#[allow(clippy::too_many_arguments)]
pub(crate) fn aplica_offset_relativo(
    id: BoxId,
    css: &ComputedStyle,
    avail_w: f32,
    avail_h: Option<f32>,
    font_size: f32,
    box_start: usize,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let (dx, dy) = relative_offset(css, avail_w, avail_h, font_size, ctx);
    desloca_desde(list, box_start, Some(id), dx, dy);
}

/// The `(dx, dy)` a `position: relative` box is shifted by; `(0, 0)` for any
/// other `position`.
///
/// `left` beats `right` and `top` beats `bottom` in LTR (CSS 2.1 §9.4.3): with
/// both of an axis present only the reading-side one shifts, and with neither
/// the box stays where it was.
pub(in crate::layout) fn relative_offset(css: &ComputedStyle, avail_w: f32, avail_h: Option<f32>, font_size: f32, ctx: &LayoutCtx) -> (f32, f32) {
    if css.position != Some(crate::style::Position::Relative) {
        return (0.0, 0.0);
    }
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let left = super::positioned::resolve_inset(css.inset_left, avail_w, &resolve);
    let right = super::positioned::resolve_inset(css.inset_right, avail_w, &resolve);
    let avail_h_axis = avail_h.unwrap_or(0.0);
    let top = super::positioned::resolve_inset(css.inset_top, avail_h_axis, &resolve);
    let bottom = super::positioned::resolve_inset(css.inset_bottom, avail_h_axis, &resolve);
    (left.or(right.map(|r| -r)).unwrap_or(0.0), top.or(bottom.map(|b| -b)).unwrap_or(0.0))
}

/// The shift of an INLINE box: its own `position: relative` offset plus that
/// of every inline box around it, up to the block that owns the line.
///
/// An inline's text is painted by the line it sits in, under no box of its
/// own, so `aplica_offset_relativo` — which shifts what a BLOCK emitted —
/// never reached it and a relative `<span>` stayed where it was
/// (`claude-relativo-em-inline`, WPT `position-relative-033`). The line asks
/// here instead, for each segment's innermost owner, and `owner_fragment`
/// asks for each owner — so text, client rects and painted surface move
/// together and nothing around them reflows.
///
/// The walk stops at the first ancestor that is not an inline box: a block or
/// an atom shifts ITS OWN subtree through `aplica_offset_relativo`, and adding
/// it here would shift the content twice. Cut: a percentage inset resolves
/// against the viewport, the line not knowing its containing block's size.
pub(in crate::layout) fn offset_do_inline(dom: &Dom, mut node: Option<NodeIdx>, ctx: &LayoutCtx) -> (f32, f32) {
    let (mut dx, mut dy) = (0.0, 0.0);
    while let Some(n) = node.filter(|&n| !is_block_level(dom, n) && !is_inline_block(dom, n)) {
        if let Some(css) = dom.computed_style_idx(n) {
            let (x, y) = relative_offset(&css, ctx.viewport_w, Some(ctx.viewport_h), font_px(&css, DEFAULT_FONT_SIZE), ctx);
            (dx, dy) = (dx + x, dy + y);
        }
        node = dom.node(n).parent;
    }
    (dx, dy)
}

/// Shifts by `(dx, dy)` everything emitted into `list` since the piece
/// position `desde` — items, reused subtrees (`pieces::shift_from`, which also
/// says which subtrees just before `desde` it still takes along) and, when
/// `rects_of` names one, the rects of that box's subtree. What an ATOM laid
/// out on the line of a relative inline needs: its box was placed by
/// `layout_block`, which knows nothing of the inline around it. `rects_of` is `None` for an atom with no body (an anchor, an edge):
/// there is no rect of its own to move. Every atom HAS a box; the `Option`
/// says whether it recorded a rect.
pub(in crate::layout) fn desloca_desde(list: &mut DisplayList, desde: usize, rects_of: Option<BoxId>, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    crate::paint::pieces::shift_from(&mut list.pieces, desde, dx, dy);
    // Subtrees served by a cached fragment (the `Piece::Child`s shifted above) have
    // no entry in `list.box_rects`: the walk below does not find them, rightly —
    // their `ChildRef`'s `dx`/`dy` is added on read by `geometry_now`. A second
    // source of truth for one answer, reconciled by hand until BT-2 removes it.
    if let Some(box_id) = rects_of {
        let tree = list.tree.clone();
        shift_box_rects(&tree, box_id, dx, dy, list);
    }
}

/// Walks the box tree from `id`, shifting every box's rect by `(dx, dy)`.
/// `tree` is passed separately from `list` (a cheap `Rc` clone at the call
/// site) so the recursion can read `tree.children` while `list.box_rects` is
/// borrowed mutably.
pub(in crate::layout) fn shift_box_rects(
    tree: &BoxTree,
    id: BoxId,
    dx: f32,
    dy: f32,
    list: &mut DisplayList,
) {
    list.box_rects.map_fragments(id, |r| Rect::new(r.x + dx, r.y + dy, r.w, r.h));
    for &child in tree.children(id) {
        shift_box_rects(tree, child, dx, dy, list);
    }
}
