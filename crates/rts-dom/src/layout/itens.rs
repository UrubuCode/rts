//! Transformar itens já desenhados: percorrer, deslocar, aplicar `transform`,
//! e registar a ordem e o retângulo de um nó.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
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
/// placeholder fica invisível para o hit-test até ser preenchido por `record_node_rect`.
pub(crate) fn reserve_node_order(list: &mut DisplayList, idx: NodeIdx) {
    if !list.node_rects.contains_key(&idx) {
        list.node_rects.insert(idx, Rect::new(0.0, 0.0, 0.0, 0.0));
        list.hit_order.push(idx);
    }
}

/// Registra uma caixa e sua geometria. Se o nó já foi reservado como ancestral,
/// apenas substitui o placeholder sem duplicar a ordem de hit-test.
pub(crate) fn record_node_rect(list: &mut DisplayList, idx: NodeIdx, rect: Rect) {
    if list.node_rects.insert(idx, rect).is_none() {
        list.hit_order.push(idx);
    }
}
