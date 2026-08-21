//! Cache de layout: reuso, invalidação, e a equivalência entre o recalculado
//! e o reaproveitado.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada. A indentação de 4 espaços é a do `mod tests` de origem e foi
//! MANTIDA: há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;


    /// Dentro de um container que ROLA, `width:%` de um filho é a porcentagem da
    /// CAIXA — não do conteúdo transbordado.
    ///
    /// O layout de um scroll container usa a largura NATURAL do conteúdo para
    /// dispor os filhos (senão o flex os comprimiria e nada transbordaria), e
    /// essa largura estava servindo também de base para as porcentagens. Numa
    /// página que aninha vários `overflow:auto` — o WhatsApp Web é uma —, cada
    /// nível multiplicava o seguinte, e o conteúdo terminava desenhado fora da
    /// janela: a tela abria vazia com tudo pintado à direita dela.
    #[test]
    fn porcentagem_dentro_de_scroll_e_da_caixa() {
        let dom = parse_html_to_dom(
            "<div style='overflow-y:auto; padding-left:40px; padding-right:40px'>               <div id='meio' style='width:100%'>                 <div style='width:3000px'>conteudo bem mais largo que a caixa</div>               </div>             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 1000.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let lista = layout_document(&dom, &ctx);
        let meio = dom.resolve(dom.query("#meio").unwrap()).unwrap();
        let rect = lista.geometry().rects[&meio];
        assert!(
            (rect.w - 920.0).abs() < 1.0,
            "100% da caixa (1000 - 80 de padding) e não do conteúdo: {rect:?}"
        );
        // E o conteúdo largo continua transbordando — o container rola, não corta.
        let geo = lista.geometry();
        let largo = dom
            .node(meio)
            .children
            .iter()
            .copied()
            .find(|c| matches!(dom.node(*c).kind, NodeKind::Element { .. }))
            .expect("o filho largo");
        assert!(
            geo.rects[&largo].w > 2900.0,
            "o filho largo mantém a largura dele"
        );
    }


    /// SEQUÊNCIA LONGA de mutações sorteadas: o reuso tem de bater com o cálculo
    /// do zero em todas elas.
    ///
    /// O teste dirigido acima cobre os caminhos que eu sabia listar; este cobre
    /// as COMBINAÇÕES, que é onde um cache erra — invalidar A e depois B, mexer
    /// num nó recém-inserido, remover o que acabou de mudar. O gerador é um LCG
    /// com semente fixa: a sequência é sempre a mesma, então uma falha é
    /// reproduzível e o teste nunca fica intermitente.
    #[test]
    fn sequencia_longa_de_mutacoes_mantem_a_equivalencia() {
        let mut dom = parse_html_to_dom(
            "<style>.a{padding:4px}.b{margin:2px}.t{font-size:14px}</style>             <main id='root'>               <div class='a'><p class='t'>um</p><p>dois</p></div>               <div class='b'><span>tres</span> quatro <b>cinco</b></div>               <ul id='l'><li>x</li><li>y</li></ul>             </main>",
        );
        let ctx = LayoutCtx {
            viewport_w: 500.0,
            viewport_h: 400.0,
            measurer: &ApproxMeasurer,
        };
        let raiz = dom.query("#root").unwrap();
        let lista = dom.query("#l").unwrap();
        let mut semente = 0x2545_F491_4F6C_DD1Du64;
        let mut sorteia = move |n: u64| {
            semente = semente
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (semente >> 33) % n
        };
        let mut vivos: Vec<crate::dom::NodeId> = dom.query_all("p, li, span, b");

        for passo in 0..120 {
            match sorteia(5) {
                0 => {
                    if !vivos.is_empty() {
                        let alvo = vivos[sorteia(vivos.len() as u64) as usize];
                        dom.set_text(alvo, &format!("t{passo}"));
                    }
                }
                1 => {
                    if !vivos.is_empty() {
                        let alvo = vivos[sorteia(vivos.len() as u64) as usize];
                        let classe = ["a", "b", "t", "sem-regra"][sorteia(4) as usize];
                        dom.set_attr(alvo, "class", classe);
                    }
                }
                2 => {
                    let novo = dom.create_element(if passo % 2 == 0 { "li" } else { "p" });
                    let txt = dom.create_text_node(&format!("novo {passo}"));
                    dom.append_child(novo, txt);
                    let pai = if passo % 3 == 0 { lista } else { raiz };
                    dom.append_child(pai, novo);
                    vivos.push(novo);
                }
                3 => {
                    if vivos.len() > 3 {
                        let i = sorteia(vivos.len() as u64) as usize;
                        let alvo = vivos.remove(i);
                        dom.remove_node(alvo);
                    }
                }
                _ => {
                    if vivos.len() > 2 {
                        // move um nó para o fim da lista (reordena irmãos)
                        let alvo = vivos[sorteia(vivos.len() as u64) as usize];
                        dom.append_child(lista, alvo);
                    }
                }
            }

            let reusado = layout_cached(&dom, &ctx);
            dom.clear_fragment_cache();
            let zero = layout_document(&dom, &ctx);
            assert_eq!(
                reusado.materialized().len(),
                zero.materialized().len(),
                "passo {passo}: nº de itens diverge"
            );
            for (i, (a, b)) in reusado
                .materialized()
                .iter()
                .zip(&zero.materialized())
                .enumerate()
            {
                assert!(
                    itens_equivalentes(a, b),
                    "passo {passo}, item {i}:
  reuso: {a:?}
  zero:  {b:?}"
                );
            }
        }
    }


    /// O caminho CACHEADO e o cálculo do zero produzem a mesma coisa, depois de
    /// cada mutação de uma sequência que passa por texto, atributo, classe,
    /// inserção e remoção.
    ///
    /// É o guarda de qualquer reuso de geometria: um cache que devolve a lista
    /// errada tem exatamente a mesma cara de um cache rápido, e o único jeito de
    /// distinguir os dois é recalcular e comparar. Vale hoje (o reuso é
    /// tudo-ou-nada) e continua valendo quando o reuso for por subárvore.
    #[test]
    fn o_layout_reusado_e_igual_ao_recalculado() {
        let mut dom = parse_html_to_dom(
            "<style>.card{padding:8px;margin:4px}.t{font-size:18px}.hi{color:#ff0000}</style>             <main id='root'>               <div class='card'><h3 class='t'>Um titulo</h3><p>texto de exemplo aqui</p></div>               <div class='card'><h3 class='t'>Outro</h3><p>mais texto <b>em negrito</b> junto</p></div>               <ul id='lista'><li>a</li><li>b</li><li>c</li></ul>             </main>",
        );
        let ctx = LayoutCtx {
            viewport_w: 640.0,
            viewport_h: 480.0,
            measurer: &ApproxMeasurer,
        };
        let alvo = dom.query("p").unwrap();
        let card = dom.query(".card").unwrap();
        let lista = dom.query("#lista").unwrap();

        let mut passo = 0;
        let conferir = |dom: &Dom, passo: &mut i32| {
            let cacheado = layout_cached(dom, &ctx);
            // Do ZERO: sem os fragmentos, nada é reusado e o resultado é o que o
            // layout produz quando calcula tudo. Comparar o reuso com ele mesmo
            // seria o mesmo que não comparar.
            dom.clear_fragment_cache();
            let recalculado = layout_document(dom, &ctx);
            assert_eq!(
                cacheado.materialized().len(),
                recalculado.materialized().len(),
                "quantidade de itens diverge no passo {passo}"
            );
            for (i, (a, b)) in cacheado
                .materialized()
                .iter()
                .zip(&recalculado.materialized())
                .enumerate()
            {
                assert!(
                    itens_equivalentes(a, b),
                    "item {i} diverge no passo {passo}:
  reuso: {a:?}
  cálculo: {b:?}"
                );
            }
            assert!(
                (cacheado.content_height - recalculado.content_height).abs() < TOL,
                "altura divergente no passo {passo}"
            );
            let mut a: Vec<_> = cacheado
                .geometry()
                .rects
                .iter()
                .map(|(i, r)| (*i, *r))
                .collect();
            let mut b: Vec<_> = recalculado
                .geometry()
                .rects
                .iter()
                .map(|(i, r)| (*i, *r))
                .collect();
            a.sort_by_key(|(idx, _)| *idx);
            b.sort_by_key(|(idx, _)| *idx);
            assert_eq!(
                a.len(),
                b.len(),
                "nº de retângulos diverge no passo {passo}"
            );
            for ((ia, ra), (ib, rb)) in a.iter().zip(&b) {
                assert_eq!(ia, ib, "nós diferentes no passo {passo}");
                assert!(
                    rects_equivalentes(ra, rb),
                    "rect de {ia} diverge no passo {passo}"
                );
            }
            *passo += 1;
        };

        conferir(&dom, &mut passo);
        dom.set_text(alvo, "outro texto bem mais longo do que o anterior era");
        conferir(&dom, &mut passo);
        dom.set_attr(alvo, "class", "hi");
        conferir(&dom, &mut passo);
        dom.set_attr(alvo, "class", "classe-que-ninguem-cita");
        conferir(&dom, &mut passo);
        let novo = dom.create_element("li");
        let txt = dom.create_text_node("d");
        dom.append_child(novo, txt);
        dom.append_child(lista, novo);
        conferir(&dom, &mut passo);
        dom.remove_node(card);
        conferir(&dom, &mut passo);
        dom.set_inner_html(lista, "<li>x</li><li>y</li>");
        conferir(&dom, &mut passo);
        assert_eq!(passo, 7, "todos os passos foram conferidos");
    }


    /// O cache de layout devolve a MESMA lista enquanto nada muda, e uma NOVA
    /// depois de qualquer mutação. Sem a segunda metade, "o frame parado custa
    /// zero" seria só outra forma de dizer que a página parou de atualizar.
    #[test]
    fn cache_de_layout_reusa_e_invalida() {
        let mut dom = parse_html_to_dom("<div id='a'><p>um</p></div>");
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let first = layout_cached(&dom, &ctx);
        let again = layout_cached(&dom, &ctx);
        assert!(
            std::rc::Rc::ptr_eq(&first, &again),
            "nada mudou: a lista é a mesma"
        );

        let alvo = dom.query("#a").unwrap();
        dom.set_text(alvo, "outro");
        let after = layout_cached(&dom, &ctx);
        assert!(
            !std::rc::Rc::ptr_eq(&first, &after),
            "o texto mudou: lista nova"
        );

        // Viewport diferente também é outro layout.
        let narrow = LayoutCtx {
            viewport_w: 300.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let small = layout_cached(&dom, &narrow);
        assert!(!std::rc::Rc::ptr_eq(&after, &small));
    }


    #[test]
    fn cache_de_medidas_invalida_largura_mutada() {
        def_div();
        let mut dom = parse_html_to_dom(
            "<div id='host' style='display:flex'><div id='card' style='width:100px;height:10px'></div></div>",
        );
        let card = dom.query("#card").unwrap();
        let card_idx = dom.resolve(card).unwrap();
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };

        let first = layout_document(&dom, &ctx);
        let _warm = layout_document(&dom, &ctx);
        let before = first.geometry().rects[&card_idx].w;
        dom.set_style_property(card, "width", "200px");
        let after = layout_document(&dom, &ctx).geometry().rects[&card_idx].w;

        assert!((before - 100.0).abs() < 0.1);
        assert!((after - 200.0).abs() < 0.1);
    }


    #[test]
    fn cache_intrinseca_invalida_texto_mutado() {
        def_div();
        let mut dom = parse_html_to_dom(
            "<div id='host' style='display:flex'><span id='text' style='display:inline-block'>a</span></div>",
        );
        let text = dom.query("#text").unwrap();
        let text_idx = dom.resolve(text).unwrap();
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };

        let before = layout_document(&dom, &ctx).geometry().rects[&text_idx].w;
        let _warm = layout_document(&dom, &ctx);
        dom.set_text(text, "uma linha de texto bem mais comprida");
        let after = layout_document(&dom, &ctx).geometry().rects[&text_idx].w;

        assert!(
            after > before,
            "a largura intrínseca deve acompanhar o novo texto"
        );
    }
