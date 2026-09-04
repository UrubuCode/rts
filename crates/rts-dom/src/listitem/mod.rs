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
/// `content_x`/`content_y` são o canto superior-esquerdo do CONTENT-BOX do item.
/// De que lado do `content_x` o marcador cai decide-o o `list-style-position`;
/// em nenhum dos dois casos ele desloca coisa nenhuma — nenhuma medida de
/// largura muda por haver marcador, que é o que o browser também faz.
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
    // `list-style-image` vence o `type` no CSS. Os pixels não vêm da URL
    // aqui — quem os baixa e decodifica é o loader do lado JS/browser, o
    // mesmo caminho de um `<img>` (`dom.set_image`/`image_of`, ver
    // `dom/formulario.rs`) — só que a CHAVE é o `<li>` em vez de um `<img>`,
    // porque um marcador de lista não tem elemento próprio. Reusa o MESMO
    // `DisplayItem::Image` que `layout_image` emite (não um segundo emissor
    // de imagem só para marcadores).
    //
    // ⚠️ NÃO FEITO: nenhum código deste crate CHAMA `dom.set_image(li, …)`
    // para um `list-style-image` — o loader (fetch + decode) que o faz para
    // `<img>` vive fora de `rts-dom` (o browser/mini-browser em TS) e ainda
    // não sabe procurar `list-style-image` na cascade de um `<li>`. Até essa
    // ponte existir, `image_of(id)` responde sempre `None` aqui e o bullet do
    // `type` continua ausente (correto: a propriedade vence o `type`, mesmo
    // sem imagem para mostrar — é o que o Chrome também faz enquanto a
    // imagem carrega).
    if css.list_style_image_url().is_some() {
        if let Some((handle, off, iw, ih)) = dom
            .image_of(id)
            .filter(|(h, _, iw, ih)| *h != 0 && *iw != 0 && *ih != 0)
        {
            // O mesmo diâmetro do bullet (`BULLET_EM`), para uma imagem de
            // marcador não dominar a linha — o Chrome escala pela mesma
            // lógica de um `list-style-image` pequeno (ícone, não foto).
            let d = font_size * BULLET_EM * 2.0;
            let dentro = css.list_style_position == Some(crate::style::ListStylePosition::Inside);
            let borda_direita = if dentro {
                content_x + d
            } else {
                content_x - font_size * MARKER_GAP_EM
            };
            let line_h = ctx.measurer.line_height(font_size);
            let rect = Rect::new(
                borda_direita - d,
                content_y + (line_h - d) / 2.0,
                d,
                d,
            );
            list.items.push(DisplayItem::Image {
                rect,
                pixels_handle: handle,
                pixels_off: off,
                img_w: iw,
                img_h: ih,
            });
        }
        return;
    }
    // `::marker { color: … }` (lote O) vence a cor herdada do `<li>` quando
    // alguma regra o declara; ver o porquê de só a cor em `Dom::marker_color`.
    let color = dom
        .marker_color(id, css)
        .or(css.color)
        .unwrap_or(0x0000_00FF);
    let line_h = ctx.measurer.line_height(font_size);
    // `list-style-position` decide de que LADO de `content_x` o marcador cai.
    //
    // `outside` (o default) põe-no fora da caixa de conteúdo, dentro do recuo que
    // o `<ul>` reservou. `inside` põe-no DENTRO, como primeira coisa da linha.
    //
    // Em nenhum dos dois a GEOMETRIA do item muda, e no `inside` isso é a parte
    // que interessa: o browser trata o marcador como uma caixa inline no início
    // da primeira linha, portanto a caixa do `<li>` é a mesma que teria sem
    // marcador nenhum. O que ainda não fazemos é EMPURRAR essa primeira linha
    // para a direita — o marcador é pintado no início dela e pode sobrepor-se à
    // primeira palavra. É um refino do fluxo inline, não deste ficheiro:
    // empurrar a linha daqui exigiria mexer na largura do item, e isso sim
    // partiria a geometria que hoje está certa.
    let dentro = css.list_style_position == Some(crate::style::ListStylePosition::Inside);
    // A borda direita do marcador: à esquerda do texto quando `outside`, no
    // próprio início do conteúdo quando `inside`.
    let borda_direita = if dentro {
        content_x + largura_marcador(kind, dom, id, font_size, ctx)
    } else {
        content_x - font_size * MARKER_GAP_EM
    };

    match kind {
        ListStyleType::Disc | ListStyleType::Circle | ListStyleType::Square => {
            let d = font_size * BULLET_EM;
            // O bullet fica centrado na primeira linha — é onde o browser o
            // alinha (à linha de base do primeiro texto, não ao topo da caixa).
            let rect = Rect::new(borda_direita - d, content_y + (line_h - d) / 2.0, d, d);
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
                ListStyleType::Square => list.items.push(DisplayItem::SolidRect {
                    rect,
                    color,
                    radius: crate::layout::Corners::ZERO,
                }),
                _ => list.items.push(DisplayItem::SolidRect {
                    rect,
                    color,
                    radius: crate::layout::Corners::same(d / 2.0),
                }),
            }
        }
        _ => {
            // Marcador TEXTUAL (número, letra, romano). Alinhado à DIREITA: o
            // ponto final de "9." e o de "10." caem na mesma vertical, que é o
            // que o browser faz e o que torna uma lista longa legível.
            let n = ordinal(dom, id);
            let text = format!("{}.", counter_text(kind, n));
            let w = ctx
                .measurer
                .text_width(&text, font_size, false, false, false);
            list.items.push(DisplayItem::Text {
                x: borda_direita - w,
                y: content_y,
                text: text.into(),
                color,
                size: font_size,
                mono: false,
                bold: false,
                // A cor JÁ vem do `::marker` quando a folha o declara (ver
                // `color` acima); peso/itálico do marcador continuam fixos —
                // mudar a fonte mudaria a medida (`w`), fora deste lote.
                italic: false,
                letter_spacing: 0.0,
                decoration: 0,
            });
        }
    }
}

/// A largura que o marcador ocupa — o que o `inside` precisa de saber para o
/// pôr no início do conteúdo em vez de o alinhar pela direita.
fn largura_marcador(
    kind: ListStyleType,
    dom: &Dom,
    id: NodeIdx,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match kind {
        ListStyleType::Disc | ListStyleType::Circle | ListStyleType::Square => {
            font_size * BULLET_EM
        }
        _ => {
            let t = format!("{}.", counter_text(kind, ordinal(dom, id)));
            ctx.measurer.text_width(&t, font_size, false, false, false)
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
    if let Some(v) = node
        .attr("value")
        .and_then(|v| v.trim().parse::<i64>().ok())
    {
        return v;
    }
    let Some(parent) = node.parent else { return 1 };
    let start = dom
        .node(parent)
        .attr("start")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(1);

    let children = &dom.node(parent).children;
    let Some(pos) = children.iter().position(|&c| c == id) else {
        return start;
    };
    // Conta para TRÁS: cada irmão anterior que também é item de lista vale 1, e
    // um `value` explícito num deles REINICIA a contagem a partir dali (é o que
    // o HTML manda, e é o que faz a varredura poder parar cedo).
    let mut n = 0i64;
    for &sib in children[..pos].iter().rev() {
        if !is_list_item(dom, sib) {
            continue;
        }
        if let Some(v) = dom
            .node(sib)
            .attr("value")
            .and_then(|v| v.trim().parse::<i64>().ok())
        {
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
pub(crate) fn counter_text(kind: ListStyleType, n: i64) -> String {
    match kind {
        ListStyleType::LowerAlpha => alphabetic(n, b'a'),
        ListStyleType::UpperAlpha => alphabetic(n, b'A'),
        ListStyleType::LowerRoman => roman(n)
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| n.to_string()),
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
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
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
mod tests;
