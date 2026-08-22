//! COLAPSO DE MARGENS — os sete casos que a auditoria do modelo de caixa
//! levanta, escritos ANTES de qualquer correção.
//!
//! **Os números não vêm da spec: vêm de um Chrome real.** A auditoria
//! (`scripts/parity/calculos/caixa.jsonl`) foi derivada de LER o Blink e o
//! nosso código, e um registo derivado de leitura é uma hipótese. A página
//! `colapso.html` que estes testes reproduzem foi medida por
//! `scripts/parity/chrome_extract.mjs` — o mesmo extrator CDP da régua de
//! paridade, viewport 1280x800, JS da página desligado — e cada valor abaixo é
//! o que o Chrome respondeu ao `getBoundingClientRect`.
//!
//! Cada caso vive dentro de um `.caso` com `border:1px` porque a borda é o que
//! IMPEDE o colapso de atravessar para fora dele: sem ela um caso contaminava
//! o seguinte, e os sete deixavam de ser sete medições independentes.
//!
//! E as asserções são todas RELATIVAS à caixa do seu próprio caso. Um caso que
//! erre a altura desloca em `y` tudo o que vem abaixo, e com valores absolutos
//! um único defeito pintaria sete testes de vermelho — o que esconde quais são
//! os defeitos e quais são o eco do primeiro.

use super::*;
use crate::table::tests::{geometria, rect};

/// A página medida no Chrome. Os sete casos numa página só, porque foi assim
/// que foram medidos: uma página por caso teria sete medições que ninguém
/// confirmou serem a mesma coisa.
const PAGINA: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;padding:0}
  body{width:1000px}
  .caso{border:1px solid #000;margin:0;padding:0}
  .b{height:20px;background:#8ab}
</style></head><body>

<div class="caso" id="c1">
  <div class="b" id="c1a" style="margin-bottom:30px"></div>
  <div class="b" id="c1b" style="margin-top:10px"></div>
</div>

<div class="caso" id="c2">
  <div class="b" id="c2a" style="margin-bottom:10px"></div>
  <div id="c2e" style="margin-top:-5px"></div>
  <div class="b" id="c2b" style="margin-top:20px"></div>
</div>

<div class="caso" id="c3">
  <div id="c3p"><div class="b" id="c3f" style="margin-top:40px"></div></div>
  <div class="b" id="c3n"></div>
</div>

<div class="caso" id="c4">
  <div id="c4p"><div class="b" id="c4f" style="margin-bottom:40px"></div></div>
  <div class="b" id="c4n" style="margin-top:10px"></div>
</div>

<div class="caso" id="c5">
  <div class="b" id="c5a"></div>
  <div id="c5e" style="margin-top:20px;margin-bottom:30px"></div>
  <div class="b" id="c5b"></div>
</div>

<div class="caso" id="c6">
  <div id="c6p" style="overflow:hidden"><div class="b" id="c6f" style="margin-top:40px"></div></div>
  <div class="b" id="c6n"></div>
</div>

<div class="caso" id="c7">
  <div id="c7fl" style="float:left;width:50px;height:60px;background:#c88"></div>
  <div class="b" id="c7a" style="margin-bottom:30px"></div>
  <div class="b" id="c7c" style="clear:both;margin-top:10px"></div>
</div>

<div class="caso" id="c8">
  <div class="b" id="c8a" style="margin-bottom:25px"></div>
  <div id="c8e" style="margin-top:20px;margin-bottom:30px"></div>
  <div class="b" id="c8b"></div>
</div>

<div class="caso" id="b1"><div id="b1p" style="padding-top:1px"><div class="b" id="b1f" style="margin-top:40px"></div></div><div class="b" id="b1n"></div></div>
<div class="caso" id="b2"><div id="b2p" style="float:left;width:200px"><div class="b" id="b2f" style="margin-top:40px"></div></div><div class="b" id="b2n" style="clear:both"></div></div>
<div class="caso" id="b3"><div id="b3p" style="display:flex;flex-direction:column"><div class="b" id="b3f" style="margin-top:40px"></div></div><div class="b" id="b3n"></div></div>
<div class="caso" id="b4"><div id="b4p" style="display:inline-block;width:200px"><div class="b" id="b4f" style="margin-top:40px"></div></div></div>
<div class="caso" id="b5"><div id="b5p" style="display:flow-root"><div class="b" id="b5f" style="margin-top:40px"></div></div><div class="b" id="b5n"></div></div>
<div class="caso" id="b6"><div id="b6p" style="border-bottom:1px solid #000"><div class="b" id="b6f" style="margin-top:40px;margin-bottom:40px"></div></div><div class="b" id="b6n"></div></div>
<div class="caso" id="b7"><div id="b7p"><div class="b" id="b7f" style="margin-top:40px;margin-bottom:40px"></div></div><div class="b" id="b7n" style="margin-top:10px"></div></div>

</body></html>"#;

/// Um pixel de tolerância — a mesma da régua de paridade. Estes casos são
/// todos de altura declarada e sem texto, portanto nada aqui depende da
/// métrica de fonte aproximada; a tolerância existe para o arredondamento e
/// não para dar folga a um defeito.
const TOL: f32 = 1.0;

/// O `y` e a altura de um id, já RELATIVOS ao topo da caixa do caso que o
/// contém — ver a nota no cabeçalho sobre porque nenhuma asserção é absoluta.
fn rel(caso: &str, id: &str) -> (f32, f32) {
    let (dom, list) = geometria(PAGINA, 1280.0);
    let base = rect(&dom, &list, &format!("#{caso}"), 0);
    let r = rect(&dom, &list, &format!("#{id}"), 0);
    (r.y - base.y, r.h)
}

/// A altura da caixa de um caso.
fn altura_do_caso(caso: &str) -> f32 {
    let (dom, list) = geometria(PAGINA, 1280.0);
    rect(&dom, &list, &format!("#{caso}"), 0).h
}

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOL
}

/// CSS 2.1 §8.3.1: o que colapsa entre dois irmãos é a margem de BAIXO do
/// anterior com a de CIMA do seguinte — dois valores distintos.
///
/// Chrome: `#c1a` fica em y=1 com 20 de altura e `#c1b` em y=51, ou seja
/// **30 de intervalo** = max(30, 10). Comparar a margem de cima do anterior
/// (que é 0) com a de cima do seguinte dá 40, e é o que a auditoria regista
/// como `caixa.margem.colapso-usa-margem-de-cima-do-anterior`.
#[test]
fn o_colapso_entre_irmaos_usa_a_margem_de_baixo_do_anterior() {
    let (ya, ha) = rel("c1", "c1a");
    let (yb, _) = rel("c1", "c1b");
    let intervalo = yb - (ya + ha);
    assert!(
        perto(intervalo, 30.0),
        "intervalo entre irmãos = {intervalo} (Chrome: 30 = max(30,10))"
    );
    assert!(perto(altura_do_caso("c1"), 72.0));
}

/// CSS 2.1 §8.3.1: N margens adjacentes colapsam DE UMA VEZ — o maior dos
/// positivos mais o menor dos negativos — e não duas a duas.
///
/// Chrome: 15 de intervalo, que é `max(10, 20) + min(-5) = 20 - 5`. Par a par
/// daria `colapso(colapso(10, -5), 20) = colapso(5, 20) = 20`.
///
/// **Este caso não é independente do bloco vazio**: as três margens só ficam
/// adjacentes porque o `#c2e` não tem altura, borda nem padding e portanto se
/// atravessa a si próprio. Fica dito porque um vermelho aqui pode ser deste
/// mecanismo ou daquele, e a auditoria trata-os como dois registos.
#[test]
fn margens_adjacentes_colapsam_por_conjunto_e_nao_par_a_par() {
    let (ya, ha) = rel("c2", "c2a");
    let (yb, _) = rel("c2", "c2b");
    let intervalo = yb - (ya + ha);
    assert!(
        perto(intervalo, 15.0),
        "intervalo = {intervalo} (Chrome: 15 pelo conjunto; 20 par a par)"
    );
}

/// CSS 2.1 §8.3.1: sem borda nem padding no topo, a margem de cima do primeiro
/// filho ATRAVESSA o pai — o pai desce, e não cresce.
///
/// Chrome: `#c3p` fica 41 abaixo do topo do caso (1 de borda + os 40 que lhe
/// vieram do filho) e tem **20 de altura**, não 60. O filho fica no mesmo `y`
/// que o pai.
///
/// Note-se o que este caso NÃO mostra: o irmão seguinte fica em 61 dos dois
/// modos, porque a borda do `.caso` impede a margem de sair. **A divergência
/// vive inteira na caixa do pai** — que é precisamente o que a régua de
/// paridade compara, elemento a elemento.
#[test]
#[ignore = "Chrome pai h=20 y=41, nos h=60 y=1 - a margem nunca atravessa o pai. Lote D"]
fn a_margem_do_primeiro_filho_atravessa_o_pai_sem_borda() {
    let (yp, hp) = rel("c3", "c3p");
    let (yf, _) = rel("c3", "c3f");
    assert!(perto(hp, 20.0), "altura do pai = {hp} (Chrome: 20)");
    assert!(perto(yp, 41.0), "y do pai = {yp} (Chrome: 41)");
    assert!(perto(yf - yp, 0.0), "o filho devia ficar no y do pai");
}

/// CSS 2.1 §8.3.1: a margem de baixo do último filho sai no fragmento em vez
/// de entrar na altura do pai de altura `auto`.
///
/// Chrome: `#c4p` tem **20 de altura** (não 60) e o irmão seguinte fica a
/// **40 de intervalo** — `max(40, 10)`, as duas margens colapsadas uma vez.
///
/// É o caso que a auditoria descreve como a soma a DOBRAR: o pai cresce pela
/// margem do filho e ainda a soma outra vez ao colapsar com o irmão.
#[test]
#[ignore = "Chrome pai h=20 e 40 de intervalo, nos h=60 e 50 - margem contada duas vezes. Lote D"]
fn a_margem_do_ultimo_filho_atravessa_o_pai_de_altura_auto() {
    let (yp, hp) = rel("c4", "c4p");
    let (yn, _) = rel("c4", "c4n");
    assert!(perto(hp, 20.0), "altura do pai = {hp} (Chrome: 20)");
    let intervalo = yn - (yp + hp);
    assert!(
        perto(intervalo, 40.0),
        "intervalo = {intervalo} (Chrome: 40 = max(40,10))"
    );
}

/// CSS 2.1 §8.3.1: um bloco sem altura, borda ou padding colapsa a própria
/// margem de cima com a de baixo e não ocupa espaço nenhum.
///
/// Chrome: `#c5e` tem **altura 0** e o que ele injeta entre os dois vizinhos
/// são **30** — `max(20, 30)` — e não `20 + 30`.
#[test]
fn um_bloco_vazio_colapsa_a_propria_margem_sobre_si() {
    let (_, he) = rel("c5", "c5e");
    assert!(perto(he, 0.0), "altura do bloco vazio = {he} (Chrome: 0)");
    let (ya, ha) = rel("c5", "c5a");
    let (yb, _) = rel("c5", "c5b");
    let intervalo = yb - (ya + ha);
    assert!(
        perto(intervalo, 30.0),
        "intervalo = {intervalo} (Chrome: 30 = max(20,30), não 50)"
    );
}

/// CSS 2.1 §9.4.1: um novo contexto de formatação de bloco BARRA o colapso —
/// a margem do filho não atravessa um pai com `overflow:hidden`.
///
/// Chrome: `#c6p` tem **60 de altura** e o filho fica 40 abaixo dele.
///
/// **Este teste passa hoje pela razão errada**, e isso está dito de propósito:
/// não há colapso nenhum no motor, portanto nada há para barrar. Ele existe
/// como GUARDA — no dia em que o colapso pai/filho entrar, é este que diz se
/// entrou a atravessar o que não devia.
#[test]
fn overflow_hidden_barra_o_colapso_atraves_do_pai() {
    let (yp, hp) = rel("c6", "c6p");
    let (yf, _) = rel("c6", "c6f");
    assert!(perto(hp, 60.0), "altura do pai BFC = {hp} (Chrome: 60)");
    assert!(
        perto(yf - yp, 40.0),
        "o filho devia ficar 40 abaixo do pai, não colapsar através dele"
    );
}

/// CSS 2.1 §9.5.2: um bloco que desceu por `clear` deixa de ser adjacente ao
/// anterior, e a margem deste já não tem nada com que colapsar.
///
/// Chrome: `#c7c` fica em 61 — exatamente o fundo do float — e não em 51, que
/// é onde ficaria se os 30 de margem do anterior ainda contassem.
///
/// **A auditoria erra o SENTIDO deste, e foi a medição que o disse.** O
/// registo `caixa.margem.barreira-clearance` prevê o bloco puxado para CIMA do
/// fundo do float pelo desconto de uma margem que já não é adjacente. Medido,
/// o motor põe-no 10 px ABAIXO (71 contra 61): o `clear` desce o cursor e a
/// margem de cima é somada por cima da descida, quando a clearance devia
/// absorvê-la. O defeito é real e o mecanismo previsto não é o que acontece —
/// o desconto errado ainda não morde aqui porque a margem guardada do anterior
/// é a de CIMA, que é zero. Corrigir o colapso entre irmãos põe lá 30 e passa
/// a haver as duas coisas ao mesmo tempo.
#[test]
fn um_bloco_com_clear_nao_desconta_a_margem_de_quem_ja_nao_e_adjacente() {
    let (yfl, hfl) = rel("c7", "c7fl");
    let (yc, _) = rel("c7", "c7c");
    assert!(
        perto(yc, yfl + hfl),
        "bloco com clear em {yc}; devia estar no fundo do float ({})",
        yfl + hfl
    );
}

/// O bloco vazio com um vizinho de margem MAIOR: as três margens (25 de baixo do
/// anterior, 20 e 30 do vazio) são adjacentes e colapsam como um CONJUNTO.
///
/// Chrome: 30 de intervalo — `max(25, 20, 30)`.
///
/// Este caso nasceu de uma limitação que eu tinha por aritmética e fui MEDIR: o
/// lote do bloco vazio, sozinho, acerta apenas enquanto a margem de baixo do
/// anterior não exceder as do vazio, porque o laço guarda UM valor e um conjunto
/// não cabe num valor. É a mesma falta que o caso do conjunto nomeia, apanhada
/// noutra forma — e é a guarda do lote dos acumuladores.
#[test]
fn o_bloco_vazio_colapsa_tambem_com_a_margem_do_vizinho_anterior() {
    let (ya, ha) = rel("c8", "c8a");
    let (yb, _) = rel("c8", "c8b");
    let intervalo = yb - (ya + ha);
    assert!(
        perto(intervalo, 30.0),
        "intervalo = {intervalo} (Chrome: 30 = max(25,20,30))"
    );
}

// ── AS BARREIRAS AO COLAPSO PAI/FILHO ───────────────────────────────────────
//
// Sete casos medidos no mesmo dump que os oito de cima, e existem por uma razão
// que o caso do `overflow:hidden` sozinho não consegue dar: ele diz que HÁ uma
// barreira e não diz que há DUAS, uma por lado. O `b6` é o que discrimina —
// uma borda só em baixo deixa a margem de cima escapar e prende a de baixo.
//
// Um lote que tratasse a barreira como uma decisão da caixa inteira passava no
// caso do `overflow` e falhava a página. É a diferença entre um caso que
// confirma e um caso que pode desmentir.

/// Um `padding-top` de um único pixel já barra: a margem do filho não atravessa
/// e o pai cresce em vez de descer.
///
/// Chrome: pai em y=1 com 61 de altura, filho em 42.
#[test]
fn um_padding_de_um_pixel_barra_o_colapso_do_primeiro_filho() {
    let (yp, hp) = rel("b1", "b1p");
    let (yf, _) = rel("b1", "b1f");
    assert!(perto(yp, 1.0) && perto(hp, 61.0), "pai y={yp} h={hp} (Chrome: 1 / 61)");
    assert!(perto(yf, 42.0), "filho y={yf} (Chrome: 42)");
}

/// Um float estabelece contexto de formatação próprio: nada colapsa através dele.
///
/// Chrome: pai em y=1 com 60 de altura.
#[test]
fn um_float_barra_o_colapso_atraves_do_pai() {
    let (yp, hp) = rel("b2", "b2p");
    assert!(perto(yp, 1.0) && perto(hp, 60.0), "pai y={yp} h={hp} (Chrome: 1 / 60)");
}

/// Um pai flex não tem colapso de margens de todo — os filhos são itens de flex.
///
/// Chrome: pai em y=1 com 60 de altura.
///
/// Passa de graça e continuará a passar: `layout_children_vertical` só é
/// chamada no ramo vertical, portanto um pai flex nunca chega ao código que o
/// lote do pai/filho vai mexer. Fica como guarda dessa afirmação.
#[test]
fn um_pai_flex_nao_colapsa_com_os_filhos() {
    let (yp, hp) = rel("b3", "b3p");
    assert!(perto(yp, 1.0) && perto(hp, 60.0), "pai y={yp} h={hp} (Chrome: 1 / 60)");
}

/// Um `inline-block` estabelece contexto próprio.
///
/// Chrome: pai em y=1 com 60 de altura.
#[test]
fn um_inline_block_barra_o_colapso_atraves_do_pai() {
    let (yp, hp) = rel("b4", "b4p");
    assert!(perto(yp, 1.0) && perto(hp, 60.0), "pai y={yp} h={hp} (Chrome: 1 / 60)");
}

/// `display:flow-root` existe SÓ para estabelecer um contexto de formatação —
/// é a única coisa que a palavra significa.
///
/// Chrome: pai em y=1 com 60 de altura.
///
/// **Passa hoje por VACUIDADE, e o comentário anterior creditava isso a uma
/// dependência que não existe.** Dizia que passaria quando a linha do
/// `style/parse` entrasse; a linha entrou (`27af61c4`) e o teste **já estava
/// verde antes dela**. Passa porque nada escapa do pai, portanto não há nada
/// para barrar — e não porque saibamos distinguir um `flow-root`.
///
/// O que a correcção do `style/` mudou é outra coisa, e é o que serve ao lote
/// do pai/filho: a distinção NÃO está no `DisplayKind`, que continua a mapear
/// `block` e `flow-root` para o mesmo valor, e passou a viver num campo
/// próprio, `ComputedStyle::flow_root`. **É esse campo que a barreira tem de
/// ler**, e é por isso que este teste passa de vacuidade a portão no dia em que
/// a margem começar a escapar.
#[test]
fn flow_root_barra_o_colapso_atraves_do_pai() {
    let (yp, hp) = rel("b5", "b5p");
    assert!(perto(yp, 1.0) && perto(hp, 60.0), "pai y={yp} h={hp} (Chrome: 1 / 60)");
}

/// **O caso que discrimina: uma borda só em baixo barra só em baixo.**
///
/// Chrome: o pai DESCE para y=41 — a margem de cima do filho atravessou-o — e
/// fica com 61 de altura, que são os 20 do filho mais os 40 da margem de baixo
/// que a borda prendeu, mais o pixel da borda. O irmão seguinte fica em 102.
///
/// Um lote que decidisse a barreira pela caixa e não pelo lado passaria em
/// todos os outros seis e falharia neste.
#[test]
#[ignore = "Chrome pai y=41 h=61 e seguinte 102, nos y=1 - a margem de cima nao escapa. Lote D"]
fn uma_borda_so_em_baixo_barra_so_a_margem_de_baixo() {
    let (yp, hp) = rel("b6", "b6p");
    let (yn, _) = rel("b6", "b6n");
    assert!(perto(yp, 41.0), "pai y={yp} (Chrome: 41 — a margem de cima escapou)");
    assert!(perto(hp, 61.0), "pai h={hp} (Chrome: 61 — a de baixo ficou presa)");
    assert!(perto(yn, 102.0), "seguinte y={yn} (Chrome: 102)");
}

/// Sem barreira nenhuma, as DUAS margens atravessam o pai.
///
/// Chrome: pai em y=41 com **20** de altura — nem a de cima nem a de baixo lá
/// estão — e o irmão seguinte em 101, que é `41 + 20 + max(40, 10)`.
#[test]
#[ignore = "Chrome pai y=41 h=20 e seguinte 101, nos y=1 h=100 - nenhuma das duas escapa. Lote D"]
fn sem_barreira_as_duas_margens_atravessam_o_pai() {
    let (yp, hp) = rel("b7", "b7p");
    let (yn, _) = rel("b7", "b7n");
    assert!(perto(yp, 41.0), "pai y={yp} (Chrome: 41)");
    assert!(perto(hp, 20.0), "pai h={hp} (Chrome: 20 — as duas margens saíram)");
    assert!(perto(yn, 101.0), "seguinte y={yn} (Chrome: 101 = 41+20+max(40,10))");
}
