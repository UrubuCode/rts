//! O corpus de floats/`clear`/BFC como testes de Rust: as duas fixtures
//! `tests/css/claude-clear.html` e `tests/css/claude-float-clear.html`,
//! copiadas VERBATIM (não reescritas), contra o que o **Chrome mediu** — o
//! `.esperado.json` ao lado de cada uma, nunca escrito à mão (ver
//! `tests/css/README.md`). Os seis desvios que motivaram este lote:
//!
//! ```text
//! claude-clear.html        #limpa-ambos.y    esperado 95  obtido 110
//!                          #limpa-direita.y  esperado 40  obtido 80
//!                          #limpa-esquerda.y esperado 80  obtido 95
//! claude-float-clear.html  #ao-lado.y        esperado 0   obtido 60
//!                          #limpo.y          esperado 60  obtido 80
//!                          #pai-so-floats.h  esperado 0   obtido 60
//! ```
//!
//! O terceiro teste deste ficheiro é diferente dos outros dois: está marcado,
//! no nome e no corpo, como DERIVADO DA SPEC — não foi medido no Chrome. Fixa
//! o lado que as duas fixtures acima não cobrem (nenhuma tem um pai que
//! ESTABELEÇA BFC só com floats lá dentro): CSS 2.1 §10.6.7 diz que um BFC
//! cresce para conter os SEUS floats, e a mesma geometria de
//! `claude-float-clear.html` — só com `display:flow-root` ou
//! `overflow:hidden` no pai — é o par positivo do `#pai-so-floats` que mede 0.

use super::*;
use crate::table::tests::{geometria, rect};

/// `tests/css/claude-clear.html`, cópia exata (a régua é o Chrome, nunca uma
/// reescrita — ver o cabeçalho do módulo).
const CLAUDE_CLEAR: &str = r#"<!DOCTYPE html>
<!-- Fixa o `clear` sozinho, com um float de cada lado, para o desvio dizer QUAL
     dos três valores falha: `left` só desce abaixo do float esquerdo, `right`
     só do direito, `both` de qualquer um. Um motor que trate os três como
     `both` passa dois destes e falha o resto. -->
<html>
<head>
<meta name="fixar-estilo-em" content="fesq,fdir,limpa-esquerda,limpa-direita,limpa-ambos,nao-limpa">
<meta name="fixar-estilo" content="clear,float">
<style>
  body { margin: 0; }
  #fesq { float: left; width: 100px; height: 80px; background: #fcc; }
  #fdir { float: right; width: 100px; height: 40px; background: #cfc; }
  #nao-limpa { clear: none; height: 15px; background: #ddd; }
  #limpa-direita { clear: right; height: 15px; background: #ccf; }
  #limpa-esquerda { clear: left; height: 15px; background: #ffc; }
  #limpa-ambos { clear: both; height: 15px; background: #cff; }
</style></head>
<body>
  <div id="fesq"></div><div id="fdir"></div>
  <div id="nao-limpa"></div>
  <div id="limpa-direita"></div>
  <div id="limpa-esquerda"></div>
  <div id="limpa-ambos"></div>
</body>
</html>"#;

/// `tests/css/claude-float-clear.html`, cópia exata.
const CLAUDE_FLOAT_CLEAR: &str = r#"<!DOCTYPE html>
<!-- Fixa: um `float` sai do fluxo normal mas o texto contorna-o; `clear` empurra
     o elemento para baixo do float. E o pai só de floats colapsa para altura 0 —
     é o efeito que surpreende quem não conhece a regra. -->
<html>
<head>
<meta name="fixar-estilo-em" content="esq,dir,limpo">
<meta name="fixar-estilo" content="float,clear">
<style>
  body { margin: 0; }
  #pai-so-floats { background: #eee; }
  #esq { float: left; width: 100px; height: 60px; background: #fcc; }
  #dir { float: right; width: 80px; height: 40px; background: #cfc; }
  #ao-lado { height: 20px; background: #ccf; }
  #limpo { clear: both; height: 25px; background: #ffc; }
</style></head>
<body>
  <div id="pai-so-floats"><div id="esq"></div><div id="dir"></div></div>
  <div id="ao-lado"></div>
  <div id="limpo"></div>
</body>
</html>"#;

/// Um pixel — a mesma tolerância do corredor `scripts/css_fixtures.sh`.
const TOL: f32 = 1.0;

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOL
}

/// Confere os quatro campos do rect de um id contra o que o Chrome mediu.
fn afirma_rect(dom: &crate::Dom, list: &crate::layout::DisplayList, id: &str, esperado: Rect) {
    let r = rect(dom, list, &format!("#{id}"), 0);
    assert!(
        perto(r.x, esperado.x) && perto(r.y, esperado.y) && perto(r.w, esperado.w) && perto(r.h, esperado.h),
        "#{id} = {r:?}, esperado (Chrome) {esperado:?}"
    );
}

/// `claude-clear.html` a 1280×800: os TRÊS valores de `clear` por lado, contra
/// `claude-clear.esperado.json`. `#nao-limpa` e os dois floats entram também —
/// são a geometria de que a clearance depende, e um desvio neles esconderia a
/// causa atrás de um desvio no efeito.
#[test]
fn claude_clear_bate_com_o_chrome() {
    let (dom, list) = geometria(CLAUDE_CLEAR, 1280.0);
    afirma_rect(&dom, &list, "fesq", Rect::new(0.0, 0.0, 100.0, 80.0));
    afirma_rect(&dom, &list, "fdir", Rect::new(1180.0, 0.0, 100.0, 40.0));
    afirma_rect(&dom, &list, "nao-limpa", Rect::new(0.0, 0.0, 1280.0, 15.0));
    // clear:right — só desce abaixo do float DIREITO (fundo em 40), não do
    // esquerdo (que desceria para 80, o desvio que este teste fixa).
    afirma_rect(
        &dom,
        &list,
        "limpa-direita",
        Rect::new(0.0, 40.0, 1280.0, 15.0),
    );
    // clear:left — só desce abaixo do float ESQUERDO (fundo em 80).
    afirma_rect(
        &dom,
        &list,
        "limpa-esquerda",
        Rect::new(0.0, 80.0, 1280.0, 15.0),
    );
    // clear:both — abaixo dos DOIS floats E do irmão anterior (que já está em
    // 95): a clearance não se soma à posição de fluxo, toma o maior dos dois.
    afirma_rect(
        &dom,
        &list,
        "limpa-ambos",
        Rect::new(0.0, 95.0, 1280.0, 15.0),
    );
}

/// `claude-float-clear.html` a 1280×800, contra
/// `claude-float-clear.esperado.json`: um float ESCAPA de um `<div>` sem BFC
/// para o BFC do antepassado (aqui, o `body`) — é por isso que `#limpo`, IRMÃO
/// de `#pai-so-floats` e não filho dele, ainda assim desce abaixo dos floats
/// que `#pai-so-floats` contém. E `#pai-so-floats` mede altura 0: não
/// estabelece BFC, então não cresce pelos floats que deixou passar.
#[test]
fn claude_float_clear_bate_com_o_chrome() {
    let (dom, list) = geometria(CLAUDE_FLOAT_CLEAR, 1280.0);
    afirma_rect(&dom, &list, "esq", Rect::new(0.0, 0.0, 100.0, 60.0));
    afirma_rect(&dom, &list, "dir", Rect::new(1200.0, 0.0, 80.0, 40.0));
    // Um bloco normal não encolhe nem desce por causa de um float: só as
    // LINHAS de texto contornam-no. `#ao-lado` não tem `clear`, então fica no
    // y do fluxo normal — logo depois de um `#pai-so-floats` de altura 0.
    afirma_rect(&dom, &list, "ao-lado", Rect::new(0.0, 0.0, 1280.0, 20.0));
    // clear:both — abaixo do float mais alto (60), não do fluxo normal (que
    // sem clearance ficaria em 20, logo depois de `#ao-lado`).
    afirma_rect(&dom, &list, "limpo", Rect::new(0.0, 60.0, 1280.0, 25.0));
    let pai = rect(&dom, &list, "#pai-so-floats", 0);
    assert!(
        perto(pai.h, 0.0),
        "#pai-so-floats não estabelece BFC — não cresce pelos floats que contém: h={}",
        pai.h
    );
}

/// DERIVADO DA SPEC, NÃO MEDIDO NO CHROME: o par positivo de
/// `#pai-so-floats` — a MESMA geometria de dois floats (100×60 esquerdo,
/// 80×40 direito), mesma largura de pai (auto, cheia), mas ESTABELECENDO BFC
/// desta vez — `flow-root` num caso, `overflow:hidden` no outro, as duas
/// formas de "clearfix" que a lista de MDN "Block formatting context" e o
/// CSS 2.1 §10.6.7 citam (ao lado de flex/grid/tabela/float/positioned/raiz,
/// que este ficheiro não isola sozinhas). Um BFC cresce para conter os SEUS
/// floats — altura esperada 60, o float mais alto — e é essa a afirmação que
/// `claude-float-clear.html` não tem como fazer, porque a fixture existe
/// justamente para fixar o caso SEM BFC.
#[test]
fn bfc_flow_root_ou_overflow_hidden_contem_os_floats_derivado_da_spec() {
    const HTML: &str = r#"<!DOCTYPE html>
<html><head><style>
  body { margin: 0; }
  #pai-flow-root { display: flow-root; background: #eee; }
  #pai-overflow { overflow: hidden; background: #eee; }
  .esq { float: left; width: 100px; height: 60px; }
  .dir { float: right; width: 80px; height: 40px; }
</style></head>
<body>
  <div id="pai-flow-root"><div class="esq"></div><div class="dir"></div></div>
  <div id="pai-overflow"><div class="esq"></div><div class="dir"></div></div>
</body>
</html>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    for id in ["pai-flow-root", "pai-overflow"] {
        let r = rect(&dom, &list, &format!("#{id}"), 0);
        assert!(
            perto(r.h, 60.0),
            "#{id} estabelece BFC — devia crescer até ao float mais alto (60): h={}",
            r.h
        );
    }
}
