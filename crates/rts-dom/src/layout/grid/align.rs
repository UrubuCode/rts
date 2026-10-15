//! Box alignment helpers inside a grid cell.

/// Offset de alinhamento de um item de tamanho `item` dentro de uma célula de
/// tamanho `cell` (start=0, center=(cell-item)/2, end=cell-item; stretch=0).
///
/// `Center`/`FlexEnd` (sem o prefixo `safe`) NÃO recortam em `.max(0.0)` — a
/// grelha é `unsafe` por omissão (css-align-3 §4.4: só as formas `safe *`
/// pedem o fallback), e um item maior do que a célula transborda dos DOIS
/// lados quando `justify-items`/`align-items` é `center` puro. O
/// `.max(0.0)` que aqui estava antes desta correcção tornava `center`/
/// `flex-end` sempre seguros, ao contrário do default da spec — `SafeCenter`/
/// `SafeEnd` (que ANTES caíam no `_ => 0.0`, i.e. start incondicional) são o
/// lugar certo para esse recorte.
pub(super) fn cell_align_offset(a: crate::style::AlignItems, cell: f32, item: f32) -> f32 {
    let free = cell - item;
    match a {
        crate::style::AlignItems::Center => free / 2.0,
        crate::style::AlignItems::FlexEnd
        | crate::style::AlignItems::Right
        | crate::style::AlignItems::SelfEnd => free,
        crate::style::AlignItems::SafeCenter => (free / 2.0).max(0.0),
        crate::style::AlignItems::SafeEnd => free.max(0.0),
        _ => 0.0, // FlexStart / Left / Stretch / SelfStart
    }
}

/// Soma os tamanhos das trilhas `start..end` mais os gaps entre elas.
pub(in crate::layout) fn span_size(sizes: &[f32], start: usize, end: usize, gap: f32) -> f32 {
    if sizes.is_empty() {
        return 0.0;
    }
    let end = end.max(start + 1).min(sizes.len());
    let start = start.min(sizes.len() - 1);
    let n = end.saturating_sub(start);
    sizes[start..end].iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * gap
}

#[cfg(test)]
mod tests {
    use super::cell_align_offset;
    use crate::style::AlignItems as A;

    #[test]
    fn center_is_unsafe_by_default() {
        assert_eq!(cell_align_offset(A::Center, 100.0, 200.0), -50.0);
        assert_eq!(cell_align_offset(A::FlexEnd, 100.0, 200.0), -100.0);
    }

    #[test]
    fn safe_falls_back_to_start_when_the_item_overflows() {
        assert_eq!(cell_align_offset(A::SafeCenter, 100.0, 200.0), 0.0);
        assert_eq!(cell_align_offset(A::SafeEnd, 100.0, 200.0), 0.0);
    }

    #[test]
    fn safe_matches_unsafe_when_the_item_fits() {
        assert_eq!(cell_align_offset(A::Center, 100.0, 40.0), 30.0);
        assert_eq!(cell_align_offset(A::SafeCenter, 100.0, 40.0), 30.0);
        assert_eq!(cell_align_offset(A::FlexEnd, 100.0, 40.0), 60.0);
        assert_eq!(cell_align_offset(A::SafeEnd, 100.0, 40.0), 60.0);
    }
}
