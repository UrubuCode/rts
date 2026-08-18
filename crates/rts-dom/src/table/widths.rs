//! A REPARTIÇÃO DA LARGURA pelas colunas — a parte do layout de tabela que
//! ninguém acerta por intuição, e por isso a que vive num ficheiro só seu.
//!
//! O algoritmo é o do CSS 2.1 §17.5.2 relido no LayoutNG: cada coluna tem uma
//! largura MÍNIMA (o que o conteúdo não consegue encolher mais — a palavra mais
//! larga) e uma MÁXIMA (o que o conteúdo quer sem quebrar), e a largura
//! disponível reparte-se entre as duas proporcionalmente à FOLGA de cada coluna.
//!
//! A tentação é repartir proporcionalmente ao máximo, e é o erro clássico: uma
//! coluna com uma frase longa e outra com um número receberiam larguras na razão
//! dos textos, e o número ficaria com 3px enquanto a frase sobra. Repartir a
//! folga (`max - min`) é o que dá a cada coluna o seu mínimo primeiro e só
//! depois divide o que sobra.

use crate::layout::LayoutCtx;
use crate::style::ResolveCtx;
use crate::{Dom, NodeIdx, NodeKind};

/// O par (mínimo, máximo) de uma coluna ou de uma célula.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct MinMax {
    pub min: f32,
    pub max: f32,
}

impl MinMax {
    fn absorve(&mut self, o: MinMax) {
        self.min = self.min.max(o.min);
        self.max = self.max.max(o.max);
    }
}

/// As larguras intrínsecas de UMA célula, já com o frame (padding+borda) dentro:
/// a coluna é dimensionada em caixa de BORDA, porque é isso que ocupa espaço na
/// linha.
pub(crate) fn cell_min_max(dom: &Dom, id: NodeIdx, parent_font: f32, ctx: &LayoutCtx) -> MinMax {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = crate::layout::font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: ctx.viewport_w,
        node_font_size: font,
        root_font_size: crate::layout::DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let border_box = css.border_box.unwrap_or(false);
    let frame = css.padding.resolve_h(&resolve) + 2.0 * css.border_width.unwrap_or(0.0);

    // Um `width` explícito na célula (ou o atributo HTML `width`, que páginas
    // reais ainda usam) FIXA a coluna: entra como mínimo E como máximo, senão a
    // repartição por folga ignorá-lo-ia — a folga de uma coluna fixa é zero.
    let fixo = largura_absoluta(css.width, &resolve)
        .map(|w| if border_box { w } else { w + frame })
        .or_else(|| largura_de_atributo(dom, id));
    if let Some(w) = fixo {
        // Ainda assim não pode ficar ABAIXO do mínimo do conteúdo: uma largura
        // que não cabe é ignorada pelo browser, não respeitada com o texto a
        // transbordar.
        let piso = min_content(dom, id, font, ctx) + frame;
        return MinMax { min: w.max(piso), max: w.max(piso) };
    }
    MinMax {
        min: min_content(dom, id, font, ctx) + frame,
        max: crate::layout::intrinsic_outer_width(dom, id, parent_font, ctx),
    }
}

/// Um `width` que conta para a largura INTRÍNSECA — ou seja, um comprimento
/// ABSOLUTO. Uma percentagem responde `None`, e é a correção que faz a diferença
/// entre uma tabela certa e uma tabela de 1280px de largura.
///
/// O motivo é que a percentagem é contra a largura da COLUNA, que é o que este
/// cálculo está a decidir: perguntá-la agora é circular. O `ResolveCtx` da
/// medição intrínseca põe `parent_content_w = viewport`, portanto resolver
/// `width:100%` aqui devolvia 1280 e essa célula passava a exigir um mínimo de
/// 1280 — medido na Wikipédia, onde punha a caixa da infobox a transbordar o
/// artigo inteiro. O CSS diz o mesmo: uma percentagem indefinida contribui como
/// `auto` para o tamanho intrínseco.
fn largura_absoluta(
    d: Option<crate::style::Dimension>,
    resolve: &ResolveCtx,
) -> Option<f32> {
    match d? {
        crate::style::Dimension::Percent(_) => None,
        // `calc()` com componente percentual tem o mesmo problema; sem ela é um
        // comprimento absoluto como qualquer outro.
        crate::style::Dimension::Calc(c) if c.pct != 0.0 => None,
        outra => outra.resolve(resolve),
    }
}

/// O atributo HTML `width` de uma célula/coluna (`<td width="120">`, ainda vivo
/// em páginas reais e em toda a Wikipédia). Só a forma em PIXELS: a forma em
/// percentagem precisa da largura da tabela, que ainda não está decidida quando
/// isto corre, e tratá-la como pixels daria uma coluna de 50px onde se pediu
/// metade da tabela.
fn largura_de_atributo(dom: &Dom, id: NodeIdx) -> Option<f32> {
    let v = dom.node(id).attr("width")?.trim();
    if v.ends_with('%') {
        return None;
    }
    v.trim_end_matches("px").parse::<f32>().ok().filter(|w| *w > 0.0)
}

/// A largura MÍNIMA do conteúdo de um nó: o ponto em que quebrar mais linhas já
/// não estreita nada — a PALAVRA mais larga.
///
/// Escrito aqui em vez de reusar o `intrinsic_content_width` do layout porque
/// aquele responde a outra pergunta (o máximo, sem quebra nenhuma) e não há como
/// derivar um do outro. A alternativa considerada foi medir o bloco com largura
/// disponível zero e ler o resultado; não serve, porque um bloco sem `width`
/// ocupa o que lhe derem e responderia zero.
fn min_content(dom: &Dom, id: NodeIdx, font: f32, ctx: &LayoutCtx) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Text(t) => t
            .split_whitespace()
            .map(|p| ctx.measurer.text_width(p, font, false, false))
            .fold(0.0f32, f32::max),
        NodeKind::Element { tag } => {
            if crate::layout::is_non_rendered_tag(tag) {
                return 0.0;
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            if css.effective_display() == Some(crate::style::DisplayKind::None) {
                return 0.0;
            }
            let f = crate::layout::font_px(&css, font);
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: crate::layout::DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let frame = css.padding.resolve_h(&resolve)
                + css.margin.resolve_h(&resolve)
                + 2.0 * css.border_width.unwrap_or(0.0);
            // Uma caixa REPLACED (imagem) não encolhe abaixo da sua largura, e um
            // `width` explícito também não: nos dois casos o mínimo é a largura.
            if let Some(w) = largura_absoluta(css.width, &resolve) {
                return w + if css.border_box.unwrap_or(false) { 0.0 } else { frame };
            }
            let mut m = 0.0f32;
            for &c in &dom.node(id).children {
                if crate::layout::is_out_of_flow(dom, c) {
                    continue;
                }
                m = m.max(min_content(dom, c, f, ctx));
            }
            m + frame
        }
        _ => 0.0,
    }
}

/// Junta as células numa lista de colunas: cada coluna fica com o maior mínimo e
/// o maior máximo das células que a ocupam SOZINHAS (colspan 1).
///
/// As células que atravessam colunas entram depois, e só para LEVANTAR o que já
/// existe — nunca para o baixar. É a regra que evita o efeito mais visível de um
/// algoritmo ingénuo: um cabeçalho com `colspan=3` a ditar a largura de três
/// colunas que o corpo da tabela já tinha dimensionado pelos seus dados.
pub(crate) fn colunas_min_max(
    cells: &[(usize, usize, MinMax)], // (coluna inicial, colspan, min/max)
    cols: usize,
    spacing: f32,
) -> Vec<MinMax> {
    let mut out = vec![MinMax::default(); cols];
    for &(col, _span, mm) in cells.iter().filter(|c| c.1 <= 1) {
        if col < cols {
            out[col].absorve(mm);
        }
    }
    // Segunda passada: as que atravessam. O espaço que a célula precisa é o dela
    // MENOS o `border-spacing` que já existe entre as colunas atravessadas — esse
    // espaço é dela também.
    for &(col, span, mm) in cells.iter().filter(|c| c.1 > 1) {
        let fim = (col + span).min(cols);
        if col >= cols {
            continue;
        }
        let n = fim - col;
        let vaos = (n.saturating_sub(1)) as f32 * spacing;
        let atual_min: f32 = out[col..fim].iter().map(|c| c.min).sum::<f32>() + vaos;
        let atual_max: f32 = out[col..fim].iter().map(|c| c.max).sum::<f32>() + vaos;
        distribui_excedente(&mut out[col..fim], mm.min - atual_min, |c| &mut c.min);
        distribui_excedente(&mut out[col..fim], mm.max - atual_max, |c| &mut c.max);
        // O máximo nunca fica abaixo do mínimo depois de levantado.
        for c in &mut out[col..fim] {
            c.max = c.max.max(c.min);
        }
    }
    out
}

/// Espalha `extra` (se positivo) igualmente pelas colunas. Igualmente e não em
/// proporção porque, no ponto em que isto corre, a proporção seria contra
/// larguras que podem ser todas zero (uma linha inteira de células vazias sob um
/// cabeçalho com colspan), e uma proporção contra zero não distribui nada.
fn distribui_excedente(cols: &mut [MinMax], extra: f32, campo: impl Fn(&mut MinMax) -> &mut f32) {
    if extra <= 0.0 || cols.is_empty() {
        return;
    }
    let quota = extra / cols.len() as f32;
    for c in cols.iter_mut() {
        *campo(c) += quota;
    }
}

/// A largura USADA de cada coluna, dada a largura de conteúdo disponível para as
/// colunas (já sem os `border-spacing`).
///
/// Três regimes, e é aqui que vive a regra que abre este ficheiro:
/// - não cabe nem o mínimo → cada coluna fica com o seu mínimo e a tabela
///   transborda (é o que o browser faz: uma tabela não encolhe o conteúdo
///   abaixo do indivisível);
/// - cabe o máximo → o que sobra vai proporcionalmente ao máximo, porque uma
///   tabela sem `width` que já está satisfeita distribui a sobra pelas colunas
///   que mais conteúdo têm;
/// - entre os dois → cada coluna recebe o seu mínimo mais a sua fatia da FOLGA.
pub(crate) fn resolve_colunas(cols: &[MinMax], disponivel: f32) -> Vec<f32> {
    let total_min: f32 = cols.iter().map(|c| c.min).sum();
    let total_max: f32 = cols.iter().map(|c| c.max).sum();
    if cols.is_empty() {
        return Vec::new();
    }
    if disponivel <= total_min || total_max <= total_min {
        // Sem folga nenhuma (todas as colunas fixas) a repartição por folga
        // dividiria por zero; o mínimo já é a resposta.
        if total_max <= total_min && disponivel > total_min && total_min > 0.0 {
            let k = disponivel / total_min;
            return cols.iter().map(|c| c.min * k).collect();
        }
        return cols.iter().map(|c| c.min).collect();
    }
    if disponivel >= total_max {
        if total_max <= 0.0 {
            let q = disponivel / cols.len() as f32;
            return vec![q; cols.len()];
        }
        let sobra = disponivel - total_max;
        return cols.iter().map(|c| c.max + sobra * (c.max / total_max)).collect();
    }
    let folga_total = total_max - total_min;
    let a_dividir = disponivel - total_min;
    cols.iter()
        .map(|c| c.min + a_dividir * ((c.max - c.min) / folga_total))
        .collect()
}

/// `table-layout: fixed` — as larguras vêm da PRIMEIRA linha (e dos `<col>`), o
/// conteúdo não é consultado, e o que sobra divide-se igualmente pelas colunas
/// sem largura declarada. É o algoritmo que existe para ser rápido: nenhuma
/// célula é medida.
///
/// `declaradas[i] == None` = coluna sem largura declarada.
pub(crate) fn resolve_fixo(declaradas: &[Option<f32>], disponivel: f32) -> Vec<f32> {
    let soma: f32 = declaradas.iter().flatten().sum();
    let livres = declaradas.iter().filter(|d| d.is_none()).count();
    if livres == 0 {
        // Todas declaradas: escalam para encher a largura da tabela (a spec manda
        // a tabela ficar com a largura pedida, não com a soma das colunas).
        if soma <= 0.0 {
            return vec![0.0; declaradas.len()];
        }
        let k = disponivel / soma;
        return declaradas.iter().map(|d| d.unwrap_or(0.0) * k).collect();
    }
    let quota = ((disponivel - soma) / livres as f32).max(0.0);
    declaradas.iter().map(|d| d.unwrap_or(quota)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(min: f32, max: f32) -> MinMax {
        MinMax { min, max }
    }

    #[test]
    fn a_folga_e_repartida_e_nao_o_maximo() {
        // Uma coluna estreita e satisfeita (10..12) ao lado de uma esfomeada
        // (10..110): 100px de folga total, 40 para dividir.
        let w = resolve_colunas(&[mm(10.0, 12.0), mm(10.0, 110.0)], 60.0);
        // A estreita leva 2% dos 40 (a sua folga é 2 de 102), não 10%.
        assert!((w[0] - 10.78).abs() < 0.05, "coluna estreita = {}", w[0]);
        assert!((w[1] - 49.22).abs() < 0.05, "coluna larga = {}", w[1]);
        assert!((w[0] + w[1] - 60.0).abs() < 0.01);
    }

    #[test]
    fn abaixo_do_minimo_a_tabela_transborda_em_vez_de_esmagar() {
        let w = resolve_colunas(&[mm(50.0, 80.0), mm(50.0, 80.0)], 40.0);
        assert_eq!(w, vec![50.0, 50.0]);
    }

    #[test]
    fn colspan_so_levanta_colunas_nunca_as_baixa() {
        // Duas colunas já com 100 cada, e um cabeçalho colspan=2 que só quer 50.
        let cols = colunas_min_max(
            &[(0, 1, mm(100.0, 100.0)), (1, 1, mm(100.0, 100.0)), (0, 2, mm(50.0, 50.0))],
            2,
            0.0,
        );
        assert_eq!(cols[0].min, 100.0);
        assert_eq!(cols[1].min, 100.0);
    }

    #[test]
    fn colspan_maior_que_as_colunas_reparte_o_que_falta_por_igual() {
        let cols = colunas_min_max(&[(0, 2, mm(100.0, 100.0))], 2, 0.0);
        assert_eq!(cols[0].min, 50.0);
        assert_eq!(cols[1].min, 50.0);
    }

    #[test]
    fn fixo_divide_o_resto_pelas_colunas_sem_largura() {
        let w = resolve_fixo(&[Some(100.0), None, None], 300.0);
        assert_eq!(w, vec![100.0, 100.0, 100.0]);
    }

    #[test]
    fn fixo_com_todas_declaradas_escala_para_a_largura_da_tabela() {
        let w = resolve_fixo(&[Some(50.0), Some(50.0)], 200.0);
        assert_eq!(w, vec![100.0, 100.0]);
    }
}
