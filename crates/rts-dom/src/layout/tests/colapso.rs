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
#[ignore = "Chrome 15, nos 25 - o lote do bloco vazio NAO o moveu (medido). Falta o acumulador. Lote C"]
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
#[ignore = "Chrome 30, nos 35 - um conjunto nao cabe no valor unico de prev_margin. Lote C"]
fn o_bloco_vazio_colapsa_tambem_com_a_margem_do_vizinho_anterior() {
    let (ya, ha) = rel("c8", "c8a");
    let (yb, _) = rel("c8", "c8b");
    let intervalo = yb - (ya + ha);
    assert!(
        perto(intervalo, 30.0),
        "intervalo = {intervalo} (Chrome: 30 = max(25,20,30))"
    );
}
