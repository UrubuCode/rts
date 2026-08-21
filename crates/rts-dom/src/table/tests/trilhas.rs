use super::*;

// ── GRADE: a largura que as tabelas herdavam errada ─────────────────────────

/// A grade do `<main>` da Wikipédia: `minmax(0,59.25rem) min-content`, num
/// container de 972px com 24px de vão.
///
/// O Chrome dá 752px à coluna de conteúdo — o que sobra depois da barra lateral
/// — e não os 948px (59.25rem) do máximo da trilha. Tratar `minmax` como o seu
/// máximo dava-nos 948, a barra lateral saía fora da janela, e os 196px de erro
/// eram herdados por tudo o que está dentro do artigo: 46 das 49 tabelas da
/// página tinham a largura errada por causa desta linha.
#[test]
fn minmax_nao_come_o_maximo_quando_ha_outra_trilha_ao_lado() {
    let html = r##"<div style="display:grid; width:972px; gap:24px;
                     grid-template-columns:minmax(0,59.25rem) min-content">
        <div id="conteudo">artigo</div>
        <div id="lado" style="width:196px">indice</div>
      </div>"##;
    let (dom, list) = geometria(html, 1280.0);
    let conteudo = rect(&dom, &list, "#conteudo", 0);
    let lado = rect(&dom, &list, "#lado", 0);
    assert!(
        (conteudo.w - 752.0).abs() < 1.0,
        "coluna de conteúdo = {}",
        conteudo.w
    );
    assert!((lado.w - 196.0).abs() < 1.0, "barra lateral = {}", lado.w);
    // E a barra lateral fica DENTRO da grade, não empurrada para fora.
    assert!(
        (lado.x - (conteudo.x + conteudo.w + 24.0)).abs() < 1.0,
        "lado em x={}",
        lado.x
    );
}

/// Uma trilha limitada SOZINHA continua a poder chegar ao seu máximo — o que
/// muda é ela ceder quando há outra ao lado, não ela deixar de crescer.
#[test]
fn minmax_sozinho_cresce_ate_ao_maximo() {
    let html = r##"<div style="display:grid; width:1000px;
                     grid-template-columns:minmax(0,300px)">
        <div id="so">x</div></div>"##;
    let (dom, list) = geometria(html, 1280.0);
    assert!((rect(&dom, &list, "#so", 0).w - 300.0).abs() < 1.0);
}

/// Uma trilha `auto` é dimensionada pelo CONTEÚDO antes de o espaço livre ser
/// repartido — e não com zero, que era o que a fazia desaparecer ao lado de uma
/// trilha fixa.
#[test]
fn trilha_auto_e_dimensionada_pelo_conteudo() {
    let html = r##"<div style="display:grid; width:600px; gap:0;
                     grid-template-columns:auto 200px">
        <div id="a"><div style="width:150px">a</div></div>
        <div id="b">b</div></div>"##;
    let (dom, list) = geometria(html, 1280.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    // `a` fica com o que sobra (é a única trilha esticável): 600 - 200.
    assert!((a.w - 400.0).abs() < 1.0, "trilha auto = {}", a.w);
    assert!(
        (b.x - 400.0).abs() < 1.0,
        "a segunda coluna começa em {}",
        b.x
    );
}

/// Uma célula `white-space:nowrap` não tem folga: mínimo e máximo coincidem na
/// frase inteira, e por isso a coluna fica com ela exatamente enquanto todo o
/// espaço a repartir vai para a vizinha.
///
/// É a forma das navboxes da Wikipédia (`.navbox-group` é `nowrap`) e era o
/// defeito medido contra o Chrome real: sem esta regra a coluna de rótulos
/// declarava como mínimo a palavra mais larga, ganhava uma fatia da folga que
/// não lhe pertencia (231px onde o Chrome dá 123), e a coluna do conteúdo pagava
/// a diferença — texto a mais a quebrar, linhas a crescer, a tabela a inchar.
///
/// A tabela é DELIBERADAMENTE mais estreita do que o conteúdo quer: é o regime
/// de repartição por folga, o único em que a folga decide alguma coisa.
#[test]
fn coluna_nowrap_nao_tem_folga_e_a_vizinha_leva_o_espaco_todo() {
    // ApproxMeasurer: cada caractere mede metade do tamanho da fonte. "aa bb cc"
    // a 16px = 8 chars = 64px inteira, 16px na palavra mais larga.
    let html = r#"<table cellspacing="0" style="width:200px">
        <tr><th style="white-space:nowrap">aa bb cc</th><td>dddddddddd eeeeeeeeee ffffffffff gggggggggg</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);

    let rotulo = rect(&dom, &list, "th", 0);
    let conteudo = rect(&dom, &list, "td", 0);

    // 8 caracteres sem quebra: a largura sai da constante do medidor.
    let oito = 8.0 * 16.0 * crate::style::PROP_ADVANCE;
    assert!(
        (rotulo.w - oito).abs() < 1.0,
        "coluna nowrap = {}",
        rotulo.w
    );
    assert!(
        (rotulo.w + conteudo.w - 200.0).abs() < 1.0,
        "as duas colunas somam {} e nao 200",
        rotulo.w + conteudo.w
    );
}

/// Sem `nowrap` a mesma coluna ENCOLHE abaixo da frase, porque quebrar linhas é
/// o que a torna estreita e isso dá-lhe folga para ceder. O par com o teste
/// acima é o que prova que a diferença vem do `white-space` e não de outra coisa
/// na repartição.
#[test]
fn a_mesma_coluna_sem_nowrap_encolhe_abaixo_da_frase() {
    fn largura_do_rotulo(estilo: &str) -> f32 {
        let html = format!(
            r#"<table cellspacing="0" style="width:200px">
            <tr><th {estilo}>aa bb cc</th><td>dddddddddd eeeeeeeeee ffffffffff gggggggggg</td></tr>
        </table>"#
        );
        let (dom, list) = geometria(&html, 800.0);
        rect(&dom, &list, "th", 0).w
    }
    let com = largura_do_rotulo(r#"style="white-space:nowrap""#);
    let sem = largura_do_rotulo("");
    assert!(sem < com, "sem nowrap = {sem}, com nowrap = {com}");
}

/// Num rótulo `nowrap` os filhos inline ficam na MESMA linha, por isso o mínimo
/// da coluna é a SOMA deles e não o maior. Um `<th>` com dois `<a>` lado a lado é
/// a forma real, e tomar o máximo dava-lhe metade da largura de que precisa.
#[test]
fn nowrap_soma_os_filhos_inline_em_vez_de_tomar_o_maior() {
    let html = r#"<table cellspacing="0" style="width:200px">
        <tr><th style="white-space:nowrap"><a>aaaa</a><a>bbbb</a></th><td>dddddddddd eeeeeeeeee ffffffffff gggggggggg</td></tr>
    </table>"#;
    let (dom, list) = geometria(html, 800.0);

    // 4 caracteres cada; na mesma linha somam 8 — e o que o teste afirma é a
    // SOMA dos dois inline, não o avanço por carácter.
    let oito = 8.0 * 16.0 * crate::style::PROP_ADVANCE;
    let rotulo = rect(&dom, &list, "th", 0);
    assert!(
        (rotulo.w - oito).abs() < 1.0,
        "coluna nowrap com dois inline = {}",
        rotulo.w
    );
}
