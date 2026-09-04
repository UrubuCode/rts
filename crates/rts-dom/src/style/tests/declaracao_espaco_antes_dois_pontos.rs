//! `declaracao_nome.rs`: uma declaração `prop : valor` (espaço ANTES dos
//! dois-pontos) não pode ser descartada inteira — CSS Syntax §5.4.4 permite
//! espaço em qualquer lado do `:`. Pin do bug de
//! `tests/css/claude-declaracao-espaco-antes-dois-pontos.html`, onde as 4
//! regras do WPT `flex-direction.html` escrevem exactamente `prop : valor`.

use super::*;

#[test]
fn parse_com_espaco_antes_dos_dois_pontos_nao_descarta_a_declaracao() {
    let com_espaco = parse_inline("flex-direction : column");
    let sem_espaco = parse_inline("flex-direction: column");
    assert_eq!(com_espaco.flex_direction, Some(FlexDirection::Column));
    assert_eq!(com_espaco.flex_direction, sem_espaco.flex_direction);
}

#[test]
fn parse_com_espaco_dos_dois_lados_do_dois_pontos_tambem_funciona() {
    let c = parse_inline("width :  80px ");
    assert_eq!(c.width, Some(Dimension::Px(80.0)));
}
