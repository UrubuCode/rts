//! Os testes do CONTAINER FLEX medido em isolamento: o que conta como item e o
//! que conta para a largura intrínseca da linha.
//!
//! Vivem num ficheiro próprio pela mesma razão que os de `flowtests.rs`: o
//! `layout.rs` passou há muito o teto e é editado por mais do que uma pessoa ao
//! mesmo tempo.
//!
//! O que fixam foi diagnosticado sobre o `<header>` da Wikipédia, onde os dois
//! filhos que o Chrome põe LADO A LADO ficavam empilhados — e o custo eram 48 px
//! herdados por 16 mil descendentes. Os testes não falam da página: um container
//! com filhos de largura conhecida chega para provar o mecanismo, e uma asserção
//! sobre a página real não dizia qual dos dois defeitos tinha voltado.

use crate::table::tests::{geometria, rect};

/// Um filho `display:none` NÃO é item de flex: não ocupa lugar no eixo
/// principal e não consome um `gap`.
///
/// Antes ele recebia caixa zero (portanto invisível) mas continuava a contar
/// para a soma que decide o wrap e para o número de gaps — um menu escondido
/// empurrava os irmãos visíveis.
#[test]
fn filho_display_none_nao_ocupa_lugar_nem_gap_no_flex() {
    let html = "<div style='display:flex; gap:10px; width:400px'>\
                  <div id='a' style='width:50px;height:20px'></div>\
                  <div style='display:none; width:300px;height:20px'></div>\
                  <div id='b' style='width:50px;height:20px'></div>\
                </div>";
    let (dom, list) = geometria(html, 800.0);
    let a = rect(&dom, &list, "#a", 0);
    let b = rect(&dom, &list, "#b", 0);
    // 50 (o `a`) + 10 (UM gap, não dois) → o `b` começa em 60.
    assert!(
        (b.x - a.x - 60.0).abs() < 0.5,
        "o escondido não ocupa lugar nem gap: a={a:?} b={b:?}"
    );
    assert!((a.y - b.y).abs() < 0.5, "mesma linha: a={a:?} b={b:?}");
}

/// Um `display:none` também não conta para a largura INTRÍNSECA do container —
/// que é o número que o `flex-wrap` do pai usa para decidir a quebra.
///
/// Este é o teste que apanha o defeito do cabeçalho: o pai encolhe ao conteúdo,
/// e se o filho escondido contasse, o pai media 300 px em vez de 50.
#[test]
fn display_none_nao_entra_na_largura_intrinseca_do_flex() {
    let html = "<div style='display:flex; width:600px'>\
                  <div id='caixa' style='display:flex; gap:10px'>\
                    <div style='width:50px;height:20px'></div>\
                    <div style='display:none; width:300px;height:20px'></div>\
                  </div>\
                </div>";
    let (dom, list) = geometria(html, 800.0);
    let caixa = rect(&dom, &list, "#caixa", 0);
    assert!(
        (caixa.w - 50.0).abs() < 0.5,
        "shrink-to-fit ignora o escondido: {caixa:?}"
    );
}

/// O espaço em branco ENTRE dois itens de flex não é um item.
///
/// O pré-passo do flex já o descartava (`trim().is_empty()`), mas a medição
/// intrínseca contava-o duas vezes: a largura do `"\n\t\t"` e mais um `gap` por
/// haver um item a mais. Duas respostas para a mesma pergunta, que é a forma
/// que este defeito toma sempre nesta casa.
#[test]
fn espacos_entre_itens_nao_incham_a_largura_do_flex() {
    // O mesmo container escrito em duas linhas e escrito colado tem de medir o
    // mesmo — a indentação do HTML não é conteúdo.
    let colado = "<div style='display:flex; width:600px'>\
                    <div id='c' style='display:flex; gap:20px'>\
<div style='width:60px;height:20px'></div><div style='width:60px;height:20px'></div>\
                    </div>\
                  </div>";
    let indentado = "<div style='display:flex; width:600px'>\n\
                       <div id='c' style='display:flex; gap:20px'>\n\
                         <div style='width:60px;height:20px'></div>\n\
                         <div style='width:60px;height:20px'></div>\n\
                       </div>\n\
                     </div>";
    let (d1, l1) = geometria(colado, 800.0);
    let (d2, l2) = geometria(indentado, 800.0);
    let a = rect(&d1, &l1, "#c", 0);
    let b = rect(&d2, &l2, "#c", 0);
    assert!((a.w - 140.0).abs() < 0.5, "60 + 20 + 60: {a:?}");
    assert!(
        (a.w - b.w).abs() < 0.5,
        "a indentação do HTML não muda a largura: colado={a:?} indentado={b:?}"
    );
}

/// `flex-wrap:wrap` só quebra quando os itens REALMENTE não cabem. Dois itens
/// cuja soma cabe ficam na mesma linha, mesmo com o HTML indentado e com um
/// irmão escondido pelo meio — que é exatamente a forma do `<header>` da
/// Wikipédia.
#[test]
fn wrap_nao_quebra_quando_os_itens_cabem() {
    let html = "<div id='h' style='display:flex; flex-wrap:wrap; gap:16px; width:600px'>\n\
                  <div id='esq' style='display:flex; gap:20px'>\n\
                    <div style='width:100px;height:50px'></div>\n\
                    <div style='display:none; width:400px;height:50px'></div>\n\
                  </div>\n\
                  <div id='dir' style='display:flex; flex-grow:1'>\n\
                    <div style='width:200px;height:32px'></div>\n\
                  </div>\n\
                </div>";
    let (dom, list) = geometria(html, 800.0);
    let esq = rect(&dom, &list, "#esq", 0);
    let dir = rect(&dom, &list, "#dir", 0);
    assert!((esq.w - 100.0).abs() < 0.5, "esquerda mede o visível: {esq:?}");
    assert!(
        dir.x > esq.x + esq.w - 0.5,
        "lado a lado, não empilhados: esq={esq:?} dir={dir:?}"
    );
    // `flex-grow:1` no segundo → ele come o resto: 600 − 100 − 16 = 484.
    assert!((dir.w - 484.0).abs() < 0.5, "o grow enche a linha: {dir:?}");
}
