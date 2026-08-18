//! `display: list-item` — o MARCADOR de um item de lista (o ponto, o círculo, o
//! quadrado, o número).
//!
//! ## Por que um módulo e não seis linhas no `layout.rs`
//!
//! O marcador não muda o fluxo: um `<li>` empilha os filhos como qualquer bloco.
//! O que ele traz é uma pergunta que o fluxo de bloco não sabe responder — *qual
//! é o meu número* — e essa pergunta atravessa irmãos, o atributo `start` do
//! `<ol>` e o atributo `value` do próprio `<li>`. Deixá-la no meio do
//! `layout_block` misturaria contagem de lista com box model num ficheiro que já
//! passou do teto.
//!
//! ## A contagem é DERIVADA, não acumulada
//!
//! Um contador mutável carregado pela travessia seria o desenho óbvio, e é o
//! errado aqui: o layout deste motor mede subárvores fora de ordem (flex e grid
//! medem um filho antes de o posicionar, e o cache de fragmentos relayouta um
//! ramo sozinho), então um contador veria a mesma lista duas vezes e numeraria
//! `1, 2, 3, 4, 3, 4`. O número é calculado a partir da POSIÇÃO do nó entre os
//! irmãos — a mesma resposta em qualquer ordem de visita, e a única que
//! sobrevive ao relayout parcial.
//!
//! O custo é O(irmãos) por item, e portanto O(n²) numa lista; é aceitável porque
//! a alternativa está errada, e porque a varredura pára no primeiro irmão que
//! não é item de lista.

use crate::layout::{DisplayItem, DisplayList, LayoutCtx, Rect};
use crate::style::{ComputedStyle, DisplayKind, ListStyleType};
use crate::{Dom, NodeIdx};

/// A distância entre o marcador e o texto, em `em`. O Chrome não expõe um número
/// para isto (o `::marker` é uma caixa cujo tamanho depende da fonte); 0.5em põe
/// o ponto onde a lista de uma página real o mostra.
const MARKER_GAP_EM: f32 = 0.5;

/// O diâmetro do `disc`/`circle` e o lado do `square`, em `em`. O Chrome desenha
/// o bullet a ~0.35em da fonte do item.
const BULLET_EM: f32 = 0.35;

/// Emite o marcador de um `display:list-item` na display list.
///
/// `content_x`/`content_y` são o canto superior-esquerdo do CONTENT-BOX do item —
/// o marcador de um `list-style-position: outside` (o default, e o único que este
/// motor desenha hoje) fica à ESQUERDA dele, fora da caixa de conteúdo, e por
/// isso não desloca nada: nenhuma medida de largura muda por haver marcador.
///
/// Não emite nada quando `list-style-type: none` — que é o caso mais comum numa
/// página real, onde `<ul>` é o markup de um menu.
pub(crate) fn emit_marker(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    content_x: f32,
    content_y: f32,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let kind = css.list_style_type.unwrap_or_else(|| tipo_da_ua(dom, id));
    if kind == ListStyleType::None {
        return;
    }
    // `list-style-image` vence o `type` no CSS. Não temos os pixels aqui (quem
    // baixa é a camada de imagem), e desenhar o bullet por baixo de uma imagem
    // que vai chegar seria pior do que não desenhar: o autor que pôs uma imagem
    // não quer o ponto. Sai sem marcador, e fica dito.
    if css.list_style_image.is_some() {
        return;
    }
    let color = css.color.unwrap_or(0x0000_00FF);
    let line_h = ctx.measurer.line_height(font_size);

    match kind {
        ListStyleType::Disc | ListStyleType::Circle | ListStyleType::Square => {
            let d = font_size * BULLET_EM;
            // O bullet fica com a borda DIREITA a `MARKER_GAP_EM` do texto e
            // centrado na primeira linha (é onde o browser o alinha: à linha de
            // base do primeiro texto, não ao topo da caixa).
            let right = content_x - font_size * MARKER_GAP_EM;
            let rect = Rect::new(right - d, content_y + (line_h - d) / 2.0, d, d);
            match kind {
                // `circle` é o ÚNICO vazado: um anel. Espessura 1px é o que o
                // Chrome desenha em qualquer tamanho de fonte usual.
                ListStyleType::Circle => list.items.push(DisplayItem::Border {
                    rect,
                    width: 1.0,
                    color,
                    radius: d / 2.0,
                }),
                // `square` é o mesmo rect sem raio — a única diferença.
                ListStyleType::Square => {
                    list.items.push(DisplayItem::SolidRect { rect, color, radius: 0.0 })
                }
                _ => list.items.push(DisplayItem::SolidRect { rect, color, radius: d / 2.0 }),
            }
        }
        _ => {
            // Marcador TEXTUAL (número, letra, romano). Alinhado à DIREITA: o
            // ponto final de "9." e o de "10." caem na mesma vertical, que é o
            // que o browser faz e o que torna uma lista longa legível.
            let n = ordinal(dom, id);
            let text = format!("{}.", counter_text(kind, n));
            let w = ctx.measurer.text_width(&text, font_size, false, false);
            list.items.push(DisplayItem::Text {
                x: content_x - font_size * MARKER_GAP_EM - w,
                y: content_y,
                text: text.into(),
                color,
                size: font_size,
                mono: false,
                bold: false,
                letter_spacing: 0.0,
                decoration: 0,
            });
        }
    }
}

/// O `list-style-type` da UA quando o autor não declarou nenhum: `decimal`
/// dentro de um `<ol>`, `disc` no resto.
///
/// Resolvido subindo até à caixa de lista mais próxima em vez de ser uma regra
/// da UA-stylesheet porque aquela regista valores por SLOT inteiro
/// (`define_style`), e não há slot para `list-style-type` — nem faria sentido
/// abrir um, já que a propriedade é herdada e o slot é por tag. Subir dá a
/// mesma resposta que a herança daria, incluindo o caso que importa: um `<ol>`
/// dentro de um `<ul>` numera, não põe pontos.
fn tipo_da_ua(dom: &Dom, id: NodeIdx) -> ListStyleType {
    let mut cur = dom.node(id).parent;
    while let Some(p) = cur {
        if let crate::NodeKind::Element { tag } = &dom.node(p).kind {
            match tag.as_str() {
                "ol" => return ListStyleType::Decimal,
                "ul" | "menu" | "dir" => return ListStyleType::Disc,
                _ => {}
            }
        }
        cur = dom.node(p).parent;
    }
    ListStyleType::Disc
}

/// O NÚMERO deste item: 1 para o primeiro da lista, e daí em diante.
///
/// Três fontes, na precedência do HTML: o atributo `value` do próprio `<li>`
/// manda; senão o `start` do `<ol>` que o contém dá o número do primeiro; senão
/// começa em 1. Um `<li>` cujo irmão anterior traz `value` continua a contar a
/// partir dele — é a regra do HTML, e é o que faz uma lista partida ao meio
/// (`<ol start=5>`) numerar certo.
fn ordinal(dom: &Dom, id: NodeIdx) -> i64 {
    let node = dom.node(id);
    if let Some(v) = node.attr("value").and_then(|v| v.trim().parse::<i64>().ok()) {
        return v;
    }
    let Some(parent) = node.parent else { return 1 };
    let start = dom
        .node(parent)
        .attr("start")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(1);

    let children = &dom.node(parent).children;
    let Some(pos) = children.iter().position(|&c| c == id) else { return start };
    // Conta para TRÁS: cada irmão anterior que também é item de lista vale 1, e
    // um `value` explícito num deles REINICIA a contagem a partir dali (é o que
    // o HTML manda, e é o que faz a varredura poder parar cedo).
    let mut n = 0i64;
    for &sib in children[..pos].iter().rev() {
        if !is_list_item(dom, sib) {
            continue;
        }
        if let Some(v) = dom.node(sib).attr("value").and_then(|v| v.trim().parse::<i64>().ok()) {
            return v + n + 1;
        }
        n += 1;
    }
    start + n
}

/// `true` se o nó é um `display:list-item` — a pergunta que a contagem faz sobre
/// cada irmão. Um `<li>` com `display:none` (ou virado `flex` pelo autor) NÃO
/// conta, e é por isso que a pergunta é sobre o display computado e não sobre a
/// tag: um menu que esconde metade dos itens numera os visíveis 1, 2, 3.
fn is_list_item(dom: &Dom, id: NodeIdx) -> bool {
    crate::layout::used_display(dom, id) == Some(DisplayKind::ListItem)
}

/// O texto do contador para um `n` — sem o ponto final, que quem chama junta.
///
/// `n` fora do domínio de um sistema (zero ou negativo em romano/alfabético)
/// volta como o próprio número em decimal. É o que o CSS manda
/// (`counter-style` faz *fallback* para `decimal`), e evita a única alternativa
/// possível, que seria devolver vazio e perder o item da vista.
fn counter_text(kind: ListStyleType, n: i64) -> String {
    match kind {
        ListStyleType::LowerAlpha => alphabetic(n, b'a'),
        ListStyleType::UpperAlpha => alphabetic(n, b'A'),
        ListStyleType::LowerRoman => roman(n).map(|s| s.to_lowercase()).unwrap_or_else(|| n.to_string()),
        ListStyleType::UpperRoman => roman(n).unwrap_or_else(|| n.to_string()),
        _ => n.to_string(),
    }
}

/// Numeração ALFABÉTICA bijetiva (a…z, aa…az, ba…): 26 não é "a0", é "z", e 27 é
/// "aa". É por isso que o laço subtrai 1 antes de dividir — a aritmética
/// posicional normal (base 26 com zero) daria "a@" e "ba" trocados.
fn alphabetic(n: i64, base: u8) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let mut n = n;
    let mut out = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.push(base + rem);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII")
}

/// Romano MAIÚSCULO, ou `None` fora do domínio (o romano não tem zero nem
/// negativo, e acima de 3999 exigiria a notação com barra que ninguém usa numa
/// lista).
fn roman(n: i64) -> Option<String> {
    if !(1..=3999).contains(&n) {
        return None;
    }
    const TAB: &[(i64, &str)] = &[
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut n = n;
    let mut out = String::new();
    for &(v, s) in TAB {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn romano_cobre_os_subtrativos() {
        assert_eq!(roman(4).unwrap(), "IV");
        assert_eq!(roman(9).unwrap(), "IX");
        assert_eq!(roman(1994).unwrap(), "MCMXCIV");
        assert!(roman(0).is_none());
    }

    #[test]
    fn alfabetico_e_bijetivo_no_salto_de_z_para_aa() {
        assert_eq!(alphabetic(1, b'a'), "a");
        assert_eq!(alphabetic(26, b'a'), "z");
        assert_eq!(alphabetic(27, b'a'), "aa");
        assert_eq!(alphabetic(52, b'A'), "AZ");
        assert_eq!(alphabetic(53, b'A'), "BA");
    }
}
