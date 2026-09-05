//! O inline como FRAGMENTOS de linha (CSS 2.1 §9.2.2): a caixa de um
//! `<span>` que quebra é a união dos pedaços que ficam em cada linha, e o
//! fundo/borda/padding pintam-se por pedaço — a borda esquerda só no primeiro,
//! a direita só no último, o topo e o fundo em todos. Extraído de `linha.rs`
//! (no teto de 500 linhas): o fragmento de cada dono, e as superfícies de uma
//! linha.
//!
//! Antes deste módulo um inline com superfície era promovido a caixa atómica
//! (`AtomicKind::Block`) e NEM QUEBRAVA — `claude-inline-fragmentos`: o
//! contentor de 130px ficava com 22px de altura onde o Blink dá 60 (três
//! linhas), e a borda engrossava a linha. Os três remendos revertidos na vaga 3
//! tentavam corrigir isso sem separar "quem pinta" de "quem flui".

use super::*;

/// Um grupo inline sem nenhum átomo com corpo — só `Marker`s (e whitespace
/// entre eles) — não cria linha, mas cada `Marker` ainda é um elemento do
/// documento e o Blink dá-lhe um retângulo 0×0 na posição onde a linha teria
/// começado (`claude-sel-has.html`). Todos caem no MESMO ponto: um marker não
/// tem largura, nenhum avança o cursor.
pub(in crate::layout) fn registar_markers_sem_linha(
    list: &mut DisplayList,
    x: f32,
    y: f32,
    runs: &[InlineRun],
) {
    for r in runs {
        if let Some((idx, AtomicKind::Marker)) = r.atomic {
            crate::inline_box::union_rect(list, idx, Rect::new(x, y, 0.0, 0.0));
        }
    }
}

/// O fragmento que ESTE dono recebe desta fatia de linha.
///
/// A altura é a content area da fonte DELE, não a do bloco que conduz o fluxo:
/// um `<span>` de 14px dentro de um título de 17,5px mede 15,75 e não 19,7. Sem
/// isto, 1 172 dos 1 257 `<span>` da Wikipédia com altura errada tinham
/// exatamente `1.125 x a fonte de um ANCESTRAL`. Fica CENTRADO na content area
/// da linha (a mesma aproximação da meia-entrelinha). Um inline por fragmentos
/// leva ainda o padding e a borda verticais — a border box, que é o que o
/// `getBoundingClientRect` do Blink responde (y=-2 com `border:2px` numa linha
/// que começa em 0).
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn fragmento_do_dono(
    dom: &Dom,
    dono: NodeIdx,
    x: f32,
    y: f32,
    w: f32,
    conteudo_da_linha: f32,
    ctx: &LayoutCtx,
    align_to_baseline: bool,
) -> Rect {
    let Some(css) = dom.computed_style_idx(dono) else {
        return Rect::new(x, y, w, conteudo_da_linha);
    };
    let Some(crate::style::Dimension::Px(fonte)) = css.font_size else {
        return Rect::new(x, y, w, conteudo_da_linha);
    };
    let conteudo = crate::inline_box::altura_do_conteudo(fonte, css.font_family.as_deref(), ctx.measurer);
    let top = if align_to_baseline {
        y - ctx.measurer.font_ascent_family(fonte, css.font_family.as_deref())
    } else {
        y + (conteudo_da_linha - conteudo) / 2.0
    };
    if crate::inline_box::inline_por_fragmentos(&css) {
        let [_, _, cima, baixo] =
            crate::inline_box::arestas_do_inline(&css, fonte, ctx.viewport_w, ctx);
        return Rect::new(x, top - cima, w, conteudo + cima + baixo);
    }
    Rect::new(x, top, w, conteudo)
}

/// As superfícies (fundo e borda) dos inlines por fragmentos de UMA linha,
/// acumuladas ao longo dos segmentos e pintadas atrás deles no fim.
#[derive(Default)]
pub(in crate::layout) struct Superficies {
    // Por ordem de primeira aparição — um ancestral aparece antes do
    // descendente, que é a ordem de pintura (o fundo do `<a>` por cima do do
    // `<span>` que o contém).
    donos: Vec<Superficie>,
}

struct Superficie {
    dono: NodeIdx,
    x0: f32,
    x1: f32,
    // este fragmento contém a aresta inicial/final do inline? (é o que decide
    // se a borda esquerda/direita se pinta aqui)
    inicio: bool,
    fim: bool,
}

impl Superficies {
    /// Um segmento de `x0` a `x1` pertence a estes donos.
    pub(in crate::layout) fn ver(&mut self, dom: &Dom, owners: &[NodeIdx], x0: f32, x1: f32) {
        for &o in owners {
            let flui = dom
                .computed_style_idx(o)
                .is_some_and(|c| crate::inline_box::inline_por_fragmentos(&c));
            if !flui {
                continue;
            }
            match self.donos.iter_mut().find(|s| s.dono == o) {
                Some(s) => {
                    s.x0 = s.x0.min(x0);
                    s.x1 = s.x1.max(x1);
                }
                None => self.donos.push(Superficie { dono: o, x0, x1, inicio: false, fim: false }),
            }
        }
    }

    /// A aresta inicial (`inicio == true`) ou final do inline `dono` está nesta linha.
    pub(in crate::layout) fn marca(&mut self, dono: NodeIdx, inicio: bool) {
        if let Some(s) = self.donos.iter_mut().find(|s| s.dono == dono) {
            if inicio {
                s.inicio = true;
            } else {
                s.fim = true;
            }
        }
    }

    /// Insere o fundo e as barras de borda de cada dono em `at` (o índice onde
    /// a linha começou), atrás do texto. CORTE dito: as cores saem cruas — sem
    /// `opacity`/`filter` do elemento, que o caminho de bloco aplica por
    /// `cor()` — e sem `border-radius`.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::layout) fn pintar(
        self,
        dom: &Dom,
        list: &mut DisplayList,
        at: usize,
        filhos_antes: usize,
        y: f32,
        conteudo_da_linha: f32,
        align_to_baseline: bool,
        ctx: &LayoutCtx,
    ) {
        let mut at = at;
        let mut poe = |list: &mut DisplayList, rect: Rect, color: u32| {
            insert_item(
                list,
                at,
                filhos_antes,
                DisplayItem::SolidRect { rect, color, radius: Corners::ZERO },
            );
            at += 1;
        };
        for s in self.donos {
            let Some(css) = dom.computed_style_idx(s.dono) else { continue };
            let r = fragmento_do_dono(
                dom,
                s.dono,
                s.x0,
                y,
                s.x1 - s.x0,
                conteudo_da_linha,
                ctx,
                align_to_baseline,
            );
            if let Some(bg) = css.bg.filter(|_| !deve_suprimir_fundo(&css)) {
                poe(list, r, bg);
            }
            let sides = crate::style::borders::resolved_sides(&css);
            let [t, rt, b, l] = crate::style::borders::used_widths(&css);
            if sides[0].paints() {
                poe(list, Rect::new(r.x, r.y, r.w, t), sides[0].color);
            }
            if sides[2].paints() {
                poe(list, Rect::new(r.x, r.y + r.h - b, r.w, b), sides[2].color);
            }
            if s.inicio && sides[3].paints() {
                poe(list, Rect::new(r.x, r.y, l, r.h), sides[3].color);
            }
            if s.fim && sides[1].paints() {
                poe(list, Rect::new(r.x + r.w - rt, r.y, rt, r.h), sides[1].color);
            }
        }
    }
}
