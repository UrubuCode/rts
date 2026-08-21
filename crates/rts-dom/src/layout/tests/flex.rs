//! Flexbox: eixo horizontal e coluna, `grow`/`shrink`/`order`, alinhamento,
//! `gap` e `wrap`.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;

    /// O `line-height` declarado vale em QUALQUER contexto, não só no fluxo
    /// inline: o mesmo parágrafo dentro de um `display:flex` respondia a altura
    /// do medidor (16×1.125) e ignorava a folha, porque o caminho de flex media
    /// o texto solto perguntando direto ao medidor. Uma linha de 16px com
    /// `line-height:2` tem 32px de altura, esteja onde estiver.
    #[test]
    fn line_height_declarado_vale_tambem_dentro_de_flex() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let altura = |html: &str| {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let idx = dom.resolve(dom.query("#alvo").unwrap()).unwrap();
            list.geometry()
                .rects
                .get(&idx)
                .expect("o alvo devia ter caixa")
                .h
        };
        let sem = altura("<div id='alvo' style='display:flex'>uma linha</div>");
        let com = altura("<div id='alvo' style='display:flex; line-height:2'>uma linha</div>");
        assert_eq!(sem, ApproxMeasurer.line_height(DEFAULT_FONT_SIZE));
        assert_eq!(com, 32.0, "16 × 2 = 32, e não a altura do medidor");
    }

    /// Um `<span>` filho de um `display:flex` é um item de flex, e um item de
    /// flex é BLOCKIFICADO pela spec — tem caixa própria, com a sua posição e o
    /// seu tamanho. O Chrome reporta `display:block` nele.
    ///
    /// Antes era achatado para uma string e pintado com o estilo do CONTAINER:
    /// não registava caixa (eram 345 dos 351 elementos `display:block` sem caixa
    /// da Wikipédia) e perdia a sua própria cor pelo caminho.
    #[test]
    fn span_filho_de_flex_tem_caixa_propria() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let dom = parse_html_to_dom(
            "<div style='display:flex'><span id='a'>um</span><span id='b'>dois</span></div>",
        );
        let list = layout_document(&dom, &ctx);
        let geo = list.geometry();
        let caixa = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            *geo.rects
                .get(&idx)
                .unwrap_or_else(|| panic!("{sel} devia ter caixa"))
        };
        let (a, b) = (caixa("#a"), caixa("#b"));
        assert!(a.w > 0.0 && a.h > 0.0, "o primeiro tem tamanho: {a:?}");
        assert!(
            b.x >= a.x + a.w,
            "o segundo à direita do primeiro: {a:?} {b:?}"
        );
    }

    /// A cor de um `<span>` filho de flex é a DELE, não a do container — o mesmo
    /// achatamento que lhe tirava a caixa pintava o texto com o estilo do pai.
    #[test]
    fn span_filho_de_flex_pinta_com_a_sua_propria_cor() {
        let list = layout(
            "<div style='display:flex; color:#0000ff'><span style='color:#ff0000'>vermelho</span></div>",
            600.0,
        );
        let texts = all_texts(&list);
        assert_eq!(texts.len(), 1, "{texts:?}");
        assert_eq!(texts[0].3, 0xFF0000FF, "a cor do span, não a do container");
    }

    #[test]
    fn flex_gap_separa_itens() {
        // gap:20px entre 3 cards de 100px: x = 0, 120, 240.
        let r = flex_card_rects("gap:20px", 3, 600.0);
        assert_eq!(r.len(), 3);
        assert!((r[0].x - 0.0).abs() < 0.5, "{r:?}");
        assert!((r[1].x - 120.0).abs() < 0.5, "card2 em 100+20: {r:?}");
        assert!((r[2].x - 240.0).abs() < 0.5, "card3 em 220+20: {r:?}");
    }

    #[test]
    fn flex_justify_content() {
        // 3 cards de 100 num container de 600 → free = 600-300 = 300.
        // space-between: x = 0, 100+150=250, 200+300=500.
        let r = flex_card_rects("justify-content:space-between", 3, 600.0);
        assert!((r[0].x - 0.0).abs() < 0.5, "{r:?}");
        assert!((r[1].x - 250.0).abs() < 0.5, "between=150: {r:?}");
        assert!((r[2].x - 500.0).abs() < 0.5, "flush no fim: {r:?}");
        // center: leading = 150 → x = 150, 250, 350.
        let r = flex_card_rects("justify-content:center", 3, 600.0);
        assert!((r[0].x - 150.0).abs() < 0.5, "center leading=150: {r:?}");
        assert!((r[2].x - 350.0).abs() < 0.5, "{r:?}");
        // flex-end: leading = 300 → x = 300, 400, 500.
        let r = flex_card_rects("justify-content:flex-end", 3, 600.0);
        assert!((r[0].x - 300.0).abs() < 0.5, "flex-end leading=300: {r:?}");
        // space-evenly: leading = between = 300/4 = 75 → x = 75, 250, 425.
        let r = flex_card_rects("justify-content:space-evenly", 3, 600.0);
        assert!((r[0].x - 75.0).abs() < 0.5, "evenly leading=75: {r:?}");
        assert!((r[1].x - 250.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_grow_distribui_o_espaco() {
        // O GRID do Bootstrap: `.col { flex: 1 0 0% }` — 3 colunas dividem o
        // container igualmente (base 0, grow distribui TUDO).
        crate::block::define(
            "row",
            crate::block::BlockDef {
                display: 2,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let list = layout(
            "<style>row{display:flex} .col{flex:1 0 0%;background:#111}</style>\
             <row><div class='col'>a</div><div class='col'>b</div><div class='col'>c</div></row>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3);
        assert!((r[0].w - 200.0).abs() < 0.5, "3 colunas iguais: {r:?}");
        assert!((r[1].x - 200.0).abs() < 0.5, "{r:?}");
        assert!((r[2].x - 400.0).abs() < 0.5, "{r:?}");
        // grow 1:2 → 200 e 400.
        let l2 = layout(
            "<style>row{display:flex} .a{flex:1 0 0%;background:#111} .b{flex:2 0 0%;background:#222}</style>\
             <row><div class='a'>a</div><div class='b'>b</div></row>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!((r2[0].w - 200.0).abs() < 0.5, "grow 1: {r2:?}");
        assert!((r2[1].w - 400.0).abs() < 0.5, "grow 2: {r2:?}");
    }

    #[test]
    fn flex_shrink_encolhe_em_overflow() {
        // shrink DEFAULT = 1 (fiel ao CSS): 2 itens de 400 em 600 encolhem para
        // 300 cada; com shrink:0 no primeiro, ele mantém 400 e o outro cede.
        crate::block::define(
            "row",
            crate::block::BlockDef {
                display: 2,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let list = layout(
            "<style>row{display:flex} .c{width:400px;background:#111}</style>\
             <row><div class='c'>a</div><div class='c'>b</div></row>",
            600.0,
        );
        let r = all_rects(&list);
        assert!(
            (r[0].w - 300.0).abs() < 0.5,
            "shrink default encolhe: {r:?}"
        );
        assert!((r[1].w - 300.0).abs() < 0.5, "{r:?}");
        let l2 = layout(
            "<style>row{display:flex} .fix{width:400px;flex-shrink:0;background:#111} .c{width:400px;background:#222}</style>\
             <row><div class='fix'>a</div><div class='c'>b</div></row>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!((r2[0].w - 400.0).abs() < 0.5, "shrink:0 nao cede: {r2:?}");
        assert!(
            (r2[1].w - 200.0).abs() < 0.5,
            "o flexivel cede tudo: {r2:?}"
        );
    }

    #[test]
    fn flex_order_e_align_self() {
        // `order` reordena visualmente (menor primeiro); `align-self` vence o
        // align-items do container; STRETCH real estica o item sem height.
        crate::block::define(
            "row",
            crate::block::BlockDef {
                display: 2,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let list = layout(
            "<style>row{display:flex;height:100px;align-items:flex-start}\
              .a{order:2;width:100px;height:20px;background:#111}\
              .b{order:1;width:100px;height:20px;align-self:center;background:#222}\
              .s{align-self:stretch;width:100px;background:#333}</style>\
             <row><div class='a'>a</div><div class='b'>b</div><div class='s'>s</div></row>",
            600.0,
        );
        // os rects saem na ordem VISUAL (pós-order): .s (0) → .b (1) → .a (2).
        let r = all_rects(&list);
        assert!((r[0].x - 0.0).abs() < 0.5, ".s primeiro (order 0): {r:?}");
        assert!((r[1].x - 100.0).abs() < 0.5, ".b no meio (order 1): {r:?}");
        assert!(
            (r[2].x - 200.0).abs() < 0.5,
            ".a por ultimo (order 2): {r:?}"
        );
        // align-self:stretch do .s (sem height): estica até a linha (100) — o
        // align-items:flex-start do container é VENCIDO pelo align-self.
        assert!((r[0].h - 100.0).abs() < 0.5, "stretch estica: {r:?}");
        // align-self:center do .b: y = (100-20)/2 = 40.
        assert!((r[1].y - 40.0).abs() < 0.5, "align-self center: {r:?}");
        // .a fica no topo (align-items:flex-start do container).
        assert!((r[2].y - 0.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_justify_overflow() {
        // 3 cards de 100 em 200 (overflow real = -100). VALIDADO contra Chrome
        // (flex-shrink:0): os space-* caem para flex-start → x = 0, 100, 200.
        for jc in [
            "space-between",
            "space-around",
            "space-evenly",
            "flex-start",
        ] {
            let r = flex_card_rects(&format!("justify-content:{jc}"), 3, 200.0);
            assert!((r[0].x - 0.0).abs() < 0.5, "{jc} overflow→start: {r:?}");
            assert!((r[1].x - 100.0).abs() < 0.5, "{jc}: {r:?}");
            assert!((r[2].x - 200.0).abs() < 0.5, "{jc}: {r:?}");
        }
        // center em overflow: leading = free/2 = -50 → x = -50, 50, 150 (Chrome).
        let r = flex_card_rects("justify-content:center", 3, 200.0);
        assert!(
            (r[0].x + 50.0).abs() < 0.5,
            "center overflow leading=-50: {r:?}"
        );
        assert!((r[2].x - 150.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_align_center_usa_altura_do_container() {
        // VALIDADO no Chrome: bar height:80, cards height:40, align-items:center
        // → cards em y=20 (centrados na altura DO CONTAINER, não na linha de 40).
        crate::block::define(
            "bar",
            crate::block::BlockDef {
                display: 2,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let dom = parse_html_to_dom(
            "<style>bar{display:flex;align-items:center;height:80px} .c{width:100px;height:40px;background:#ff0000}</style>\
             <bar><div class='c'>a</div><div class='c'>b</div></bar>",
        );
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let ys: Vec<f32> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(rect.y),
                _ => None,
            })
            .collect();
        assert!(
            ys.iter().all(|&y| (y - 20.0).abs() < 0.5),
            "cards centrados em y=20: {ys:?}"
        );
    }

    #[test]
    fn flex_align_items_center() {
        // 1 card baixo + 1 alto: com align-items:center o baixo desce metade da folga.
        crate::block::define(
            "row",
            crate::block::BlockDef {
                display: 2,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let dom = parse_html_to_dom(
            "<style>row{display:flex;align-items:center} .a{height:20px;width:50px;background:#111111} .b{height:60px;width:50px;background:#222222}</style>\
             <row><div class='a' id='a'>x</div><div class='b' id='b'>y</div></row>",
        );
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<(f32, f32)> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some((rect.x, rect.y)),
                _ => None,
            })
            .collect();
        // ordena por x: o card 'a' (baixo, x menor) deve ter y MAIOR que o 'b' (alto).
        let mut s = rects.clone();
        s.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
        assert!(s[0].1 > s[1].1, "card baixo centralizado desce: {s:?}");
    }

    #[test]
    fn badges_fluem_e_quebram_linha_no_wrap() {
        // <tags display:wrap> com badges: fluem lado a lado e QUEBRAM para a próxima
        // linha quando não cabem (inline-block flow). Cada badge dimensiona pelo
        // conteúdo (shrink-to-fit), não estica para a largura toda.
        crate::block::define(
            "tags",
            crate::block::BlockDef {
                display: 1,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::block::define(
            "badge",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        // 4 badges; numa largura estreita (200) eles não cabem todos numa linha.
        let dom = parse_html_to_dom(
            "<tags>\
               <badge style='background:#111;padding:6'>rust</badge>\
               <badge style='background:#222;padding:6'>cranelift</badge>\
               <badge style='background:#333;padding:6'>typescript</badge>\
               <badge style='background:#444;padding:6'>egui</badge>\
             </tags>",
        );
        let ctx = LayoutCtx {
            viewport_w: 200.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = all_rects(&list);
        assert_eq!(rects.len(), 4);
        // shrink-to-fit: nenhum badge ocupa a largura toda (200) — cada um é estreito.
        assert!(
            rects.iter().all(|r| r.w < 150.0),
            "badges estreitos (conteúdo): {rects:?}"
        );
        // QUEBROU linha: há pelo menos 2 valores distintos de Y (não todos na mesma linha).
        let ys: std::collections::BTreeSet<i32> = rects.iter().map(|r| r.y as i32).collect();
        assert!(
            ys.len() >= 2,
            "deve haver quebra de linha (Ys distintos): {rects:?}"
        );
        // o primeiro badge começa no canto (x=0).
        assert_eq!(rects[0].x, 0.0);
    }

    #[test]
    fn flex_column_empilha_com_gap() {
        // `flex-direction:column`: itens empilham na VERTICAL (main = Y) com o gap
        // entre eles; align default (stretch) → cada item ocupa a largura.
        let list = layout(
            "<div style='display:flex; flex-direction:column; gap:10; background:#111'>\
               <div style='background:#222; height:30'>a</div>\
               <div style='background:#333; height:40'>b</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3, "container + 2 filhos: {r:?}");
        // filhos: o 1º em y=0; o 2º abaixo (30 + gap 10 = 40) — NÃO lado a lado.
        assert_eq!(r[1].y, 0.0);
        assert_eq!((r[1].h, r[2].h), (30.0, 40.0));
        assert_eq!(r[2].y, 40.0);
        assert_eq!(r[1].x, r[2].x, "mesmo X (coluna, não row)");
        // stretch (default): os itens ocupam a largura do container.
        assert_eq!(r[1].w, 600.0);
    }

    #[test]
    fn flex_column_margin_auto_empurra() {
        // O padrão do Bootstrap cover: header + main(mt-auto/mb-auto) + footer numa
        // coluna com altura — os margins auto ABSORVEM o espaço livre e centralizam
        // o main (spec flexbox §8.1; mb-auto/mt-auto).
        let list = layout(
            "<div style='display:flex; flex-direction:column; height:300; background:#111'>\
               <div style='background:#222; height:20'>h</div>\
               <div style='background:#333; height:60; margin-top:auto; margin-bottom:auto'>m</div>\
               <div style='background:#444; height:20'>f</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 4);
        // free = 300 - (20+60+20) = 200; 2 autos → 100 cada.
        assert_eq!(r[1].y, 0.0); // header no topo
        assert_eq!(r[2].y, 120.0); // main: 20 + 100 (mt-auto)
        assert_eq!(r[3].y, 280.0); // footer: 120 + 60 + 100 (mb-auto)
    }

    #[test]
    fn flex_column_justify_center() {
        // justify-content atua no eixo PRINCIPAL (Y em column) quando o container
        // tem altura: um item de 50 num container de 300 centra em y=125.
        let list = layout(
            "<div style='display:flex; flex-direction:column; height:300; justify-content:center'>\
               <div style='background:#222; height:50'>x</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].y, 125.0, "{r:?}");
    }
