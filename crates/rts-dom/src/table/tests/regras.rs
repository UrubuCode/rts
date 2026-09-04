use super::*;

/// `table-layout: fixed` decide as colunas pela PRIMEIRA linha e ignora o
/// conteúdo das seguintes — é o algoritmo que existe para não medir nada.
#[test]
fn table_layout_fixed_ignora_o_conteudo_das_linhas_seguintes() {
    let corpo = r#"
        <tr><td style="width:50px">a</td><td>b</td></tr>
        <tr><td>uma frase muito comprida que num layout auto alargaria esta coluna toda</td><td>x</td></tr>"#;
    let auto = format!(r#"<table style="width:300px;border-spacing:0">{corpo}</table>"#);
    let fixo = format!(
        r#"<table style="width:300px;border-spacing:0;table-layout:fixed">{corpo}</table>"#
    );

    let (d1, l1) = geometria(&auto, 800.0);
    let (d2, l2) = geometria(&fixo, 800.0);
    let auto_c1 = rect(&d1, &l1, "td", 0).w;
    let fixo_c1 = rect(&d2, &l2, "td", 0).w;

    // No fixo a primeira coluna fica com os 50px pedidos, custe o que custar.
    // `td` tem `padding: 1px` na folha de UA (lote I): 50 de `width`
    // (content-box) + 2×1px de padding = 52 de caixa.
    assert!(
        (fixo_c1 - 52.0).abs() < 0.5,
        "fixed deu {fixo_c1} à coluna de 50px"
    );
    // No auto a frase da segunda linha alarga-a — é a diferença entre os dois.
    assert!(auto_c1 > fixo_c1 + 1.0, "auto={auto_c1} fixo={fixo_c1}");
}

/// `outside` (o default) fora dela — sem que a caixa do item mude em nenhum dos
/// dois, que é o que o browser faz.
/// As quatro respondem ao `getComputedStyle`, que é o que o harness de paridade
/// compara contra o Chrome. Uma propriedade que o layout usa e o `fmt` não
/// serializa aparece vazia ao lado do valor do Chrome — já nos parou uma vez.
#[test]
fn as_propriedades_novas_respondem_ao_computed_style() {
    let css = crate::style::parse_inline(
        "border-collapse:collapse;border-spacing:2px 4px;table-layout:fixed;list-style-position:inside",
    );
    assert_eq!(css.get_property("border-collapse"), "collapse");
    assert_eq!(css.get_property("border-spacing"), "2px 4px");
    assert_eq!(css.get_property("table-layout"), "fixed");
    assert_eq!(css.get_property("list-style-position"), "inside");
}

/// A infobox da Wikipédia — o caso real que motivou este módulo, reduzido ao
/// que tem de particular: `<tbody>`, `colspan=2` em todas as linhas, e um
/// `display:table` do AUTOR aninhado dentro de uma célula.
///
/// Pinado porque a primeira medição sobre a página real mostrou estas células
/// sem caixa nenhuma, e valia a pena que o caso deixasse de depender de uma
/// corrida de duas horas para se saber se funciona.
#[test]
fn a_infobox_da_wikipedia_da_caixa_a_todas_as_celulas() {
    let html = r##"<div style="width:750px"><table class="infobox"><tbody>
      <tr><th colspan="2">Republica Federativa do Brasil</th></tr>
      <tr><td colspan="2"><div style="display:table; width:100%;">
        <div style="display:table-cell;"><div>bandeira</div></div>
        <div style="display:table-cell;"><div>armas</div></div>
      </div></td></tr>
    </tbody></table></div>"##;
    let (dom, list) = geometria(html, 1280.0);
    // Nenhuma das quatro caixas de célula pode sair 0x0 — era o sintoma.
    for sel in ["th", "td"] {
        let r = rect(&dom, &list, sel, 0);
        assert!(r.w > 1.0 && r.h > 1.0, "<{sel}> saiu {r:?}");
    }
    // As duas linhas EMPILHAM (não ficam ambas no mesmo y, que é o que acontece
    // quando um `<tr>` cai no fluxo inline em vez do algoritmo de tabela).
    let l0 = rect(&dom, &list, "tr", 0);
    let l1 = rect(&dom, &list, "tr", 1);
    assert!(
        l1.y >= l0.y + l0.h - 0.5,
        "as linhas não empilharam: {} e {}",
        l0.y,
        l1.y
    );
    // E o `display:table` do autor, aninhado numa célula, também reparte.
    let celulas = dom.query_all("div[style*=table-cell]");
    assert_eq!(celulas.len(), 2, "as duas células do table aninhado");
}

/// Células sem `table-row` por pai ficam LADO A LADO, numa linha anónima — não
/// empilhadas à largura toda.
///
/// É a forma que a Wikipédia escreve para pôr a bandeira ao lado das armas
/// (`<div style="display:table">` com dois `display:table-cell` diretos), e sem
/// a linha anónima do CSS §17.2.1 as duas caixas caem uma debaixo da outra com a
/// largura inteira — que foi o que este teste apanhou antes de existir.
#[test]
fn celulas_sem_linha_ganham_uma_linha_anonima_e_ficam_lado_a_lado() {
    let html = r##"<div style="display:table; width:400px; border-spacing:0">
        <div style="display:table-cell">bandeira</div>
        <div style="display:table-cell">armas</div>
      </div>"##;
    let (dom, list) = geometria(html, 1280.0);
    let g = list.geometry_now();
    let cel: Vec<_> = dom
        .query_all("div[style*=table-cell]")
        .into_iter()
        .map(|id| *g.rects.get(&dom.resolve(id).unwrap()).expect("caixa"))
        .collect();
    assert_eq!(cel.len(), 2);
    assert!(
        (cel[0].y - cel[1].y).abs() < 0.5,
        "empilharam: y={} e {}",
        cel[0].y,
        cel[1].y
    );
    assert!(
        (cel[1].x - (cel[0].x + cel[0].w)).abs() < 0.5,
        "não ficaram encostadas"
    );
    assert!(
        (cel[0].w + cel[1].w - 400.0).abs() < 0.5,
        "juntas deviam dar a tabela"
    );
}

/// Uma célula solta ANTES de um `<tr>` não se junta a ele: são duas linhas, e
/// só células CONSECUTIVAS partilham a linha anónima.
#[test]
fn a_linha_anonima_fecha_quando_aparece_uma_linha_de_verdade() {
    let html = r##"<div style="display:table; width:400px; border-spacing:0">
        <div style="display:table-cell">solta</div>
        <div style="display:table-row"><div style="display:table-cell">na linha</div></div>
      </div>"##;
    let (dom, list) = geometria(html, 1280.0);
    let g = list.geometry_now();
    let cel: Vec<_> = dom
        .query_all("div[style*=table-cell]")
        .into_iter()
        .map(|id| *g.rects.get(&dom.resolve(id).unwrap()).expect("caixa"))
        .collect();
    assert_eq!(cel.len(), 2);
    assert!(
        cel[1].y >= cel[0].y + cel[0].h - 0.5,
        "deviam ser duas linhas"
    );
}

/// A MINIATURA da Wikipédia: `figure { display: table }` com uma imagem e um
/// `figcaption { display: table-caption }`. A figura tem de ganhar a largura da
/// imagem, e a legenda tem de ganhar caixa.
///
/// Pinado porque a medição sobre a página real mostrou as três figuras com 0px
/// de largura e as 24 legendas sem caixa nenhuma — duas causas numa só forma:
/// sem célula anónima a tabela não tem coluna, a soma das colunas é zero, e o
/// shrink-to-fit dá-lhe zero.
#[test]
fn figure_como_tabela_ganha_a_largura_do_conteudo_e_a_legenda_ganha_caixa() {
    let html = r##"<div style="width:700px">
      <figure style="display:table">
        <div style="width:250px;height:180px">imagem</div>
        <figcaption style="display:table-caption">Bandeira do Brasil</figcaption>
      </figure></div>"##;
    let (dom, list) = geometria(html, 1280.0);
    let fig = rect(&dom, &list, "figure", 0);
    let cap = rect(&dom, &list, "figcaption", 0);
    assert!(fig.w > 200.0, "a figura encolheu para {}", fig.w);
    assert!(fig.w < 300.0, "a figura ocupou o pai todo: {}", fig.w);
    assert!(
        cap.w > 1.0 && cap.h > 1.0,
        "a legenda saiu sem caixa: {cap:?}"
    );
}

/// Uma legenda fica FORA da grade: não é uma coluna, e por isso não reparte
/// largura com as células nem entra na contagem de colunas.
#[test]
fn a_legenda_nao_vira_uma_coluna_da_tabela() {
    let html = r##"<table style="width:300px;border-spacing:0">
        <caption>uma legenda bastante comprida</caption>
        <tr><td style="width:100px">a</td><td style="width:200px">b</td></tr>
      </table>"##;
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "td", 0);
    let b = rect(&dom, &list, "td", 1);
    let cap = rect(&dom, &list, "caption", 0);
    // `td`/`th` têm `padding: 1px` na folha de UA (lote I): 100/200 de
    // `width` (content-box) + 2×1px de padding = 102/202 de caixa.
    assert!(
        (a.w - 102.0).abs() < 0.5,
        "a legenda mexeu na coluna 1: {}",
        a.w
    );
    assert!(
        (b.w - 202.0).abs() < 0.5,
        "a legenda mexeu na coluna 2: {}",
        b.w
    );
    assert!(
        cap.y + cap.h <= a.y + 0.5,
        "a legenda devia ficar ACIMA da grade"
    );
}

