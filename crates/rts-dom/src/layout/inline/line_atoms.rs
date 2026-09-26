//! The ATOM branch of `line.rs`'s per-segment emission loop: what happens
//! when a `Segment` in a line is a widget, a replaced element, an
//! inline-block, or a generated atom, rather than text.
//!
//! Moved out of `layout_inline_flow` (`line.rs`, teto de 500) — a pure move,
//! nothing changed. It is its own function rather than a smaller one because
//! the `for seg in line` loop's atomic arm ends in `continue`, never falling
//! through to the text arm below it, so the whole branch is a single unit
//! with one call site.

use super::vertical_align::Envelope;
use super::*;

/// Emits one atomic segment (`seg.atomic` is `Some`) into `list`, advancing
/// `seg_x` past it. Mirrors exactly what the `if let Some((a_idx, caixa,
/// kind)) = seg.atomic { … }` arm did inline in `layout_inline_flow`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn emit_atom(
    dom: &Dom,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
    seg: &Segment,
    seg_x: &mut f32,
    x: f32,
    cy: f32,
    line_advance: f32,
    line_at: usize,
    surfaces: &mut super::inline_fragments::Surfaces,
    text_top: f32,
    ascent: f32,
    font_size: f32,
    family: Option<&str>,
    envelope: &Option<Envelope>,
    content_w: f32,
    cb_h: Option<f32>,
    on_baseline: bool,
    text_owner_anchor: f32,
    content: f32,
    line_id: &super::LineScope,
) {
    let Some((a_idx, box_id, kind)) = seg.atomic else {
        return;
    };
    // Float and static-position anchors have nothing on the line (`static_anchor.rs`).
    if super::static_anchor::outside_line(dom, (a_idx, box_id, kind), *seg_x, x, cy, cy + line_advance, line_at, list) {
        return;
    }
    let (start_index, (rx, ry)) = (list.pieces.len(), crate::layout::positioned::relative::inline_offset(dom, seg.owners.last().copied(), content_w, cb_h, ctx));
    match kind {
        AtomicKind::Widget => {
            // WIDGET inline: pinta a caixa no lugar (botão via layout_button;
            // campo de texto via layout_input com o avail da linha).
            let wcss = dom.computed_style_idx(a_idx).unwrap_or_default();
            let itype = dom
                .node(a_idx)
                .attr("type")
                .map(|t| t.to_ascii_lowercase())
                .unwrap_or_default();
            if matches!(itype.as_str(), "submit" | "button" | "reset") {
                layout_button(
                    dom,
                    a_idx,
                    box_id,
                    &wcss,
                    *seg_x,
                    cy,
                    None,
                    ctx,
                    list,
                );
            } else {
                // `None` de altura disponível: uma caixa atómica numa
                // linha não tem containing block de altura definida, e
                // é isso que faz `height:%` valer `auto` — como no
                // browser.
                layout_input(
                    dom, a_idx, box_id, &wcss, *seg_x, cy, seg.ww, None, None, None, ctx, list,
                );
            }
        }
        AtomicKind::Replaced => {
            // REPLACED inline (um `<img>` no meio do texto): a caixa é o
            // tamanho já medido. Só se pinta quando há pixels — e aí é
            // `layout_image` que o faz, o mesmo caminho do fluxo de bloco,
            // em vez de um segundo emissor de imagem só para o inline.
            // Replaced inline senta na BASELINE (§10.8; `claude-img-ficheiro`: y=15).
            let top = text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh;
            if super::line::is_canvas(dom, a_idx) {
                // O `<canvas>` pinta-se SEMPRE que está na linha, e
                // não só quando tem desenho: a superfície pode
                // chegar depois, e é `layout_canvas` que reserva a
                // caixa entretanto — a mesma doutrina que o
                // `<img>` segue no caminho de bloco.
                let ccss = dom.computed_style_idx(a_idx).unwrap_or_default();
                layout_canvas(dom, a_idx, box_id, &ccss, *seg_x, top, seg.ww.max(1.0), cb_h, ctx, list);
            } else if dom.image_dims(a_idx).is_some() {
                let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                layout_image(dom, a_idx, box_id, &icss, *seg_x, top, seg.ww.max(1.0), cb_h, None, None, ctx, list);
            }
        }
        AtomicKind::Block => {
            // Um inline-block PINTA-SE como bloco (fundo, borda,
            // padding) mas na posição que a linha lhe deu. É o mesmo
            // `layout_block` da corrida de inline-blocks irmãos —
            // não um segundo emissor — só que o x/y vem do fluxo.
            // Inline-block VAZIO senta o fundo na baseline (§10.8.1; caret do Bootstrap a y=9).
            let top = match envelope {
                Some(env) => super::line_baseline::atom_top(dom, seg, cy, env, font_size, family, content_w, ctx),
                None => cy,
            };
            layout_block(
                dom,
                a_idx,
                box_id,
                *seg_x,
                top,
                seg.ww.max(1.0),
                None,
                None,
                None,
                false,
                true,
                // Inline-block atômico de uma linha: isolado, como
                // qualquer inline-block (estabelece BFC próprio).
                &BlockFormattingContext::new(),
                ctx,
                list,
            );
        }
        AtomicKind::Generated(pe, GeneratedPart::Atom) => {
            // A generated inline-block always makes `envelope` Some.
            let top = envelope
                .as_ref()
                .map_or(cy, |env| super::line_baseline::atom_top(dom, seg, cy, env, font_size, family, content_w, ctx));
            super::pseudo_inline::paint_atom(dom, a_idx, pe, box_id, *seg_x, top, content_w, ctx, list);
        }
        AtomicKind::Marker
        | AtomicKind::Break
        | AtomicKind::EdgeStart
        | AtomicKind::EdgeEnd
        | AtomicKind::Generated(..)
        | AtomicKind::Float
        | AtomicKind::StaticAnchor => {}
    }
    crate::layout::positioned::relative::shift_from(list, start_index, kind.tem_corpo().then_some(box_id), rx, ry);
    surfaces.cover(dom, &seg.owners, *seg_x, *seg_x + seg.ww);
    match kind {
        AtomicKind::EdgeStart => surfaces.mark(a_idx, true),
        AtomicKind::EdgeEnd => surfaces.mark(a_idx, false),
        AtomicKind::Generated(pe, GeneratedPart::Start) => surfaces.open_generated(dom, a_idx, pe, *seg_x, seg.ww, content_w, ctx),
        AtomicKind::Generated(pe, GeneratedPart::End) => surfaces.close_generated(dom, a_idx, pe, content_w, ctx),
        _ => {}
    }
    // A CAIXA DO PRÓPRIO: só regista aqui quem NADA mais registou.
    // `Widget`/`Block` chamam `layout_input`/`layout_button`/
    // `layout_block` INCONDICIONALMENTE (o `match` acima), e cada
    // um já grava a SUA — a border box (correta); unir aqui
    // `seg.ww`/`seg.wh` (a OUTER, com margem, que é o que a LINHA
    // reserva) inflava o rect do nó com a margem por cima da que
    // já tinha: um `inline-block` com `margin-bottom:5px`
    // respondia h=25 em vez de 20.
    //
    // `Replaced` é DIFERENTE: só grava quando há pixels (o mesmo
    // guard do `match` acima) — sem imagem decodificada,
    // `layout_image` nunca corre e É esta união que dá caixa ao
    // `<img>` enquanto não há pixels.
    // Uma aresta não é caixa própria: o dono (que está em `owners`)
    // recebe-a como fragmento no laço abaixo.
    let already_registered = matches!(
        kind,
        AtomicKind::Widget | AtomicKind::Block | AtomicKind::EdgeStart | AtomicKind::EdgeEnd | AtomicKind::Generated(..)
    ) || (kind == AtomicKind::Replaced
        && (dom.image_dims(a_idx).is_some() || super::line::is_canvas(dom, a_idx)));
    if !already_registered {
        let own_rect = match kind {
            // `Marker`: inline SEM conteúdo (`<span></span>`) —
            // no Blink dá 0×0, não a altura do strut. Um vazio
            // com `content` gerado nunca chega aqui — `runs.rs`
            // só emite `Marker` quando não gerou run nenhum.
            AtomicKind::Marker => Rect::new(*seg_x, text_top, 0.0, 0.0),
            AtomicKind::Break => Rect::new(*seg_x, text_top, 0.0, content),
            AtomicKind::Replaced =>
                Rect::new(*seg_x, text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh, seg.ww, seg.wh),
            _ => Rect::new(*seg_x, cy, seg.ww, seg.wh),
        };
        crate::inline_box::union_rect(list, a_idx, Rect::new(own_rect.x + rx, own_rect.y + ry, own_rect.w, own_rect.h), line_id);
    }
    // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
    // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
    // 528px de altura mede 17px no browser, não 528. É a mesma regra
    // que já vale para o texto, aplicada ao que não é texto.
    //
    // `Marker`: `a_idx` está no fim de `seg.owners` (todo
    // container inline entra na própria cadeia, para dar caixa a
    // um `::before`/`::after` gerado por ele) — mas um marker
    // GENUÍNO (sem gerado nenhum, único caso em que existe) não
    // tem fragmento a dar-se, e isto devolvia a altura que a
    // `própria` acima acabou de zerar. `owner != a_idx` evita-o.
    let not_itself = |o: &&NodeIdx| !(kind == AtomicKind::Marker && **o == a_idx);
    for &owner in seg.owners.iter().filter(not_itself) {
        crate::inline_box::union_rect(
            list,
            owner,
            super::inline_fragments::owner_fragment(
                dom,
                owner,
                *seg_x,
                // The text's own anchor: `cy + meia` made the span 5px too tall on a line an atom grew.
                text_owner_anchor,
                seg.ww,
                content,
                ctx,
                on_baseline,
                (content_w, cb_h),
            ), line_id,
        );
    }
    *seg_x += seg.ww;
}
