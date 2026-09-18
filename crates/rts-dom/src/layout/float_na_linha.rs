//! Um FLOAT que aparece A MEIO de um fluxo inline.
//!
//! CSS 2.1 §9.5.1, regras 6 e 8: o topo de um float não fica acima do topo da
//! caixa de linha que contém o conteúdo que o precede, e fica o mais acima
//! possível. Na prática que os browsers seguem: se o float cabe no que a linha
//! em que aparece ainda tem livre, o topo dele é o topo DESSA linha, e a linha
//! encurta à volta dele; se não cabe, desce para a linha seguinte.
//!
//! O modelo anterior não tinha esta pergunta. Um float filho directo FECHAVA o
//! fluxo inline (`vertical.rs`) e punha-se abaixo da última linha, e um float
//! dentro de um `<span>` partia o span em três caixas (`boxes/build.rs`) para
//! chegar ao mesmo sítio — `<div>antes<div style=float:left></div>depois</div>`
//! saía em duas linhas onde o Blink dá uma, com o texto encostado ao float.
//!
//! ## Como, e o custo declarado
//!
//! A âncora (`AtomicKind::Float`) tem largura zero e entra na quebra como um
//! `Marker`. Por cada âncora, pela ordem do documento: quebra-se o fluxo com as
//! exclusões que já existem, acha-se a linha em que ela caiu e quanto dessa
//! linha já está ocupado antes dela, decide-se o topo, coloca-se o float
//! (`float_colocar.rs`, o mesmo caminho do filho directo) e passa-se à próxima
//! — que já vê este float nas exclusões. O chamador quebra uma última vez com
//! todas. **São `k + 1` quebras para `k` floats no fluxo**, e zero a mais
//! quando não há nenhum, que é quase sempre.
//!
//! O topo de cada linha é previsto pelo ÍNDICE dela (`y + i × lh`) — a mesma
//! aproximação declarada da largura de quebra em `linha.rs`, e pela mesma
//! razão: uma linha com um átomo mais alto desloca as seguintes, e o float
//! fica uma fração de linha acima do sítio real.

use super::*;
use crate::boxes::BoxId;

/// Coloca, no BFC, cada float ancorado em `runs`. Não pinta a linha nem a
/// quebra de vez: devolve depois de o último float estar posto, e o chamador
/// quebra com as exclusões que ficaram.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn coloca_ancorados(
    dom: &Dom,
    arvore: &crate::boxes::BoxTree,
    runs: &[InlineRun],
    // Quebra os runs com estas exclusões — o `wrap_runs` do chamador, com os
    // parâmetros do contentor já fixados.
    quebrar: &dyn Fn(&[Exclusao]) -> Vec<Vec<Segment>>,
    (x, y, content_w, lh): (f32, f32, f32, f32),
    // `white-space: nowrap`/`pre` do contentor: a linha inteira é uma palavra.
    nowrap: bool,
    parent_css: &ComputedStyle,
    font_size: f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let ancoras: Vec<(NodeIdx, BoxId)> = runs
        .iter()
        .filter_map(|r| match r.atomic {
            Some((no, Some(caixa), AtomicKind::Float)) => Some((no, caixa)),
            _ => None,
        })
        .collect();
    for (no, caixa) in ancoras {
        let exclusoes = bfc.snapshot();
        let linhas = quebrar(&exclusoes);
        let Some((i, ocupado)) = onde_caiu(&linhas, no, nowrap) else { continue };
        // Um float de altura/largura em percentagem resolve contra um bloco de
        // altura indefinida aqui: o fluxo inline não conhece a altura do
        // contentor, e é o mesmo `None` que um átomo da linha já recebe.
        let medida = super::float_colocar::mede_float(dom, arvore, no, caixa, content_w, None, parent_css, font_size, ctx);
        let topo_da_linha = y + i as f32 * lh;
        let (_, livre) = banda_livre(&exclusoes, topo_da_linha, lh, x, content_w);
        // Cabe no que a linha ainda tem? Uma âncora no INÍCIO da linha cabe
        // sempre nesta pergunta — se o float não couber na banda nem sozinho,
        // quem desce é a procura de `coloca_float`, que conhece os fundos dos
        // outros floats e não apenas a linha seguinte.
        let cabe = ocupado <= 0.0 || ocupado + medida.0 <= livre + 0.01;
        let topo = if cabe { topo_da_linha } else { topo_da_linha + lh };
        let side = dom
            .computed_style_idx(no)
            .and_then(|c| c.float_side)
            .unwrap_or(crate::style::FloatSide::Left);
        super::float_colocar::coloca_float(dom, no, caixa, side, medida, topo, x, content_w, None, bfc, ctx, list);
    }
}

/// A linha em que a âncora de `no` caiu, e a largura que o float tem de deixar
/// livre nela: o que está ANTES da âncora e o que vem colado DEPOIS dela até à
/// próxima oportunidade de quebra.
///
/// O "depois" é o que o Blink e o Gecko fazem, e o que o WPT
/// `CSS2/floats/float-nowrap-*` fixa: o float é tratado depois da palavra em
/// que aparece, porque a linha não pode acabar a meio dela. Numa linha
/// `nowrap` não há quebra nenhuma, portanto conta o resto da linha — e uma
/// linha que transborda empurra o float para baixo dela. Contar só o que
/// estava antes deixava o float na linha e perdeu esses dois reftests.
///
/// ⚠️ APROXIMAÇÃO DECLARADA na quebra normal: conta os segmentos seguintes
/// SEM vão à frente e sem espaço dentro. Um segmento colado que tem um espaço
/// no meio fica de fora inteiro, em vez de contar o prefixo até ao espaço —
/// isso pedia o medidor de texto, que esta pergunta não tem.
fn onde_caiu(linhas: &[Vec<Segment>], no: NodeIdx, nowrap: bool) -> Option<(usize, f32)> {
    let largura = |seg: &Segment| seg.lead_w + if seg.atomic.is_some() { seg.ww } else { seg.text_width };
    for (i, linha) in linhas.iter().enumerate() {
        let Some(k) = linha
            .iter()
            .position(|seg| matches!(seg.atomic, Some((a, _, AtomicKind::Float)) if a == no))
        else {
            continue;
        };
        let antes: f32 = linha[..k].iter().map(largura).sum();
        let colado: f32 = linha[k + 1..]
            .iter()
            .take_while(|seg| nowrap || (seg.lead_w <= 0.0 && !seg.text.contains(char::is_whitespace)))
            .map(largura)
            .sum();
        return Some((i, antes + colado));
    }
    None
}

/// A âncora de `id` no fluxo inline, quando `id` flutua; `None` quando não.
///
/// Largura zero, e nada mais: não se desce no float (o conteúdo é dele,
/// disposto por `layout_block` quando é colocado), e os inlines à volta não o
/// contam como conteúdo seu — `owners` vazio. Sem caixa não há com que o
/// dispor, e o varredor segue pelo caminho de sempre.
pub(in crate::layout) fn ancora(dom: &Dom, id: NodeIdx, caixa: Option<BoxId>, cor: u32) -> Option<InlineRun> {
    // `float` declarado e não anulado por `position: absolute/fixed` (CSS 2.1
    // §9.7 — um absoluto não flutua).
    let flutua = dom.computed_style_idx(id).is_some_and(|c| {
        c.float_side.is_some_and(|f| f != crate::style::FloatSide::None)
            && !c.position.is_some_and(|p| p.out_of_flow())
    });
    (flutua && caixa.is_some()).then(|| InlineRun {
        text: String::new(),
        color: cor,
        bold: false,
        italic: false,
        deco: 0,
        owners: Vec::new(),
        atomic: Some((id, caixa, AtomicKind::Float)),
        ww: 0.0,
        wh: 0.0,
    })
}
