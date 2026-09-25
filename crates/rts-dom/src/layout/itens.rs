//! Transformar itens já desenhados: percorrer, deslocar, aplicar `transform`,
//! e registar a ordem e o retângulo de um nó.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
use crate::boxes::BoxId;
/// Percorre itens próprios e subárvores na ordem de pintura, acumulando o
/// deslocamento. Recursivo pela mesma razão que a estrutura é uma árvore: um
/// fragmento pode ter reusado outro.
pub(in crate::layout) fn walk_items(
    items: &[DisplayItem],
    children: &[ChildRef],
    dx: f32,
    dy: f32,
    f: &mut impl FnMut(&DisplayItem, f32, f32),
) {
    let mut next_child = 0usize;
    for (i, item) in items.iter().enumerate() {
        // Um `EndClip` só deixa passar à frente dele os filhos que JÁ existiam
        // quando foi emitido — ver a doc da variante. Para todo o resto o empate
        // no índice resolve-se a favor do filho, que é o que põe uma subárvore
        // reusada no meio dos itens próprios.
        // Um `BeginClip` empurra à sua FRENTE os filhos que já existiam antes
        // dele: o `at` deles foi deslocado pela inserção do marcador, e sem isto
        // o conteúdo inteiro da página cai dentro de um clip que não é dele.
        if let DisplayItem::BeginClip { filhos_antes, .. } = item {
            while next_child < *filhos_antes && next_child < children.len() {
                let c = &children[next_child];
                walk_items(
                    &c.fragment.items,
                    &c.fragment.children,
                    dx + c.dx,
                    dy + c.dy,
                    f,
                );
                next_child += 1;
            }
        }
        let teto = match item {
            DisplayItem::EndClip { filhos_dentro } => *filhos_dentro,
            _ => children.len(),
        };
        while next_child < teto.min(children.len()) && children[next_child].at <= i {
            let c = &children[next_child];
            walk_items(
                &c.fragment.items,
                &c.fragment.children,
                dx + c.dx,
                dy + c.dy,
                f,
            );
            next_child += 1;
        }
        f(item, dx, dy);
    }
    for c in &children[next_child..] {
        walk_items(
            &c.fragment.items,
            &c.fragment.children,
            dx + c.dx,
            dy + c.dy,
            f,
        );
    }
}

/// DESLOCA um item de pintura por `(dx, dy)`.
///
/// É a operação que torna um fragmento de layout REUSÁVEL: o desenho de uma
/// subárvore cujo conteúdo e constraints não mudaram é o mesmo desenho, na
/// posição nova. Tudo o que um item carrega é geometria absoluta em coordenadas
/// de conteúdo, então deslocar é somar — exceto o que é tamanho (`radius`,
/// `blur`, `size` do texto), que não se move.
pub(in crate::layout) fn translate_item(it: &mut DisplayItem, dx: f32, dy: f32) {
    let shift = |r: &mut Rect| {
        r.x += dx;
        r.y += dy;
    };
    match it {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Shadow { rect, .. }
        | DisplayItem::GradientRect { rect, .. }
        | DisplayItem::Border { rect, .. }
        | DisplayItem::Image { rect, .. }
        | DisplayItem::Pixels { rect, .. }
        | DisplayItem::BeginClip { rect, .. } => shift(rect),
        DisplayItem::Text { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        DisplayItem::Quad { pts, .. } => {
            for p in pts.iter_mut() {
                p.0 += dx;
                p.1 += dy;
            }
        }
        // A matriz descreve pontos em coordenadas de CONTEÚDO já absolutas —
        // deslocar a subárvore por (dx,dy) é compor uma translação PURA
        // depois dela: `nova(p) = mat(p) + (dx,dy)`, que em `e`/`f` é somar
        // direto (a parte linear a/b/c/d não muda por uma translação).
        DisplayItem::PushTransform { mat } => {
            mat.e += dx;
            mat.f += dy;
        }
        DisplayItem::EndClip { .. } | DisplayItem::PopTransform => {}
    }
}

/// Reserva uma posição de pintura antes de layoutar os descendentes. Um retângulo
/// placeholder fica invisível para o hit-test até ser preenchido por
/// [`record_box_rect`].
///
/// Por CAIXA, e só por caixa: a variante por nó, que traduzia `NodeIdx` para as
/// caixas dele contra `list.tree`, morreu com o último chamador que só sabia
/// o nó (BT-2a).
pub(crate) fn reserve_box_order(list: &mut DisplayList, box_id: BoxId) {
    if !list.box_rects.contains_key(&box_id) {
        list.box_rects.insert(box_id, Rect::new(0.0, 0.0, 0.0, 0.0));
        list.hit_order.push(box_id);
    }
}

/// Registra a geometria de UMA caixa. Se ela já foi reservada como ancestral,
/// apenas substitui o placeholder sem duplicar a ordem de hit-test. A única
/// variante que existe: um nó pode ter produzido fragmentos distintos na
/// BoxTree, e só a caixa exacta diz qual deles é este.
pub(crate) fn record_box_rect(list: &mut DisplayList, box_id: BoxId, rect: Rect) {
    if list.box_rects.insert(box_id, rect).is_none() {
        list.hit_order.push(box_id);
    }
}

/// Grows ONE box's rectangle to take in another of its fragments — a box
/// that breaks across lines, recorded one line at a time.
///
/// Not `inline_box::union_rect`, which does this by NODE and so cannot reach
/// a box with none (a generated inline). Nor its placeholder sentinel: that
/// guards against `reserve_node_order`, which only ever reserves boxes that
/// name a node.
pub(crate) fn union_box_rect(list: &mut DisplayList, box_id: BoxId, rect: Rect) {
    match list.box_rects.get_mut(&box_id) {
        Some(old) => *old = old.union(rect),
        None => record_box_rect(list, box_id, rect),
    }
}
