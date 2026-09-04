//! O medidor de texto ACTIVO (`layout::medidor_ativo`) muda a geometria que
//! `bounding_component` responde para fora — o comportamento que fecha o
//! finding "duas verdades para a mesma geometria" da auditoria estrutural de
//! 2026-09-04
//! (`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/05-texto-e-fontes.md`).
//!
//! Não se verifica nenhum número absoluto: verifica-se que `bounding_component`
//! bate, em TODOS os casos, com o que `layout::bounding_rect` dá directamente
//! para o mesmo medidor — a mesma garantia que
//! `consulta::geometria_em_lote_responde_o_mesmo_que_a_singular` já usa para a
//! versão em lote, aplicada aqui ao REGISTO do medidor em vez de à forma da
//! chamada.
//!
//! `set_active`/`clear_active` mexem num `thread_local`, e o executor de testes
//! reusa a mesma thread do SO para vários testes ao longo da execução — por
//! isso cada teste aqui limpa o que registou antes de terminar, mesmo quando
//! não precisaria para o seu próprio resultado, para não vazar estado para o
//! PRÓXIMO teste que corra nessa thread.

use super::*;
use crate::layout::medidor_ativo::{clear_active, set_active};
use crate::layout::{ApproxMeasurer, LayoutCtx, TextMeasurer, bounding_rect};
use std::rc::Rc;

/// Medidor de teste com linha SEMPRE mais alta que a do `ApproxMeasurer` — o
/// dobro do tamanho da fonte — para que uma mudança na geometria só possa vir
/// de estar a ser realmente consultado, nunca de coincidir por acidente com a
/// aproximação headless.
struct MedidorFalso;

impl TextMeasurer for MedidorFalso {
    fn text_width(&self, text: &str, size: f32, _mono: bool, _bold: bool, _italic: bool) -> f32 {
        text.chars().count() as f32 * size * 2.0
    }
    fn line_height(&self, size: f32) -> f32 {
        size * 2.0
    }
    fn identity(&self) -> u64 {
        // Fixo e != 0 (o default do `ApproxMeasurer`, que não sobrescreve
        // `identity`): entra na chave do cache de layout, e um medidor de
        // teste que colidisse com essa chave leria de volta a resposta
        // cacheada ERRADA na segunda chamada deste ficheiro.
        0xFA15E
    }
}

/// Um parágrafo de uma linha só — a viewport é larga o bastante para que nem o
/// `MedidorFalso` (o dobro da largura por carácter) force uma quebra. A altura
/// do bloco fica então inteiramente decidida por `line_height`, o que faz da
/// comparação um teste do MEDIDOR, e não do algoritmo de quebra de linha.
fn documento_com_paragrafo() -> (Dom, NodeId) {
    // `install_ua_defaults` só corre uma vez por thread; um teste anterior
    // nesta mesma thread pode ter registado "p" com outra coisa. Redefine
    // aqui, como `pintura::bounding_rect_none_para_texto` já faz, para não
    // depender da ordem de execução dos testes.
    crate::block::define(
        "p",
        crate::block::BlockDef {
            display: crate::block::DISPLAY_VERTICAL,
            indent: 0.0,
            prefix: crate::block::PREFIX_NONE,
            flags: 0,
        },
    );
    let dom = parse_html_to_dom("<p>oi</p>");
    let id = dom.query("p").unwrap();
    (dom, id)
}

/// Altura (`bounding_component(id, 3)`) do parágrafo pelo caminho DIRECTO:
/// constrói o `LayoutCtx` com `measurer` à mão e chama `bounding_rect` — o
/// MESMO que `bounding_component` faz por dentro depois de resolver o medidor
/// activo. É o oráculo contra o qual os três testes comparam.
fn altura_direta(dom: &Dom, id: NodeId, measurer: &dyn TextMeasurer) -> f32 {
    let ctx = LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer,
    };
    bounding_rect(dom, idx(dom, id), &ctx)
        .expect("um <p> é um elemento-bloco: tem de ter rect")
        .h
}

#[test]
fn sem_medidor_ativo_bounding_component_mede_como_o_approxmeasurer() {
    clear_active();
    let (dom, id) = documento_com_paragrafo();
    let esperado = altura_direta(&dom, id, &ApproxMeasurer);
    assert_eq!(
        dom.bounding_component(id, 3),
        esperado,
        "sem backend nenhum registado, a altura tem de ser a do layout headless"
    );
}

#[test]
fn medidor_ativo_registado_muda_a_altura_que_bounding_component_responde() {
    clear_active();
    let (dom, id) = documento_com_paragrafo();
    let sem_medidor = dom.bounding_component(id, 3);
    set_active(Rc::new(MedidorFalso));
    let com_medidor = dom.bounding_component(id, 3);
    let esperado_com_medidor = altura_direta(&dom, id, &MedidorFalso);
    clear_active();
    assert_ne!(
        sem_medidor, com_medidor,
        "o MedidorFalso dobra a altura de linha — a resposta tem de mudar"
    );
    assert_eq!(
        com_medidor, esperado_com_medidor,
        "e tem de ser EXACTAMENTE a altura que o MedidorFalso dá, não uma aproximação dela"
    );
}

#[test]
fn limpar_o_medidor_ativo_volta_a_responder_como_o_approxmeasurer() {
    clear_active();
    let (dom, id) = documento_com_paragrafo();
    let original = dom.bounding_component(id, 3);
    set_active(Rc::new(MedidorFalso));
    assert_ne!(dom.bounding_component(id, 3), original, "pré-condição: o registo mudou a resposta");
    clear_active();
    assert_eq!(
        dom.bounding_component(id, 3),
        original,
        "limpar tem de devolver exactamente a resposta de antes de registar"
    );
}
