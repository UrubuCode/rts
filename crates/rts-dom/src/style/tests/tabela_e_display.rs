//! Os mecanismos gerados da tabela `css_props!` e o `inline-block`
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

#[test]
fn mecanismos_gerados_da_tabela() {
    // A REFATORAÇÃO data-driven: herança, diff-de-animação e interpolação são
    // GERADOS da tabela css_props! — este teste prova que os 3 mecanismos batem
    // com o comportamento antigo (campo a campo) e cobre o caso que o modelo
    // antigo deixava dessincronizar (campo animável fora da lista fixa).
    // herança: só os campos [inh] descem.
    let mut child = ComputedStyle::default();
    let mut parent = ComputedStyle::default();
    parent.color = Some(0x112233FF); // inh
    parent.bg = Some(0x445566FF); // NÃO herda (box)
    parent.font_size = Some(Dimension::Px(20.0)); // inh
    child.inherit_from(&parent);
    assert_eq!(child.color, Some(0x112233FF));
    assert_eq!(child.font_size, Some(Dimension::Px(20.0)));
    assert_eq!(child.bg, None); // bg não herda
    // diff animado: campo [anim] dispara; não-animável não.
    let mut a = ComputedStyle::default();
    let mut b = ComputedStyle::default();
    b.display = Some(DisplayKind::Flex); // não-animável
    assert!(!a.differs_animated(&b));
    b.bg = Some(0xFF0000FF); // animável
    assert!(a.differs_animated(&b));
    // interpolação: animável interpola (preto→vermelho no meio = meio-vermelho),
    // não-animável salta pro destino.
    a.bg = Some(0x000000FF);
    let mid = ComputedStyle::interpolate_animated(&a, &b, 0.5);
    assert_eq!(mid.bg, Some(0x800000FF));
    assert_eq!(mid.display, Some(DisplayKind::Flex)); // discreto: já no destino
}

#[test]
fn inline_block_serializa_com_o_proprio_nome() {
    // `getComputedStyle(el).display` devolve o keyword USADO. Enquanto
    // `inline-block` partilhou a variante com `inline`, respondia `inline` — 8
    // desvios no corpus de fixtures, todos com esta forma.
    let mut c = ComputedStyle::default();
    c.display = Some(DisplayKind::InlineBlock);
    assert_eq!(c.get_property("display"), "inline-block");
    c.display = Some(DisplayKind::Inline);
    assert_eq!(c.get_property("display"), "inline");
}

#[test]
fn inline_block_e_de_nivel_inline_e_nao_de_bloco() {
    // A armadilha que a variante nova cria: quem perguntava "é de bloco?" com
    // `display != Inline` passa a errar. `is_inline_level` é a pergunta que não
    // se desatualiza.
    assert!(DisplayKind::InlineBlock.is_inline_level());
    assert!(DisplayKind::Inline.is_inline_level());
    assert!(!DisplayKind::Block.is_inline_level());
    // e continua a empilhar os filhos no mesmo eixo do `inline` (wrap).
    assert_eq!(
        DisplayKind::InlineBlock.to_display_code(),
        DisplayKind::Inline.to_display_code()
    );
}
