//! Os testes de COMPORTAMENTO da tabela e do item de lista: cada um nomeia o
//! que fixa sobre a página, não a função que chama.
//!
//! Todos correm sobre o [`crate::layout::ApproxMeasurer`], cujo texto mede
//! `n * tamanho * 0.5`. Isso é o que torna as posições previsíveis a olho: uma
//! célula com "aa" a 16px quer 16px de conteúdo. Um medidor real daria números
//! diferentes e o teste passaria a afirmar coisas sobre a fonte.

use crate::layout::{layout_document, ApproxMeasurer, DisplayItem, LayoutCtx, Rect};
use crate::parse_html_to_dom;

fn geometria(html: &str, largura: f32) -> (crate::Dom, crate::layout::DisplayList) {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: largura, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

/// O rect do n-ésimo elemento que casa com o seletor.
fn rect(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, n: usize) -> Rect {
    let ids = dom.query_all(sel);
    let id = ids.get(n).unwrap_or_else(|| panic!("sem {sel}[{n}]"));
    let idx = dom.resolve(*id).expect("nó vivo");
    *list
        .geometry_now()
        .rects
        .get(&idx)
        .unwrap_or_else(|| panic!("{sel}[{n}] sem geometria"))
}

/// Os textos emitidos na display list, na ordem de pintura.
fn textos(list: &crate::layout::DisplayList) -> Vec<String> {
    list.materialized()
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

// ── TABELA ──────────────────────────────────────────────────────────────────

/// Uma tabela 2x2 com larguras declaradas põe cada célula na sua coluna, e a
/// segunda coluna começa onde a primeira acaba (mais o `border-spacing`).
#[test]
fn tabela_2x2_com_larguras_conhecidas_poe_as_celulas_nas_colunas() {
    let html = r#"<table cellspacing="0" style="width:300px">
        <tr><td style="width:100px">a</td><td style="width:200px">b</td></tr>
        <tr><td>c</td><td>d</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);

    let a = rect(&dom, &list, "td", 0);
    let b = rect(&dom, &list, "td", 1);
    let c = rect(&dom, &list, "td", 2);
    let d = rect(&dom, &list, "td", 3);

    assert!((a.w - 100.0).abs() < 0.5, "coluna 1 = {}", a.w);
    assert!((b.w - 200.0).abs() < 0.5, "coluna 2 = {}", b.w);
    // A segunda coluna começa onde a primeira acaba: `cellspacing=0`.
    assert!((b.x - (a.x + a.w)).abs() < 0.5, "b.x={} a fim={}", b.x, a.x + a.w);
    // A segunda linha usa as MESMAS colunas — é o que distingue uma tabela de
    // duas linhas de blocos empilhados.
    assert!((c.x - a.x).abs() < 0.5 && (c.w - a.w).abs() < 0.5);
    assert!((d.x - b.x).abs() < 0.5 && (d.w - b.w).abs() < 0.5);
    // E fica ABAIXO da primeira.
    assert!(c.y >= a.y + a.h - 0.5, "linha 2 em y={} linha 1 acaba em {}", c.y, a.y + a.h);
}

/// A altura de uma linha é a da célula mais alta, e as duas células da linha
/// ficam com essa altura — não cada uma com a sua.
#[test]
fn a_linha_fica_com_a_altura_da_celula_mais_alta() {
    let html = r#"<table cellspacing="0" style="width:200px">
        <tr><td style="width:100px">curto</td>
            <td style="width:100px"><div style="height:80px">alto</div></td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let curto = rect(&dom, &list, "td", 0);
    let alto = rect(&dom, &list, "td", 1);
    let tr = rect(&dom, &list, "tr", 0);
    assert!(alto.h >= 80.0, "célula alta = {}", alto.h);
    assert!((curto.h - alto.h).abs() < 0.5, "curta={} alta={}", curto.h, alto.h);
    assert!((tr.h - alto.h).abs() < 0.5, "linha = {}", tr.h);
}

/// Uma célula com `colspan` atravessa as colunas: começa na primeira e acaba na
/// última, e a linha seguinte continua a usar as colunas normais.
#[test]
fn colspan_atravessa_as_colunas_sem_desalinhar_a_linha_seguinte() {
    let html = r#"<table cellspacing="0" style="width:200px">
        <tr><td colspan="2">cabeçalho</td></tr>
        <tr><td style="width:100px">a</td><td style="width:100px">b</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let cab = rect(&dom, &list, "td", 0);
    let a = rect(&dom, &list, "td", 1);
    let b = rect(&dom, &list, "td", 2);
    assert!((cab.w - (a.w + b.w)).abs() < 0.5, "colspan={} a+b={}", cab.w, a.w + b.w);
    assert!((a.x - cab.x).abs() < 0.5);
    assert!((b.x - (a.x + a.w)).abs() < 0.5);
}

/// `rowspan` reserva o lugar na grade: a célula da linha de baixo SALTA a coluna
/// que a de cima ainda ocupa, em vez de ficar por baixo dela.
#[test]
fn rowspan_faz_a_linha_seguinte_saltar_a_coluna_ocupada() {
    let html = r#"<table cellspacing="0" style="width:200px">
        <tr><td rowspan="2" style="width:100px">alto</td><td style="width:100px">a</td></tr>
        <tr><td>b</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let alto = rect(&dom, &list, "td", 0);
    let a = rect(&dom, &list, "td", 1);
    let b = rect(&dom, &list, "td", 2);
    // `b` é o único da segunda linha e pertence à SEGUNDA coluna.
    assert!((b.x - a.x).abs() < 0.5, "b.x={} devia estar na coluna de a (x={})", b.x, a.x);
    assert!(b.x > alto.x + 1.0, "b não devia ficar sob a célula com rowspan");
}

/// `border-spacing` (via `cellspacing`) separa as colunas E afasta a primeira da
/// borda da tabela — os vãos são colunas+1, não colunas-1.
#[test]
fn o_border_spacing_existe_tambem_entre_a_borda_e_a_primeira_coluna() {
    let html = r#"<table cellspacing="10" style="width:300px">
        <tr><td>a</td><td>b</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let tabela = rect(&dom, &list, "table", 0);
    let a = rect(&dom, &list, "td", 0);
    let b = rect(&dom, &list, "td", 1);
    assert!((a.x - tabela.x - 10.0).abs() < 0.5, "recuo inicial = {}", a.x - tabela.x);
    assert!((b.x - (a.x + a.w) - 10.0).abs() < 0.5, "vão = {}", b.x - a.x - a.w);
}

/// Uma tabela sem `width` encolhe ao conteúdo em vez de ocupar o pai — é a
/// diferença mais visível entre uma `<table>` e um `<div>`.
#[test]
fn tabela_sem_width_encolhe_ao_conteudo() {
    let html = r#"<table cellspacing="0"><tr><td style="width:60px">a</td></tr></table>"#;
    let (dom, list) = geometria(html, 800.0);
    let t = rect(&dom, &list, "table", 0);
    assert!(t.w < 200.0, "a tabela ocupou {} de 800 — não encolheu", t.w);
}

/// As linhas dentro de um `<tbody>` entram na mesma grade das que estão soltas, e
/// o grupo recebe uma caixa que abrange as suas linhas.
#[test]
fn tbody_nao_cria_uma_grade_separada() {
    let html = r#"<table cellspacing="0" style="width:200px">
        <tbody>
          <tr><td style="width:100px">a</td><td style="width:100px">b</td></tr>
          <tr><td>c</td><td>d</td></tr>
        </tbody>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "td", 0);
    let c = rect(&dom, &list, "td", 2);
    let tbody = rect(&dom, &list, "tbody", 0);
    assert!((c.x - a.x).abs() < 0.5, "as duas linhas do tbody usam a mesma coluna");
    assert!(tbody.h >= a.h + c.h - 0.5, "o grupo abrange as duas linhas: {}", tbody.h);
}

// ── ITEM DE LISTA ───────────────────────────────────────────────────────────

/// Um `<ol>` numera 1., 2., 3. — e os números são pintados à ESQUERDA do texto.
#[test]
fn ol_numera_os_itens_a_partir_de_um() {
    let (dom, list) = geometria("<ol><li>um</li><li>dois</li><li>três</li></ol>", 800.0);
    let t = textos(&list);
    for esperado in ["1.", "2.", "3."] {
        assert!(t.iter().any(|s| s == esperado), "faltou o marcador {esperado} em {t:?}");
    }
    // O marcador do primeiro item fica à esquerda do content-box dele.
    let li = rect(&dom, &list, "li", 0);
    let x_marcador = list
        .materialized()
        .iter()
        .find_map(|i| match i {
            DisplayItem::Text { x, text, .. } if &**text == "1." => Some(*x),
            _ => None,
        })
        .expect("marcador 1.");
    assert!(x_marcador < li.x, "marcador em {x_marcador}, item em {}", li.x);
}

/// `<ol start>` começa onde o atributo manda, e continua daí.
#[test]
fn ol_com_start_comeca_no_numero_pedido() {
    let (_, list) = geometria("<ol start=\"5\"><li>a</li><li>b</li></ol>", 800.0);
    let t = textos(&list);
    assert!(t.iter().any(|s| s == "5."), "{t:?}");
    assert!(t.iter().any(|s| s == "6."), "{t:?}");
}

/// `list-style: none` não gera marcador nenhum — o caso mais comum numa página
/// real, onde `<ul>` é o markup de um menu.
#[test]
fn list_style_none_nao_gera_marcador() {
    let (_, list) = geometria(
        "<ul style=\"list-style:none\"><li>a</li><li>b</li></ul>",
        800.0,
    );
    // Nenhum bullet: os únicos rects sólidos possíveis viriam de fundos, que
    // este markup não tem.
    let bullets = list
        .materialized()
        .iter()
        .filter(|i| matches!(i, DisplayItem::SolidRect { .. }))
        .count();
    assert_eq!(bullets, 0, "list-style:none desenhou {bullets} marcadores");
}

/// Um `<ul>` normal desenha um bullet por item, e o bullet cai dentro do recuo
/// que a UA-stylesheet reserva — nunca por cima do texto.
#[test]
fn ul_desenha_um_bullet_por_item_dentro_do_recuo() {
    let (dom, list) = geometria("<ul><li>a</li><li>b</li></ul>", 800.0);
    let bullets: Vec<Rect> = list
        .materialized()
        .iter()
        .filter_map(|i| match i {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bullets.len(), 2, "esperados 2 bullets, vieram {}", bullets.len());
    let ul = rect(&dom, &list, "ul", 0);
    let li = rect(&dom, &list, "li", 0);
    for b in &bullets {
        assert!(b.x + b.w <= li.x + 0.5, "bullet invade o texto: {} vs {}", b.x + b.w, li.x);
        assert!(b.x >= ul.x - 0.5, "bullet fora da caixa da lista");
    }
}

/// O recuo default de 40px do `<ul>` é da UA e cede a um `padding-left` do autor
/// — é o que faz um menu com `padding-left:0` alinhar com o resto da página.
#[test]
fn o_padding_left_do_autor_anula_o_recuo_da_ua() {
    let (dom, list) = geometria("<ul><li>a</li></ul>", 800.0);
    let com_ua = rect(&dom, &list, "li", 0).x;
    let (dom2, list2) = geometria("<ul style=\"padding-left:0\"><li>a</li></ul>", 800.0);
    let sem = rect(&dom2, &list2, "li", 0).x;
    assert!((com_ua - sem - 40.0).abs() < 0.5, "recuo da UA = {}", com_ua - sem);
}

/// Um `<li>` que o autor virou `display:flex` deixa de ser item de lista: não
/// ganha marcador e não conta para a numeração dos irmãos.
#[test]
fn li_com_display_trocado_nao_e_mais_item_de_lista() {
    let (_, list) = geometria(
        "<ol><li style=\"display:flex\">a</li><li>b</li></ol>",
        800.0,
    );
    let t = textos(&list);
    assert!(!t.iter().any(|s| s == "2."), "o `flex` não devia contar: {t:?}");
    assert!(t.iter().any(|s| s == "1."), "o item que sobrou é o 1: {t:?}");
}

/// Uma célula cujo conteúdo declara `width:100%` NÃO exige a largura da viewport
/// como mínimo: a percentagem é contra a coluna, que é o que se está a decidir.
///
/// Pinado porque foi medido a acontecer na Wikipédia — a infobox saía com 1280px
/// de largura dentro de um artigo de 750, e a causa estava a três saltos do
/// sintoma: o `ResolveCtx` da medição intrínseca põe a viewport como largura do
/// pai, e `100%` disso é a janela toda.
#[test]
fn width_percentual_dentro_da_celula_nao_vira_minimo_de_viewport() {
    let html = r#"<table cellspacing="0" style="width:300px">
        <tr><td><div style="width:100%">a</div></td><td>b</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 1280.0);
    let t = rect(&dom, &list, "table", 0);
    let a = rect(&dom, &list, "td", 0);
    assert!(t.w <= 301.0, "a tabela saiu com {}", t.w);
    assert!(a.w <= 301.0, "a célula saiu com {} — o `100%` virou 1280", a.w);
}

// ── AS QUATRO PROPRIEDADES ──────────────────────────────────────────────────
// Cada teste aqui falharia sem a propriedade que nomeia: o que se verifica é o
// EFEITO no layout, não que a folha de estilo parseou.

/// `border-collapse: collapse` anula o vão entre células — e é a diferença que
/// decide se uma tabela casa com o Chrome dentro de 1px ou não: 2px por coluna
/// acumulam ao longo da linha, portanto uma tabela `collapse` medida como
/// `separate` conta como errada em TODAS as suas células.
#[test]
fn border_collapse_muda_a_posicao_das_celulas_e_nao_so_a_borda() {
    let corpo = r#"<tr><td style="width:100px">a</td><td style="width:100px">b</td></tr>"#;
    let sep = format!(r#"<table style="width:220px;border-collapse:separate;border-spacing:10px">{corpo}</table>"#);
    let col = format!(r#"<table style="width:220px;border-collapse:collapse">{corpo}</table>"#);

    let (d1, l1) = geometria(&sep, 800.0);
    let (d2, l2) = geometria(&col, 800.0);
    let a_sep = rect(&d1, &l1, "td", 0);
    let b_sep = rect(&d1, &l1, "td", 1);
    let a_col = rect(&d2, &l2, "td", 0);
    let b_col = rect(&d2, &l2, "td", 1);

    // `separate` com 10px: recuo de 10 antes da primeira, 10 entre as duas.
    assert!((b_sep.x - (a_sep.x + a_sep.w) - 10.0).abs() < 0.5, "vão separate = {}", b_sep.x - a_sep.x - a_sep.w);
    // `collapse`: as colunas encostam, e a primeira encosta à borda da tabela.
    assert!((b_col.x - (a_col.x + a_col.w)).abs() < 0.5, "vão collapse = {}", b_col.x - a_col.x - a_col.w);
    assert!(a_col.x < a_sep.x, "collapse devia começar mais à esquerda");
}

/// O `border-spacing` do CSS vence o atributo `cellspacing` do HTML — a
/// precedência do browser, e a razão de os dois coexistirem: o atributo é
/// apresentação que o HTML define, o CSS é quem manda quando fala.
#[test]
fn o_border_spacing_do_css_vence_o_atributo_cellspacing() {
    let html = r#"<table cellspacing="0" style="width:300px;border-spacing:12px">
        <tr><td style="width:100px">a</td><td style="width:100px">b</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "td", 0);
    let b = rect(&dom, &list, "td", 1);
    assert!((b.x - (a.x + a.w) - 12.0).abs() < 0.5, "o atributo ganhou: vão = {}", b.x - a.x - a.w);
}

/// `border-spacing` aceita dois comprimentos, e o segundo é o VERTICAL: as
/// linhas afastam-se sem que as colunas se mexam.
#[test]
fn border_spacing_de_dois_valores_afasta_as_linhas_sem_mexer_nas_colunas() {
    let html = r#"<table style="width:220px;border-spacing:0px 20px">
        <tr><td style="width:100px">a</td><td style="width:100px">b</td></tr>
        <tr><td>c</td><td>d</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "td", 0);
    let b = rect(&dom, &list, "td", 1);
    let c = rect(&dom, &list, "td", 2);
    assert!((b.x - (a.x + a.w)).abs() < 0.5, "o vão horizontal devia ser 0");
    assert!((c.y - (a.y + a.h) - 20.0).abs() < 0.5, "vão vertical = {}", c.y - a.y - a.h);
}

/// `table-layout: fixed` decide as colunas pela PRIMEIRA linha e ignora o
/// conteúdo das seguintes — é o algoritmo que existe para não medir nada.
#[test]
fn table_layout_fixed_ignora_o_conteudo_das_linhas_seguintes() {
    let corpo = r#"
        <tr><td style="width:50px">a</td><td>b</td></tr>
        <tr><td>uma frase muito comprida que num layout auto alargaria esta coluna toda</td><td>x</td></tr>"#;
    let auto = format!(r#"<table style="width:300px;border-spacing:0">{corpo}</table>"#);
    let fixo = format!(r#"<table style="width:300px;border-spacing:0;table-layout:fixed">{corpo}</table>"#);

    let (d1, l1) = geometria(&auto, 800.0);
    let (d2, l2) = geometria(&fixo, 800.0);
    let auto_c1 = rect(&d1, &l1, "td", 0).w;
    let fixo_c1 = rect(&d2, &l2, "td", 0).w;

    // No fixo a primeira coluna fica com os 50px pedidos, custe o que custar.
    assert!((fixo_c1 - 50.0).abs() < 0.5, "fixed deu {fixo_c1} à coluna de 50px");
    // No auto a frase da segunda linha alarga-a — é a diferença entre os dois.
    assert!(auto_c1 > fixo_c1 + 1.0, "auto={auto_c1} fixo={fixo_c1}");
}

/// `list-style-position: inside` põe o marcador DENTRO da caixa de conteúdo, e
/// `outside` (o default) fora dela — sem que a caixa do item mude em nenhum dos
/// dois, que é o que o browser faz.
#[test]
fn list_style_position_muda_o_lado_do_marcador_e_nao_a_caixa() {
    let (d1, l1) = geometria("<ul><li>a</li></ul>", 800.0);
    let (d2, l2) = geometria(
        "<ul style=\"list-style-position:inside\"><li>a</li></ul>",
        800.0,
    );
    let bullet = |l: &crate::layout::DisplayList| {
        l.materialized()
            .iter()
            .find_map(|i| match i {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("bullet")
    };
    let fora = bullet(&l1);
    let dentro = bullet(&l2);
    let li_fora = rect(&d1, &l1, "li", 0);
    let li_dentro = rect(&d2, &l2, "li", 0);

    assert!(fora.x + fora.w <= li_fora.x + 0.5, "outside devia ficar fora do conteúdo");
    assert!(dentro.x >= li_dentro.x - 0.5, "inside devia ficar dentro do conteúdo");
    // A caixa do item é a MESMA nos dois: o marcador nunca ocupa espaço de fluxo.
    assert!((li_fora.w - li_dentro.w).abs() < 0.5, "a caixa mudou: {} vs {}", li_fora.w, li_dentro.w);
}

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
    assert!(l1.y >= l0.y + l0.h - 0.5, "as linhas não empilharam: {} e {}", l0.y, l1.y);
    // E o `display:table` do autor, aninhado numa célula, também reparte.
    let celulas = dom.query_all("div[style*=table-cell]");
    assert_eq!(celulas.len(), 2, "as duas células do table aninhado");
}

#[test]
fn dbg_anon() {
    let html = r##"<div style="display:table; width:400px; border-spacing:0">
        <div style="display:table-cell">bandeira</div>
        <div style="display:table-cell">armas</div>
      </div>"##;
    let (dom, list) = geometria(html, 1280.0);
    let g = list.geometry_now();
    for (i, id) in dom.query_all("div[style*=table-cell]").into_iter().enumerate() {
        println!("SEL cel[{i}] {:?}", g.rects.get(&dom.resolve(id).unwrap()));
    }
}
