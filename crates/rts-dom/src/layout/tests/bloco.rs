//! Fluxo de BLOCO: empilhamento vertical, box model, cards lado a lado,
//! `border-box` e margens.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada. A indentação de 4 espaços é a do `mod tests` de origem e foi
//! MANTIDA: há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;


    #[test]
    fn block_ocupa_largura_do_container() {
        // <div> sem width: bloco ocupa a largura do viewport menos o frame.
        // Aqui só padding=10 (margin/border=0): content = 600 - 20 = 580; a CAIXA
        // (content+padding) = 600 (largura cheia).
        let list = layout("<div style='background:#112233; padding:10'>x</div>", 600.0);
        let r = first_rect(&list);
        assert_eq!(r.x, 0.0);
        assert_eq!(r.y, 0.0);
        assert_eq!(r.w, 600.0); // content(580) + padding(2×10) = 600
    }


    #[test]
    fn width_percent_resolve_contra_container() {
        // width:50% de um viewport 800 → content=400; sem padding/border a caixa=400.
        let list = layout("<div style='background:#111111; width:50%'>x</div>", 800.0);
        let r = first_rect(&list);
        assert_eq!(r.w, 400.0); // 50% de 800
        assert_eq!(r.x, 0.0);
    }


    #[test]
    fn blocos_empilham_vertical() {
        // dois <div> com altura de 1 linha empilham: o 2º começa abaixo do 1º.
        // Sem box (sem bg) — só checa o Y das linhas de texto.
        //
        // Lido pela ÁRVORE (`walk`) e não por `list.items`: um filho de bloco é
        // emitido como FRAGMENTO, e os itens dele vivem no fragmento. Ler
        // `items` direto respondia zero textos e o teste falhava a dizer que os
        // blocos não pintavam, quando o que não pintava era a leitura.
        let list = layout("<div>um</div><div>dois</div>", 600.0);
        let mut texts: Vec<f32> = Vec::new();
        list.walk(|it, _dx, dy| {
            if let DisplayItem::Text { y, .. } = it {
                texts.push(y + dy);
            }
        });
        texts.sort_by(f32::total_cmp);
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0], 0.0); // primeiro no topo
        // uma linha do medidor aproximado: 16 × 1.125.
        let uma_linha = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        assert!(
            texts[1] >= uma_linha,
            "segundo bloco abaixo do primeiro (y={})",
            texts[1]
        );
    }


    #[test]
    fn fundo_vem_antes_do_texto_filho_no_zorder() {
        // O SolidRect (fundo) deve estar ANTES do Text na lista (pinta atrás).
        let list = layout("<div style='background:#222222; padding:8'>oi</div>", 600.0);
        let i_rect = list
            .materialized()
            .iter()
            .position(|it| matches!(it, DisplayItem::SolidRect { .. }));
        let i_text = list
            .materialized()
            .iter()
            .position(|it| matches!(it, DisplayItem::Text { .. }));
        assert!(
            i_rect < i_text,
            "fundo (idx {i_rect:?}) deve vir antes do texto (idx {i_text:?})"
        );
    }


    #[test]
    fn box_model_content_box_offset_do_texto() {
        // content-box: o texto começa deslocado por margin+border+padding.
        // padding=14, border=2, margin=6 → offset = 22. (MDN: outer = m+b+p+content)
        //
        // O `border-style: solid` é indispensável e não é decoração do teste:
        // `border-width` SOZINHO não cria borda nenhuma, porque o estilo por
        // omissão é `none` e uma borda que não pinta também não ocupa espaço.
        // Sem ele o offset certo passa a ser 20 (só margem e padding) — foi o
        // que este teste passou a acusar quando as bordas por lado começaram a
        // respeitar o estilo, e a premissa errada era a do teste.
        let list = layout(
            "<div style='background:#111111; padding:14; border-width:2; border-style:solid; margin:6'>z</div>",
            600.0,
        );
        let txt = all_texts(&list)
            .first()
            .map(|(_, x, y, _)| (*x, *y))
            .expect("texto");
        assert_eq!(txt.0, 22.0); // x = margin(6)+border(2)+padding(14)
        assert_eq!(txt.1, 22.0); // y idem
        // a caixa (fundo) NÃO inclui a margin: começa em (6,6).
        let r = first_rect(&list);
        assert_eq!(r.x, 6.0);
        assert_eq!(r.y, 6.0);
    }


    #[test]
    fn tres_cards_empilham_no_vertical() {
        // <div> vertical (default): 3 cards empilham — mesmo x, Y crescente, cada
        // um com sua caixa de 30% de 900 = 270.
        let list = layout(
            "<div style='background:#111;width:30%'>a</div>\
             <div style='background:#222;width:30%'>b</div>\
             <div style='background:#333;width:30%'>c</div>",
            900.0,
        );
        let rects: Vec<Rect> = all_rects(&list);
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.x == 0.0)); // mesmo x (vertical)
        assert!(rects.iter().all(|r| (r.w - 270.0).abs() < 0.01)); // 30% de 900
        assert!(rects[0].y < rects[1].y && rects[1].y < rects[2].y); // Y crescente
    }


    #[test]
    fn cards_lado_a_lado_no_horizontal() {
        // <row display:horizontal> com 3 <div> cada 30% → ficam LADO A LADO: X
        // crescente, MESMO y (topo), cada caixa 270 de largura. (O caso do
        // stat-card: era isto que o egui colapsava; agora o layout do DOM resolve.)
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
            "<row>\
               <div style='background:#111;width:30%'>a</div>\
               <div style='background:#222;width:30%'>b</div>\
               <div style='background:#333;width:30%'>c</div>\
             </row>",
        );
        let ctx = LayoutCtx {
            viewport_w: 900.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = all_rects(&list);
        assert_eq!(rects.len(), 3);
        // mesmo Y (lado a lado, não empilhado).
        assert!(
            rects.iter().all(|r| r.y == rects[0].y),
            "todos no mesmo topo: {rects:?}"
        );
        // X crescente: card 2 à direita do 1, card 3 à direita do 2.
        assert!(
            rects[0].x < rects[1].x && rects[1].x < rects[2].x,
            "X crescente: {rects:?}"
        );
        // cada caixa 30% de 900 = 270 (a % resolve contra o content do <row>).
        assert!(
            rects.iter().all(|r| (r.w - 270.0).abs() < 1.0),
            "largura ~270: {rects:?}"
        );
        // o 2º começa onde o 1º termina (sem sobrepor): x[1] ≈ x[0] + w[0].
        assert!(
            (rects[1].x - (rects[0].x + rects[0].w)).abs() < 1.0,
            "encostados: {rects:?}"
        );
    }


    #[test]
    fn cards_com_filhos_nao_esticam_o_ultimo() {
        // REGRESSÃO (bug visto na tela): 3 cards width:32% COM filhos (<p>) num <row>
        // largo — o ÚLTIMO não pode esticar até a borda. Cada um = 32% da largura,
        // o resto fica vazio à direita (como no navegador). p=wrap pra bater o real.
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
        crate::block::define(
            "p",
            crate::block::BlockDef {
                display: 1,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let dom = parse_html_to_dom(
            "<row>\
               <div style='background:#111;width:32%'><p>256</p><p>testes</p></div>\
               <div style='background:#222;width:32%'><p>31%</p><p>paridade</p></div>\
               <div style='background:#333;width:32%'><p>5</p><p>fases</p></div>\
             </row>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3);
        // TODOS com a MESMA largura = 32% de 1000 = 320 (o 3º NÃO estica).
        for (i, r) in rects.iter().enumerate() {
            assert!(
                (r.w - 320.0).abs() < 1.0,
                "card[{i}] devia ter 320 (32%), tem {}: {rects:?}",
                r.w
            );
        }
        // o último termina BEM antes da borda (3×320=960 < 1000), sobra vazio.
        let last = rects[2];
        assert!(
            last.x + last.w <= 1000.0,
            "último não passa da borda: {last:?}"
        );
    }


    #[test]
    fn border_box_faz_3_cards_caberem() {
        // box-sizing:border-box: width:32% INCLUI padding+border → a CAIXA é 32%,
        // 3 cards = 96% (cabem, sobra ~4%). Sem border-box (content-box) cada caixa
        // seria 32%+frame e estouraria. Prova a propriedade real do CSS.
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
            "<style>.card{box-sizing:border-box;width:32%;padding:14;border-width:2;background:#1a2030}</style>\
             <row>\
               <div class='card'>a</div><div class='card'>b</div><div class='card'>c</div>\
             </row>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3);
        // cada CAIXA = 32% de 1000 = 320 (border-box: o width É a caixa inteira).
        for (i, r) in rects.iter().enumerate() {
            assert!(
                (r.w - 320.0).abs() < 1.0,
                "card[{i}] caixa=320 (border-box): {rects:?}"
            );
        }
        // 3×320=960 < 1000: cabem com folga (sobra ~40 = 4%).
        let last = rects[2];
        assert!(last.x + last.w <= 1000.0, "cabem todos: {rects:?}");
        assert!(
            1000.0 - (last.x + last.w) >= 30.0,
            "sobra espaço à direita: {rects:?}"
        );
    }


    #[test]
    fn display_vem_do_css_nao_do_defineblock() {
        // O `display:flex` no <style> faz <row> dispor os filhos LADO A LADO, sem
        // precisar de defineBlock. `display:none` some. É o motor lendo o display DO
        // CSS. (`<div>` é block via a UA-stylesheet `ua.ts` em produção; nos testes
        // unitários — sem o prelude TS — registramos o default à mão.)
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
            "<style>row{display:flex} hide{display:none} \
                    .c{width:30%;background:#111}</style>\
             <row>\
               <div class='c'>a</div><div class='c'>b</div><div class='c'>c</div>\
             </row>\
             <hide>invisível</hide>",
        );
        let ctx = LayoutCtx {
            viewport_w: 900.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3, "3 cards (o <hide> display:none não pinta)");
        // display:flex do CSS → lado a lado (X crescente, mesmo Y).
        assert!(
            rects[0].x < rects[1].x && rects[1].x < rects[2].x,
            "lado a lado: {rects:?}"
        );
        assert!(
            rects.iter().all(|r| r.y == rects[0].y),
            "mesma linha: {rects:?}"
        );
        // display:none → o texto "invisível" NÃO está na lista.
        let has_invisivel = list
            .materialized()
            .iter()
            .any(|it| matches!(it, DisplayItem::Text { text, .. } if text.contains("invisível")));
        assert!(!has_invisivel, "display:none não renderiza o conteúdo");
    }


    #[test]
    fn margin_vertical_empilha_sem_deslocar_horizontal() {
        // margin_v (UA-stylesheet) separa blocos no VERTICAL mas NÃO empurra no
        // eixo horizontal (como `margin: Npx 0` do navegador para h1/p). Dois
        // parágrafos com margin_v: o 2º começa mais abaixo, mas ambos em x=0.
        crate::block::define(
            "p",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        crate::style::define_style("p", crate::style::SLOT_MARGIN_V, 16);
        let dom = parse_html_to_dom("<p>um</p><p>dois</p>");
        let ctx = LayoutCtx {
            viewport_w: 600.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(f32, f32)> = list
            .materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 2);
        // X: ambos em 0 (margin VERTICAL não desloca horizontal).
        assert_eq!(texts[0].0, 0.0, "1º texto em x=0: {texts:?}");
        assert_eq!(
            texts[1].0, 0.0,
            "2º texto em x=0 (margin não empurrou): {texts:?}"
        );
        // Y: o 2º bem abaixo (margin colapsado entre eles + altura da linha).
        assert!(
            texts[1].1 > texts[0].1 + 20.0,
            "2º empilhado abaixo: {texts:?}"
        );
    }
