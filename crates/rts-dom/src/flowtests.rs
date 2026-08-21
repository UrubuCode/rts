//! Os testes de comportamento do FLUXO DE BLOCO: o colapso de margens verticais
//! e a ordem de pintura de um elemento fora do fluxo.
//!
//! Vivem num módulo próprio e não dentro do `layout.rs` porque aquele ficheiro
//! passou há muito o teto e é editado por mais do que uma pessoa ao mesmo
//! tempo; e não dentro de `table/tests.rs` porque não têm nada de tabela — o
//! que partilham com ele são só os ajudantes, que montam um documento e leem a
//! geometria de um seletor.

use crate::layout::{DisplayItem, Rect};
use crate::table::tests::{geometria, rect};

/// As três regras do colapso de margens verticais (CSS 2.1 §8.3.1), pelo efeito
/// na posição do segundo bloco. A do meio é a que estava errada.
#[test]
fn o_colapso_de_margens_nao_e_o_maximo_quando_uma_delas_e_negativa() {
    let caso = |estilo: &str| -> f32 {
        let html = format!(
            "<div style='height:30px'>a</div><div style='height:30px; {estilo}'>b</div>"
        );
        let (dom, list) = geometria(&html, 600.0);
        rect(&dom, &list, "div", 1).y
    };
    // Duas positivas: a maior vence (0 e 10 → 10).
    assert!((caso("margin-top:10px") - 40.0).abs() < 0.5, "positiva: {}", caso("margin-top:10px"));
    // Sinais mistos: SOMAM. Uma margem negativa puxa o bloco para cima, e é o
    // `margin-top` negativo dos gutters `.row` do Bootstrap; com `max(a,b)` a
    // negativa era simplesmente ignorada e o bloco ficava em 30.
    assert!((caso("margin-top:-10px") - 20.0).abs() < 0.5, "negativa: {}", caso("margin-top:-10px"));
    // E uma negativa maior que a altura anterior puxa para cima na mesma.
    assert!((caso("margin-top:-40px") - (-10.0)).abs() < 0.5, "muito negativa: {}", caso("margin-top:-40px"));
}

/// Um elemento fora do fluxo (`position:fixed`) pinta DEPOIS do conteúdo, não
/// antes — senão fica atrás dele.
///
/// O sintoma numa página real é o dropdown a desaparecer por trás do artigo. A
/// causa não era a ordem em que ele é calculado (essa estava certa: a passada
/// out-of-flow corre no fim) mas o deslocamento dos índices: ao inserir o fundo
/// da sua própria caixa no índice 0 da lista de topo, o fixed empurrava para
/// depois de si a subárvore onde vive a página inteira.
#[test]
fn um_fixed_pinta_por_cima_do_fluxo_e_nao_por_baixo() {
    let html = "<div style='background:#111; height:30px'>fluxo</div>                <div style='position:fixed; top:0; left:0; width:50px; height:20px; background:#900'>t</div>";
    let (_, list) = geometria(html, 600.0);
    let ordem: Vec<Rect> = list
        .materialized()
        .iter()
        .filter_map(|i| match i {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    let ultimo = ordem.last().expect("algum fundo pintado");
    assert!((ultimo.w - 50.0).abs() < 0.5, "o último a pintar devia ser o fixed: {ordem:?}");
}

// ── FLEX: a distribuição de espaço ──────────────────────────────────────────

/// `flex-grow` reparte a SOBRA na proporção dos pesos — não as larguras finais
/// nessa proporção. Um item com peso 2 leva o dobro DO QUE SOBRA, e não fica com
/// o dobro da largura do irmão.
#[test]
fn o_grow_reparte_a_sobra_na_proporcao_dos_pesos() {
    let (d, l) = geometria(
        "<div style='display:flex; width:300px'>           <div id='a' style='flex-basis:0'>a</div>           <div id='b' style='flex-basis:0; flex-grow:2'>b</div></div>",
        800.0,
    );
    let (a, b) = (rect(&d, &l, "#a", 0).w, rect(&d, &l, "#b", 0).w);
    // Sem `flex-grow` declarado o default é 0, portanto `a` não cresce.
    assert!(a < 1.0, "sem grow, `a` não devia crescer: {a}");
    assert!((b - 300.0).abs() < 0.5, "`b` devia levar a sobra toda: {b}");
}

/// Com bases zeradas e pesos 1 e 2, a sobra reparte-se em 1/3 e 2/3.
#[test]
fn dois_pesos_de_grow_repartem_um_terco_e_dois_tercos() {
    let (d, l) = geometria(
        "<div style='display:flex; width:300px'>           <div id='a' style='flex-basis:0; flex-grow:1'>a</div>           <div id='b' style='flex-basis:0; flex-grow:2'>b</div></div>",
        800.0,
    );
    assert!((rect(&d, &l, "#a", 0).w - 100.0).abs() < 0.5);
    assert!((rect(&d, &l, "#b", 0).w - 200.0).abs() < 0.5);
}

/// `flex-shrink` retira o excesso ponderado pela BASE, e não só pelo peso — é a
/// parte que um algoritmo ingénuo erra. Duas bases de 200 e 400 num container de
/// 300 cedem 100 e 200, não 150 cada.
#[test]
fn o_shrink_e_ponderado_pela_base_e_nao_so_pelo_peso() {
    let (d, l) = geometria(
        "<div style='display:flex; width:300px'>           <div id='a' style='width:200px'>a</div>           <div id='b' style='width:400px'>b</div></div>",
        800.0,
    );
    assert!((rect(&d, &l, "#a", 0).w - 100.0).abs() < 0.5, "a={}", rect(&d, &l, "#a", 0).w);
    assert!((rect(&d, &l, "#b", 0).w - 200.0).abs() < 0.5, "b={}", rect(&d, &l, "#b", 0).w);
}

/// `flex-shrink: 0` não encolhe, e o irmão absorve o excesso todo.
#[test]
fn um_item_com_shrink_zero_nao_encolhe_e_o_irmao_absorve_tudo() {
    let (d, l) = geometria(
        "<div style='display:flex; width:300px'>           <div id='a' style='width:200px; flex-shrink:0'>a</div>           <div id='b' style='width:400px'>b</div></div>",
        800.0,
    );
    assert!((rect(&d, &l, "#a", 0).w - 200.0).abs() < 0.5, "shrink:0 encolheu");
    assert!((rect(&d, &l, "#b", 0).w - 100.0).abs() < 0.5);
}

/// Uma `flex-basis` declarada vence a largura do CONTEÚDO — é a base, e o
/// conteúdo só decide quando ela é `auto`.
#[test]
fn a_flex_basis_declarada_vence_a_largura_do_conteudo() {
    let (d, l) = geometria(
        "<div style='display:flex; width:300px'>           <div id='a' style='flex-basis:100px'>um texto bastante mais comprido que cem pixels</div>           <div id='b' style='flex-grow:1'>b</div></div>",
        800.0,
    );
    assert!((rect(&d, &l, "#a", 0).w - 100.0).abs() < 0.5, "a basis foi ignorada: {}", rect(&d, &l, "#a", 0).w);
}

/// Um item flex cujo CONTEÚDO declara `width:%` não passa a pedir a linha toda.
///
/// É o defeito que punha o `<h1>` da Wikipédia 120px abaixo do sítio. A
/// percentagem é contra o item, que é o que a medição está a decidir; resolvida
/// contra a VIEWPORT dava 50% de 1280 dentro de uma caixa de 220, a base do
/// primeiro item enchia a linha, o irmão com `flex-grow` quebrava para a linha
/// seguinte e o container ficava com o dobro da altura. O mesmo defeito, no
/// mesmo dia, tinha aparecido numa célula de tabela a exigir 1280px de mínimo.
#[test]
fn um_filho_com_width_percentual_nao_faz_o_item_flex_encher_a_linha() {
    let com = "<div style='display:flex; flex-wrap:wrap; width:600px'>                 <div id='a'><div style='width:50%'>x</div></div>                 <div id='b' style='flex-grow:1'>b</div></div>";
    let sem = "<div style='display:flex; flex-wrap:wrap; width:600px'>                 <div id='a'><div>x</div></div>                 <div id='b' style='flex-grow:1'>b</div></div>";
    let (d1, l1) = geometria(com, 1280.0);
    let (d2, l2) = geometria(sem, 1280.0);
    let (a1, b1) = (rect(&d1, &l1, "#a", 0), rect(&d1, &l1, "#b", 0));
    let (a2, b2) = (rect(&d2, &l2, "#a", 0), rect(&d2, &l2, "#b", 0));
    // A percentagem no filho não pode mudar NADA da disposição dos dois itens.
    assert!((a1.w - a2.w).abs() < 0.5, "a com % = {} contra {}", a1.w, a2.w);
    // E os dois ficam na MESMA linha — o sintoma era o segundo descer.
    assert!((b1.y - a1.y).abs() < 0.5, "o irmão quebrou para a linha de baixo: y={}", b1.y);
    assert!((b1.w - b2.w).abs() < 0.5, "b com % = {} contra {}", b1.w, b2.w);
}

// ── INLINE: a caixa é a UNIÃO dos fragmentos de linha ───────────────────────
//
// O que estes fixam é a definição do `getBoundingClientRect` de um elemento
// `display:inline`: a bounding box de TODOS os seus fragmentos de linha, e não
// a caixa da linha nem um fragmento só.
//
// Existem porque a união foi acusada de ser a maior fonte de erro de largura da
// página (9 865 inlines fora de 1px, 500 962 px), e a acusação não se sustentou:
// 89% desse erro (446 342 px) está em inlines cujo NÚMERO DE LINHAS difere do
// do Chrome, onde a união é aritmética correta sobre quebras diferentes. Estes
// testes são o que impede que uma tentativa de mexer no número mexa na
// definição.
//
// As medidas são as do `ApproxMeasurer` (n * tamanho * 0,5): a 16px cada
// carácter mede 8px e a linha 18px.

/// Um inline que cabe numa linha mede o SEU texto — não a largura da linha nem
/// a do bloco que o contém. É a cara mais simples da regra e a que o relatório
/// de paridade dizia estar partida.
#[test]
fn um_inline_de_uma_linha_mede_o_seu_texto_e_nao_a_linha() {
    let (d, l) = geometria("<p style='width:400px'>antes <a id='t'>alvo</a> depois</p>", 800.0);
    let p = rect(&d, &l, "p", 0);
    let t = rect(&d, &l, "#t", 0);
    assert!((p.w - 400.0).abs() < 0.5, "o bloco é o bloco: {}", p.w);
    // "alvo" = 4 caracteres. A largura vem da CONSTANTE e não de um número
    // escrito à mão: o que este teste afirma é que o inline mede o seu texto,
    // não quanto mede um carácter — e escrever 32 aqui fazia-o falhar sempre
    // que o avanço fosse recalibrado, por uma razão que não é a dele.
    let ch = 16.0 * crate::style::PROP_ADVANCE;
    assert!((t.w - 4.0 * ch).abs() < 0.5, "o inline devia medir o seu texto, mediu {}", t.w);
    // e começa depois de "antes " (6 caracteres), não no início da linha.
    assert!((t.x - 6.0 * ch).abs() < 0.5, "x do inline: {}", t.x);
}

/// Um inline que quebra em três linhas tem UMA caixa que as contém às três:
/// larga (chega às duas margens) e alta (três linhas). É deliberadamente maior
/// do que qualquer um dos fragmentos — é o que o browser devolve.
#[test]
fn um_inline_que_quebra_em_tres_linhas_da_a_uniao_larga_e_alta() {
    // A largura vem do NUMERO DE LINHAS que se quer, e o numero de caracteres
    // que cabem numa linha depende da calibracao do medidor: com 150px o texto
    // da tres linhas com folga dos dois lados de PROP_ADVANCE, em vez de ficar
    // no limiar entre duas e tres.
    let html = "<p style='width:150px'><a id='t'>aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn</a></p>";
    let (d, l) = geometria(html, 800.0);
    let t = rect(&d, &l, "#t", 0);
    // Três linhas de 18px: a união vai do topo da primeira ao fundo da terceira.
    assert!((t.h - 54.0).abs() < 0.5, "a união devia cobrir as três linhas, tem {}", t.h);
    // E é larga: as linhas cheias chegam perto do limite dos 200px.
    assert!(t.w > 110.0, "a união devia ser larga, tem {}", t.w);
    assert!(t.w <= 200.5, "a união não pode passar o bloco: {}", t.w);
    // Guardar UM fragmento só daria a altura de uma linha — era a segunda cara
    // da acusação, e é isto que a recusa.
    assert!(t.h > 18.5, "guardou um fragmento só: {}", t.h);
}

/// Um inline dentro de outro tem caixa PRÓPRIA, contida na do pai e mais
/// estreita — o pai não lhe empresta a sua largura nem lhe rouba a posição.
#[test]
fn um_inline_dentro_de_outro_tem_a_sua_propria_caixa_mais_estreita() {
    let (d, l) = geometria(
        "<p style='width:400px'>antes <a id='t'>al<span id='s'>vo</span></a> depois</p>",
        800.0,
    );
    let t = rect(&d, &l, "#t", 0);
    let s = rect(&d, &l, "#s", 0);
    let ch = 16.0 * crate::style::PROP_ADVANCE;
    assert!((t.w - 4.0 * ch).abs() < 0.5, "o <a> mede 'alvo': {}", t.w);
    assert!((s.w - 2.0 * ch).abs() < 0.5, "o <span> mede só 'vo': {}", s.w);
    // contido: começa depois do pai e acaba com ele.
    assert!(s.x >= t.x - 0.5 && s.x + s.w <= t.x + t.w + 0.5, "o filho saiu do pai: {s:?} em {t:?}");
    assert!(s.x > t.x + 0.5, "o filho devia começar depois de 'al': {}", s.x);
}

/// Um inline com uma caixa ATÓMICA dentro (um `<img>`) mede a LARGURA que ela
/// ocupa na linha, mas fica com a altura da FONTE — não com a da imagem.
///
/// É a regra que o Chrome também aplica: nesta página, um `<a>` à volta de um
/// `<span>` `inline-block` de 26px de altura reporta 17px, a content area da
/// sua própria fonte. Confundir as duas era somar a altura de cada replaced à
/// caixa do link que o embrulha.
#[test]
fn um_inline_com_caixa_atomica_dentro_leva_a_largura_dela_mas_nao_a_altura() {
    let (d, l) = geometria(
        "<p style='width:400px'><a id='t'>x<img width='40' height='40'>y</a></p>",
        800.0,
    );
    let t = rect(&d, &l, "#t", 0);
    // "x" + 40 da imagem + "y": o que se afirma é que a imagem CONTA, e por
    // isso os dois caracteres vêm da constante e o 40 é o número do teste.
    let ch = 16.0 * crate::style::PROP_ADVANCE;
    assert!((t.w - (40.0 + 2.0 * ch)).abs() < 0.5, "a largura devia incluir a imagem: {}", t.w);
    // A altura é a da fonte (18), não a da imagem (40).
    assert!((t.h - 18.0).abs() < 0.5, "levou a altura da imagem em vez da fonte: {}", t.h);
}
