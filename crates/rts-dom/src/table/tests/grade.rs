use super::*;


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

    // `td`/`th` têm `padding: 1px` na folha de UA (lote I, medido no Chrome
    // real via `tests/css/claude-ua-th.html`): `width` é CONTENT-BOX (default
    // sem `box-sizing`), então a caixa (o que `rect` devolve) é
    // content + 2×1px de padding — 100→102, 200→202.
    assert!((a.w - 102.0).abs() < 0.5, "coluna 1 = {}", a.w);
    assert!((b.w - 202.0).abs() < 0.5, "coluna 2 = {}", b.w);
    // A segunda coluna começa onde a primeira acaba: `cellspacing=0`.
    assert!(
        (b.x - (a.x + a.w)).abs() < 0.5,
        "b.x={} a fim={}",
        b.x,
        a.x + a.w
    );
    // A segunda linha usa as MESMAS colunas — é o que distingue uma tabela de
    // duas linhas de blocos empilhados.
    assert!((c.x - a.x).abs() < 0.5 && (c.w - a.w).abs() < 0.5);
    assert!((d.x - b.x).abs() < 0.5 && (d.w - b.w).abs() < 0.5);
    // E fica ABAIXO da primeira.
    assert!(
        c.y >= a.y + a.h - 0.5,
        "linha 2 em y={} linha 1 acaba em {}",
        c.y,
        a.y + a.h
    );
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
    assert!(
        (curto.h - alto.h).abs() < 0.5,
        "curta={} alta={}",
        curto.h,
        alto.h
    );
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
    assert!(
        (cab.w - (a.w + b.w)).abs() < 0.5,
        "colspan={} a+b={}",
        cab.w,
        a.w + b.w
    );
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
    assert!(
        (b.x - a.x).abs() < 0.5,
        "b.x={} devia estar na coluna de a (x={})",
        b.x,
        a.x
    );
    assert!(
        b.x > alto.x + 1.0,
        "b não devia ficar sob a célula com rowspan"
    );
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
    assert!(
        (a.x - tabela.x - 10.0).abs() < 0.5,
        "recuo inicial = {}",
        a.x - tabela.x
    );
    assert!(
        (b.x - (a.x + a.w) - 10.0).abs() < 0.5,
        "vão = {}",
        b.x - a.x - a.w
    );
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
    assert!(
        (c.x - a.x).abs() < 0.5,
        "as duas linhas do tbody usam a mesma coluna"
    );
    assert!(
        tbody.h >= a.h + c.h - 0.5,
        "o grupo abrange as duas linhas: {}",
        tbody.h
    );
}

/// Um `<ol>` numera 1., 2., 3. — e os números são pintados à ESQUERDA do texto.
/// `<ol start>` começa onde o atributo manda, e continua daí.
/// real, onde `<ul>` é o markup de um menu.
/// Um `<ul>` normal desenha um bullet por item, e o bullet cai dentro do recuo
/// que a UA-stylesheet reserva — nunca por cima do texto.
/// O recuo default de 40px do `<ul>` é da UA e cede a um `padding-left` do autor
/// — é o que faz um menu com `padding-left:0` alinhar com o resto da página.
#[test]
fn o_padding_left_do_autor_anula_o_recuo_da_ua() {
    let (dom, list) = geometria("<ul><li>a</li></ul>", 800.0);
    let com_ua = rect(&dom, &list, "li", 0).x;
    let (dom2, list2) = geometria("<ul style=\"padding-left:0\"><li>a</li></ul>", 800.0);
    let sem = rect(&dom2, &list2, "li", 0).x;
    assert!(
        (com_ua - sem - 40.0).abs() < 0.5,
        "recuo da UA = {}",
        com_ua - sem
    );
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
    assert!(
        a.w <= 301.0,
        "a célula saiu com {} — o `100%` virou 1280",
        a.w
    );
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
    let sep = format!(
        r#"<table style="width:220px;border-collapse:separate;border-spacing:10px">{corpo}</table>"#
    );
    let col = format!(r#"<table style="width:220px;border-collapse:collapse">{corpo}</table>"#);

    let (d1, l1) = geometria(&sep, 800.0);
    let (d2, l2) = geometria(&col, 800.0);
    let a_sep = rect(&d1, &l1, "td", 0);
    let b_sep = rect(&d1, &l1, "td", 1);
    let a_col = rect(&d2, &l2, "td", 0);
    let b_col = rect(&d2, &l2, "td", 1);

    // `separate` com 10px: recuo de 10 antes da primeira, 10 entre as duas.
    assert!(
        (b_sep.x - (a_sep.x + a_sep.w) - 10.0).abs() < 0.5,
        "vão separate = {}",
        b_sep.x - a_sep.x - a_sep.w
    );
    // `collapse`: as colunas encostam, e a primeira encosta à borda da tabela.
    assert!(
        (b_col.x - (a_col.x + a_col.w)).abs() < 0.5,
        "vão collapse = {}",
        b_col.x - a_col.x - a_col.w
    );
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
    assert!(
        (b.x - (a.x + a.w) - 12.0).abs() < 0.5,
        "o atributo ganhou: vão = {}",
        b.x - a.x - a.w
    );
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
    assert!(
        (b.x - (a.x + a.w)).abs() < 0.5,
        "o vão horizontal devia ser 0"
    );
    assert!(
        (c.y - (a.y + a.h) - 20.0).abs() < 0.5,
        "vão vertical = {}",
        c.y - a.y - a.h
    );
}

