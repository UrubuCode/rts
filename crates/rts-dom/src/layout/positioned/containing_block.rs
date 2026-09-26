//! O CONTAINING BLOCK de um `position:absolute`/`fixed` é a PADDING BOX do
//! ancestral positioned — não a border box (CSS 2.1 §10.1: "the containing
//! block is formed by the padding edge of the ancestor"). A borda fica FORA
//! do container onde `top`/`right`/`bottom`/`left` são medidos.
//!
//! `posicionado.rs` só tinha o BORDER-BOX guardado na geometria por nó (era
//! `node_rects`; hoje `box_rects`, agregado por `DisplayList::rect_of_node` —
//! o mesmo retângulo que `getBoundingClientRect` reporta) e usava-o direto
//! como origem do containing block — um ancestral com QUALQUER borda
//! deslocava todo o conteúdo absoluto pela largura dela, nos dois eixos.
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

/// Converte o border-box guardado em `flow_rects` (a geometria por nó) para a
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

/// Converte o border-box para a CONTENT box do MESMO nó — onde os filhos em
/// fluxo normal começam. Diferente de [`padding_box`]: o CONTAINING BLOCK de
/// um `position:absolute` é a padding box (CSS 2.1 §10.1), mas a STATIC
/// POSITION de um fora-de-fluxo (`posicao_estatica.rs`) é onde o CONTEÚDO
/// começaria — um passo mais para dentro, através do padding.
///
/// O padding pode ser percentual; sem o `avail_w` que o layout do PAI de
/// `css` usou de verdade para o resolver, aproxima-se com a LARGURA do
/// próprio border-box — exacto para `px`/`em`/`rem` (a maioria dos casos, e
/// os medidos por este lote), e o único caso onde diverge (padding em `%`)
/// não tem fixture a pedir mais.
///
/// A percentagem resolve pela [`ContainingBlock`], no eixo INLINE, e os quatro
/// lados no MESMO eixo de propósito: um `padding-top` percentual é contra a
/// largura do bloco contentor, não contra a altura (CSS 2.1 §8.4). É a regra
/// que faz um `padding-top:56.25%` ser a caixa de 16:9 que toda a web usa, e
/// escrevê-la aqui é o que impede o próximo a passar por este ficheiro de a
/// "corrigir" para a altura. O que a entidade acrescenta face ao `ResolveCtx`
/// solto é o outro meio da regra: um border-box de largura não finita — o que
/// uma medição em max-content passa — é uma base INDEFINIDA, e a percentagem
/// computa a `auto`, que aqui é o zero do `unwrap_or`, em vez de um padding
/// infinito que comia a caixa inteira.
pub(in crate::layout) fn content_box(border_box: Rect, css: &ComputedStyle, ctx: &LayoutCtx) -> Rect {
    let pb = padding_box(border_box, css);
    let resolve = ResolveCtx {
        parent_content_w: border_box.w,
        node_font_size: font_px(css, DEFAULT_FONT_SIZE),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let cb = crate::style::ContainingBlock::horizontal_tb(border_box.w, Some(border_box.h));
    let padding = |lado: crate::style::Side| match lado {
        crate::style::Side::Len(d) => cb
            .resolve(d, crate::style::Axis::Inline, &resolve)
            .unwrap_or(0.0),
        _ => 0.0,
    };
    let pt = padding(css.padding.top);
    let pr = padding(css.padding.right);
    let pbo = padding(css.padding.bottom);
    let pl = padding(css.padding.left);
    Rect::new(
        pb.x + pl,
        pb.y + pt,
        (pb.w - pl - pr).max(0.0),
        (pb.h - pt - pbo).max(0.0),
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

    fn ctx() -> LayoutCtx<'static> {
        LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &crate::layout::measure::measure::ApproxMeasurer,
        }
    }

    /// `content_box` desce um passo A MAIS que `padding_box` (usado pela
    /// `posicao_estatica.rs`): um `padding:20px` sem borda nenhuma NÃO desloca
    /// o containing block (a padding box começa na mesma origem do border box
    /// quando não há borda) mas DESLOCA onde o conteúdo — e a posição estática
    /// de um fora-de-fluxo — começaria.
    #[test]
    fn padding_sem_borda_desloca_o_content_mas_nao_a_padding_box() {
        let border_box = Rect::new(0.0, 0.0, 100.0, 300.0);
        let mut css = ComputedStyle::default();
        css.padding = crate::style::Edges::all(crate::style::Side::px_len(20.0));
        assert_eq!(padding_box(border_box, &css), border_box);
        let cb = content_box(border_box, &css, &ctx());
        assert_eq!(cb, Rect::new(20.0, 20.0, 60.0, 260.0));
    }

    /// Um padding percentual é contra a LARGURA nos quatro lados.
    ///
    /// Numa caixa de 100×300, `padding:10%` vale 10 em cima e em baixo também —
    /// não 30. É a regra que faz um `padding-top:56.25%` ser a caixa de 16:9, e
    /// está aqui porque a resolução passou a perguntar por EIXO: a pergunta
    /// existe agora, e o que se pina é a resposta ser `Inline` nos quatro.
    #[test]
    fn um_padding_percentual_e_contra_a_largura_nos_quatro_lados() {
        let mut css = ComputedStyle::default();
        css.padding = crate::style::Edges::all(crate::style::Side::Len(
            crate::style::Dimension::Percent(10.0),
        ));
        let cb = content_box(Rect::new(0.0, 0.0, 100.0, 300.0), &css, &ctx());
        assert_eq!(cb, Rect::new(10.0, 10.0, 80.0, 280.0));
    }

    /// Um padding percentual contra uma largura NÃO FINITA computa a `auto` —
    /// que aqui é zero — em vez de comer a caixa inteira.
    ///
    /// Uma largura infinita é o que uma medição em max-content passa. Antes, os
    /// quatro lados resolviam para infinito e a altura do content-box saía
    /// `(300 − ∞ − ∞).max(0)` = 0: uma caixa sem conteúdo nenhum, sem nada a
    /// dizer porquê. É o mesmo defeito que dava `w: inf` a um item
    /// shrink-to-fit, visto do outro lado da subtracção — e por isso a régua é
    /// a altura, que é finita e conhecida, e não a largura, que continua
    /// infinita porque o border-box o é.
    #[test]
    fn um_padding_percentual_sem_base_nao_devora_a_caixa() {
        let mut css = ComputedStyle::default();
        css.padding = crate::style::Edges::all(crate::style::Side::Len(
            crate::style::Dimension::Percent(50.0),
        ));
        let cb = content_box(Rect::new(0.0, 0.0, f32::INFINITY, 300.0), &css, &ctx());
        assert_eq!(cb.y, 0.0, "sem base, o padding de topo é zero e não infinito");
        assert_eq!(cb.h, 300.0, "a altura do conteúdo sobrevive: {cb:?}");
    }
}
