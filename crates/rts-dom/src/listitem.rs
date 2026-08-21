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
    // `list-style-image` vence o `type` no CSS. Não temos os pixels aqui (quem
    // baixa é a camada de imagem), e desenhar o bullet por baixo de uma imagem
    // que vai chegar seria pior do que não desenhar: o autor que pôs uma imagem
    // não quer o ponto. Sai sem marcador, e fica dito.
    if css.list_style_image_url().is_some() {
        return;
    }
    let color = css.color.unwrap_or(0x0000_00FF);
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
            // o MARCADOR de lista (bullet/número) não é conteúdo do autor: no
            // browser herda o estilo do `<li>`, mas nada aqui lho passa ainda —
            // fica regular, como já ficava o peso na linha acima.
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
mod tests {
    use super::*;

    /// Os bullets emitidos: quadrados pequenos e redondos.
    fn bullets(list: &DisplayList) -> Vec<(f32, f32)> {
        list.materialized()
            .iter()
            .filter_map(|i| match i {
                DisplayItem::SolidRect { rect, radius, .. }
                    if radius.tl > 0.0 && (rect.w - rect.h).abs() < 0.01 && rect.w < 12.0 =>
                {
                    Some((rect.x, rect.y))
                }
                _ => None,
            })
            .collect()
    }

    /// O marcador acompanha o item quando a subárvore dele é DESLOCADA — por um
    /// `transform:translate`, por `position`, ou por ser um item de flex/grid.
    ///
    /// O que isto fixa é uma PROPRIEDADE e não uma coordenada: o marcador cai à
    /// esquerda da caixa do seu item e dentro de um `em`, seja qual for a
    /// posição final dela. Um teste com números fixos falharia à próxima
    /// recalibração do medidor aproximado, que já houve duas esta semana.
    ///
    /// Existe porque um deslocamento neste motor NÃO reescreve os itens: um
    /// translate puro soma ao `dx`/`dy` do `ChildRef` que aponta para a
    /// subárvore (ver `layout.rs`, no bloco do transform). Um marcador emitido
    /// fora dessa subárvore ficaria parado enquanto a lista se move — o que
    /// estas formas todas provam que hoje não acontece.
    #[test]
    fn o_marcador_acompanha_o_item_quando_a_subarvore_e_deslocada() {
        let casos: &[(&str, &str)] = &[
            ("transform no pai", "<div style='transform:translate(200px,50px)'><ul><li>aa</li></ul></div>"),
            ("transform no ul", "<ul style='transform:translate(200px,50px)'><li>aa</li></ul>"),
            ("transform no li", "<ul><li style='transform:translate(200px,50px)'>aa</li></ul>"),
            ("transform no avo", "<div style='transform:translate(200px,0)'><div><div><ul><li>aa</li></ul></div></div></div>"),
            ("transform com irmao antes", "<div style='transform:translate(200px,0)'><p>x</p><ul><li>aa</li><li>bb</li></ul></div>"),
            ("absolute no pai", "<div style='position:absolute; left:200px; top:50px'><ul><li>aa</li></ul></div>"),
            ("absolute no li", "<div style='position:relative'><ul><li style='position:absolute; left:300px; top:100px'>aa</li></ul></div>"),
            ("fixed no li", "<ul><li style='position:fixed; left:300px; top:100px'>aa</li></ul>"),
            ("item de flex", "<ul style='display:flex'><li>aa</li><li>bb</li></ul>"),
            ("flex centrado", "<div style='display:flex; justify-content:center; width:600px'><div><ul><li>aa</li></ul></div></div>"),
            ("dentro de grid", "<div style='display:grid'><ul><li>aa</li></ul></div>"),
            ("dentro de celula", "<table><tr><td><ul><li>aa</li></ul></td></tr></table>"),
            ("li centrado por margin auto", "<ul><li style='width:100px; margin-left:auto; margin-right:auto'>aa</li></ul>"),
            ("contentor rolavel", "<div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li></ul></div>"),
        ];
        for (nome, html) in casos {
            // A caixa do item é o medidor: um fundo declarado, que é pintado no
            // border-box do `<li>` e sofre exatamente o mesmo deslocamento que a
            // subárvore dele. O TEXTO não serviria — `text-align` move-o dentro
            // da caixa e o marcador (que é `outside`) fica onde deve, à borda.
            let html = html
                .replace("<li style='", "<li style='background:#900;")
                .replace("<li>", "<li style='background:#900'>");
            let (_, list) = crate::table::tests::geometria(&html, 600.0);
            let caixas: Vec<(f32, f32)> = list
                .materialized()
                .iter()
                .filter_map(|i| match i {
                    DisplayItem::SolidRect { rect, color, .. } if *color == 0x9900_00FF => {
                        Some((rect.x, rect.y))
                    }
                    _ => None,
                })
                .collect();
            let pontos = bullets(&list);
            assert_eq!(pontos.len(), caixas.len(), "{nome}: um marcador por item");
            for (i, (&(bx, by), &(cx, cy))) in pontos.iter().zip(caixas.iter()).enumerate() {
                assert!(
                    bx < cx && cx - bx <= 16.0,
                    "{nome}[{i}]: marcador em {bx} devia estar à esquerda de {cx} e dentro de um em"
                );
                assert!(
                    (by - cy).abs() <= 16.0,
                    "{nome}[{i}]: marcador em y={by} longe da caixa em y={cy}"
                );
            }
        }
    }

    /// Os textos pintados — o que prova que o marcador textual existe.
    fn textos(html: &str) -> Vec<String> {
        let (_, list) = crate::table::tests::geometria(html, 600.0);
        list.materialized()
            .iter()
            .filter_map(|i| match i {
                DisplayItem::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }

    /// `list-style-image: none` NÃO é uma imagem — o marcador do `type` continua
    /// a ser desenhado.
    ///
    /// Este teste vale os 457 números que faltavam na página da Wikipédia. A
    /// folha dela tem `ol{…;list-style-image:none}`, e essa linha sozinha
    /// apagava o marcador de TODOS os `<ol>` do documento — com a numeração a
    /// funcionar por trás, que é o que tornava o defeito invisível: um `<ol>`
    /// isolado numerava, e por isso nenhum teste o apanhava.
    ///
    /// As três formas em que uma folha real escreve isto estão aqui, porque foi
    /// a variação entre elas que fez a busca demorar: a longa, a longa herdada
    /// do `<ol>` para o `<li>`, e o shorthand com dois `none`.
    #[test]
    fn list_style_image_none_nao_apaga_o_marcador() {
        for (nome, html) in [
            ("<ol> nu", "<ol><li>aa</li><li>bb</li></ol>"),
            (
                "image:none no ol (a regra da Wikipédia)",
                "<style>ol{list-style-image:none}</style><ol><li>aa</li><li>bb</li></ol>",
            ),
            (
                "image:none no li",
                "<style>ol li{list-style-image:none}</style><ol><li>aa</li><li>bb</li></ol>",
            ),
        ] {
            let t = textos(html);
            assert!(
                t.contains(&"1.".to_string()) && t.contains(&"2.".to_string()),
                "{nome}: os marcadores deviam estar lá — {t:?}"
            );
        }
    }

    /// `list-style: none none` continua a apagar o marcador — pelo TYPE.
    ///
    /// A folha da Wikipédia escreve-o assim em `.plainlist ol` e no índice, e é
    /// a forma que mais se parece com a que se acabou de corrigir. Aqui o
    /// marcador tem MESMO de desaparecer, e por outra razão: o primeiro `none`
    /// é um `list-style-type` válido. Se a correção de cima tivesse sido feita
    /// no shorthand em vez de na pergunta sobre a imagem, este caso passava a
    /// desenhar bullets onde a página não os tem.
    #[test]
    fn list_style_none_none_continua_a_apagar_pelo_type() {
        let t = textos("<style>ol{list-style:none none}</style><ol><li>aa</li></ol>");
        assert!(!t.contains(&"1.".to_string()), "{t:?}");
        let (_, list) =
            crate::table::tests::geometria("<style>ul{list-style:none none}</style><ul><li>aa</li></ul>", 600.0);
        assert_eq!(bullets(&list).len(), 0);
    }

    /// E o outro lado da mesma regra, que é o que impede a correção de ser um
    /// `is_some()` trocado por `true`: uma imagem A SÉRIO continua a substituir
    /// o marcador. Sem esta metade, "não apagar com `none`" e "nunca apagar"
    /// passariam os dois no teste de cima.
    #[test]
    fn uma_imagem_a_serio_continua_a_substituir_o_marcador() {
        let t = textos("<style>ol{list-style-image:url(p.png)}</style><ol><li>aa</li></ol>");
        assert!(!t.contains(&"1.".to_string()), "{t:?}");
        // e o bullet do `<ul>` também não é desenhado por baixo da imagem.
        let (_, list) =
            crate::table::tests::geometria("<style>ul{list-style-image:url(p.png)}</style><ul><li>aa</li></ul>", 600.0);
        assert_eq!(bullets(&list).len(), 0);
    }

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

    /// Zerar o `padding-left` do `<ul>` põe o marcador FORA da caixa da lista, em
    /// coordenada negativa quando a lista encosta à margem esquerda.
    ///
    /// **É o que o Chrome faz**, e foi medido, não deduzido: um `<ul>` com
    /// `padding-left:0` encostado a `x=0` desenha o marcador em `x` negativo e
    /// ele simplesmente não aparece — sem `clamp` que o encoste à margem, sem o
    /// sobrepor ao texto do item, e sem abrir scroll horizontal
    /// (`scrollWidth == clientWidth`). O mesmo `<ul>` com `margin-left:100px`
    /// mostra-o a ~14px à esquerda do item, que é a mesma distância que este
    /// ficheiro usa.
    ///
    /// Por isso NÃO há aqui um `max(0.0)` a impedir o negativo: passaria neste
    /// teste e desenharia num sítio onde o browser não desenha. O marcador está
    /// ancorado ao content-box do `<li>` e o recuo onde ele cabe é o padding que
    /// o `<ul>` reserva — tirar o padding tira o sítio.
    ///
    /// Fica fixado porque é a única forma que faz o marcador sair da caixa do
    /// pai sem nada estar errado, e um `reset` de folha de estilo escreve
    /// exatamente isto. Quem investigar um marcador "no sítio errado" deve
    /// começar por perguntar se ele está em `x` negativo; nesse caso a pergunta
    /// é da camada que PINTA, que ao contrário do browser pode não estar a
    /// descartar o que cai fora.
    #[test]
    fn zerar_o_padding_do_ul_poe_o_marcador_fora_da_caixa_como_o_browser() {
        let (dom, list) = crate::table::tests::geometria("<ul><li>aa</li></ul>", 600.0);
        let ul = crate::table::tests::rect(&dom, &list, "ul", 0);
        let dentro = bullets(&list)[0];
        assert!(
            dentro.0 > ul.x,
            "com o padding da UA o marcador cabe dentro do <ul>: {} vs {}",
            dentro.0,
            ul.x
        );

        let (dom, list) =
            crate::table::tests::geometria("<ul style='padding-left:0'><li>aa</li></ul>", 600.0);
        let ul = crate::table::tests::rect(&dom, &list, "ul", 0);
        let fora = bullets(&list)[0];
        assert!(
            fora.0 < ul.x,
            "sem padding o marcador sai pela esquerda do <ul>: {} vs {}",
            fora.0,
            ul.x
        );
        assert!(fora.0 < 0.0, "e encostado à margem cai em x negativo: {}", fora.0);
    }

    /// O marcador continua colado ao seu item depois de a lista ser ACHATADA, e
    /// cai dentro dos mesmos clips que ele.
    ///
    /// O achatamento não é hipotético: a camada que pinta chama `materialize()`
    /// sempre que existe uma região com scroll próprio, para poder escrever o
    /// offset dentro do `BeginClip` dessa região — um `BeginClip` pode viver numa
    /// subárvore partilhada, e mutá-lo no lugar mexeria no desenho de todos os
    /// nós que a reusam. Ou seja: há um caminho que só corre quando há scroll de
    /// região e que reescreve `items` inteiro, e nenhuma medição de página
    /// parada passa por ele.
    ///
    /// A segunda metade é a que interessa e não se via na primeira: o offset da
    /// região é aplicado ao que está ENTRE o `BeginClip` e o `EndClip` dela. Um
    /// marcador do lado de fora dessa fronteira ficaria parado enquanto o item
    /// dele rola — o marcador a flutuar sobre o conteúdo, que é o sintoma. Por
    /// isso o teste compara a JANELA DE CLIPS de cada marcador com a do seu
    /// item, e não só as coordenadas.
    #[test]
    fn o_marcador_fica_nos_mesmos_clips_que_o_item_depois_de_achatar() {
        let casos: &[(&str, &str)] = &[
            ("lista simples", "<ul><li>aa</li><li>bb</li></ul>"),
            ("regiao rolavel", "<div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li><li>cc</li></ul></div>"),
            ("regiao rolavel deslocada", "<div style='transform:translate(200px,0)'><div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li><li>cc</li></ul></div></div>"),
            ("duas regioes", "<div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li></ul></div><div style='overflow:auto; height:20px'><ul><li>cc</li><li>dd</li></ul></div>"),
            ("clip-path por cima", "<div style='clip-path:inset(0)'><div style='overflow:auto;height:20px'><ul><li>aa</li><li>bb</li></ul></div></div>"),
        ];
        for (nome, html) in casos {
            let html = html
                .replace("<li style='", "<li style='background:#900;")
                .replace("<li>", "<li style='background:#900'>");
            let (_, mut list) = crate::table::tests::geometria(&html, 600.0);
            let antes = marcadores_e_itens(&list);
            list.materialize();
            let depois = marcadores_e_itens(&list);
            assert_eq!(antes, depois, "{nome}: o achatamento mudou o desenho");

            // A janela de clips ABERTOS em cada índice da lista já plana.
            let mut abertos: Vec<usize> = Vec::new();
            let mut janela: Vec<Vec<usize>> = Vec::new();
            for (i, it) in list.items.iter().enumerate() {
                match it {
                    DisplayItem::BeginClip { .. } => {
                        janela.push(abertos.clone());
                        abertos.push(i);
                    }
                    DisplayItem::EndClip { .. } => {
                        abertos.pop();
                        janela.push(abertos.clone());
                    }
                    _ => janela.push(abertos.clone()),
                }
            }
            let mut caixas = Vec::new();
            let mut pontos = Vec::new();
            for (i, it) in list.items.iter().enumerate() {
                if let DisplayItem::SolidRect { rect, radius, color } = it {
                    if radius.tl > 0.0 && (rect.w - rect.h).abs() < 0.01 && rect.w < 12.0 {
                        pontos.push((rect.x, janela[i].clone()));
                    } else if *color == 0x9900_00FF {
                        caixas.push((rect.x, janela[i].clone()));
                    }
                }
            }
            assert_eq!(pontos.len(), caixas.len(), "{nome}: um marcador por item");
            for (i, ((bx, bj), (cx, cj))) in pontos.iter().zip(caixas.iter()).enumerate() {
                assert_eq!(
                    bj, cj,
                    "{nome}[{i}]: o marcador está noutra janela de clips que o item —                      o offset de scroll da região move um e não o outro"
                );
                assert!(bx < cx && cx - bx <= 16.0, "{nome}[{i}]: {bx} vs {cx}");
            }
        }
    }

    /// Os marcadores e as caixas dos itens, na ordem de pintura — o desenho que
    /// o achatamento não pode alterar.
    fn marcadores_e_itens(list: &DisplayList) -> Vec<(bool, u32, u32)> {
        list.materialized()
            .iter()
            .filter_map(|i| match i {
                DisplayItem::SolidRect { rect, radius, color } => {
                    if radius.tl > 0.0 && (rect.w - rect.h).abs() < 0.01 && rect.w < 12.0 {
                        Some((true, rect.x.to_bits(), rect.y.to_bits()))
                    } else if *color == 0x9900_00FF {
                        Some((false, rect.x.to_bits(), rect.y.to_bits()))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }
}

