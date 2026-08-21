//! Raios por canto, `transform-origin` e as transformações individuais
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── Raios POR CANTO (ver `style::radius`) ────────────────────────────────────

#[test]
fn canto_declarado_sozinho_nao_arredonda_os_outros() {
    // A regra que bloqueava isto e que continua de pé: escrever um canto no raio
    // ÚNICO arredondaria os quatro. O canto vai para o campo dele, e o campo
    // único fica como estava.
    let s = parse_inline("border-top-left-radius: 8px");
    assert_eq!(s.corner_tl, Some(8.0));
    assert_eq!(s.corner_tr, None);
    assert_eq!(
        s.corner_radius, None,
        "o raio único NÃO é tocado por um canto"
    );
}

#[test]
fn border_radius_continua_a_responder_o_que_respondia() {
    // A condição inegociável do lote: quem já lê `corner_radius` não pode receber
    // resposta diferente. O shorthand escreve os quatro cantos POR CIMA disso.
    let s = parse_inline("border-radius: 6px");
    assert_eq!(s.corner_radius, Some(6.0), "o campo único, como sempre");
    assert_eq!(
        (s.corner_tl, s.corner_tr, s.corner_br, s.corner_bl),
        (Some(6.0), Some(6.0), Some(6.0), Some(6.0))
    );
}

#[test]
fn shorthand_de_cantos_copia_o_canto_diagonalmente_oposto() {
    // A regra dos cantos NÃO é a dos shorthands de caixa: com dois valores, o
    // segundo vale para os dois cantos da DIAGONAL, não para os adjacentes.
    let dois = parse_inline("border-radius: 1px 2px");
    assert_eq!(
        (
            dois.corner_tl,
            dois.corner_tr,
            dois.corner_br,
            dois.corner_bl
        ),
        (Some(1.0), Some(2.0), Some(1.0), Some(2.0))
    );
    let tres = parse_inline("border-radius: 1px 2px 3px");
    assert_eq!(
        (
            tres.corner_tl,
            tres.corner_tr,
            tres.corner_br,
            tres.corner_bl
        ),
        (Some(1.0), Some(2.0), Some(3.0), Some(2.0))
    );
    let quatro = parse_inline("border-radius: 1px 2px 3px 4px");
    assert_eq!(quatro.corner_bl, Some(4.0));
}

#[test]
fn cantos_logicos_caem_nos_cantos_fisicos_em_ltr() {
    // `border-start-start-radius` é o canto superior esquerdo em LTR — o mesmo
    // corte de `style::logical`.
    let s = parse_inline(
        "border-start-start-radius: 1px; border-start-end-radius: 2px; \
         border-end-end-radius: 3px; border-end-start-radius: 4px",
    );
    assert_eq!(
        (s.corner_tl, s.corner_tr, s.corner_br, s.corner_bl),
        (Some(1.0), Some(2.0), Some(3.0), Some(4.0))
    );
}

#[test]
fn canto_eliptico_fica_pelo_raio_horizontal() {
    // Um canto do CSS são DOIS raios; o modelo tem um número por canto. Fica o
    // horizontal, e o teste fixa isso em vez de o deixar por descobrir.
    assert_eq!(
        parse_inline("border-top-left-radius: 10px 20px").corner_tl,
        Some(10.0)
    );
    // e a parte depois da `/` no shorthand é a vertical — descartada igual.
    assert_eq!(
        parse_inline("border-radius: 5px / 15px").corner_tl,
        Some(5.0)
    );
}

#[test]
fn computed_de_um_canto_responde_o_canto() {
    let s = parse_inline("border-radius: 6px");
    assert_eq!(s.get_property("border-top-left-radius"), "6px");
    let vazio = parse_inline("color: red");
    assert_eq!(
        vazio.get_property("border-top-left-radius"),
        "",
        "el.style é vazio"
    );
    assert_eq!(vazio.computed_value("border-top-left-radius", None), "0px");
}

// ── `transform-origin` e `text-decoration-color` ────────────────────────────

#[test]
fn transform_origin_guarda_o_ponto_e_o_inicial_e_o_centro() {
    // GUARDADA, sem geometria: o layout roda em torno do centro da caixa, que é
    // o inicial da spec — logo o valor declarado só muda alguma coisa quando o
    // `layout.rs` o ler. O que se fixa aqui é o valor, não o efeito.
    use crate::style::Dimension::{Percent, Px};
    let s = parse_inline("transform-origin: left top");
    let p = s.transform_origin.expect("o ponto é guardado");
    assert_eq!((p.x, p.y), (Percent(0.0), Percent(0.0)));
    assert_eq!(
        parse_inline("transform-origin: 10px 20px")
            .transform_origin
            .unwrap()
            .x,
        Px(10.0)
    );
    // o inicial é o centro — o mesmo ponto que o layout já assume.
    assert_eq!(
        parse_inline("color:red").computed_value("transform-origin", None),
        "50% 50%"
    );
}

#[test]
fn text_decoration_color_vem_da_longhand_e_do_shorthand() {
    // A longhand, e também o shorthand: `underline dotted red` traz a cor junto,
    // e o parser da LINHA ignora os tokens que não são de linha — sem este ramo
    // a cor não tinha por onde entrar.
    assert_eq!(
        parse_inline("text-decoration-color: #ff0000").text_decoration_color,
        Some(0xFF0000FF)
    );
    let s = parse_inline("text-decoration: underline dotted #00ff00");
    assert_eq!(s.text_decoration_color, Some(0x00FF00FF));
    assert_eq!(
        s.text_decoration,
        Some(crate::style::values::TextDecoration::Underline)
    );
    // `text-decoration-line` NÃO aceita cor (é a longhand da linha, e mais nada).
    assert_eq!(
        parse_inline("text-decoration-line: underline").text_decoration_color,
        None
    );
}

// ── Propriedades individuais de transformação e aliases do WebKit ────────────

#[test]
fn rotate_individual_pinta_pelo_mesmo_transform_do_shorthand() {
    // Esta tem EFEITO REAL e não é vocabulário: escreve o `Transform` que o
    // layout já aplica. Um campo próprio seria uma segunda descrição da mesma
    // transformação, e alguém teria de as compor.
    let s = parse_inline("rotate: 45deg");
    assert_eq!(s.transform.expect("cria a transformação").rot_deg, 45.0);
    // as outras componentes ficam NEUTRAS — e o neutro da escala é 1, não 0.
    let t = s.transform.unwrap();
    assert_eq!(
        (t.sx, t.sy),
        (1.0, 1.0),
        "um Default de zeros encolheria a caixa a nada"
    );
    // `turn` e `rad` também, pelo mesmo parser de ângulo do shorthand.
    assert_eq!(
        parse_inline("rotate: 0.5turn").transform.unwrap().rot_deg,
        180.0
    );
    // `scale` com um valor vale para os dois eixos.
    let e = parse_inline("scale: 2").transform.unwrap();
    assert_eq!((e.sx, e.sy), (2.0, 2.0));
}

#[test]
fn sintaxe_de_flexbox_de_2009_cai_nos_campos_de_hoje() {
    // O `google.css` ainda escreve a flexbox antiga. Estes três têm NOME
    // diferente, não só prefixo, por isso não bastava tirar o `-webkit-`.
    assert_eq!(
        parse_inline("-webkit-box-orient: vertical").flex_direction,
        Some(crate::style::FlexDirection::Column)
    );
    // `justify` é o nome antigo de `space-between`.
    assert_eq!(
        parse_inline("-webkit-box-pack: justify").justify,
        Some(crate::style::JustifyContent::SpaceBetween)
    );
    assert_eq!(
        parse_inline("-webkit-box-align: center").align_items,
        Some(crate::style::AlignItems::Center)
    );
    // e o alias puro do shorthand chega ao mesmo sítio que o nome nu.
    let a = parse_inline("-webkit-transform: rotate(90deg)");
    assert_eq!(a.transform.unwrap().rot_deg, 90.0);
}

#[test]
fn svg_e_contadores_sao_recusa_e_nao_lista_de_afazeres() {
    use crate::style::inert::is_inert;
    // SVG: reconhecer ~300 declarações faria a cobertura subir sem um pixel
    // mudar. A coluna mede trabalho feito, não trabalho parecido com feito.
    assert!(is_inert("fill") && is_inert("stroke") && is_inert("stroke-dasharray"));
    // contadores só imprimem através de `content`, que é de outro dono.
    assert!(is_inert("counter-reset") && is_inert("quotes"));
    // 3D: o Transform deste motor é 2D.
    assert!(is_inert("perspective") && is_inert("transform-style"));
    // e o que está ADIADO continua do lado das desconhecidas, de propósito.
    assert!(!is_inert("filter") && !is_inert("mask-size") && !is_inert("clip-path"));
}
