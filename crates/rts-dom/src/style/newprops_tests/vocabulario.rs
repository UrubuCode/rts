//! O vocabulário novo (`style::vocab`) e as reconhecidas-e-não-modeladas
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── Lote 2: o vocabulário novo (ver `style::vocab`) ──────────────────────────

#[test]
fn eixos_de_background_position_escrevem_o_mesmo_campo_do_shorthand() {
    // Esta é das poucas do lote com EFEITO REAL: o campo é o que o render já
    // pinta, então declarar por eixo passa a mover mesmo o fundo.
    use crate::style::Dimension::{Percent, Px};
    let s = parse_inline("background-position-x: 10px; background-position-y: bottom");
    let p = s.bg_position.expect("os eixos criam a posição");
    assert_eq!(p.x, Px(10.0));
    assert_eq!(p.y, Percent(100.0), "`bottom` é 100% no eixo vertical");
    // e um eixo sozinho não apaga o outro já declarado pelo shorthand.
    let s = parse_inline("background-position: 20px 30px; background-position-x: 0");
    let p = s.bg_position.unwrap();
    assert_eq!((p.x, p.y), (Px(0.0), Px(30.0)));
}

#[test]
fn font_stretch_computa_em_percentagem() {
    // O computed do Chrome responde a percentagem mesmo quando o autor escreveu
    // o keyword — é a definição da spec, não uma conversão nossa.
    assert_eq!(
        parse_inline("font-stretch: condensed").font_stretch,
        Some(75.0)
    );
    assert_eq!(parse_inline("font-stretch: 87.5%").font_stretch, Some(87.5));
    assert_eq!(
        parse_inline("font-stretch: condensed").get_property("font-stretch"),
        "75%"
    );
    assert_eq!(
        parse_inline("color:red").computed_value("font-stretch", None),
        "100%"
    );
    assert_eq!(
        parse_inline("color:red").get_property("font-stretch"),
        "",
        "el.style vazio"
    );
}

#[test]
fn keywords_do_lote_voltam_pelo_computed() {
    // O que estas propriedades PROMETEM hoje é exatamente isto: a declaração
    // sobrevive e o computed responde-a. A geometria não muda — está dito no
    // comentário de cada tipo em `style::vocab`.
    let s = parse_inline(
        "text-overflow: ellipsis; object-fit: cover; hyphens: auto; \
         scrollbar-width: thin; caption-side: bottom; text-wrap: balance; \
         unicode-bidi: isolate",
    );
    assert_eq!(s.get_property("text-overflow"), "ellipsis");
    assert_eq!(s.get_property("object-fit"), "cover");
    assert_eq!(s.get_property("hyphens"), "auto");
    assert_eq!(s.get_property("scrollbar-width"), "thin");
    assert_eq!(s.get_property("caption-side"), "bottom");
    assert_eq!(s.get_property("text-wrap"), "balance");
    assert_eq!(s.get_property("unicode-bidi"), "isolate");
    // sem declaração, cada uma responde o INICIAL da spec.
    let vazio = parse_inline("color: red");
    assert_eq!(vazio.computed_value("text-overflow", None), "clip");
    assert_eq!(vazio.computed_value("object-fit", None), "fill");
    assert_eq!(vazio.computed_value("-webkit-line-clamp", None), "none");
}

#[test]
fn zoom_e_line_clamp_aceitam_as_duas_formas() {
    assert_eq!(parse_inline("zoom: 150%").zoom, Some(1.5));
    assert_eq!(parse_inline("zoom: 2").zoom, Some(2.0));
    assert_eq!(parse_inline("zoom: normal").zoom, Some(1.0));
    assert_eq!(parse_inline("-webkit-line-clamp: 3").line_clamp, Some(3));
    assert_eq!(parse_inline("-webkit-line-clamp: none").line_clamp, None);
    // um clamp de 0 linhas não existe; o valor é recusado em vez de guardado.
    assert_eq!(parse_inline("-webkit-line-clamp: 0").line_clamp, None);
}

#[test]
fn place_shorthands_expandem_para_os_campos_que_ja_existem() {
    // `place-*` não é campo novo: são dois campos antigos escritos de uma vez —
    // o mesmo que `flex-flow` faz. Um campo próprio seria uma segunda resposta
    // para "qual é o alinhamento deste item".
    let s = parse_inline("place-content: center space-between");
    assert_eq!(s.align_content, Some(crate::style::JustifyContent::Center));
    assert_eq!(s.justify, Some(crate::style::JustifyContent::SpaceBetween));
    // um valor só vale para os dois eixos.
    let um = parse_inline("place-self: center");
    assert_eq!(um.align_self, Some(crate::style::AlignItems::Center));
    assert_eq!(um.justify_self, Some(crate::style::AlignItems::Center));
}

#[test]
fn word_spacing_normal_e_zero() {
    // Mesma convenção do `letter-spacing` ao lado — e sem ela, `normal` caía no
    // parser de comprimento e desaparecia.
    assert_eq!(parse_inline("word-spacing: normal").word_spacing, Some(0.0));
    assert_eq!(parse_inline("word-spacing: 4px").word_spacing, Some(4.0));
}

// ── Lote 3: reconhecidas-e-não-modeladas, e `pointer-events` ────────────────

#[test]
fn propriedade_recusada_nao_conta_como_desconhecida() {
    // A coluna das desconhecidas é a lista do que falta fazer. `will-change` não
    // falta — foi recusada, e por um motivo escrito. Misturar as duas fazia a
    // lista mentir sobre o tamanho do trabalho.
    use crate::style::inert::is_inert;
    assert!(is_inert("will-change"));
    assert!(is_inert("page-break-inside"));
    assert!(is_inert("scroll-behavior"));
    assert!(
        is_inert("-webkit-appearance"),
        "o prefixo não muda a resposta"
    );
    assert!(is_inert("-moz-user-select"));
    // e o que é trabalho por fazer continua do outro lado da linha.
    assert!(!is_inert("filter"), "pintura por decidir NÃO é recusa");
    assert!(!is_inert("clip-path"));
    assert!(!is_inert("object-fit"), "essa está implementada");
}

#[test]
fn pointer_events_e_guardado_e_herda() {
    // Tem campo (e não entrou na lista de recusadas) porque o teste de acerto do
    // DOM já existe: ligá-lo é ler este campo. Até lá o clique atravessa na mesma.
    use crate::style::vocab::PointerEvents;
    assert_eq!(
        parse_inline("pointer-events: none").pointer_events,
        Some(PointerEvents::None)
    );
    assert_eq!(
        parse_inline("pointer-events: none").get_property("pointer-events"),
        "none"
    );
    assert_eq!(
        parse_inline("color: red").computed_value("pointer-events", None),
        "auto"
    );
    // um valor de SVG que não modelamos não é guardado como se fosse outro.
    assert_eq!(
        parse_inline("pointer-events: visiblePainted").pointer_events,
        None
    );
}
