//! Longhands de `transition-*`/`animation-*` e as propriedades LÓGICAS
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── Longhands de `transition-*` / `animation-*` (ver `style::timing`) ─────────

#[test]
fn longhands_de_transition_acumulam_num_so_spec() {
    // O que falhava antes: uma folha que escreve a forma LONGA não ligava
    // transição nenhuma, porque só o shorthand tinha braço no parse. As três
    // longhands numa regra têm de dar o mesmo spec que o shorthand equivalente.
    let longa = parse_inline(
        "transition-duration: 0.3s; transition-delay: 100ms; \
         transition-timing-function: ease-in",
    );
    let curta = parse_inline("transition: 0.3s ease-in 100ms");
    assert_eq!(longa.transition, curta.transition);
    let t = longa.transition.expect("as longhands criam o spec");
    assert_eq!(t.duration_ms, 300.0);
    assert_eq!(t.delay_ms, 100.0);
}

#[test]
fn ordem_das_longhands_nao_muda_o_resultado() {
    // Cada longhand lê o spec já presente e escreve só o seu campo; se alguma
    // reinicializasse o spec, a que viesse antes seria apagada.
    let a = parse_inline("transition-delay: 1s; transition-duration: 2s");
    let b = parse_inline("transition-duration: 2s; transition-delay: 1s");
    assert_eq!(a.transition, b.transition);
    assert_eq!(a.transition.unwrap().delay_ms, 1000.0);
}

#[test]
fn cubic_bezier_com_espacos_chega_inteira_pela_longhand() {
    // O shorthand parte o valor por espaços e por isso perde uma curva escrita
    // com espaço depois da vírgula — a forma que toda ferramenta emite. Pela
    // longhand o valor inteiro vai para o parser da curva.
    let s = parse_inline(
        "transition-duration:.2s; transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1)",
    );
    assert_eq!(
        s.transition.unwrap().easing,
        crate::anim::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)
    );
}

#[test]
fn transition_property_none_desliga_a_transicao() {
    // O único valor de `transition-property` que este modelo sabe honrar: o
    // modelo transiciona `all`, mas `none` significa "nada", e isso ele sabe.
    let s = parse_inline("transition: 0.3s ease; transition-property: none");
    assert!(s.transition.is_none());
    // e um nome de propriedade NÃO desliga (continuamos a transicionar tudo).
    let s = parse_inline("transition: 0.3s ease; transition-property: opacity");
    assert!(s.transition.is_some());
}

#[test]
fn animation_por_longhands_nomeia_o_keyframes() {
    // `animation-name` + `animation-duration` é a forma que o CSS gerado usa; sem
    // ela o `@keyframes` existia e nunca era encontrado por nenhum elemento.
    let s = parse_inline(
        "animation-name: fade; animation-duration: 250ms; \
         animation-iteration-count: infinite; animation-direction: alternate",
    );
    let a = s.animation.expect("as longhands criam o spec");
    assert_eq!(a.name, "fade");
    assert_eq!(a.duration_ms, 250.0);
    assert_eq!(a.iterations, None, "infinite = sem limite de iterações");
    assert_eq!(a.direction, crate::anim::AnimDirection::Alternate);
}

#[test]
fn prefixo_webkit_e_um_alias_do_shorthand() {
    let a = parse_inline("-webkit-transition: 0.5s linear");
    let b = parse_inline("transition: 0.5s linear");
    assert_eq!(a.transition, b.transition);
}

#[test]
fn lista_por_virgula_usa_o_primeiro_tempo() {
    // `transition-duration: .3s, .2s` dá tempos a duas propriedades; o modelo tem
    // um spec só, e lê o primeiro. Documentado, não silencioso.
    let s = parse_inline("transition-duration: 0.3s, 0.2s");
    assert_eq!(s.transition.unwrap().duration_ms, 300.0);
}

#[test]
fn computed_de_uma_longhand_responde_o_valor_dela() {
    // `transition-duration` respondia `all 0.3s 0s` — o shorthand inteiro, que
    // nem é um valor válido da propriedade perguntada.
    let s = parse_inline("transition: 0.3s ease-in 100ms");
    assert_eq!(s.get_property("transition-duration"), "0.3s");
    assert_eq!(s.get_property("transition-delay"), "0.1s");
    assert_eq!(s.get_property("transition-timing-function"), "ease-in");
    // sem nada declarado, o computed é o INICIAL da spec, não vazio.
    let vazio = parse_inline("color: red");
    // O `get_property` é também o `el.style.x`, que responde vazio para o que o
    // elemento não declarou; quem cai no INICIAL é o `computed_value`. São dois
    // consumidores com semânticas opostas — ver o cabeçalho de `style::initial`.
    assert_eq!(vazio.get_property("transition-duration"), "");
    assert_eq!(vazio.computed_value("transition-duration", None), "0s");
    assert_eq!(vazio.computed_value("animation-name", None), "none");
    assert_eq!(vazio.computed_value("animation-iteration-count", None), "1");
}

// ── Propriedades LÓGICAS: `inset*` e bordas `-inline-`/`-block-` ─────────────

#[test]
fn inset_logico_escreve_o_offset_do_lado_fisico() {
    // O corte é LTR: start=left/top, end=right/bottom — o mesmo que
    // `padding-inline-start` já assumia (ver `style::logical`).
    let s = parse_inline("position:absolute; inset-inline-start: 10px; inset-block-end: 4px");
    assert_eq!(s.inset_left, Some(Dimension::Px(10.0)));
    assert_eq!(s.inset_bottom, Some(Dimension::Px(4.0)));
    assert_eq!(s.inset_right, None, "o lado oposto fica por declarar");
}

#[test]
fn inset_shorthand_segue_a_ordem_da_caixa() {
    // top right bottom left, com os omitidos a copiar o lado oposto.
    let um = parse_inline("inset: 0");
    assert_eq!(um.inset_top, Some(Dimension::Px(0.0)));
    assert_eq!(um.inset_left, Some(Dimension::Px(0.0)));
    let dois = parse_inline("inset: 1px 2px");
    assert_eq!(dois.inset_top, Some(Dimension::Px(1.0)));
    assert_eq!(dois.inset_right, Some(Dimension::Px(2.0)));
    assert_eq!(dois.inset_bottom, Some(Dimension::Px(1.0)));
    assert_eq!(dois.inset_left, Some(Dimension::Px(2.0)));
    let quatro = parse_inline("inset: 1px 2px 3px 4px");
    assert_eq!(quatro.inset_bottom, Some(Dimension::Px(3.0)));
    assert_eq!(quatro.inset_left, Some(Dimension::Px(4.0)));
    // e o eixo sozinho toca só os dois lados dele.
    let eixo = parse_inline("inset-inline: 5px");
    assert_eq!(
        (eixo.inset_left, eixo.inset_right),
        (Some(Dimension::Px(5.0)), Some(Dimension::Px(5.0)))
    );
    assert_eq!(eixo.inset_top, None);
}

#[test]
fn borda_logica_e_a_mesma_borda_do_lado_fisico() {
    // A tradução tem de cair exatamente no modelo de bordas que já existe — se
    // divergisse, haveria duas respostas para "qual é a borda esquerda".
    let logica = parse_inline("border-inline-start-width: 3px; border-inline-start-style: solid");
    let fisica = parse_inline("border-left-width: 3px; border-left-style: solid");
    assert_eq!(logica.border_widths.left, fisica.border_widths.left);
    assert_eq!(logica.border_left_style, Some(BorderStyle::Solid));
    // o shorthand de lado lógico também.
    let s = parse_inline("border-inline-end: 2px dashed #000");
    assert_eq!(s.border_right_style, Some(BorderStyle::Dashed));
    // e o eixo de bloco vai para topo/fundo.
    let b = parse_inline("border-block-end-style: dotted");
    assert_eq!(b.border_bottom_style, Some(BorderStyle::Dotted));
}
