//! Lote DISP — `display` para de perder informação no parse.
//!
//! CSS Display Module Level 3 §2 lê `display` como DUAS palavras
//! (`<display-outside> || <display-inside>`, mais o `list-item` opcional) e
//! trata a forma de uma palavra como o atalho que a spec diz que é. Estes
//! testes fixam que a sintaxe de duas/três palavras passa a ser LIDA — e que
//! nenhuma resposta de serialização que já existia mudou ao aceitá-la.

use crate::style::DisplayKind;
use crate::style::parse::parse_inline;

/// `inline flow-root` é a soletração de duas palavras do keyword legado
/// `inline-block` (CSS Display 3 §2.5) — a mesma caixa, e por isso tem de
/// serializar de volta como `inline-block`, não como `flow-root` nem `flow`.
#[test]
fn inline_flow_root_e_lido_como_inline_block() {
    let c = parse_inline("display:inline flow-root");
    assert_eq!(c.display, Some(DisplayKind::InlineBlock));
    assert_eq!(c.computed_value("display", None), "inline-block");
}

/// `block flow-root` computa exactamente como o keyword `flow-root` sozinho:
/// mesma caixa (`Block`), e o bit que a distingue (`flow_root`) continua a
/// ser levantado, ordem dos tokens à parte.
#[test]
fn block_flow_root_e_lido_como_flow_root() {
    let a = parse_inline("display:block flow-root");
    let b = parse_inline("display:flow-root block");
    for c in [&a, &b] {
        assert_eq!(c.display, Some(DisplayKind::Block));
        assert_eq!(c.flow_root, Some(true));
        assert_eq!(c.computed_value("display", None), "flow-root");
    }
}

/// A forma de uma palavra continua a ser o atalho — nada nesta mudança pode
/// fazer `display:flow-root` deixar de levantar o bit.
#[test]
fn flow_root_de_uma_palavra_nao_regride() {
    let c = parse_inline("display:flow-root");
    assert_eq!(c.display, Some(DisplayKind::Block));
    assert_eq!(c.flow_root, Some(true));
    assert_eq!(c.computed_value("display", None), "flow-root");
}

/// `block flow` e `inline flow` são as soletrações de duas palavras de
/// `block` e `inline` — sem `flow-root`, não estabelecem contexto próprio.
#[test]
fn block_flow_e_inline_flow_sao_lidos() {
    let bloco = parse_inline("display:block flow");
    assert_eq!(bloco.display, Some(DisplayKind::Block));
    assert_eq!(bloco.flow_root, None, "flow sem -root não é flow-root");
    assert_eq!(bloco.computed_value("display", None), "block");

    let inline = parse_inline("display:inline flow");
    assert_eq!(inline.display, Some(DisplayKind::Inline));
    assert_eq!(inline.computed_value("display", None), "inline");
}

/// `block flex` / `inline flex` são as soletrações de duas palavras de
/// `flex` / `inline-flex` — a ordem dos dois tokens não importa.
#[test]
fn block_flex_e_inline_flex_de_duas_palavras() {
    assert_eq!(parse_inline("display:block flex").display, Some(DisplayKind::Flex));
    assert_eq!(parse_inline("display:flex block").display, Some(DisplayKind::Flex));
    let ifx = parse_inline("display:inline flex");
    assert_eq!(ifx.display, Some(DisplayKind::InlineFlex));
    assert_eq!(ifx.computed_value("display", None), "inline-flex");
    let ifx2 = parse_inline("display:flex inline");
    assert_eq!(ifx2.display, Some(DisplayKind::InlineFlex));
}

/// `list-item` combina com `flow` (a forma de três palavras que o browser
/// usa como valor computado de `display:list-item`) — e SÓ com `flow`.
#[test]
fn list_item_de_duas_e_tres_palavras() {
    assert_eq!(
        parse_inline("display:block flow list-item").display,
        Some(DisplayKind::ListItem)
    );
    assert_eq!(
        parse_inline("display:list-item").display,
        Some(DisplayKind::ListItem),
        "a forma de uma palavra não regride"
    );
    // `list-item` não combina com `flex`/`grid`/`table` — a spec não define
    // essa caixa, e não há onde guardar as duas coisas ao mesmo tempo.
    assert_eq!(parse_inline("display:flex list-item").display, None);
}

/// Um token repetido, desconhecido, ou uma sequência longa demais é recusado
/// — cai no `None` (default da tag), nunca numa adivinhação.
#[test]
fn combinacao_invalida_e_recusada() {
    assert_eq!(parse_inline("display:block block").display, None, "outer repetido");
    assert_eq!(parse_inline("display:flow flow").display, None, "inner repetido");
    assert_eq!(parse_inline("display:run-in flow").display, None, "run-in não é modelado");
    assert_eq!(parse_inline("display:contents").display, None, "contents não é modelado");
    assert_eq!(
        parse_inline("display:block flow flow-root extra").display,
        None,
        "mais de três tokens"
    );
}

/// `inline-grid` e `inline-table` guardam o display EXTERIOR, e as duas
/// soletrações — uma palavra e duas — dão a mesma resposta.
///
/// Este teste afirmava o contrário quando foi escrito: que as duas formas
/// partilhavam a mesma PERDA, porque `inline-grid` caía no mesmo valor que
/// `grid`. Era o defeito que `inline-flex` teve e que custou uma grelha
/// inline a tomar a largura do bloco inteiro e a cair numa linha própria. As
/// variantes existem agora e o teste pina o que elas garantem.
#[test]
fn inline_grid_e_inline_table_guardam_o_display_exterior() {
    assert!(DisplayKind::InlineGrid.is_inline_level());
    assert!(DisplayKind::InlineTable.is_inline_level());
    assert!(DisplayKind::InlineGrid.is_grid_container());
    assert!(DisplayKind::InlineTable.is_table_box());
    assert_eq!(parse_inline("display:inline-grid").display, Some(DisplayKind::InlineGrid));
    assert_eq!(parse_inline("display:inline grid").display, Some(DisplayKind::InlineGrid));
    assert_eq!(parse_inline("display:inline-table").display, Some(DisplayKind::InlineTable));
    assert_eq!(parse_inline("display:inline table").display, Some(DisplayKind::InlineTable));
}
