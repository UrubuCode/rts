//! Fora do fluxo: `absolute`, `fixed`, floats, `clear`, hit-test e `z-index`.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;

    #[test]
    fn hit_test_escolhe_o_no_mais_profundo() {
        // Filho dentro do pai: clicar dentro do filho devolve o FILHO (menor
        // área = mais profundo); clicar no pai fora do filho devolve o pai;
        // fora de tudo devolve None.
        def_div();
        let dom = parse_html_to_dom(
            "<div id=pai style='padding:50px'><div id=filho style='height:20px'>x</div></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let pai = dom.query("#pai").unwrap();
        let filho = dom.query("#filho").unwrap();
        let pai_idx = dom.resolve(pai).unwrap();
        let filho_idx = dom.resolve(filho).unwrap();
        let fr = list.geometry().rects[&filho_idx];
        let hit = list.hit_test(fr.x + fr.w / 2.0, fr.y + fr.h / 2.0);
        assert_eq!(hit, Some(filho_idx));
        // canto do pai (dentro do padding, fora do filho).
        let pr = list.geometry().rects[&pai_idx];
        let hit2 = list.hit_test(pr.x + 5.0, pr.y + 5.0);
        assert_eq!(hit2, Some(pai_idx));
        // fora de tudo.
        assert_eq!(list.hit_test(-10.0, -10.0), None);
    }

    /// `display:none` num ANCESTRAL remove a subárvore inteira do layout — e um
    /// `position:absolute` lá dentro não é exceção.
    ///
    /// Era o pior número da página: um `<input type=checkbox; height:100%>` de um
    /// menu escondido da Wikipédia continuava a ser medido, e como o pai
    /// escondido não tem caixa, a procura do containing block saltava-o e
    /// ancorava-o num contentor com a altura do DOCUMENTO — 96 665px de altura
    /// para um controlo invisível.
    #[test]
    fn absolute_dentro_de_display_none_nao_tem_caixa() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let dom = parse_html_to_dom(
            "<div style='position:relative;height:400px'>               <div style='display:none;position:relative'>                 <i id='x' style='position:absolute;height:100%'>a</i>               </div>             </div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
        assert!(
            list.geometry().rects.get(&idx).is_none(),
            "um absoluto num ramo escondido não gera caixa: {:?}",
            list.geometry().rects.get(&idx)
        );
        // e o que NÃO está escondido continua a ser posicionado.
        let dom = parse_html_to_dom(
            "<div style='position:relative;height:400px'>               <i id='y' style='position:absolute;height:100%'>a</i>             </div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#y").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("este devia ter caixa");
        assert_eq!(r.h, 400.0, "100% da altura do containing block: {r:?}");
    }

    #[test]
    fn hit_test_respeita_z_index_em_elementos_sobrepostos() {
        let dom = parse_html_to_dom(
            "<style>#back { position:absolute; left:0; top:0; width:200px; height:200px; z-index:10 } #front { position:absolute; left:0; top:0; width:100px; height:100px; z-index:0 }</style><div id='back'></div><div id='front'></div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let back = dom.resolve(dom.query("#back").unwrap()).unwrap();
        assert_eq!(list.hit_test(50.0, 50.0), Some(back));
    }

    #[test]
    fn absolute_ancora_no_containing_block() {
        // `position:absolute` com right:0/top:0 ancora no canto do ANCESTRAL
        // positioned (relative), não do viewport (o padrão do google: ícone no
        // canto da caixa de busca).
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma x absoluto a partir de `margin-left:100px`.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style>\
             <div style='position:relative;width:400px;height:50px;margin-left:100px'>\
             <span style='position:absolute;top:0px;right:0px;width:30px;height:30px;background:#00f'>i</span>\
             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        // o span azul: canto direito da caixa (x=100, w=400 → 500) menos a largura.
        let sp = list
            .materialized()
            .iter()
            .find_map(|it| match it {
                DisplayItem::SolidRect { rect, color, .. } if *color == 0x0000FFFF => Some(*rect),
                _ => None,
            })
            .expect("span absolute");
        assert!(
            (sp.x - 470.0).abs() < 2.0,
            "x do abs: {} (esperado ~470 = 100+400-30)",
            sp.x
        );
        assert!(
            sp.y.abs() < 2.0 || (sp.y - 0.0).abs() < 2.0,
            "y do abs: {}",
            sp.y
        );
    }

    #[test]
    fn float_left_right_dividem_a_linha() {
        // O header clássico (brand+nav do Bootstrap cover): float:left e
        // float:right consecutivos dividem a MESMA linha; o irmão não-float
        // começa abaixo do float mais alto. O pai NÃO estabelece BFC (é um
        // `<div>` comum, sem `overflow`/`flow-root`/etc.) — pelo CSS 2.1
        // §10.6.7 só o BFC responsável cresce para conter os floats, e este
        // não é ele: ver a asserção de `r[0].h` mais abaixo.
        let list = layout(
            "<div style='background:#111'>               <div style='float:left; background:#222; width:100; height:30'>brand</div>               <div style='float:right; background:#333; width:150; height:40'>nav</div>               <div style='background:#444; height:20'>abaixo</div>             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 4);
        assert_eq!(
            (r[1].x, r[1].y),
            (0.0, 0.0),
            "left encosta na esquerda: {r:?}"
        );
        assert_eq!(
            (r[2].x, r[2].y),
            (450.0, 0.0),
            "right encosta na direita (600-150)"
        );
        // ⚠️ MUDOU, e a mudança é a correção: este bloco começava em y=40 — o
        // modelo antigo empurrava para baixo do float tudo o que não fosse
        // float. Pelo CSS a caixa de bloco ao lado de um float NÃO desce nem
        // encolhe: sobrepõe-se a ele, e só as suas linhas contornam. Medido no
        // Chrome, na Wikipédia: a `<figure>` com `float:right` fica em y=5877 e
        // o `<p>` seguinte em y=5869, ACIMA do topo do float, com a largura
        // cheia da coluna. Ver [`Exclusao`].
        assert_eq!(
            r[3].y, 0.0,
            "o não-float sobrepõe-se ao float, não desce: {r:?}"
        );
        // ⚠️ MUDOU outra vez, e desta vez é a entidade que faltava
        // (`layout::bfc::BlockFormattingContext`) a fechar a divergência que
        // este teste PINAVA de propósito até aqui: um float só faz o pai
        // crescer quando o pai é o BFC responsável (CSS 2.1 §10.6.7), e este
        // `<div>` comum não é. Era 40 (o float mais alto) porque o
        // crescimento era incondicional; agora o pai mede só o que o FLUXO
        // NORMAL pede — `r[3]` (o bloco de 20, que não desce nem encolhe por
        // causa do float, ver acima) é o único que ocupa espaço na conta dele.
        // Os 40 do float não desaparecem: escapam para o BFC do ANTEPASSADO
        // deste `<div>` (aqui, fora do que este teste lê).
        assert_eq!(
            r[0].h, 20.0,
            "sem BFC o pai não cresce pelos floats — só o fluxo normal (20): {r:?}"
        );
    }

    #[test]
    fn texto_corre_ao_lado_do_float_em_vez_de_descer() {
        // O caso da Wikipédia, em isolamento e com a figura de largura
        // DECLARADA (sem `<img>`, portanto imune ao tamanho intrínseco):
        // `float:right` de 200 seguido de um `<p>`. Medido no Chrome, o
        // parágrafo fica ACIMA do topo do float e com a largura cheia — só as
        // suas LINHAS encolhem para a banda livre.
        let list = layout(
            &format!(
                "<div style='background:#111'>                   <div style='float:right; background:#222; width:200; height:100'></div>                   <p style='background:#333'>{FRASE}</p>                 </div>"
            ),
            600.0,
        );
        let r = all_rects(&list);
        let p = r[2];
        assert_eq!(p.y, 16.0, "o bloco NÃO desce abaixo do float: {r:?}");
        assert_eq!(p.w, 600.0, "o bloco NÃO encolhe — mantém a largura: {r:?}");
        // as linhas que cruzam o float cabem na banda de 400; nenhuma invade.
        let t = all_texts(&list);
        let cruzam: Vec<_> = t.iter().filter(|(_, _, y, _)| *y < 100.0).collect();
        assert!(cruzam.len() >= 2, "várias linhas ao lado do float: {t:?}");
        for (txt, x, y, _) in &cruzam {
            let largura = txt.chars().count() as f32 * 16.0 * crate::style::PROP_ADVANCE;
            assert!(
                *x + largura <= 400.5,
                "linha em y={y} invade o float: {txt:?} x={x}"
            );
        }
    }

    #[test]
    fn linha_abaixo_do_float_volta_a_largura_toda() {
        // A exclusão é uma FAIXA, não um estado do parágrafo: assim que as
        // linhas passam o fundo do float, voltam a ter a largura do content. Um
        // float baixo (30) deixa só a primeira linha estreita.
        let list = layout(
            &format!(
                "<div style='background:#111'>                   <div style='float:right; background:#222; width:300; height:30'></div>                   <p>{FRASE} {FRASE}</p>                 </div>"
            ),
            600.0,
        );
        let t = all_texts(&list);
        let acima: Vec<_> = t.iter().filter(|(_, _, y, _)| *y < 30.0).collect();
        let abaixo: Vec<_> = t.iter().filter(|(_, _, y, _)| *y >= 30.0).collect();
        assert!(
            !acima.is_empty() && !abaixo.is_empty(),
            "linhas dos dois lados: {t:?}"
        );
        let largura = |v: &Vec<&(String, f32, f32, u32)>| {
            let ch = 16.0 * crate::style::PROP_ADVANCE;
            v.iter()
                .map(|(s, _, _, _)| s.chars().count() as f32 * ch)
                .fold(0.0, f32::max)
        };
        assert!(
            largura(&acima) <= 300.5,
            "linha ao lado do float é curta: {acima:?}"
        );
        assert!(
            largura(&abaixo) > 300.5,
            "abaixo do float a linha volta à largura toda: {abaixo:?}"
        );
    }

    #[test]
    fn float_left_empurra_o_inicio_da_linha() {
        // Um `float:left` não encurta a linha pela direita: desloca o COMEÇO
        // dela. É a diferença que um teste só de largura não apanha.
        let list = layout(
            &format!(
                "<div style='background:#111'>                   <div style='float:left; background:#222; width:150; height:100'></div>                   <p>{FRASE}</p>                 </div>"
            ),
            600.0,
        );
        let t = all_texts(&list);
        let cruzam: Vec<_> = t.iter().filter(|(_, _, y, _)| *y < 100.0).collect();
        assert!(!cruzam.is_empty(), "há texto ao lado do float: {t:?}");
        for (txt, x, y, _) in &cruzam {
            assert!(
                *x >= 150.0,
                "linha em y={y} começa depois do float: {txt:?} x={x}"
            );
        }
    }

    #[test]
    fn clear_continua_a_descer_abaixo_do_float() {
        // O `clear` é o que sobra do comportamento antigo, e agora é a ÚNICA
        // forma de descer: quem não o declara passa ao lado.
        let list = layout(
            "<div style='background:#111'>               <div style='float:right; background:#222; width:200; height:100'></div>               <div style='clear:both; background:#333; height:20'></div>             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[2].y, 100.0, "o `clear` desce abaixo do float: {r:?}");
    }

    #[test]
    fn position_fixed_sai_do_fluxo_e_posiciona_no_viewport() {
        // O caso do dropdown do Bootstrap cover: um `position:fixed` DENTRO de um
        // flex row NÃO pode empurrar os irmãos (sai do fluxo) e pinta contra o
        // viewport pelos offsets (bottom/right).
        let list = layout(
            "<div style='display:flex; background:#111'>\
               <div style='position:fixed; bottom:10; right:10; width:50; height:20; background:#900'>t</div>\
               <div style='background:#222; height:30'>conteudo</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3);
        // o item NO FLUXO começa em x=0 (o fixed não o empurrou).
        assert_eq!(r[1].x, 0.0, "{r:?}");
        assert_eq!(r[1].h, 30.0);
        // o fixed: x = 600-50-10 = 540; y = viewport_h(600)-20-10 = 570. Pintado por
        // ÚLTIMO (por cima do fluxo).
        assert_eq!((r[2].x, r[2].y), (540.0, 570.0), "{r:?}");
        assert_eq!((r[2].w, r[2].h), (50.0, 20.0));
    }

    #[test]
    fn viewport_e_o_containing_block_da_raiz() {
        // `height:100%` no elemento RAIZ resolve contra a viewport_h (600 no
        // helper) — o `h-100` do html/body de páginas reais.
        //
        // Declarado em `html, body` e não num `<div>` de topo: o parser cria
        // `<html>`/`<body>` implícitos, como qualquer browser, e um `<div>` já
        // não é filho direto do documento — a percentagem dele resolve contra o
        // pai, de altura automática, que é o que o Chrome também faz. É por isto
        // mesmo que as páginas reais escrevem `html, body { height: 100% }` (a
        // Wikipédia escreve-o): a corrente de percentagens tem de partir da
        // raiz. O que o teste pina — a viewport é o containing block da raiz —
        // continua provado, e agora pelo caminho real.
        let list = layout(
            "<style>html,body{height:100%}</style><div style='height:100%;background:#111'>x</div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].h, 600.0, "{r:?}");
    }
