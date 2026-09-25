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
        if let Some((idx, _, AtomicKind::Marker)) = r.atomic {
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
    let css = dom.computed_style_idx(dono);
    let com_arestas = css.as_deref().is_some_and(crate::inline_box::inline_por_fragmentos);
    // A relative inline — or one inside a relative inline — is shifted HERE, the
    // one place its fragment is made: client rects and painted surface move together.
    let (dx, dy) = super::relativo::offset_do_inline(dom, Some(dono), ctx);
    fragmento_com_estilo(css.as_deref(), com_arestas, x + dx, y + dy, w, conteudo_da_linha, ctx, align_to_baseline)
}

/// [`fragmento_do_dono`] for a style that is not a node's — a generated
/// box's. `com_arestas`: the rect is the border box (vertical padding and
/// border added), which a node asks through `inline_por_fragmentos` and a
/// generated inline with a surface always is.
#[allow(clippy::too_many_arguments)]
fn fragmento_com_estilo(
    css: Option<&ComputedStyle>,
    com_arestas: bool,
    x: f32,
    y: f32,
    w: f32,
    conteudo_da_linha: f32,
    ctx: &LayoutCtx,
    align_to_baseline: bool,
) -> Rect {
    let Some(css) = css else {
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
    if com_arestas {
        let [_, _, cima, baixo] =
            crate::inline_box::arestas_do_inline(css, fonte, ctx.viewport_w, ctx);
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

/// Whose surface it is. A generated inline (`::before`/`::after`) has no
/// node, so it is named by its originating element and the pseudo-element —
/// the same pair `pseudo/mod.rs` keys counters by.
#[derive(Clone, Copy, PartialEq)]
enum Dono {
    No(NodeIdx),
    Gerada(NodeIdx, crate::style::PseudoElement),
}

struct Superficie {
    dono: Dono,
    x0: f32,
    x1: f32,
    // este fragmento contém a aresta inicial/final do inline? (é o que decide
    // se a borda esquerda/direita se pinta aqui)
    inicio: bool,
    fim: bool,
    // A generated inline whose end edge has not been seen yet: every segment
    // until then is its content. A node's surface does not need this — each
    // segment names its node owners — but a segment cannot name a generated
    // box (it has no node), and the pseudo's text is by construction exactly
    // what lies between its two edges in the run order.
    aberta: bool,
}

impl Superficies {
    /// Um segmento de `x0` a `x1` pertence a estes donos — and to every
    /// generated inline still open.
    pub(in crate::layout) fn ver(&mut self, dom: &Dom, owners: &[NodeIdx], x0: f32, x1: f32) {
        for s in self.donos.iter_mut().filter(|s| s.aberta) {
            s.x0 = s.x0.min(x0);
            s.x1 = s.x1.max(x1);
        }
        for &o in owners {
            let flui = dom
                .computed_style_idx(o)
                .is_some_and(|c| crate::inline_box::inline_por_fragmentos(&c));
            if !flui {
                continue;
            }
            match self.donos.iter_mut().find(|s| s.dono == Dono::No(o)) {
                Some(s) => {
                    s.x0 = s.x0.min(x0);
                    s.x1 = s.x1.max(x1);
                }
                None => self.donos.push(Superficie { dono: Dono::No(o), x0, x1, inicio: false, fim: false, aberta: false }),
            }
        }
    }

    /// A aresta inicial (`inicio == true`) ou final do inline `dono` está nesta linha.
    pub(in crate::layout) fn marca(&mut self, dono: NodeIdx, inicio: bool) {
        if let Some(s) = self.donos.iter_mut().find(|s| s.dono == Dono::No(dono)) {
            if inicio {
                s.inicio = true;
            } else {
                s.fim = true;
            }
        }
    }

    /// The start edge of the generated inline `pe` of `id` is the segment at
    /// `x` of width `ww`: its surface opens after its left margin. `base_w`
    /// is the line's width, the base the edge was sized against.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::layout) fn abre_gerada(&mut self, dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, x: f32, ww: f32, base_w: f32, ctx: &LayoutCtx) {
        let (ml, _) = margens_da_gerada(dom, id, pe, base_w, ctx);
        let dono = Dono::Gerada(id, pe);
        self.donos.push(Superficie { dono, x0: x + ml, x1: x + ww, inicio: true, fim: false, aberta: true });
    }

    /// Its end edge was just seen (and [`Self::ver`] already took it in): the
    /// surface closes before its right margin.
    pub(in crate::layout) fn fecha_gerada(&mut self, dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, base_w: f32, ctx: &LayoutCtx) {
        let (_, mr) = margens_da_gerada(dom, id, pe, base_w, ctx);
        if let Some(s) = self.donos.iter_mut().find(|s| s.aberta && s.dono == Dono::Gerada(id, pe)) {
            s.x1 -= mr;
            s.fim = true;
            s.aberta = false;
        }
    }

    /// Insere o fundo e as barras de borda de cada dono em `at` (the piece
    /// position where the line began), atrás do texto. CORTE dito: as cores saem cruas — sem
    /// `opacity`/`filter` do elemento, que o caminho de bloco aplica por
    /// `cor()` — e sem `border-radius`.
    ///
    /// Returns what the NEXT line starts with: a generated inline still open
    /// here goes on there, with no start edge and no extent yet.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::layout) fn pintar(
        self,
        dom: &Dom,
        list: &mut DisplayList,
        at: usize,
        y: f32,
        conteudo_da_linha: f32,
        align_to_baseline: bool,
        ctx: &LayoutCtx,
    ) -> Superficies {
        let mut at = at;
        let mut poe = |list: &mut DisplayList, rect: Rect, color: u32| {
            list.pieces.insert(at, Piece::Item(DisplayItem::SolidRect { rect, color, radius: Corners::ZERO }));
            at += 1;
        };
        let mut seguinte = Superficies::default();
        for s in self.donos {
            if s.aberta {
                let (x0, x1) = (f32::INFINITY, f32::NEG_INFINITY);
                seguinte.donos.push(Superficie { x0, x1, inicio: false, fim: false, ..s });
            }
            if s.x1 < s.x0 {
                continue;
            }
            let (css, r) = match s.dono {
                Dono::No(n) => {
                    let Some(css) = dom.computed_style_idx(n) else { continue };
                    let r = fragmento_do_dono(dom, n, s.x0, y, s.x1 - s.x0, conteudo_da_linha, ctx, align_to_baseline);
                    (css, r)
                }
                Dono::Gerada(n, pe) => {
                    let Some(caixa) = dom.pseudo_box(n, pe) else { continue };
                    let r = fragmento_com_estilo(Some(&caixa.css), true, s.x0, y, s.x1 - s.x0, conteudo_da_linha, ctx, align_to_baseline);
                    // Each line's fragment of the generated inline joins its
                    // box's rect, so `rect_of_box` answers the union the way a
                    // real inline's `union_rect` does. By node, because a
                    // surface is named by `(node, pseudo)`: the copy of the
                    // pseudo a LATER fragment of a split inline repeats
                    // (`runs.rs`) is unioned into the same box.
                    if let Some(gerada) = list.tree.generated_of(n, pe) {
                        super::itens::union_box_rect(list, gerada, r);
                    }
                    (std::rc::Rc::new(caixa.css), r)
                }
            };
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
        seguinte
    }
}

/// The horizontal margins of the generated box `pe` of `id`, resolved as
/// `pseudo_inline.rs` resolved them when it sized the edges.
fn margens_da_gerada(dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, base_w: f32, ctx: &LayoutCtx) -> (f32, f32) {
    dom.pseudo_box(id, pe).map_or((0.0, 0.0), |caixa| {
        let fonte = font_px(&caixa.css, DEFAULT_FONT_SIZE);
        super::pseudo_inline::margens_horizontais(&caixa.css, fonte, base_w, ctx)
    })
}

/// Este grupo é TODO o conteúdo do dono?
///
/// A caixa gerada do DONO envolve todo o conteúdo dele — e só existe como run
/// aqui quando este grupo É todo o conteúdo. Com filhos de bloco pelo meio, o
/// conteúdo do dono parte-se em vários grupos e a caixa gerada teria de virar
/// um bloco anónimo, que é maquinaria de árvore de caixas que este layout não
/// tem; nesse caso não se gera nada, que é o estado anterior, em vez de a pôr
/// num pedaço arbitrário do conteúdo.
///
/// Contado sobre os filhos que geram conteúdo. Os nós de texto só com espaços não contam: um HTML
/// indentado põe um antes e outro depois de cada elemento, e compará-los
/// fazia um `<div>` com o `<span>` numa linha indentada parecer conteúdo
/// partido, e perdia a caixa gerada em quase toda a página real.
pub(in crate::layout) fn group_is_whole_owner(
    dom: &Dom,
    dono: NodeIdx,
    group: &[(NodeIdx, crate::boxes::BoxId)],
) -> bool {
    // A comment counts on NEITHER side. It has no box, so it never enters the
    // group (`PassoDoFluxo::SemCaixa` only opens one); before BT-2a it was in
    // the group with no box and counted on both sides — the same equality.
    let conta = |n: NodeIdx| match &dom.node(n).kind {
        NodeKind::Comment(_) => false,
        NodeKind::Text(t) => !t.trim().is_empty(),
        _ => true,
    };
    let filhos_com_conteudo = dom.node(dono).children.iter().filter(|&&c| conta(c)).count();
    group.iter().filter(|&&(c, _)| conta(c)).count() == filhos_com_conteudo
}
