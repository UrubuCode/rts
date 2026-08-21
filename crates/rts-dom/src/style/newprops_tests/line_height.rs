//! `line-height` e a herança de propriedades declaradas no `body`
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

#[test]
fn line_height_sem_unidade_chega_ao_layout() {
    // A cascade responde `1.625` e a linha tem de sair a 26px (1.625 × 16), que é
    // o que o Chrome computa nos <p> da Wikipédia. Sem isto a linha cai no default
    // do medidor (20,8) e o parágrafo inteiro fica com o espaçamento errado.
    crate::block::define(
        "p",
        crate::block::BlockDef {
            display: 0,
            indent: 0.0,
            prefix: 0,
            flags: 0,
        },
    );
    let texto = "palavra ".repeat(40);
    let ys = |decl: &str| -> Vec<f32> {
        let list = layout(&format!("<p style='{decl}'>{texto}</p>"), 400.0);
        itens(&list)
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { y, .. } => Some(*y),
                _ => None,
            })
            .collect()
    };
    let com = ys("line-height:1.625");
    assert!(
        com.len() > 2,
        "o texto tem de quebrar em várias linhas: {com:?}"
    );
    assert_eq!(com[1] - com[0], 26.0);
    // `em` é relativo ao font-size do próprio elemento: mesmo número que o
    // multiplicador. Era ignorado por completo antes (caía em 20,8).
    let em = ys("line-height:1.625em");
    assert_eq!(em[1] - em[0], 26.0);
    // `%` também é do font-size do elemento, não do container.
    let pct = ys("line-height:162.5%");
    assert_eq!(pct[1] - pct[0], 26.0);
    // e a forma absoluta continua absoluta.
    let px = ys("line-height:26px");
    assert_eq!(px[1] - px[0], 26.0);
}

#[test]
fn line_height_normal_e_o_mesmo_que_nao_declarar() {
    // A spec diz que `normal` é o valor INICIAL — declarar ou omitir tem de dar a
    // mesma linha. Dava 1,2×font declarado contra 1,3×font (o medidor) omitido:
    // a mesma propriedade com duas alturas conforme fosse escrita.
    crate::block::define(
        "p",
        crate::block::BlockDef {
            display: 0,
            indent: 0.0,
            prefix: 0,
            flags: 0,
        },
    );
    let texto = "palavra ".repeat(40);
    let delta = |decl: &str| -> f32 {
        let list = layout(&format!("<p style='{decl}'>{texto}</p>"), 400.0);
        let ys: Vec<f32> = itens(&list)
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { y, .. } => Some(*y),
                _ => None,
            })
            .collect();
        ys[1] - ys[0]
    };
    assert_eq!(delta("line-height:normal"), delta(""));
    // e o computed reporta `normal`, que é o único valor de line-height que o
    // browser não resolve para px.
    assert_eq!(
        parse_inline("line-height: normal").get_property("line-height"),
        "normal"
    );
}

#[test]
fn line_height_negativo_e_recusado() {
    // A spec proíbe negativo; recusar deixa a declaração cair (o que o browser
    // faz) em vez de encolher a linha para trás.
    assert_eq!(parse_inline("line-height: -1.5").line_height, None);
    assert_eq!(parse_inline("line-height: -10px").line_height, None);
}

#[test]
fn propriedade_herdada_declarada_no_body_chega_aos_descendentes() {
    // A tipografia da folha real vive no `body` (a Wikipédia declara-a lá), então
    // a herança a partir dele é o caminho que sustenta a página inteira: cor,
    // família, tamanho e line-height chegam todos por aqui.
    let css = |html: &str| {
        let dom = crate::parse_html_to_dom(html);
        let idx = dom.resolve(dom.query("p").unwrap()).unwrap();
        dom.computed_style_idx(idx).unwrap_or_default()
    };
    let por_regra = css("<style>body{line-height:1.6;color:#ff0000}</style><body><p>x</p></body>");
    assert_eq!(
        por_regra.line_height,
        Some(crate::style::LineHeight::Mult(1.6))
    );
    assert_eq!(por_regra.color, Some(0xFF0000FF));
    // e pelo `style=""` do próprio body, que é outro caminho até ao mesmo campo.
    let por_inline = css("<html><body style='line-height:1.6'><p>x</p></body></html>");
    assert_eq!(
        por_inline.line_height,
        Some(crate::style::LineHeight::Mult(1.6))
    );
    // um ancestral qualquer serve — o `body` não tem nada de especial na cascade.
    let por_div = css("<style>div{line-height:1.6}</style><div><p>x</p></div>");
    assert_eq!(
        por_div.line_height,
        Some(crate::style::LineHeight::Mult(1.6))
    );
}

#[test]
fn body_implicito_faz_a_regra_do_body_chegar_a_um_fragmento_sem_as_tres_tags() {
    // O caso que o teste acima NÃO cobre, porque escreve `<body>`: um fragmento
    // que não escreve NENHUMA das três tags. É o que qualquer teste escreve, o
    // que um `innerHTML` recebe, e o que grande parte da web serve.
    //
    // O parser cria `<html>` e `<body>` implícitos, como qualquer browser. Sem
    // eles a regra `body{…}` não casava com elemento nenhum e toda a
    // propriedade HERDADA declarada aí sumia em silêncio: a herança funcionava,
    // o ancestral é que não existia. Na Wikipédia isso valia 20,8px de altura
    // de linha onde o Chrome computa 26.
    let dom = crate::parse_html_to_dom(
        "<style>body{color:#ff0000;line-height:1.6}</style><div><p>x</p></div>",
    );
    // as duas tags existem mesmo, e não só por efeito na cascade.
    assert!(
        dom.query("html").is_some(),
        "o <html> implícito tem de estar na árvore"
    );
    assert!(
        dom.query("body").is_some(),
        "o <body> implícito tem de estar na árvore"
    );
    // e o descendente HERDA o que foi declarado nelas.
    let p = dom.resolve(dom.query("p").unwrap()).unwrap();
    let css = dom.computed_style_idx(p).unwrap_or_default();
    assert_eq!(
        css.color,
        Some(0xFF0000FF),
        "a cor declarada em body{{}} herda"
    );
    assert_eq!(css.line_height, Some(crate::style::LineHeight::Mult(1.6)));
}

#[test]
fn line_height_normal_bate_com_as_alturas_do_chrome() {
    // Os números são do corpus `tests/css/*.esperado.json`, medidos no Chrome
    // real: a altura de uma caixa de uma linha, por tamanho de fonte, quando
    // `line-height` é `normal`. Cinco tamanhos batem exatamente.
    use crate::style::normal_line_height as lh;
    assert_eq!(lh(8.0), 9.0);
    assert_eq!(lh(16.0), 18.0); // o caso dominante: 37 das 62 amostras
    assert_eq!(lh(20.0), 23.0); // sem o arredondamento para cima sairia 22,5
    assert_eq!(lh(24.0), 27.0);
    assert_eq!(lh(30.0), 34.0); // idem: 33,75
    // 32px é o único que erra, por 1px e com uma amostra só — dentro da
    // tolerância do comparador. Fixado para a divergência ser visível se alguém
    // recalibrar a constante.
    assert_eq!(lh(32.0), 36.0);
}

#[test]
fn borda_por_lado_entra_na_geometria_da_caixa() {
    // Os números são do Chrome, via `tests/css/claude-border-lados`: um <div> de
    // 200x20 (content-box) com uma borda de UM lado cresce SÓ desse lado.
    // Antes, a largura da borda era um escalar aplicado aos quatro lados, e uma
    // `border-bottom: 5px` alargava a caixa nos quatro ou em nenhum.
    let caixa = |decl: &str| -> Rect {
        let html =
            format!("<div style='width:200px;height:20px;background:#eeeeee;{decl}'>x</div>");
        first_solid(&layout(&html, 1280.0))
            .expect("a caixa pinta um fundo")
            .0
    };
    assert_eq!(caixa("border-top:10px solid #000").h, 30.0);
    assert_eq!(caixa("border-top:10px solid #000").w, 200.0);
    assert_eq!(caixa("border-right:15px solid #000").w, 215.0);
    assert_eq!(caixa("border-bottom:5px solid #000").h, 25.0);
    assert_eq!(caixa("border-left:25px solid #000").w, 225.0);
    // quatro lados diferentes: 200+2+4 de largura, 20+1+3 de altura.
    let quatro = caixa(
        "border-top:1px solid #000;border-right:2px solid #000;\
         border-bottom:3px solid #000;border-left:4px solid #000",
    );
    assert_eq!((quatro.w, quatro.h), (206.0, 24.0));
}

#[test]
fn lado_sem_estilo_nao_ocupa_espaco() {
    // Regra da spec que o corpus mede: `border-style: none` faz a largura USADA
    // ser zero, por mais que o autor declare 30px. É a mesma regra que já decidia
    // a PINTURA — o layout e o render tinham de concordar sobre a mesma caixa.
    let caixa = |decl: &str| -> Rect {
        let html =
            format!("<div style='width:200px;height:20px;background:#eeeeee;{decl}'>x</div>");
        first_solid(&layout(&html, 1280.0))
            .expect("a caixa pinta um fundo")
            .0
    };
    let r = caixa("border-top:10px solid #000;border-right-width:30px");
    assert_eq!(
        (r.w, r.h),
        (200.0, 30.0),
        "o lado sem estilo não ocupa nada"
    );
    // e o shorthand curto DEPOIS de um lado sobrepõe-no (ordem da cascade),
    // enquanto o lado depois do curto vence só naquele lado.
    assert_eq!(
        caixa("border:6px solid #000;border-left:20px solid #000").w,
        226.0
    );
    assert_eq!(
        caixa("border-left:20px solid #000;border:6px solid #000").w,
        212.0
    );
}
