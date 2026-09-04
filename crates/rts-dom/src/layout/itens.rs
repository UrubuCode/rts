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
        DisplayItem::EndClip { .. } => {}
    }
}

/// Aplica uma matriz `transform` (já composta e em torno da origem — ver
/// `layout/transformacao.rs`) a um `DisplayItem`, mutando suas coords.
///
/// O BACKEND (`rts-egui`) só pinta retângulos e texto AXIS-ALIGNED — não há
/// mesh rotacionado — então isto é uma APROXIMAÇÃO deliberada e não a pintura
/// exata que a matriz descreve: o CANTO `(x,y)` do item move-se pelo ponto
/// exato que a matriz calcula (`Mat2d::apply`), e `w`/`h` escalam pela norma de
/// cada coluna da matriz (`sqrt(a²+b²)`, `sqrt(c²+d²)`) — o fator de escala que
/// uma rotação/skew pura induz no eixo, ignorando a inclinação. Cobre
/// translate/scale exatamente e rotate/skew "razoavelmente" (o mesmo corte que
/// já existia para rotate antes deste lote, agora extensivo a skew/matrix).
/// Rodar o backend em si é outra fatia — ver o PLAN.
pub(in crate::layout) fn apply_transform_to_item(it: &mut DisplayItem, mat: &super::Mat2d) {
    let sx = (mat.a * mat.a + mat.b * mat.b).sqrt();
    let sy = (mat.c * mat.c + mat.d * mat.d).sqrt();
    match it {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Border { rect, .. }
        | DisplayItem::GradientRect { rect, .. }
        | DisplayItem::Shadow { rect, .. }
        | DisplayItem::Image { rect, .. }
        | DisplayItem::Pixels { rect, .. } => {
            let (nx, ny) = mat.apply(rect.x, rect.y);
            rect.x = nx;
            rect.y = ny;
            rect.w *= sx;
            rect.h *= sy;
        }
        DisplayItem::Text { x, y, size, .. } => {
            let (nx, ny) = mat.apply(*x, *y);
            *x = nx;
            *y = ny;
            *size *= sy; // escala o texto na vertical (aproxima).
        }
        DisplayItem::BeginClip { rect, .. } => {
            let (nx, ny) = mat.apply(rect.x, rect.y);
            rect.x = nx;
            rect.y = ny;
            rect.w *= sx;
            rect.h *= sy;
        }
        DisplayItem::EndClip { .. } => {}
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
