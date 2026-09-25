//! Um átomo inline (inline-flex, inline-block, botão, canvas) no MEIO de
//! texto é disposto pela SUA caixa da árvore.
//!
//! Regressão do WPT `css-flexbox/flexbox-baseline-*`: um `A` de texto seguido
//! de `display:inline-flex` entra no fluxo inline, e o fluxo só levava a
//! caixa de um fragmento de inline partido — para todos os outros o átomo
//! chegava a `layout_block` com `caixa: None`, e os `expect` do contentor
//! flex/grid/tabela rebentavam. A segunda metade do mesmo defeito não
//! rebentava, mentia: sem caixa, o bloco não reservava a sua posição na ordem
//! de hit-test antes dos filhos, e um link dentro de um inline-block perdia o
//! clique para o próprio inline-block.
//!
//! Os números são os do Chrome para as dimensões declaradas (a caixa tem a
//! `width`/`height` do autor mais padding e borda, CSS 2.1 §10.3.9/§10.6.6) e
//! a colocação é a da linha: `vertical-align:top` põe o topo do átomo no topo
//! da linha, e o texto antes dele empurra-o para a direita.

use crate::table::tests::{geometria, rect};

fn xywh(dom: &crate::Dom, list: &crate::paint::DisplayList, sel: &str) -> (f32, f32, f32, f32) {
    let r = rect(dom, list, sel, 0);
    (r.x, r.y, r.w, r.h)
}

/// O átomo `#t`, precedido de texto, fica na PRIMEIRA linha, depois do texto,
/// com a border box que declara.
fn atomo_depois_de_texto(estilo: &str, conteudo: &str, w: f32, h: f32) {
    let html = format!(
        r#"<div id=p style="font:16px/20px monospace;width:400px">A <div id=t style="{estilo};vertical-align:top">{conteudo}</div> B</div>"#
    );
    let (dom, list) = geometria(&html, 800.0);
    let (x, y, tw, th) = xywh(&dom, &list, "#t");
    assert_eq!((tw, th), (w, h), "{estilo}: a caixa do átomo tem a medida declarada");
    assert_eq!(y, 0.0, "{estilo}: o átomo fica na primeira linha");
    assert!(x > 0.0 && x < 40.0, "{estilo}: o átomo vem logo a seguir a `A `, x={x}");
    let (_, _, pw, _) = xywh(&dom, &list, "#p");
    assert_eq!(pw, 400.0);
}

#[test]
fn inline_flex_depois_de_texto_e_disposto_na_linha() {
    atomo_depois_de_texto("display:inline-flex;width:16px;height:16px", "a", 16.0, 16.0);
}

/// `inline-grid` e `inline-table` a meio de texto dispõem-se sem rebentar e
/// com a medida declarada. O que NÃO se afirma aqui é a linha: o Chrome
/// põe-nos na primeira linha, ao lado do `A`, e este motor ainda os abre numa
/// linha própria (y=20), porque `is_inline_block` só reconhece `inline-block`
/// e `inline-flex` como átomos. É um defeito à parte, anterior a este, e fica
/// aberto em vez de ser escondido por uma asserção à medida do erro.
#[test]
fn inline_grid_e_inline_table_depois_de_texto_tem_a_sua_caixa() {
    for (estilo, conteudo, w, h) in [
        ("display:inline-grid;width:20px;height:12px", "a", 20.0, 12.0),
        ("display:inline-table;width:30px;height:18px", "<div style=display:table-cell>a</div>", 30.0, 18.0),
    ] {
        let html = format!(r#"<div style="font:16px/20px monospace;width:400px">A <div id=t style="{estilo}">{conteudo}</div> B</div>"#);
        let (dom, list) = geometria(&html, 800.0);
        let (_, _, tw, th) = xywh(&dom, &list, "#t");
        assert_eq!((tw, th), (w, h), "{estilo}");
    }
}

#[test]
fn inline_flex_com_padding_soma_padding_a_border_box() {
    atomo_depois_de_texto("display:inline-flex;width:16px;height:16px;padding:10px", "a", 36.0, 36.0);
}

/// O caso exacto do `flexbox-baseline-single-item-001a`: o primeiro filho do
/// inline-flex é ABSOLUTO. Ele sai do fluxo e não conta para a linha, mas o
/// contentor continua a ter de ser disposto — e a medida dele é a declarada
/// mais o padding.
#[test]
fn inline_flex_com_filho_absoluto_no_meio_de_texto() {
    let (dom, list) = geometria(
        r#"<div style="font:14px serif;width:400px">A
  <div id=f style="display:inline-flex;height:16px;width:16px;padding:4px;background:pink">
    <div id=abs style="position:absolute;top:0;font-size:8px">abs</div>
    <div id=item style="font:26px serif">a</div>
  </div></div>"#,
        800.0,
    );
    let (fx, _, fw, fh) = xywh(&dom, &list, "#f");
    assert_eq!((fw, fh), (24.0, 24.0), "16px + 4px de padding de cada lado");
    assert!(fx > 0.0, "o contentor vem depois do `A`");
    // `top:0` com `left:auto`: o absoluto fica no topo do documento, na sua
    // posição estática horizontal — dentro do contentor, depois do padding.
    let (ax, ay, _, _) = xywh(&dom, &list, "#abs");
    assert_eq!(ay, 0.0);
    assert_eq!(ax, fx + 4.0, "a posição estática horizontal é a aresta do conteúdo do contentor");
    // O item em fluxo fica dentro do contentor, na aresta do conteúdo.
    let (ix, _, _, _) = xywh(&dom, &list, "#item");
    assert_eq!(ix, fx + 4.0);
}

#[test]
fn inline_block_com_filho_absoluto_no_meio_de_texto() {
    let (dom, list) = geometria(
        r#"<div style="font:16px/20px monospace;width:400px">A <span id=b style="display:inline-block;width:30px;height:10px;vertical-align:top"><span id=abs style="position:absolute;top:50px">x</span>y</span> C</div>"#,
        800.0,
    );
    let (bx, by, bw, bh) = xywh(&dom, &list, "#b");
    assert_eq!((by, bw, bh), (0.0, 30.0, 10.0));
    let (ax, ay, _, _) = xywh(&dom, &list, "#abs");
    assert_eq!((ax, ay), (bx, 50.0), "posição estática horizontal = início do conteúdo do inline-block");
}

/// `<button>` e `<canvas>` no meio de texto são átomos que se dispõem pelo seu
/// próprio caminho (`layout_button`/`layout_canvas`), e esses caminhos também
/// recebiam a caixa `None`.
#[test]
fn botao_e_canvas_no_meio_de_texto_ficam_na_linha() {
    let (dom, list) = geometria(
        r#"<div style="font:16px/20px monospace;width:400px">A <input id=bt type=button value=ok style="vertical-align:top"> B <canvas id=cv width=12 height=14 style="vertical-align:top"></canvas> C</div>"#,
        800.0,
    );
    let (bx, by, bw, bh) = xywh(&dom, &list, "#bt");
    assert!(bx > 0.0 && bw > 0.0 && bh > 0.0);
    assert_eq!(by, 0.0);
    let (cx, _, cw, ch) = xywh(&dom, &list, "#cv");
    assert_eq!((cw, ch), (12.0, 14.0), "o canvas tem os atributos width/height");
    assert!(cx > bx + bw, "o canvas vem depois do botão na mesma linha");
}

/// O clique no link dentro de um inline-block a meio de texto atinge o LINK.
///
/// A ordem de hit-test é a de pintura: o inline-block reserva a sua posição
/// ANTES de dispor os filhos, e o link, registado depois, fica por cima. Sem a
/// caixa, o inline-block registava-se no FIM, por cima do link, e o clique
/// devolvia o inline-block — o que o Chrome nunca faz (`elementFromPoint`
/// devolve o `<a>`).
#[test]
fn link_dentro_de_inline_block_no_meio_de_texto_recebe_o_clique() {
    let (dom, list) = geometria(
        r##"<p style="font:16px/20px monospace">texto <span id=ib style="display:inline-block;padding:4px;background:#eee"><a id=l href="#">link</a></span> mais</p>"##,
        800.0,
    );
    let l = dom.resolve(dom.query("#l").unwrap()).unwrap();
    let ib = dom.resolve(dom.query("#ib").unwrap()).unwrap();
    let r = rect(&dom, &list, "#l", 0);
    assert!(r.w > 0.0 && r.h > 0.0, "o link tem caixa: {r:?}");
    assert_eq!(list.hit_test(r.x + r.w / 2.0, r.y + r.h / 2.0), Some(l));
    // E o padding do inline-block, fora do link, continua a ser dele.
    let b = rect(&dom, &list, "#ib", 0);
    assert_eq!(list.hit_test(b.x + 1.0, b.y + 1.0), Some(ib));
}

/// Um inline partido cujas corridas são só espaço não gera caixa nenhuma
/// (`<span><div>…</div></span>`): o `<div>` que o partiu é IRMÃO na árvore e
/// é disposto por lá, uma vez — que é o que o Chrome pinta. O fluxo inline
/// descia pelo DOM do `<span>` e dispunha o `<div>` segunda vez, e o
/// inline-block lá dentro chegava a `layout_block` sem caixa e rebentava. É o
/// `<?xml … ?>` dos `text-transform-bicameral-*` do CSS2, que o parser faz
/// elemento e que embrulha o corpo inteiro.
#[test]
fn bloco_dentro_de_inline_so_de_espaco_e_disposto_uma_vez() {
    let (dom, list) = geometria(
        r#"<div><span><div id=d><span id=ib style="display:inline-block;width:30px"><b>xyz</b></span></div></span></div>"#,
        800.0,
    );
    let (_, _, w, _) = xywh(&dom, &list, "#ib");
    assert_eq!(w, 30.0);
    let pintados = list
        .materialized()
        .iter()
        .filter(|it| matches!(it, crate::paint::DisplayItem::Text { text, .. } if text.contains("xyz")))
        .count();
    assert_eq!(pintados, 1, "o texto do bloco pinta-se uma vez");
}
