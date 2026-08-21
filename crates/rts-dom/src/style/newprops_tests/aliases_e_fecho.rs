//! Aliases de fornecedor, as duas sintaxes de flexbox e o fecho da lista
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── LOTE E: aliases de fornecedor e as duas sintaxes antigas de flexbox ─────

#[test]
fn prefixo_de_fornecedor_cai_na_propriedade_nua() {
    // 16 nomes do corpus são o mesmo nome com um prefixo. A tentativa sem
    // prefixo é a ÚLTIMA do `parse`, e por isso resolve-os todos sem uma
    // segunda lista de aliases a dessincronizar da primeira.
    use crate::style::values::BorderStyle;
    assert_eq!(parse_inline("-webkit-box-sizing: border-box").border_box, Some(true));
    assert!(parse_inline("-webkit-box-shadow: 0 2px 4px #000").box_shadow.is_some());
    assert_eq!(parse_inline("-moz-column-gap: 8px").gap, Some(Dimension::Px(8.0)));
    assert!(parse_inline("-ms-transform: scale(2)").transform.is_some());
    assert!(parse_inline("-o-transform: scale(2)").transform.is_some());
    // e os quatro prefixos, não só os dois que `vocab`/`timing` já cortavam.
    assert_eq!(parse_inline("-o-object-fit: cover").get_property("object-fit"), "cover");
    // uma que não existe continua a não existir: a tentativa não inventa nomes.
    assert_eq!(parse_inline("-webkit-nao-existe: 1").get_property("nao-existe"), "");
    let _ = BorderStyle::Solid;
}

#[test]
fn flexbox_moderna_prefixada_e_alias_mas_a_de_2009_nao_e() {
    // A distinção que decidiu o lote. `-webkit-flex-direction` é a MODERNA com
    // um prefixo — mesmo nome, mesmo valor, alias puro. `-webkit-box-direction`
    // e `-ms-flex-direction` são as sintaxes de 2009/2012, com semântica
    // diferente: traduzi-las por prefixo daria valores errados em silêncio.
    use crate::style::values::FlexDirection;
    assert_eq!(
        parse_inline("-webkit-flex-direction: column").flex_direction,
        Some(FlexDirection::Column)
    );
    // a antiga é RECUSADA com motivo, não aplicada e não desconhecida.
    use crate::style::inert::is_inert;
    assert!(is_inert("-ms-flex-direction"), "sintaxe de 2012");
    assert!(is_inert("-ms-flex"), "e o shorthand dela");
    assert!(is_inert("-webkit-box-flex"), "sintaxe de 2009");
    assert!(is_inert("-webkit-box-ordinal-group"));
    // e o corte tem de deixar a MODERNA em paz — é o risco todo desta função.
    assert!(!is_inert("flex"), "a moderna nua NÃO é recusada");
    assert!(!is_inert("flex-direction"));
    assert!(!is_inert("-webkit-flex-grow"), "nem a moderna prefixada");
}

#[test]
fn a_antiga_que_o_vocab_ja_traduzia_continua_a_ser_traduzida() {
    // `box-orient`/`box-pack`/`box-align` são de 2009 mas o `style::vocab` já as
    // mapeava nos campos de hoje, e ele corre ANTES do `inert` na cadeia. O
    // grupo novo das recusadas não pode ter-lhes roubado o caminho.
    use crate::style::values::FlexDirection;
    assert_eq!(
        parse_inline("-webkit-box-orient: vertical").flex_direction,
        Some(FlexDirection::Column)
    );
    assert_eq!(
        parse_inline("-webkit-box-pack: justify").justify,
        Some(crate::style::values::JustifyContent::SpaceBetween)
    );
}

// ── LOTE F: o fecho da lista ────────────────────────────────────────────────

#[test]
fn grid_auto_flow_le_o_eixo_e_o_dense_em_qualquer_ordem() {
    use crate::style::grid_lines::GridAutoFlow;
    assert_eq!(
        parse_inline("grid-auto-flow: column dense").grid_auto_flow,
        Some(GridAutoFlow { coluna: true, dense: true })
    );
    assert_eq!(
        parse_inline("grid-auto-flow: dense").grid_auto_flow,
        Some(GridAutoFlow { coluna: false, dense: true })
    );
    // o Chrome imprime o eixo mesmo quando o autor o omitiu.
    assert_eq!(parse_inline("grid-auto-flow: dense").get_property("grid-auto-flow"), "row dense");
    // um token fora da gramática invalida tudo, em vez de dar um `row` que
    // ninguém escreveu.
    assert_eq!(parse_inline("grid-auto-flow: column banana").grid_auto_flow, None);
}

#[test]
fn grid_auto_columns_usa_o_mesmo_tipo_de_trilha_que_a_irma() {
    // A metade que faltava: `grid-auto-rows` já existia, tipada e CONSUMIDA pelo
    // layout. Guardar esta como string crua teria sido um segundo modelo de
    // trilha dentro da mesma tabela — e `GridTrack` já sabe ler `minmax(0, 1fr)`.
    use crate::style::GridTrack;
    assert_eq!(
        parse_inline("grid-auto-columns: minmax(0, 1fr)").grid_auto_columns,
        Some(GridTrack::Fr(1.0)),
        "minmax com máximo flexível É a trilha flexível"
    );
    assert_eq!(parse_inline("grid-auto-columns: min-content").grid_auto_columns, Some(GridTrack::Auto));
    // e a irmã continua a responder o que respondia.
    assert!(parse_inline("grid-auto-rows: 1fr").grid_auto_rows.is_some());
}

#[test]
fn grid_gap_e_o_nome_antigo_de_gap() {
    // Alias puro, reentregue ao braço do `gap` em vez de uma segunda expansão
    // do par — que divergiria da primeira à primeira correção.
    assert_eq!(parse_inline("grid-gap: 8px").gap, parse_inline("gap: 8px").gap);
    assert_eq!(parse_inline("grid-gap: 8px").row_gap, parse_inline("gap: 8px").row_gap);
}

#[test]
fn a_caixa_logica_deixa_de_ser_assimetrica() {
    use crate::style::values::Side;
    // O buraco que encontrei ao verificar o denominador: o `parse` tinha
    // `margin-block-end` por literal mas não `padding-block-end`, que caía como
    // desconhecida ao lado de uma irmã que funcionava. A tradução de eixo fecha
    // as quatro famílias, e este teste é o que impede a assimetria de voltar.
    assert_eq!(parse_inline("padding-block-end: 4px").padding.bottom, Side::Len(Dimension::Px(4.0)));
    assert_eq!(parse_inline("padding-block-start: 4px").padding.top, Side::Len(Dimension::Px(4.0)));
    assert_eq!(parse_inline("margin-inline-end: 4px").margin.right, Side::Len(Dimension::Px(4.0)));
    assert_eq!(parse_inline("padding-inline-start: 4px").padding.left, Side::Len(Dimension::Px(4.0)));
}

#[test]
fn dimensoes_logicas_caem_na_largura_e_na_altura() {
    use crate::style::values::Side;
    // `inline-size` é a largura em escrita horizontal — o mesmo corte LTR que o
    // resto de `style::logical` assume. Reentrega ao `parse` para apanhar
    // keywords e `calc()` sem uma segunda leitura de comprimento.
    assert_eq!(parse_inline("inline-size: 120px").width, Some(Dimension::Px(120.0)));
    assert_eq!(parse_inline("block-size: 40px").height, Some(Dimension::Px(40.0)));
    assert_eq!(parse_inline("min-inline-size: 10px").min_width, Some(Dimension::Px(10.0)));
    // e a forma antiga do WebKit para a margem lógica entra pela mesma porta.
    assert_eq!(parse_inline("-webkit-margin-end: 6px").margin.right, Side::Len(Dimension::Px(6.0)));
}

#[test]
fn place_items_expande_para_os_dois_eixos() {
    use crate::style::values::AlignItems;
    let um = parse_inline("place-items: center");
    assert_eq!(um.align_items, Some(AlignItems::Center));
    assert_eq!(um.grid_justify_items, Some(AlignItems::Center), "um valor vale para os dois");
}

#[test]
fn a_cauda_do_texto_e_do_fundo_e_guardada_com_o_valor_verdadeiro() {
    use crate::style::painting::{BackgroundAttachment, BoxDecorationBreak, LineBreak};
    assert_eq!(
        parse_inline("background-attachment: fixed").background_attachment,
        Some(BackgroundAttachment::Fixed)
    );
    assert_eq!(
        parse_inline("box-decoration-break: clone").box_decoration_break,
        Some(BoxDecorationBreak::Clone)
    );
    assert_eq!(parse_inline("line-break: strict").line_break, Some(LineBreak::Strict));
    assert_eq!(parse_inline("caret-color: rgb(1, 2, 3)").get_property("caret-color"), "rgb(1, 2, 3)");
    assert_eq!(
        parse_inline("text-decoration-thickness: 2px").get_property("text-decoration-thickness"),
        "2px"
    );
    // `from-font` pede uma métrica que o medidor não expõe: cai em não-declarada
    // em vez de virar um comprimento inventado.
    assert_eq!(parse_inline("text-decoration-thickness: from-font").text_decoration_thickness, None);
}

#[test]
fn backdrop_filter_e_recusa_medida_e_nao_lista_de_afazeres() {
    // Recusada em 2026-08-21 com número: ZERO elementos precisam dela nas duas
    // páginas testadas, contra 3-4 passes de GPU por elemento por frame. O
    // motivo está em `docs/ui/css-support.md` §4.5.1 e no `inert.rs`.
    use crate::style::inert::is_inert;
    assert!(is_inert("backdrop-filter"));
    assert!(is_inert("-webkit-backdrop-filter"), "a grafia que as folhas escrevem");
    // e o `filter` normal NÃO é recusa — é do agente do paint, e está implementado.
    assert!(!is_inert("filter"));
}
