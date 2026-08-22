//! A auditoria de `calculos/estilo.jsonl`, EXECUTADA — lote A: os valores.
//!
//! Dez registos que se respondem com um `parse_inline`. Cada teste carrega o
//! `id` do registo que põe à prova, e quando um falhar o que se corrige é o
//! REGISTO: o teste diz o que o motor faz, e o registo é que afirmava outra
//! coisa. Um registo que se ajusta ao código volta a ser uma afirmação sobre o
//! que o código parece fazer, que é o defeito que esta execução existe para
//! fechar.
//!
//! **Um dos dez estava errado**, e está anotado no teste que o apanhou.

use crate::style::parse::parse_inline;
use crate::style::values::{Dimension, Side};

/// `estilo.calc.min-max-clamp` — só `calc()` linear; as três funções de
/// comparação não parseiam e a declaração cai.
#[test]
fn min_max_clamp_nao_parseiam() {
    assert!(parse_inline("width:calc(1px + 2px)").width.is_some(), "o calc linear é a base");
    assert_eq!(parse_inline("width:clamp(1rem,2vw,2rem)").width, None);
    assert_eq!(parse_inline("width:min(10px,2vw)").width, None);
    assert_eq!(parse_inline("width:max(10px,2vw)").width, None);
}

/// `estilo.unidade.tabela` — doze unidades da spec que o parse recusa. Não são
/// aproximadas: a declaração inteira cai.
#[test]
fn as_unidades_em_falta_derrubam_a_declaracao() {
    for u in [
        "10ch", "10ex", "10cm", "10mm", "10in", "10q", "10lh", "10vmin", "10vmax", "10dvh",
        "10svh", "10lvh",
    ] {
        assert_eq!(
            parse_inline(&format!("width:{u}")).width,
            None,
            "unidade {u} passou a ser aceite — o registo estilo.unidade.tabela mudou"
        );
    }
    // O contraste: as seis que aceitamos.
    for u in ["10px", "10%", "10em", "10rem", "10vw", "10vh"] {
        assert!(parse_inline(&format!("width:{u}")).width.is_some(), "{u}");
    }
}

/// `estilo.font.tamanho-por-palavra-chave` — nenhuma das palavras-chave existe,
/// e o tamanho fica por declarar (herda).
#[test]
fn as_palavras_chave_de_font_size_nao_existem() {
    for k in ["medium", "large", "x-large", "smaller", "larger", "small"] {
        assert_eq!(parse_inline(&format!("font-size:{k}")).font_size, None, "{k}");
    }
}

/// `estilo.font.peso-numerico` — o peso colapsa num booleano com corte em 600.
///
/// O que se perde está nas duas primeiras linhas: 500 e 400 respondem o mesmo, e
/// `bolder`/`lighter` — que a spec manda calcular a partir do peso do PAI —
/// respondem uma constante.
#[test]
fn o_peso_da_fonte_colapsa_em_booleano() {
    let bold = |v: &str| parse_inline(&format!("font-weight:{v}")).bold;
    assert_eq!(bold("400"), Some(false));
    assert_eq!(bold("500"), Some(false), "500 e 400 são indistinguíveis aqui");
    assert_eq!(bold("600"), Some(true), "o corte");
    assert_eq!(bold("900"), Some(true));
    assert_eq!(bold("bolder"), Some(true), "constante, não relativo ao pai");
    assert_eq!(bold("lighter"), Some(false), "idem");
}

/// `estilo.font.shorthand-nao-reseta` — o shorthand `font` não repõe as
/// longhands que omite, e a spec manda repô-las ao inicial.
#[test]
fn o_shorthand_font_nao_repoe_as_omitidas() {
    let c = parse_inline("font-weight:bold;font-style:italic;line-height:2;font:16px Arial");
    assert_eq!(c.bold, Some(true), "o Chrome repunha a 400");
    assert_eq!(c.italic, Some(true), "o Chrome repunha a normal");
    assert!(c.line_height.is_some(), "o Chrome repunha a normal");
}

/// `estilo.font.shorthand-exige-digito` — o `font` procura o primeiro token que
/// começa por dígito. Sem ele a declaração cai INTEIRA, família incluída.
#[test]
fn o_shorthand_font_sem_digito_cai_inteiro() {
    let c = parse_inline("font:bold medium serif");
    assert_eq!(c.font_size, None);
    assert_eq!(c.font_family, None, "a família também se perde");
    assert_eq!(c.bold, None, "e o peso, que vinha antes do tamanho");
}

/// `estilo.font.family-lista` — guardamos só a PRIMEIRA família, e é isso que o
/// `getComputedStyle` responde. Sem a lista não há fallback para consultar.
#[test]
fn a_lista_de_font_family_perde_tudo_menos_a_primeira() {
    let c = parse_inline("font-family:Georgia, 'Times New Roman', serif");
    assert_eq!(c.font_family.as_deref(), Some("Georgia"));
    assert_eq!(c.computed_value("font-family", None), "Georgia");
}

/// `estilo.keyword.unset` / `estilo.keyword.initial` / `estilo.keyword.revert` —
/// as três são ignoradas, portanto a declaração ANTERIOR fica a valer.
///
/// É a diferença que importa: no browser um `color:unset` desfaz o `red`; aqui
/// não o desfaz, e a página fica com a cor que o autor mandou tirar.
#[test]
fn unset_initial_e_revert_sao_ignorados() {
    let vermelho = parse_inline("color:red").color;
    for kw in ["unset", "initial", "revert", "revert-layer"] {
        assert_eq!(
            parse_inline(&format!("color:red;color:{kw}")).color,
            vermelho,
            "`color:{kw}` devia desfazer o vermelho"
        );
    }
}

/// `estilo.keyword.largas-nao-sao-shorthand-aware` — **o registo estava ERRADO
/// no mecanismo, e este teste é o que o corrigiu.**
///
/// O registo dizia que `margin:inherit` "não existe". Existe: o parse
/// RECONHECE-O e regista o nome em `inherit_props`. Quem o deita fora é o
/// `copy_property`, cujo `_ => {}` não conhece `margin` — a declaração é
/// capturada e depois descartada em silêncio.
///
/// A diferença decide onde a correção vai: é uma linha no `copy_property`, não
/// uma mudança de parse. O efeito líquido é o mesmo, a causa não.
#[test]
fn margin_inherit_e_capturado_e_depois_descartado() {
    let c = parse_inline("margin:8px;margin:inherit");
    assert_eq!(
        c.inherit_props.as_deref().map(|v| v.as_slice()),
        Some(["margin".to_string()].as_slice()),
        "o parse RECONHECE o `inherit` e guarda o nome"
    );
    assert_eq!(
        c.margin.top,
        Side::Len(Dimension::Px(8.0)),
        "e o `copy_property` não sabe copiar `margin`, portanto nada acontece"
    );
}

/// `caixa.contentor.flow-root-perde-o-significado` — a palavra deixou de ser
/// deitada fora no parse.
///
/// `flow-root` existe para dizer "sou uma caixa de bloco que ESTABELECE um
/// contexto de formatação", e era mapeada para `DisplayKind::Block` sem mais —
/// a única coisa que a distingue de `block` desaparecia ali.
///
/// O campo é um `bool` ao lado do `display`, e não uma variante nova: a variante
/// obrigava a tratar o caso em quatro ficheiros fora do `style/`, dois deles de
/// outros agentes. É o mesmo arranjo que o `border_box` já faz para o
/// `box-sizing`.
#[test]
fn flow_root_guarda_o_que_o_distingue_de_block() {
    use crate::style::DisplayKind;
    let c = parse_inline("display:flow-root");
    assert_eq!(c.display, Some(DisplayKind::Block), "a CAIXA é de bloco");
    assert_eq!(c.flow_root, Some(true), "e o que o distingue fica guardado");
    // O `block` simples não levanta o campo — é a metade que prova que o teste
    // mede a distinção e não a presença da declaração.
    assert_eq!(parse_inline("display:block").flow_root, None);
    // E o computado responde a palavra, como o browser.
    assert_eq!(c.computed_value("display", None), "flow-root");
    assert_eq!(parse_inline("display:block").computed_value("display", None), "block");
}
