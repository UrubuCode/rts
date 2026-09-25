//! The ATOM branch of `linha.rs`'s per-segment emission loop: what happens
//! when a `Segment` in a line is a widget, a replaced element, an
//! inline-block, or a generated atom, rather than text.
//!
//! Moved out of `layout_inline_flow` (`linha.rs`, teto de 500) — a pure move,
//! nothing changed. It is its own function rather than a smaller one because
//! the `for seg in line` loop's atomic arm ends in `continue`, never falling
//! through to the text arm below it, so the whole branch is a single unit
//! with one call site.

use super::alinhamento_vertical::Envelope;
use super::*;

/// Emits one atomic segment (`seg.atomic` is `Some`) into `list`, advancing
/// `seg_x` past it. Mirrors exactly what the `if let Some((a_idx, caixa,
/// kind)) = seg.atomic { … }` arm did inline in `layout_inline_flow`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn emitir_atomo(
    dom: &Dom,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
    seg: &Segment,
    seg_x: &mut f32,
    x: f32,
    cy: f32,
    line_advance: f32,
    at_linha: usize,
    superficies: &mut super::inline_fragmentos::Superficies,
    text_top: f32,
    ascent: f32,
    font_size: f32,
    family: Option<&str>,
    envelope: &Option<Envelope>,
    content_w: f32,
    na_baseline: bool,
    text_owner_anchor: f32,
    conteudo: f32,
    line_id: &super::LineScope,
) {
    let Some((a_idx, caixa, kind)) = seg.atomic else {
        return;
    };
    // Float and static-position anchors have nothing on the line (`ancora_estatica.rs`).
    if super::ancora_estatica::fora_da_linha(dom, (a_idx, caixa, kind), *seg_x, x, cy, cy + line_advance, at_linha, list) {
        return;
    }
    let (desde, (rx, ry)) = (list.pieces.len(), super::relativo::offset_do_inline(dom, seg.owners.last().copied(), ctx));
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
                    caixa,
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
                    dom, a_idx, caixa, &wcss, *seg_x, cy, seg.ww, None, None, None, ctx, list,
                );
            }
        }
        AtomicKind::Replaced => {
            // REPLACED inline (um `<img>` no meio do texto): a caixa é o
            // tamanho já medido. Só se pinta quando há pixels — e aí é
            // `layout_image` que o faz, o mesmo caminho do fluxo de bloco,
            // em vez de um segundo emissor de imagem só para o inline.
            // Replaced inline senta na BASELINE (§10.8; `claude-img-ficheiro`: y=15).
            let topo = text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh;
            if super::linha::e_canvas(dom, a_idx) {
                // O `<canvas>` pinta-se SEMPRE que está na linha, e
                // não só quando tem desenho: a superfície pode
                // chegar depois, e é `layout_canvas` que reserva a
                // caixa entretanto — a mesma doutrina que o
                // `<img>` segue no caminho de bloco.
                let ccss = dom.computed_style_idx(a_idx).unwrap_or_default();
                layout_canvas(dom, a_idx, caixa, &ccss, *seg_x, topo, seg.ww.max(1.0), ctx, list);
            } else if dom.image_dims(a_idx).is_some() {
                let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                layout_image(dom, a_idx, caixa, &icss, *seg_x, topo, seg.ww.max(1.0), None, None, ctx, list);
            }
        }
        AtomicKind::Block => {
            // Um inline-block PINTA-SE como bloco (fundo, borda,
            // padding) mas na posição que a linha lhe deu. É o mesmo
            // `layout_block` da corrida de inline-blocks irmãos —
            // não um segundo emissor — só que o x/y vem do fluxo.
            // Inline-block VAZIO senta o fundo na baseline (§10.8.1; caret do Bootstrap a y=9).
            let topo = match envelope {
                Some(env) => super::linha_baseline::topo_do_atomo(dom, seg, cy, env, font_size, family, content_w, ctx),
                None => cy,
            };
            layout_block(
                dom,
                a_idx,
                caixa,
                *seg_x,
                topo,
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
        AtomicKind::Gerada(pe, ParteGerada::Atomo) => {
            // A generated inline-block always makes `envelope` Some.
            let topo = envelope
                .as_ref()
                .map_or(cy, |env| super::linha_baseline::topo_do_atomo(dom, seg, cy, env, font_size, family, content_w, ctx));
            super::pseudo_inline::pintar_atomo(dom, a_idx, pe, caixa, *seg_x, topo, content_w, ctx, list);
        }
        AtomicKind::Marker
        | AtomicKind::Break
        | AtomicKind::ArestaInicio
        | AtomicKind::ArestaFim
        | AtomicKind::Gerada(..)
        | AtomicKind::Float
        | AtomicKind::Estatica => {}
    }
    super::relativo::desloca_desde(list, desde, kind.tem_corpo().then_some(caixa), rx, ry);
    superficies.ver(dom, &seg.owners, *seg_x, *seg_x + seg.ww);
    match kind {
        AtomicKind::ArestaInicio => superficies.marca(a_idx, true),
        AtomicKind::ArestaFim => superficies.marca(a_idx, false),
        AtomicKind::Gerada(pe, ParteGerada::Inicio) => superficies.abre_gerada(dom, a_idx, pe, *seg_x, seg.ww, content_w, ctx),
        AtomicKind::Gerada(pe, ParteGerada::Fim) => superficies.fecha_gerada(dom, a_idx, pe, content_w, ctx),
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
    let ja_registado = matches!(
        kind,
        AtomicKind::Widget | AtomicKind::Block | AtomicKind::ArestaInicio | AtomicKind::ArestaFim | AtomicKind::Gerada(..)
    ) || (kind == AtomicKind::Replaced
        && (dom.image_dims(a_idx).is_some() || super::linha::e_canvas(dom, a_idx)));
    if !ja_registado {
        let propria = match kind {
            // `Marker`: inline SEM conteúdo (`<span></span>`) —
            // no Blink dá 0×0, não a altura do strut. Um vazio
            // com `content` gerado nunca chega aqui — `runs.rs`
            // só emite `Marker` quando não gerou run nenhum.
            AtomicKind::Marker => Rect::new(*seg_x, text_top, 0.0, 0.0),
            AtomicKind::Break => Rect::new(*seg_x, text_top, 0.0, conteudo),
            AtomicKind::Replaced =>
                Rect::new(*seg_x, text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh, seg.ww, seg.wh),
            _ => Rect::new(*seg_x, cy, seg.ww, seg.wh),
        };
        crate::inline_box::union_rect(list, a_idx, Rect::new(propria.x + rx, propria.y + ry, propria.w, propria.h), line_id);
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
    let sem_a_si = |o: &&NodeIdx| !(kind == AtomicKind::Marker && **o == a_idx);
    for &owner in seg.owners.iter().filter(sem_a_si) {
        crate::inline_box::union_rect(
            list,
            owner,
            super::inline_fragmentos::fragmento_do_dono(
                dom,
                owner,
                *seg_x,
                // The text's own anchor: `cy + meia` made the span 5px too tall on a line an atom grew.
                text_owner_anchor,
                seg.ww,
                conteudo,
                ctx,
                na_baseline,
            ), line_id,
        );
    }
    *seg_x += seg.ww;
}
