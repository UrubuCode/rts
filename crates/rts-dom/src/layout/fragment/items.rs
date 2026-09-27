//! The three producer writers of a list's geometry: reserve a box's place in
//! the hit order, record its rectangle, add one line fragment of an inline.
//! (Walking the pieces is `paint/pieces.rs`; shifting an item is
//! `paint/item.rs::translate_item`, moved there in PQ-C1 because it touches
//! nothing of layout.)
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
use crate::boxes::BoxId;
/// Reserva uma posição de pintura antes de layoutar os descendentes. Um retângulo
/// placeholder fica invisível para o hit-test até ser preenchido por
/// [`record_box_rect`]. The position is a `Piece::Rect` pushed NOW, before the
/// descendants' pieces: the hit order is read from those marks in sequence
/// order, so an ancestor stays below its descendants.
///
/// Por CAIXA, e só por caixa: a variante por nó, que traduzia `NodeIdx` para as
/// caixas dele contra `list.tree`, morreu com o último chamador que só sabia
/// o nó (BT-2a).
pub(crate) fn reserve_box_order(list: &mut DisplayList, box_id: BoxId) {
    if list.box_rects.reserve(box_id) {
        list.pieces.push(Piece::Rect(box_id));
    }
}

/// Registra a geometria de UMA caixa. Se ela já foi reservada como ancestral,
/// apenas substitui o placeholder sem duplicar a ordem de hit-test. A única
/// variante que existe: um nó pode ter produzido fragmentos distintos na
/// BoxTree, e só a caixa exacta diz qual deles é este.
pub(crate) fn record_box_rect(list: &mut DisplayList, box_id: BoxId, rect: Rect) {
    if list.box_rects.insert(box_id, rect) {
        list.pieces.push(Piece::Rect(box_id));
    }
}

/// One line's piece of a box that breaks across lines: it grows that line's
/// fragment, or starts the box's next one (`box_rects.rs`, I4). The only
/// writer an inline has — `inline_box::union_rect` resolves a node to its
/// boxes and comes here, a generated inline (no node) comes here directly.
pub(crate) fn add_line_fragment(list: &mut DisplayList, box_id: BoxId, rect: Rect, line: super::LineId) {
    if list.box_rects.add_on_line(box_id, rect, line) {
        list.pieces.push(Piece::Rect(box_id));
    }
}
