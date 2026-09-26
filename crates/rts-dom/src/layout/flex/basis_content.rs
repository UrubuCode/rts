//! `flex-basis: content` no eixo de LINHA (Flexbox §7.2.3): a base do item é
//! SEMPRE o conteúdo — ao contrário de `flex-basis: auto`, que primeiro olha
//! para o `width` do item e só cai no conteúdo quando ele também é `auto`.
//!
//! `flex_base_outer` (`limits.rs`) resolve os dois casos ao mesmo
//! `None` (`Dimension::MaxContent.resolve()` devolve `None`, tal como
//! `Dimension::Auto`) e por isso caíam os dois no MESMO fallback
//! (`child_outer_width`, que olha para `width` primeiro) — um
//! `flex-basis:content` com `width` declarado usava o `width`, quando a
//! spec pede que a keyword `content` o IGNORE sempre. Módulo próprio (e não
//! mais uma função em `limits.rs`) porque a resposta é por EIXO.
//!
//! O espelho para COLUNA **não vive aqui**: está em `layout_children_column`,
//! onde o item já é montado, e usa `column_shrink::altura_conteudo_sem_height`
//! — a mesma aproximação que `min_main_auto` documenta, e com o mesmo limite.
//! Ela soma cada filho pela SUA própria altura, um modelo de blocos
//! EMPILHADOS, por isso só é consultada quando há um `height` declarado para
//! ignorar. Sem `height`, a medida natural do item já É a do conteúdo e é
//! mais fiel: inline-blocks na mesma linha e floats lado a lado não empilham,
//! e a soma dava-lhes o triplo da altura (os seis casos de
//! `flexbox-flex-basis-content-004a`, medidos no Blink).
//!
//! O que continua por fazer é a segunda passada de layout que mediria o item
//! ignorando a `height` sem somar filhos — só ela cobre os dois casos que
//! ambas as vias erram: um elemento SUBSTITUÍDO como item (Blink 14, motor
//! 10) e inline replaced dentro do item (Blink 22, motor 34).

use super::*;

/// A base OUTER de um item no eixo de LINHA quando `flex-basis` é
/// literalmente `content`: o conteúdo do item (`content_natural_width`, que
/// nunca olha para o `width` do próprio nó — só para os filhos) mais o
/// frame (margem + borda + padding). Nunca consulta `css.width`.
pub(in crate::layout) fn base_outer_linha_forcado_pelo_conteudo(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    container_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let font = font_px(css, font_size);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let frame = css.margin.resolve_h(&resolve)
        + { let [_, r, _, l] = crate::style::borders::used_widths(css); l + r }
        + css.padding.resolve_h(&resolve);
    content_natural_width(dom, id, font, ctx) + frame
}
