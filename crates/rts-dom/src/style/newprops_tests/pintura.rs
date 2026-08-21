//! A cauda de pintura (`style::painting`) e as camadas da máscara
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── LOTE C: a cauda de pintura (ver `style::painting`) ───────────────────────

#[test]
fn background_clip_guarda_as_quatro_caixas_incluindo_text() {
    // `text` é o valor do idioma "texto com gradiente" e é o que 12 das 43
    // declarações do corpus escrevem. Guardá-lo não pinta nada — ver o
    // comentário do tipo para porque isso não é uma regressão.
    use crate::style::painting::BackgroundClip;
    assert_eq!(
        parse_inline("background-clip: padding-box").background_clip,
        Some(BackgroundClip::PaddingBox)
    );
    assert_eq!(
        parse_inline("background-clip: text").background_clip,
        Some(BackgroundClip::Text)
    );
    // a grafia prefixada é a que as folhas escrevem para o valor `text`.
    assert_eq!(
        parse_inline("-webkit-background-clip: text").background_clip,
        Some(BackgroundClip::Text)
    );
    assert_eq!(
        parse_inline("color: red").computed_value("background-clip", None),
        "border-box"
    );
}

#[test]
fn os_dois_blend_mode_partilham_o_vocabulario_mas_nao_o_campo() {
    // O vocabulário é o mesmo (é o `<blend-mode>` da spec), e por isso é um tipo
    // só — dois enums com as mesmas 16 variantes seriam duas respostas à mesma
    // pergunta. Mas os CAMPOS são distintos: uma folha que declare os dois na
    // mesma regra tem de os manter separados, e um campo partilhado apagaria um.
    use crate::style::painting::BlendMode;
    let s = parse_inline("mix-blend-mode: multiply; background-blend-mode: screen");
    assert_eq!(s.mix_blend_mode, Some(BlendMode::Multiply));
    assert_eq!(s.background_blend_mode, Some(BlendMode::Screen));
    // as 16 do corpus são aceites, e um valor que não existe não vira outro.
    assert_eq!(
        parse_inline("mix-blend-mode: luminosity").mix_blend_mode,
        Some(BlendMode::Luminosity)
    );
    assert_eq!(parse_inline("mix-blend-mode: plusma").mix_blend_mode, None);
}

#[test]
fn text_shadow_reusa_a_sombra_de_caixa_mas_sem_spread() {
    // O reúso é a decisão do lote; o corte é que `text-shadow` NÃO tem spread. O
    // parser de caixa lê um quarto comprimento como spread, e guardá-lo aqui
    // seria inventar um valor que a spec desta propriedade não define.
    let s = parse_inline("text-shadow: 1px 2px 3px 4px red")
        .text_shadow
        .unwrap();
    assert_eq!((s.dx, s.dy, s.blur), (1.0, 2.0, 3.0));
    assert_eq!(s.spread, 0.0, "text-shadow não tem spread");
    // e a forma normal, que é a que as folhas escrevem.
    let n = parse_inline("text-shadow: 0 1px 2px rgba(0,0,0,0.5)")
        .text_shadow
        .unwrap();
    assert_eq!((n.dx, n.dy, n.blur), (0.0, 1.0, 2.0));
    assert!(parse_inline("text-shadow: none").text_shadow.is_none());
}

#[test]
fn text_shadow_computa_com_a_cor_a_frente_como_o_chrome() {
    // O Chrome serializa a sombra com a cor primeiro, mesmo quando o autor a
    // escreveu no fim. Responder pela ordem do autor era o desvio fácil.
    let s = parse_inline("text-shadow: 2px 2px rgb(255, 0, 0)");
    assert_eq!(s.get_property("text-shadow"), "rgb(255, 0, 0) 2px 2px 0px");
    assert_eq!(
        parse_inline("color: red").computed_value("text-shadow", None),
        "none"
    );
}

#[test]
fn a_cauda_de_pintura_nao_muda_um_pixel_hoje() {
    // O que este lote NÃO promete, fixado ao lado do que promete. Nenhuma das
    // quatro tem consumidor: se um dia o pintor as ler, este teste cai — e é
    // esse o sinal, não uma defesa do estado atual.
    let sem = layout(
        "<div style='width:50px;height:10px;background:#ff0000'>a</div>",
        800.0,
    );
    let com = layout(
        "<div style='width:50px;height:10px;background:#ff0000;background-clip:content-box;\
         mix-blend-mode:multiply;background-blend-mode:screen;text-shadow:1px 1px 2px #000'>a</div>",
        800.0,
    );
    assert_eq!(format!("{:?}", itens(&sem)), format!("{:?}", itens(&com)));
}

// ── LOTE D: o resto da cauda de pintura e a máscara ─────────────────────────

#[test]
fn background_origin_reusa_as_caixas_do_clip_mas_recusa_text() {
    // Reusa o tipo em vez de um enum gémeo — mas a spec não define `text` aqui,
    // e aceitá-lo guardaria uma caixa que esta propriedade não tem.
    use crate::style::painting::BackgroundClip;
    assert_eq!(
        parse_inline("background-origin: content-box").background_origin,
        Some(BackgroundClip::ContentBox)
    );
    assert_eq!(
        parse_inline("background-origin: text").background_origin,
        None
    );
    // e o `clip` continua a aceitá-lo: são campos distintos com um tipo comum.
    assert_eq!(
        parse_inline("background-clip: text").background_clip,
        Some(BackgroundClip::Text)
    );
}

#[test]
fn as_camadas_da_mascara_reusam_a_gramatica_do_fundo() {
    // `mask-size`/`-position`/`-repeat` têm a MESMA gramática das de fundo, e é
    // o parser de fundo que as lê. Um segundo parser divergiria do primeiro à
    // primeira correção.
    let s = parse_inline("mask-size: cover; mask-position: 50% 50%; mask-repeat: no-repeat");
    assert_eq!(s.get_property("mask-size"), "cover");
    assert_eq!(s.get_property("mask-position"), "50% 50%");
    assert_eq!(s.get_property("mask-repeat"), "no-repeat");
    // a grafia prefixada é a que as folhas escrevem.
    assert_eq!(
        parse_inline("-webkit-mask-size: contain").get_property("mask-size"),
        "contain"
    );
}

#[test]
fn guardar_as_camadas_da_mascara_nao_mexe_na_supressao_de_fundo() {
    // A condição que o lote tinha de verificar antes de entrar: quem decide
    // suprimir o fundo é `layout::deve_suprimir_fundo`, que lê APENAS
    // `mask_image`. Uma caixa com `mask-size` e sem `mask-image` continua a
    // pintar o fundo — o contrário seria um fundo a desaparecer sem máscara.
    let com_tamanho = layout(
        "<div style='width:50px;height:10px;background:#ff0000;mask-size:cover'>a</div>",
        800.0,
    );
    let simples = layout(
        "<div style='width:50px;height:10px;background:#ff0000'>a</div>",
        800.0,
    );
    assert_eq!(
        format!("{:?}", itens(&simples)),
        format!("{:?}", itens(&com_tamanho))
    );
    assert!(
        first_solid(&com_tamanho).is_some(),
        "o fundo continua pintado"
    );
}

#[test]
fn tab_size_guarda_a_contagem_e_recusa_o_comprimento() {
    // `tab-size` aceita um número (caracteres) OU um comprimento (largura), e os
    // dois não cabem no mesmo `f32` sem perder qual é qual. Só o número entra;
    // um comprimento devolve `None` em vez de ser guardado como se fosse contagem.
    assert_eq!(parse_inline("tab-size: 4").tab_size, Some(4.0));
    assert_eq!(parse_inline("tab-size: 4px").tab_size, None);
    assert_eq!(
        parse_inline("color: red").computed_value("tab-size", None),
        "8"
    );
}

#[test]
fn scrollbar_color_exige_as_duas_cores() {
    // A spec pede polegar E calha. `auto` e uma cor sozinha não são guardados:
    // deduzir a calha a partir do polegar daria uma cor que o autor não escreveu.
    let s = parse_inline("scrollbar-color: rgb(255, 0, 0) rgb(0, 0, 255)");
    assert_eq!(
        s.get_property("scrollbar-color"),
        "rgb(255, 0, 0) rgb(0, 0, 255)"
    );
    assert_eq!(parse_inline("scrollbar-color: auto").scrollbar_color, None);
    assert_eq!(parse_inline("scrollbar-color: red").scrollbar_color, None);
}

#[test]
fn text_fill_color_e_underline_offset_sao_guardados_sem_pintar() {
    // As duas caudas do texto. `-webkit-text-fill-color` é a outra metade do
    // idioma do texto com gradiente; quem pinta texto lê `color`, não esta.
    let s = parse_inline("-webkit-text-fill-color: rgb(0, 128, 0)");
    assert_eq!(s.get_property("-webkit-text-fill-color"), "rgb(0, 128, 0)");
    assert_eq!(
        parse_inline("text-underline-offset: 3px").get_property("text-underline-offset"),
        "3px"
    );
    // `auto` é o inicial, e um `Option` já o exprime sem variante extra.
    assert_eq!(
        parse_inline("text-underline-offset: auto").text_underline_offset,
        None
    );
    assert_eq!(
        parse_inline("text-decoration-style: wavy").get_property("text-decoration-style"),
        "wavy"
    );
}

#[test]
fn print_color_adjust_e_recusada_pelo_mesmo_motivo_que_os_page_break() {
    // Não há impressão nenhuma. Vai para a coluna das recusadas, não para a das
    // implementadas — a diferença é o que impede a contagem de subir sem um
    // pixel mudar.
    use crate::style::inert::is_inert;
    assert!(is_inert("print-color-adjust"));
    assert!(
        is_inert("-webkit-print-color-adjust"),
        "a grafia que as folhas escrevem"
    );
    assert!(is_inert("color-adjust"), "o nome antigo");
}
