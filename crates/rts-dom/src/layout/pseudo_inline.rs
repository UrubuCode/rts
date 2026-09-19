//! The generated box (`::before`/`::after`) on the INLINE path: what a
//! pseudo that is not block-level hands to the line.
//!
//! It used to hand bare text — a run with a colour, a weight and a
//! decoration — so `width`, `height`, `padding`, `border`, `margin` and
//! `background` of an inline or `inline-block` pseudo were dropped
//! (`claude-pseudo-caixa-gerada`, measured in Blink). CSS 2.1 §12.1 says the
//! pseudo generates a BOX, and it now arrives at the line as one of the two
//! boxes a real element would be:
//!
//! - **`inline-block`**: one atom (`AtomicKind::Gerada(_, Atomo)`), measured
//!   and painted as a [`CaixaGerada`] by `pseudo_caixa.rs` — the same
//!   `montar`/`pintar` the block and flex-item roles use, not a fourth copy.
//! - **`inline`**: its text, bracketed by a start and an end edge
//!   (`Gerada(_, Inicio | Fim)`) that carry margin + border + padding as
//!   width on the line, exactly as `ArestaInicio`/`ArestaFim` do for a real
//!   inline with a surface. The background and borders are painted per line
//!   fragment by `inline_fragmentos::Superficies`, behind the text.
//!
//! Rejected: making the inline pseudo an atom too. It would have been one
//! code path instead of two, but an atom does not break across lines, and a
//! long generated string (a citation, a `content: attr(href)`) must.

use super::*;
use super::pseudo_caixa::{CaixaGerada, altura_das_linhas, linhas_do_texto, montar, resolve_arestas};
use crate::inline_box::ParteGerada;

/// The runs a generated box (`::before`/`::after`) of `id` hands to the line,
/// or none if the cascade generates no box or the box is block-level.
///
/// `donos` é a CADEIA inline inteira terminada no elemento originante, e não só
/// ele. No browser a caixa gerada está dentro da caixa do elemento e um clique
/// nela atinge o elemento — mas também está dentro de cada inline que o
/// envolve, exatamente como o texto normal está.
///
/// Isto já esteve errado, e o sintoma era invisível até o resto ficar certo:
/// com `owners: vec![id]` um `<span><a></a></span>` em que todo o conteúdo do
/// `<a>` vem de `a::before` deixava o `<span>` sem geometria NENHUMA, porque
/// nada lhe chamava `union_rect`. Na Wikipédia eram os 397 retrolinks da lista
/// de referências. Um fragmento gerado é um fragmento: conta para a união dos
/// ancestrais como qualquer outro, e é `uniontests.rs` que o fixa.
///
/// `display:block`/`flex`/`grid` não entra aqui: esse pseudo gera uma caixa
/// de BLOCO própria — `pseudo_bloco.rs`, só para o DONO de um fluxo vertical
/// — e não pode ser entregue aqui também, ou o conteúdo pinta DUAS vezes. Um
/// pseudo de bloco de um elemento que NÃO é dono de fluxo vertical (um
/// `<span>` a meio de uma linha) não tem hoje onde a caixa de bloco se
/// prenda — fica sem nenhuma das duas, o que é mais estreito do que "sempre
/// inline" mas nunca duplicado.
///
/// Still cut: `position:absolute` on the pseudo is flowed like the inline
/// most pseudos are.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn pseudo_run(
    dom: &Dom,
    id: NodeIdx,
    // The inline chain around the originating element, it included and last.
    donos: &[NodeIdx],
    pe: crate::style::PseudoElement,
    // The colour already resolved from the context — the generated box
    // inherits it when it declares no `color`.
    cor_herdada: u32,
    // idem for italics: the generated box inherits the element's style.
    herdado_italico: bool,
    // The line's width: the base of the pseudo's percentages and the cap of
    // an `inline-block` pseudo's shrink-to-fit width.
    base_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let Some(caixa) = dom.pseudo_box(id, pe) else {
        return Vec::new();
    };
    use crate::style::DisplayKind;
    let display = caixa.css.effective_display();
    if matches!(display, Some(DisplayKind::Block | DisplayKind::Flex | DisplayKind::Grid)) {
        return Vec::new();
    }
    let atomo = |parte: ParteGerada, ww: f32, wh: f32| InlineRun {
        text: String::new(),
        color: cor_herdada,
        bold: false,
        italic: false,
        deco: 0,
        owners: donos.to_vec(),
        atomic: Some((id, None, AtomicKind::Gerada(pe, parte))),
        ww,
        wh,
    };
    // `display: contents` generates no box: the text alone takes its place,
    // with no edge, border, background or atom (CSS Display 3 §2.5). Painting
    // the box drew a 100px red border Blink does not draw (WPT
    // `display-contents-before-after-002`).
    let sem_caixa = caixa.css.display_contents == Some(true);
    if display == Some(DisplayKind::InlineBlock) && !sem_caixa {
        let medida = medir_atomo(caixa, base_w, ctx);
        crate::bump!(inline_runs);
        return vec![atomo(ParteGerada::Atomo, medida.w, medida.h)];
    }
    let bordas = if sem_caixa { None } else { arestas_do_inline_gerado(&caixa.css, base_w, ctx) };
    crate::bump!(inline_runs);
    let texto = InlineRun {
        text: caixa.texto,
        color: cor_visivel(&caixa.css, caixa.css.color.unwrap_or(cor_herdada)),
        bold: caixa.css.bold.unwrap_or(false),
        // a caixa gerada é do PRÓPRIO elemento: nenhuma tag nova entra, por isso
        // a UA não tem aqui nada a dizer — só o CSS do pseudo e o que herdou.
        italic: caixa.css.italic.unwrap_or(herdado_italico),
        deco: decoration_code(&caixa.css),
        owners: donos.to_vec(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    };
    match bordas {
        Some([esq, dir]) => vec![atomo(ParteGerada::Inicio, esq, 0.0), texto, atomo(ParteGerada::Fim, dir, 0.0)],
        None => vec![texto],
    }
}

/// Width on the line of the start and end edges of an `inline` pseudo —
/// margin + border + padding on each side — or `None` when it has no surface
/// and no horizontal margin, so that the common bare-text pseudo (88 of 100
/// pseudo rules on the Wikipedia sheet) gets no extra runs at all.
fn arestas_do_inline_gerado(css: &ComputedStyle, base_w: f32, ctx: &LayoutCtx) -> Option<[f32; 2]> {
    let fonte = font_px(css, DEFAULT_FONT_SIZE);
    let [esq, dir, ..] = crate::inline_box::arestas_do_inline(css, fonte, base_w, ctx);
    let (ml, mr) = margens_horizontais(css, fonte, base_w, ctx);
    let tem = crate::inline_box::cria_caixa_apesar_de_inline(css) || ml != 0.0 || mr != 0.0;
    tem.then_some([ml + esq, dir + mr])
}

/// The pseudo's left and right margins, resolved as the edges are.
pub(in crate::layout) fn margens_horizontais(css: &ComputedStyle, fonte: f32, base_w: f32, ctx: &LayoutCtx) -> (f32, f32) {
    let r = contexto(base_w, fonte, ctx);
    (css.margin.left.resolve(&r).unwrap_or(0.0), css.margin.right.resolve(&r).unwrap_or(0.0))
}

fn contexto(base_w: f32, fonte: f32, ctx: &LayoutCtx) -> ResolveCtx {
    ResolveCtx {
        parent_content_w: base_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    }
}

/// An `inline-block` pseudo measured as the atom it is: its declared
/// `width`/`height`, or shrink-to-fit — the max-content width of its text,
/// capped by what the line offers (CSS 2.1 §10.3.9) — with its text broken
/// at that width and one line box per line.
fn medir_atomo(caixa: crate::pseudo::PseudoBox, base_w: f32, ctx: &LayoutCtx) -> CaixaGerada {
    let css = &caixa.css;
    let fonte = font_px(css, DEFAULT_FONT_SIZE);
    let r = contexto(base_w, fonte, ctx);
    let arestas = resolve_arestas(css, &r);
    let texto = super::segmento::collapse_ws(&caixa.texto, false).into_owned();
    let conteudo_w = css.width.and_then(|d| d.resolve(&r)).unwrap_or_else(|| {
        let disponivel = base_w - arestas.ml - arestas.mr - arestas.valores[1] - arestas.valores[3];
        let max_content = linhas_do_texto(css, &texto, f32::INFINITY, fonte, ctx)
            .iter()
            .map(|l| ctx.measurer.text_width(l, fonte, css.font_family.as_deref().is_some_and(crate::style::is_mono_family), css.bold.unwrap_or(false), css.italic.unwrap_or(false)))
            .fold(0.0, f32::max);
        max_content.min(disponivel.max(0.0))
    });
    let linhas = linhas_do_texto(css, &texto, conteudo_w, fonte, ctx);
    let conteudo_h = css
        .height
        .and_then(|d| d.resolve(&r))
        .unwrap_or_else(|| altura_das_linhas(css, &linhas, fonte, ctx));
    montar(caixa, arestas, conteudo_w, conteudo_h, linhas, fonte)
}

/// Paints the `inline-block` pseudo `pe` of `id` at the place the line gave
/// it. Its vertical place follows the rule the line applies to a real
/// `inline-block` (`linha.rs`, `AtomicKind::Block`): an empty one shorter
/// than the line sits its bottom margin edge on the `baseline`; otherwise its
/// top is the line's top `cy`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn pintar_atomo(
    dom: &Dom,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    x: f32,
    cy: f32,
    baseline: f32,
    line_h: f32,
    base_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let Some(caixa) = dom.pseudo_box(id, pe) else {
        return;
    };
    let medida = medir_atomo(caixa, base_w, ctx);
    let topo = if medida.linhas.is_empty() && medida.h < line_h { baseline - medida.h } else { cy };
    super::pseudo_caixa::pintar(list, &medida, x, topo, ctx);
}
