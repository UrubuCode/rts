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
