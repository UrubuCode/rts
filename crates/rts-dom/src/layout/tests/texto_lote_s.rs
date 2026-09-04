//! Comportamentos do lote S (texto): `text-overflow: ellipsis`,
//! `word-spacing`, `tab-size`, `-webkit-line-clamp`. Cada teste nomeia o
//! CONTRATO, não a função — ver `tests/css/claude-{text-overflow,
//! word-spacing,tab-size,line-clamp}.html` para as fixtures sem `.esperado`
//! que este mesmo comportamento fixa no corpus (medidas contra o Edge, não
//! contra o Chrome — o motor deste crate é o `ApproxMeasurer`, então a
//! comparação certa aqui é RELATIVA: mais largo que, tantas linhas que, e não
//! um pixel do Chrome).

use crate::table::tests::{geometria, rect};

/// `text-overflow: ellipsis` corta o TEXTO PINTADO, não a caixa: a largura de
/// `#caixa` é a declarada (120px), com ou sem elipse — é a garantia que
/// `segmento::aplicar_elipse` documenta ("a caixa não muda de tamanho").
#[test]
fn ellipsis_nao_altera_a_largura_da_caixa() {
    let html = r#"<style>
      body{margin:0;font:16px/20px monospace}
      #caixa{width:120px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
    </style><div id="caixa">um texto comprido demais para a caixa toda</div>"#;
    let (dom, list) = geometria(html, 600.0);
    let r = rect(&dom, &list, "#caixa", 0);
    assert_eq!(r.w, 120.0, "a caixa não encolhe nem cresce por causa do corte");
}

/// `word-spacing` positivo alarga a linha: o MESMO texto, no mesmo container
/// largo (nunca quebra), mede mais largo com `word-spacing:10px` do que sem —
/// e o inverso com um valor negativo, que aperta.
#[test]
fn word_spacing_alarga_e_aperta_a_linha() {
    // `inline-block`: a caixa mede-se ao CONTEÚDO (shrink-to-fit) em vez de
    // encher o container — sem isto o `width:auto` de um `div` de bloco
    // responde sempre a largura do containing block, e a diferença de
    // `word-spacing` desaparece dentro da folga.
    let html_base = |ws: &str| {
        format!(
            r#"<style>body{{margin:0;font:16px/20px monospace;white-space:nowrap}}
            #a{{display:inline-block;{ws}}}</style><div id="a">um dois tres</div>"#
        )
    };
    let (dom0, list0) = geometria(&html_base(""), 2000.0);
    let (dom10, list10) = geometria(&html_base("word-spacing:10px"), 2000.0);
    let (domneg, listneg) = geometria(&html_base("word-spacing:-2px"), 2000.0);
    let w0 = rect(&dom0, &list0, "#a", 0).w;
    let w10 = rect(&dom10, &list10, "#a", 0).w;
    let wneg = rect(&domneg, &listneg, "#a", 0).w;
    assert!(w10 > w0, "word-spacing:10px devia alargar a linha ({w10} <= {w0})");
    assert!(wneg < w0, "word-spacing:-2px devia apertar a linha ({wneg} >= {w0})");
    // dois espaços entre três palavras: a diferença é ~2x o word-spacing.
    assert!(
        (w10 - w0 - 20.0).abs() < 2.0,
        "esperava ~20px a mais (2 espaços x 10px), obtido {}",
        w10 - w0
    );
}

/// Um `\t` em `white-space:pre` avança até ao próximo tab-stop de `tab-size`
/// colunas — `tab-size:8` avança mais do que `tab-size:2` para o MESMO "a\tb"
/// a partir da coluna 0 (7 espaços contra 1).
#[test]
fn tab_em_pre_avanca_ate_ao_tab_stop() {
    // `display:inline-block` no `<pre>`, pela mesma razão do teste de
    // `word-spacing` acima: um `<pre>` de bloco tem `width:auto` = a largura
    // do containing block, não a do conteúdo, e a diferença entre tab-sizes
    // desapareceria dentro dessa largura fixa.
    let html = |ts: u32| {
        format!(
            r#"<style>body{{margin:0;font:16px/20px monospace}}
            pre{{display:inline-block;tab-size:{ts}}}</style><pre>a	b</pre>"#
        )
    };
    let (dom8, list8) = geometria(&html(8), 2000.0);
    let (dom2, list2) = geometria(&html(2), 2000.0);
    let w8 = rect(&dom8, &list8, "pre", 0).w;
    let w2 = rect(&dom2, &list2, "pre", 0).w;
    assert!(
        w8 > w2,
        "tab-size:8 devia avançar mais do que tab-size:2 ({w8} <= {w2})"
    );
}

/// `-webkit-line-clamp: N` limita a caixa a N linhas — a altura de um bloco
/// com o clamp é a de N linhas, contra a altura de M > N linhas do mesmo
/// texto sem o clamp.
#[test]
fn line_clamp_limita_a_altura_a_n_linhas() {
    let texto = "um dois tres quatro cinco seis sete oito nove dez";
    let sem_clamp = format!(
        r#"<style>body{{margin:0;font:16px/20px monospace}}
        div{{width:100px}}</style><div>{texto}</div>"#
    );
    let com_clamp = format!(
        r#"<style>body{{margin:0;font:16px/20px monospace}}
        div{{width:100px;display:-webkit-box;-webkit-box-orient:vertical;
        -webkit-line-clamp:2;overflow:hidden}}</style><div>{texto}</div>"#
    );
    let (dom0, list0) = geometria(&sem_clamp, 600.0);
    let (dom1, list1) = geometria(&com_clamp, 600.0);
    let h_sem = rect(&dom0, &list0, "div", 0).h;
    let h_com = rect(&dom1, &list1, "div", 0).h;
    // line-height:20px, duas linhas => 40px; sem clamp o texto (>2 linhas
    // largas de 100px) mede mais alto.
    assert_eq!(h_com, 40.0, "line-clamp:2 a 20px/linha devia dar 40px, deu {h_com}");
    assert!(h_sem > h_com, "sem clamp devia ser mais alto ({h_sem} <= {h_com})");
}

/// A caixa de um `inline-block` na linha é a BORDER box (o que
/// `getBoundingClientRect` devolve no Chrome), e o PITCH até à linha
/// seguinte é a MARGIN box — as duas medidas coexistem sem se misturar.
/// Fixa o desvio medido no Blink em `claude-word-spacing.html`: três divs
/// `inline-block` com `margin-bottom:5px`, cada um numa linha própria
/// (separados por `<br>`) — o rect do elemento não inclui a margem, mas a
/// linha seguinte começa depois dela.
#[test]
fn inline_block_com_margem_rect_e_border_box_pitch_e_margin_box() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    div{background:#eee;margin-bottom:5px;display:inline-block}</style>
    <div id="a">um dois tres</div><br><div id="b">um dois tres</div>"#;
    let (dom, list) = geometria(html, 2000.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    assert_eq!(a.y, 0.0);
    assert_eq!(a.h, 20.0, "o rect do elemento não inclui a margin-bottom");
    assert_eq!(b.y, 25.0, "o pitch (20 conteúdo + 5 margem) inclui a margem UMA vez");
    assert_eq!(b.h, 20.0, "o segundo elemento não herda a margem do primeiro");
}

/// Um rect SEM ÁREA — `w=0` ou `h=0` — que é o que o motor devolve para um
/// inline sem fragmento nenhum (às vezes uma geometria explícita 0×0, às
/// vezes nenhuma, conforme o caminho de emissão; o mesmo idioma que
/// `uniontests.rs` já usa para "não gera caixa").
fn sem_area(r: Option<crate::layout::Rect>) -> bool {
    r.is_none_or(|r| r.w <= 0.0 || r.h <= 0.0)
}

fn rect_opt(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str) -> Option<crate::layout::Rect> {
    let id = *dom.query_all(sel).first()?;
    let idx = dom.resolve(id)?;
    list.geometry_now().rects.get(&idx).copied()
}

/// Um `<span></span>` inline VAZIO — sem texto, sem filhos, sem `content`
/// gerado — não produz fragmento de linha: no Blink `getBoundingClientRect`
/// dá 0×0, não a altura do strut que a linha usa para as OUTRAS caixas.
#[test]
fn span_inline_vazio_nao_tem_area() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}</style>
    <p>a<span id="vazio"></span>b</p>"#;
    let (dom, list) = geometria(html, 600.0);
    let r = rect_opt(&dom, &list, "#vazio");
    assert!(sem_area(r), "#vazio mediu {r:?}, esperava 0×0 (sem fragmento)");
}

/// A mesma regra quando o único conteúdo é um espaço COLAPSÁVEL — ele vira
/// separador entre "c" e "d" e não sobra texto nenhum para o `<span>` pintar.
#[test]
fn span_so_com_espaco_colapsavel_nao_tem_area() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}</style>
    <p>c<span id="espaco"> </span>d</p>"#;
    let (dom, list) = geometria(html, 600.0);
    let r = rect_opt(&dom, &list, "#espaco");
    assert!(sem_area(r), "#espaco mediu {r:?}, esperava 0×0 (sem fragmento)");
}

/// `width`/`height` NÃO SE APLICAM a um inline não-substituído (CSS 2.1
/// §10.3.1/§10.6.1) — nem quando `display` não está declarado NENHUM (um
/// `<span>` puro, sem default de bloco) nem quando o único conteúdo do
/// inline é vazio. Fixa o desvio medido no Blink em `claude-sel-has.html`:
/// `div, span { height: 20px }` sem `display` declarado em lado nenhum —
/// o `<div>` (tem default de bloco) RESPEITA a altura; os `<span>` (sem
/// default nenhum, plain inline) IGNORAM-NA e ficam sem fragmento.
#[test]
fn span_sem_display_ignora_width_e_height_declarados() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    div, span { height: 20px; }</style>
    <div id="rotulo-com"><span></span></div>
    <span id="rotulo-sem"></span>"#;
    let (dom, list) = geometria(html, 600.0);
    let div = rect_opt(&dom, &list, "#rotulo-com").expect("o <div> respeita a height");
    assert_eq!(div.h, 20.0, "o <div> (default de bloco) usa a height declarada");
    assert!(
        sem_area(rect_opt(&dom, &list, "#rotulo-com span")),
        "o <span> vazio dentro do <div> ignora a height"
    );
    assert!(
        sem_area(rect_opt(&dom, &list, "#rotulo-sem")),
        "o <span> vazio solto ignora a height"
    );
}

/// `claude-display-basico.html` (medido no Chrome, 2026-08-18) continua a
/// dar 19px a um `display:inline` EXPLÍCITO com texto — este teste é o
/// regresso: a correção acima só muda o caso do `display` NÃO declarado
/// (`effective_display() == None`), nunca o `display:inline` escrito.
/// O HTML e o `.esperado.json` EXATOS de `claude-sel-has.html` (lote O,
/// medido no Chrome — `git show feat/dom-vaga-3:tests/css/claude-sel-has.*`),
/// pinados aqui porque o corredor do corpus não corre neste crate: seis
/// `<div>`/`<span>` com a MESMA `height`/`background-color` declaradas
/// (`div, span { height:20px; background-color:#fff }`) — os `<div>` (têm
/// default de bloco) respeitam as duas; os `<span>` vazios (sem `display`,
/// sem default) IGNORAM as duas e não geram fragmento nenhum (0×0).
///
/// Fixa a regressão de `cria_caixa_via_dimensoes`: uma primeira versão desta
/// função deixava um `background-color` (que os quatro `<span>` também têm)
/// dar ao `<span>` a sua PRÓPRIA linha de largura total — `rotulo-com`/
/// `rotulo-sem` chegaram a medir `1280×20`, onde o Chrome dá `0×0`.
#[test]
fn sel_has_contra_o_chrome() {
    let html = r#"<style>body{margin:0}
    div, span { height: 20px; background-color: #ffffff; }</style>
  <div class="card" id="card-com-erro"><span class="erro"></span></div>
  <div class="card" id="card-sem-erro"><span></span></div>

  <div class="box" id="box-filho"><span class="img"></span></div>
  <div class="box" id="box-neto"><span><span class="img"></span></span></div>

  <span class="rotulo" id="rotulo-com"></span><span class="obrigatorio"></span>
  <span class="rotulo" id="rotulo-sem"></span><span></span>
"#;
    let (dom, list) = geometria(html, 1280.0);
    let bloco = |sel: &str, esperado: (f32, f32, f32, f32)| {
        let r = rect_opt(&dom, &list, sel).unwrap_or_else(|| panic!("{sel} sem geometria"));
        assert_eq!((r.x, r.y, r.w, r.h), esperado, "{sel}");
    };
    bloco("#card-com-erro", (0.0, 0.0, 1280.0, 20.0));
    bloco("#card-sem-erro", (0.0, 20.0, 1280.0, 20.0));
    bloco("#box-filho", (0.0, 40.0, 1280.0, 20.0));
    bloco("#box-neto", (0.0, 60.0, 1280.0, 20.0));
    for sel in ["#rotulo-com", "#rotulo-sem"] {
        assert!(
            sem_area(rect_opt(&dom, &list, sel)),
            "{sel} devia ser 0×0 (sem fragmento) — não uma linha própria de largura total"
        );
    }
}

/// `<span style="background:yellow">texto</span>` — SEM `display` declarado,
/// COM texto e fundo — mede o TEXTO (não uma largura/altura declarada, que
/// aqui nem existe) e PINTA o fundo. É o padrão mais comum de página real, e
/// a razão de `bloco.rs` ignorar `width`/`height` DENTRO de `layout_block`
/// em vez de recusar a caixa inteira (que apagava o fundo também).
#[test]
fn span_com_fundo_e_texto_mede_o_texto_e_pinta_o_fundo() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    span{background:#ffff00}</style>
    <p>a <span id="rotulo">texto</span> b</p>"#;
    let (dom, list) = geometria(html, 600.0);
    let r = rect_opt(&dom, &list, "#rotulo").expect("tem texto, tem caixa");
    assert!(r.w > 0.0 && r.h > 0.0, "mede o conteúdo: {r:?}");
    // `list.items` só tem o que ESTE nível pintou diretamente — subárvores em
    // cache de fragmento (`layout_block_reusing`) ficam por REFERÊNCIA em
    // `list.children`, não copiadas; `materialized()` é o que as achata.
    let cores: Vec<u32> = list
        .materialized()
        .iter()
        .filter_map(|it| match it {
            crate::layout::DisplayItem::SolidRect { color, .. } => Some(*color),
            _ => None,
        })
        .collect();
    let pintou_fundo = cores.contains(&0xffff00ff);
    assert!(pintou_fundo, "o fundo amarelo tem de estar na DisplayList; cores={cores:08x?} r={r:?}");
}

/// O CORTE ao lado: o MESMO `<span>` sem `display`, com fundo E `height`
/// declarados, mas SEM conteúdo — 0×0 e NADA pintado, porque sem conteúdo
/// não há linha para o fundo se apoiar (o Blink também dá 0 aqui, mesmo com
/// `background-color` — é `claude-sel-has.html`, `#rotulo-com`/`#rotulo-sem`).
#[test]
fn span_vazio_com_fundo_e_height_nao_pinta_nada() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    span{background:#ffff00;height:20px}</style>
    <span id="rotulo"></span>"#;
    let (dom, list) = geometria(html, 600.0);
    assert!(
        sem_area(rect_opt(&dom, &list, "#rotulo")),
        "vazio: 0×0, mesmo com background e height declarados"
    );
    // Um `SolidRect` de área ZERO pode existir na DisplayList (o mesmo caminho
    // de pintura corre, só que a caixa mede 0×0) — invisível, e é essa a parte
    // que interessa: NENHUM pixel do fundo amarelo é visível sem conteúdo.
    let pintou_algo_visivel = list.materialized().iter().any(|it| {
        matches!(it, crate::layout::DisplayItem::SolidRect { color, rect, .. }
            if *color == 0xffff00ff && rect.w > 0.0 && rect.h > 0.0)
    });
    assert!(!pintou_algo_visivel, "sem conteúdo, nenhum pixel do fundo deve ficar visível");
}

#[test]
fn display_inline_explicito_com_texto_continua_a_ignorar_a_altura() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    span{display:inline;width:300px;height:300px}</style>
    <span id="em-linha">abc</span>"#;
    let (dom, list) = geometria(html, 1280.0);
    let r = rect_opt(&dom, &list, "#em-linha").expect("tem texto, tem caixa");
    assert!(r.h < 300.0, "300px de height não pode ter sido aplicado: {r:?}");
}

/// O CORTE ao lado da regra acima: um `inline-block` VAZIO continua a ter
/// caixa (é a mesma família de átomo do `claude-vertical-align.html`, que
/// não pode regredir) — a diferença entre os dois é `display`, não
/// "vazio"/"não vazio".
#[test]
fn inline_block_vazio_continua_com_caixa() {
    let html = r#"<style>body{margin:0;font:16px/20px monospace}
    span{display:inline-block;width:10px;height:10px}</style>
    <p>a<span id="bloco"></span>b</p>"#;
    let (dom, list) = geometria(html, 600.0);
    let r = rect_opt(&dom, &list, "#bloco").expect("inline-block vazio continua a ter geometria");
    assert_eq!((r.w, r.h), (10.0, 10.0));
}
