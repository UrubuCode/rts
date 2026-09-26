//! Where free space puts an item: `justify_offsets` (leading space and the
//! gap between items, from a `justify-content`/`align-content` value) and
//! `align_offset` (one item's offset inside a line's cross size).
//!
//! Apart from `column.rs` because neither is a column question: the row
//! (`row.rs`), the multi-line pass (`lines.rs`), the baseline fallback
//! (`baseline.rs`), the wrapped column and the grid (`grid/collapse.rs`) all
//! read them. `column.rs` re-exports both so every existing path resolves.
//! Rejected: moving `justify_e_align`/`fisico_para_coluna`/`mirror_justify`
//! here too — those RESOLVE a column's keywords, which is column-specific,
//! where these two only turn a resolved keyword into numbers.

/// `Start`/`End` entram aqui só para `align-content` (multi-linha), que chama
/// isto DIRETO com o valor cru — `justify-content` já os resolveu para
/// `FlexStart`/`FlexEnd` em `fisico_para_eixo`/`fisico_para_coluna` antes de
/// chegar aqui. Tratados como `FlexStart`/`FlexEnd` (mesma posição física
/// que já tinham); `wrap-reverse` não os espelha aqui — nenhum valor é
/// espelhado no `align-content` hoje, o que fica fora deste lote.
pub(in crate::layout) fn justify_offsets(j: crate::style::JustifyContent, free: f32, n: usize) -> (f32, f32) {
    use crate::style::JustifyContent as J;
    if free <= 0.0 {
        return match j {
            J::Center => (free / 2.0, 0.0), // leading negativo = transbordo centrado
            J::FlexEnd | J::End => (free, 0.0), // todo o overflow no start
            // flex-start E os space-* → flush no start (fiel ao Chrome em overflow).
            J::FlexStart | J::SpaceBetween | J::SpaceAround | J::SpaceEvenly | J::Left | J::Start => (0.0, 0.0),
            J::Right => (free, 0.0),
        };
    }
    match j {
        J::FlexStart | J::Left | J::Start => (0.0, 0.0),
        J::FlexEnd | J::Right | J::End => (free, 0.0),
        J::Center => (free / 2.0, 0.0),
        J::SpaceBetween => {
            if n > 1 {
                (0.0, free / (n - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        J::SpaceAround => {
            if n >= 1 {
                (free / (2 * n) as f32, free / n as f32)
            } else {
                (0.0, 0.0)
            }
        }
        J::SpaceEvenly => (free / (n + 1) as f32, free / (n + 1) as f32),
    }
}

/// Offset no eixo cruzado de um item, dado o align-items, a altura da linha `line_h`
/// e a altura outer do item `item_h`. (stretch é tratado como flex-start aqui — o
/// esticar real exige passar altura imposta ao layout_block, fase futura.)
///
/// `Baseline` cai em `FlexStart`: o alinhamento por baseline REAL (grupo por
/// linha, ascent por item) só está feito no eixo de LINHA
/// (`layout/flex_baseline.rs`, que resolve o offset ANTES de chegar aqui —
/// esta função só vê o `Baseline` de uma coluna, ou de um item cujo grupo não
/// tinha ninguém para partilhar a baseline). É o fallback que a própria spec
/// prevê (Flexbox §8.5) quando o eixo cruzado não tem baseline partilhável.
/// `LastBaseline` cai em `FlexEnd` pelo mesmo motivo (ver o doc da variante).
/// `safe` (css-align §4.4): um item que transborda cai no início, nunca negativo.
pub(in crate::layout) fn align_offset(a: crate::style::AlignItems, line_h: f32, item_h: f32) -> f32 {
    use crate::style::AlignItems as A;
    let free = line_h - item_h;
    match a {
        A::Stretch | A::FlexStart | A::Baseline => 0.0,
        A::FlexEnd | A::LastBaseline => free,
        A::SafeEnd | A::SafeCenter => (if a == A::SafeEnd { free } else { free / 2.0 }).max(0.0),
        A::Center => free / 2.0,
    }
}
