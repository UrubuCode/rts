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
//! - **`inline-block`**: one atom (`AtomicKind::Generated(_, Atom)`), measured
//!   and painted as a [`GeneratedBox`] by `pseudo_box.rs` — the same
//!   `build`/`paint` the block and flex-item roles use, not a fourth copy.
//! - **`inline`**: its text, bracketed by a start and an end edge
//!   (`Generated(_, Start | End)`) that carry margin + border + padding as
//!   width on the line, exactly as `EdgeStart`/`EdgeEnd` do for a real
//!   inline with a surface. The background and borders are painted per line
//!   fragment by `inline_fragments::Superficies`, behind the text.
//!
//! Rejected: making the inline pseudo an atom too. It would have been one
//! code path instead of two, but an atom does not break across lines, and a
//! long generated string (a citation, a `content: attr(href)`) must.

use super::*;
use crate::layout::block::pseudo_box::{GeneratedBox, lines_height, text_lines, build, resolve_edges};
use crate::inline_box::GeneratedPart;

/// The runs a generated box (`::before`/`::after`) of `id` hands to the line,
/// or none if the cascade generates no box or the box is block-level.
///
/// `owners` é a CADEIA inline inteira terminada no elemento originante, e não só
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
/// de BLOCO própria — `pseudo_block.rs`, só para o DONO de um fluxo vertical
/// — e não pode ser entregue aqui também, ou o conteúdo pinta DUAS vezes. Um
/// pseudo de bloco de um elemento que NÃO é dono de fluxo vertical (um
/// `<span>` a meio de uma linha) não tem hoje onde a caixa de bloco se
/// prenda — fica sem nenhuma das duas, o que é mais estreito do que "sempre
/// inline" mas nunca duplicado.
///
/// Still cut: `position:absolute` on the pseudo is flowed like the inline
/// most pseudos are.
///
/// **The atom's box is looked up by NODE here** (`BoxTree::generated_of`), in
/// the document's memoised tree — the one every layout list carries — because
/// `line.rs` calls this with the owner's node and no box. `runs.rs` has the
/// box it is walking and calls [`pseudo_run_of_box`] with the exact one:
/// for a split inline, the node's first box is not the fragment being walked.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn pseudo_run(
    dom: &Dom,
    id: NodeIdx,
    owners: &[NodeIdx],
    pe: crate::style::PseudoElement,
    inherited_color: u32,
    inherited_italic: bool,
    base_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    // The tree holds a generated box for every pseudo the cascade generates
    // (`box-tree.md` §10: the build asked the same `Dom::pseudo_box`), so a
    // pseudo with no box here is one with no content, and no run.
    let Some(generated) = dom.box_tree().generated_of(id, pe) else {
        return Vec::new();
    };
    pseudo_run_of_box(dom, id, generated, owners, pe, inherited_color, inherited_italic, base_w, ctx)
}

/// [`pseudo_run`] with the generated box already found: `generated` is what the
/// atom carries. Existence and content still come from `Dom::pseudo_box`, so
/// the runs are what they were before the box had an identity — this lot
/// names the box, it does not move it.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn pseudo_run_of_box(
    dom: &Dom,
    id: NodeIdx,
    generated: crate::boxes::BoxId,
    // The inline chain around the originating element, it included and last.
    owners: &[NodeIdx],
    pe: crate::style::PseudoElement,
    // The colour already resolved from the context — the generated box
    // inherits it when it declares no `color`.
    inherited_color: u32,
    // idem for italics: the generated box inherits the element's style.
    inherited_italic: bool,
    // The line's width: the base of the pseudo's percentages and the cap of
    // an `inline-block` pseudo's shrink-to-fit width.
    base_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let Some(pseudo) = dom.pseudo_box(id, pe) else {
        return Vec::new();
    };
    use crate::style::DisplayKind;
    let display = pseudo.css.effective_display();
    if matches!(display, Some(DisplayKind::Block | DisplayKind::Flex | DisplayKind::Grid)) {
        return Vec::new();
    }
    let atom = |part: GeneratedPart, ww: f32, wh: f32| InlineRun {
        text: String::new(),
        color: inherited_color,
        bold: false,
        italic: false,
        deco: 0,
        owners: owners.to_vec(),
        atomic: Some((id, generated, AtomicKind::Generated(pe, part))),
        ww,
        wh,
    };
    // `display: contents` generates no box: the text alone takes its place,
    // with no edge, border, background or atom (CSS Display 3 §2.5). Painting
    // the box drew a 100px red border Blink does not draw (WPT
    // `display-contents-before-after-002`).
    let boxless = pseudo.css.display_contents == Some(true);
    if display == Some(DisplayKind::InlineBlock) && !boxless {
        let measured = measure_atom((generated, pseudo), base_w, ctx);
        crate::bump!(inline_runs);
        return vec![atom(GeneratedPart::Atom, measured.w, measured.h)];
    }
    let edges = if boxless { None } else { generated_inline_edges(&pseudo.css, base_w, ctx) };
    crate::bump!(inline_runs);
    let text_run = InlineRun {
        text: pseudo.text,
        color: cor_visivel(&pseudo.css, pseudo.css.color.unwrap_or(inherited_color)),
        bold: pseudo.css.bold.unwrap_or(false),
        // a caixa gerada é do PRÓPRIO elemento: nenhuma tag nova entra, por isso
        // a UA não tem aqui nada a dizer — só o CSS do pseudo e o que herdou.
        italic: pseudo.css.italic.unwrap_or(inherited_italic),
        deco: decoration_code(&pseudo.css),
        owners: owners.to_vec(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    };
    match edges {
        Some([left, right]) => vec![atom(GeneratedPart::Start, left, 0.0), text_run, atom(GeneratedPart::End, right, 0.0)],
        None => vec![text_run],
    }
}

/// Width on the line of the start and end edges of an `inline` pseudo —
/// margin + border + padding on each side — or `None` when it has no surface
/// and no horizontal margin, so that the common bare-text pseudo (88 of 100
/// pseudo rules on the Wikipedia sheet) gets no extra runs at all.
fn generated_inline_edges(css: &ComputedStyle, base_w: f32, ctx: &LayoutCtx) -> Option<[f32; 2]> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let [left, right, ..] = crate::inline_box::arestas_do_inline(css, font, base_w, ctx);
    let (ml, mr) = horizontal_margins(css, font, base_w, ctx);
    let has_box = crate::inline_box::cria_caixa_apesar_de_inline(css) || ml != 0.0 || mr != 0.0;
    has_box.then_some([ml + left, right + mr])
}

/// The pseudo's left and right margins, resolved as the edges are.
pub(in crate::layout) fn horizontal_margins(css: &ComputedStyle, font: f32, base_w: f32, ctx: &LayoutCtx) -> (f32, f32) {
    let r = resolve_ctx(base_w, font, ctx);
    (css.margin.left.resolve(&r).unwrap_or(0.0), css.margin.right.resolve(&r).unwrap_or(0.0))
}

fn resolve_ctx(base_w: f32, font: f32, ctx: &LayoutCtx) -> ResolveCtx {
    ResolveCtx {
        parent_content_w: base_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    }
}

/// An `inline-block` pseudo measured as the atom it is: its declared
/// `width`/`height`, or shrink-to-fit — the max-content width of its text,
/// capped by what the line offers (CSS 2.1 §10.3.9) — with its text broken
/// at that width and one line box per line.
fn measure_atom((generated, pseudo): (crate::boxes::BoxId, crate::pseudo::PseudoBox), base_w: f32, ctx: &LayoutCtx) -> GeneratedBox {
    let css = &pseudo.css;
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let r = resolve_ctx(base_w, font, ctx);
    let edges = resolve_edges(css, &r);
    let text = super::segment::collapse_ws(&pseudo.text, false).into_owned();
    let content_w = css.width.and_then(|d| d.resolve(&r)).unwrap_or_else(|| {
        let available = base_w - edges.ml - edges.mr - edges.values[1] - edges.values[3];
        let max_content = text_lines(css, &text, f32::INFINITY, font, ctx)
            .iter()
            .map(|l| ctx.measurer.text_width(l, font, css.font_family.as_deref().is_some_and(crate::style::is_mono_family), css.bold.unwrap_or(false), css.italic.unwrap_or(false)))
            .fold(0.0, f32::max);
        max_content.min(available.max(0.0))
    });
    let lines = text_lines(css, &text, content_w, font, ctx);
    let content_h = css
        .height
        .and_then(|d| d.resolve(&r))
        .unwrap_or_else(|| lines_height(css, &lines, font, ctx));
    build((generated, pseudo), edges, content_w, content_h, lines, font)
}

/// Paints the `inline-block` pseudo `pe` of `id` at the place the line gave
/// it: `x` and its `top`, which the line decides by the same §10.8.1
/// envelope as a real `inline-block` (`line_baseline.rs`).
///
/// `generated` is the atom's box as the line has it — the exact box of THIS
/// fragment when the originating inline is split, so each fragment records
/// its own geometry.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn paint_atom(
    dom: &Dom,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    generated: crate::boxes::BoxId,
    x: f32,
    top: f32,
    base_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let Some(pseudo) = dom.pseudo_box(id, pe) else {
        return;
    };
    let measured = measure_atom((generated, pseudo), base_w, ctx);
    crate::layout::block::pseudo_box::paint(list, &measured, x, top, ctx);
}
