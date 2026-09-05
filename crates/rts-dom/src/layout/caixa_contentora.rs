//! O CONTAINING BLOCK de um `position:absolute`/`fixed` é a PADDING BOX do
//! ancestral positioned — não a border box (CSS 2.1 §10.1: "the containing
//! block is formed by the padding edge of the ancestor"). A borda fica FORA
//! do container onde `top`/`right`/`bottom`/`left` são medidos.
//!
//! `posicionado.rs` só tinha o BORDER-BOX guardado em `node_rects` (é o
//! mesmo retângulo que `getBoundingClientRect` reporta) e usava-o direto como
//! origem do containing block — um ancestral com QUALQUER borda deslocava
//! todo o conteúdo absoluto pela largura dela, nos dois eixos.
//!
//! Achado pelo lote `flex-align-justify-familia`: 31 dos 33 reftests do WPT
//! do lote comparam um flex container com `align-items`/`justify-content` E
//! `border: 1px solid` contra uma referência com `position:absolute` sobre o
//! MESMO `div` — o lado flex já acertava a caixa do item, o lado absoluto é
//! que desviava 1px em x E em y (o desvio de `border-width`). Confirmado com
//! `claude-paint-dump` nos dois lados de `flexbox_align-items-center.html`:
//! o item flex saía em (25,49) e o item absoluto da referência saía em
//! (24,48) para os MESMOS `top`/`left` — 1px de borda perdido.
//!
//! Só a BORDA se subtrai — a padding fica DENTRO da padding box por
//! definição (`content-box ⊆ padding-box ⊆ border-box`) — e a largura da
//! borda nunca é percentual em CSS, por isso a conversão não precisa de
//! `ResolveCtx` nenhum: dá para calcular só com o `ComputedStyle` do
//! ancestral, sem re-resolver nada contra o pai dele.

use super::*;

/// Converte o border-box guardado em `node_rects` (`flow_rects`) para a
/// padding-box do MESMO nó — a caixa contra a qual `top`/`right`/`bottom`/
/// `left` de um descendente `position:absolute`/`fixed` são medidos.
pub(in crate::layout) fn padding_box(border_box: Rect, css: &ComputedStyle) -> Rect {
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    Rect::new(
        border_box.x + bl,
        border_box.y + bt,
        (border_box.w - bl - br).max(0.0),
        (border_box.h - bt - bb).max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::borders::{SideName, set_side_style, set_side_width};
    use crate::style::{BorderStyle, ComputedStyle};

    /// `border: <w>px solid` uniforme — o shorthand que as referências do WPT
    /// deste lote usam (`border: 1px solid black`).
    fn com_borda_uniforme(w: f32) -> ComputedStyle {
        let mut css = ComputedStyle::default();
        css.border_width = Some(w);
        css.border_style = Some(BorderStyle::Solid);
        css
    }

    #[test]
    fn borda_de_1px_desloca_a_origem_e_encolhe_as_dimensoes() {
        let border_box = Rect::new(8.0, 16.0, 482.0, 98.0);
        let css = com_borda_uniforme(1.0);
        let pb = padding_box(border_box, &css);
        // O caso EXACTO de `flexbox_align-items-center.html`: o ancestral
        // `div{border:1px solid}` tem border-box (8,16,482,98); a padding-box
        // (= o containing block do span absoluto) começa 1px dentro nos dois
        // eixos e perde 1px de cada lado nas duas dimensões.
        assert_eq!(pb, Rect::new(9.0, 17.0, 480.0, 96.0));
    }

    #[test]
    fn sem_borda_a_padding_box_e_a_border_box() {
        let border_box = Rect::new(8.0, 16.0, 482.0, 98.0);
        let css = ComputedStyle::default();
        let pb = padding_box(border_box, &css);
        assert_eq!(pb, border_box);
    }

    #[test]
    fn borda_por_lado_assimetrica_desloca_cada_eixo_pelo_seu_proprio_lado() {
        let border_box = Rect::new(0.0, 0.0, 100.0, 50.0);
        let mut css = ComputedStyle::default();
        set_side_width(&mut css, SideName::Top, Some(2.0));
        set_side_width(&mut css, SideName::Right, Some(4.0));
        set_side_width(&mut css, SideName::Bottom, Some(6.0));
        set_side_width(&mut css, SideName::Left, Some(8.0));
        set_side_style(&mut css, SideName::Top, Some(BorderStyle::Solid));
        set_side_style(&mut css, SideName::Right, Some(BorderStyle::Solid));
        set_side_style(&mut css, SideName::Bottom, Some(BorderStyle::Solid));
        set_side_style(&mut css, SideName::Left, Some(BorderStyle::Solid));
        let pb = padding_box(border_box, &css);
        assert_eq!(pb, Rect::new(8.0, 2.0, 100.0 - 8.0 - 4.0, 50.0 - 2.0 - 6.0));
    }
}
