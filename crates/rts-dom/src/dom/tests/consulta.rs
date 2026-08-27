//! Seletores, `query*`, travessia por elemento e geometria consultada.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn query_por_tag_id_classe() {
        let dom = parse_html_to_dom(
            "<div class='card'><span id='alvo'>x</span><b class='hl a'>y</b></div>",
        );
        // tag
        let span = dom.query("span").unwrap();
        assert_eq!(tag(&dom, idx(&dom, span)), "span");
        // #id
        assert_eq!(dom.query("#alvo"), Some(span));
        // .classe (mesmo dentro de class multi-valor "hl a")
        let b = dom.query(".hl").unwrap();
        assert_eq!(tag(&dom, idx(&dom, b)), "b");
        assert_eq!(dom.query(".a"), Some(b));
        // sem match
        assert_eq!(dom.query("#naoexiste"), None);
        assert_eq!(dom.query(".naoexiste"), None);
    }


    #[test]
    fn navegacao_dom() {
        // parentNode / first|lastChild / next|previousSibling sobre <div><a/><b/><i/></div>
        let dom = parse_html_to_dom("<div><a>1</a><b>2</b><i>3</i></div>");
        let div = dom.query("div").unwrap();
        let a = dom.query("a").unwrap();
        let b = dom.query("b").unwrap();
        let i = dom.query("i").unwrap();
        // parentNode
        assert_eq!(dom.parent_of(a), Some(div));
        // Pai do div = o `<body>` implícito (era o `#document`); ver `body_idx`.
        assert_eq!(
            dom.parent_of(div).map(|p| idx(&dom, p)),
            Some(body_idx(&dom))
        );
        // first/lastChild do div
        assert_eq!(dom.first_child(div), Some(a));
        assert_eq!(dom.last_child(div), Some(i));
        // siblings
        assert_eq!(dom.next_sibling(a), Some(b));
        assert_eq!(dom.next_sibling(b), Some(i));
        assert_eq!(dom.next_sibling(i), None); // último
        assert_eq!(dom.previous_sibling(i), Some(b));
        assert_eq!(dom.previous_sibling(a), None); // primeiro
    }


    #[test]
    fn seletor_composto() {
        let dom = parse_html_to_dom(
            "<p class=\"card big\" id=\"x\">a</p><p class=\"card\">b</p><div class=\"card\">c</div>",
        );
        assert_eq!(dom.query_all("p.card").len(), 2);
        assert_eq!(dom.query_all(".card.big").len(), 1);
        assert_eq!(dom.query_all("p.card#x").len(), 1);
        assert_eq!(dom.query_all("div.card").len(), 1);
    }


    #[test]
    fn seletor_combinadores() {
        let dom = parse_html_to_dom(
            "<div id=\"root\"><section><p class=\"a\">1</p></section><p class=\"b\">2</p><span>3</span><p class=\"c\">4</p></div>",
        );
        assert_eq!(dom.query_all("#root p").len(), 3); // descendente
        assert_eq!(dom.query_all("#root > p").len(), 2); // filho direto
        assert_eq!(dom.query_all("p.b + span").len(), 1); // irmão imediato
        assert_eq!(dom.query_all("p.b ~ p").len(), 1); // irmão geral
        assert_eq!(dom.query_all("section p.a").len(), 1);
    }


    #[test]
    fn seletor_atributo() {
        let dom = parse_html_to_dom(
            "<a href=\"https://x.com/page\">1</a><a href=\"http://y.org\">2</a><input type=\"text\"><input disabled>",
        );
        assert_eq!(dom.query_all("[href]").len(), 2);
        assert_eq!(dom.query_all("[disabled]").len(), 1);
        assert_eq!(dom.query_all("[type=text]").len(), 1);
        assert_eq!(dom.query_all("[href^=https]").len(), 1);
        assert_eq!(dom.query_all("[href$=.org]").len(), 1);
        assert_eq!(dom.query_all("[href*=x.com]").len(), 1);
    }


    #[test]
    fn seletor_pseudo_estrutural() {
        let dom = parse_html_to_dom("<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul><div></div>");
        assert_eq!(dom.query_all("li:first-child").len(), 1);
        assert_eq!(dom.query_all("li:last-child").len(), 1);
        assert_eq!(dom.query_all("li:nth-child(2)").len(), 1);
        assert_eq!(dom.query_all("li:nth-child(odd)").len(), 2);
        assert_eq!(dom.query_all("li:nth-child(even)").len(), 2);
        assert_eq!(dom.query_all("li:nth-child(2n+1)").len(), 2);
        assert_eq!(dom.query_all("div:empty").len(), 1);
        assert_eq!(dom.query_all("ul:only-child").len(), 0);
    }


    #[test]
    fn seletor_pseudo_estado() {
        // :checked/:disabled/:required mapeiam para presença de atributo (#1752).
        let dom = parse_html_to_dom(
            "<input type=\"checkbox\" checked><input type=\"checkbox\"><input required><button disabled>x</button>",
        );
        assert_eq!(dom.query_all("input:checked").len(), 1);
        assert_eq!(dom.query_all(":disabled").len(), 1); // o button
        assert_eq!(dom.query_all("input:required").len(), 1);
        assert_eq!(dom.query_all("input:enabled").len(), 3); // os 3 inputs (nenhum disabled)
    }


    #[test]
    fn seletor_lista_e_invalidos() {
        // bugs da verificação adversarial corrigidos (#1752).
        let dom = parse_html_to_dom("<div><p>1</p><a>2</a><span>3</span></div>");
        // lista por vírgula: p, a → casa qualquer um.
        assert_eq!(dom.query_all("p, a").len(), 2);
        assert_eq!(dom.query_all("p, a, span").len(), 3);
        // combinador duplo `>>` → inválido, casa 0.
        assert_eq!(dom.query_all("div >> p").len(), 0);
        // universal no meio `p*` → inválido.
        assert_eq!(dom.query_all("p*").len(), 0);
    }


    #[test]
    fn seletor_root_em_fragmento() {
        // :root casa o `<html>`, e um fragmento passou a TER um: o parser cria
        // `<html>`/`<body>` implícitos como qualquer browser. A expectativa muda
        // aqui porque a estrutura de topo é ela própria o que este teste pina —
        // e a ausência dessas tags apagava, em silêncio, toda a propriedade
        // HERDADA declarada em `body{…}`.
        let dom = parse_html_to_dom("<div id=\"a\">x</div><div id=\"b\">y</div>");
        assert_eq!(dom.query_all(":root").len(), 1);
        // com UM só top-level, :root casa esse 1.
        let dom2 = parse_html_to_dom("<html><body>x</body></html>");
        assert_eq!(dom2.query_all(":root").len(), 1);
    }


    #[test]
    fn seletor_atributo_com_colchete_no_valor() {
        // [data-x="a]b"] — o `]` literal no valor aspado não fecha o seletor.
        let dom = parse_html_to_dom("<div data-x=\"a]b\">x</div>");
        assert_eq!(dom.query_all("[data-x=\"a]b\"]").len(), 1);
    }


    #[test]
    fn traversal_por_elemento() {
        // firstElementChild/nextElementSibling pulam texto e comentário (#1757).
        let dom = parse_html_to_dom(
            "<div id=\"a\"><!--c--><p class=\"x\">um</p>txt<span>dois</span></div>",
        );
        let div = dom.query("#a").unwrap();
        let p = dom.first_element_child(div).unwrap();
        // node_name no Rust é minúsculo (a fachada TS faz toUpperCase).
        assert_eq!(dom.node_name(p).as_deref(), Some("p")); // pulou o comentário
        let span = dom.next_element_sibling(p).unwrap();
        assert_eq!(dom.node_name(span).as_deref(), Some("span")); // pulou o texto "txt"
        assert_eq!(dom.last_element_child(div), Some(span));
        assert_eq!(dom.parent_element(p), Some(div));
        // matches/closest com seletor simples.
        assert!(dom.matches_selector(p, ".x"));
        assert!(!dom.matches_selector(p, ".y"));
        assert_eq!(dom.closest(p, "#a"), Some(div));
        assert_eq!(dom.closest(p, "p"), Some(p)); // o próprio nó conta
    }


    #[test]
    fn query_por_subarvore() {
        // querySelector restrito à subárvore (#1758): o <p> dentro de #b não deve
        // ser achado pela busca dentro de #a.
        let dom = parse_html_to_dom(
            "<div id=\"a\"><p class=\"x\">in-a</p></div><div id=\"b\"><p class=\"x\">in-b</p></div>",
        );
        let a = dom.query("#a").unwrap();
        let found = dom.query_within(a, ".x").unwrap();
        assert_eq!(dom.text_content(found).as_deref(), Some("in-a")); // só o de dentro de #a
        assert_eq!(dom.query_all_within(a, ".x").len(), 1); // não vê o de #b
        // mas a busca global vê os dois.
        assert_eq!(dom.query_all(".x").len(), 2);
    }


    #[test]
    fn get_elements_by() {
        let dom = parse_html_to_dom(
            "<div class=\"card\"><p class=\"card\">x</p></div><span name=\"f\">y</span><span name=\"f\">z</span>",
        );
        assert_eq!(dom.get_elements_by_class_name("card").len(), 2); // div + p
        assert_eq!(dom.get_elements_by_tag_name("span").len(), 2);
        assert_eq!(dom.get_elements_by_name("f").len(), 2); // os 2 spans
        // '*' = todos os elementos.
        // `*` conta também o `<html>` e o `<body>` implícitos, como no browser.
        assert_eq!(dom.get_elements_by_tag_name("*").len(), 6); // html,body,div,p,span,span
    }


    #[test]
    fn matcher_universal_e_multi_classe() {
        // BUG (verificação adversarial): "*" não casava; multi-classe não tokenizava.
        let dom = parse_html_to_dom("<div class=\"a b\"><p class=\"a\">x</p></div>");
        // "*" casa todos os elementos — incluindo o `<html>`/`<body>` que o
        // parser cria como qualquer browser (ver `body_idx`).
        assert_eq!(dom.query_all("*").len(), 4);
        // multi-classe = AND: só o div tem 'a' E 'b'.
        assert_eq!(dom.get_elements_by_class_name("a b").len(), 1);
        assert_eq!(dom.get_elements_by_class_name("a").len(), 2); // div e p têm 'a'
        // ordem dos tokens não importa.
        assert_eq!(dom.get_elements_by_class_name("b a").len(), 1);
    }


    #[test]
    fn geometria_em_lote_responde_o_mesmo_que_a_singular() {
        // O que se pina NÃO é que o layout está certo — é que pedir as caixas de
        // uma vez dá o MESMO que pedi-las uma a uma. É a única coisa que separa
        // "o extrator de paridade ficou rápido" de "o extrator de paridade
        // passou a medir outra coisa", e a diferença entre as duas seria um
        // número de paridade a melhorar sem ninguém ter corrigido layout.
        let dom = parse_html_to_dom(
            "<style>.a{width:100px;height:30px;margin:5px}</style>\
             <div class=\"a\">um</div><p>dois<span>três</span></p><div><div>n</div></div>",
        );
        let ids: Vec<NodeId> = dom.query_all("div, p, span");
        assert!(ids.len() >= 5, "a fixture tem de ter elementos para comparar");
        let lote = dom.bounding_components_many(&ids);
        for (i, &id) in ids.iter().enumerate() {
            for k in 0..4i64 {
                assert_eq!(
                    lote[i * 4 + k as usize],
                    dom.bounding_component(id, k),
                    "nó {i}, componente {k}"
                );
            }
        }
        // E um id que não resolve responde zeros nas quatro, como a singular.
        let morto = NodeId {
            generation: u32::MAX,
            idx: u32::MAX,
        };
        assert_eq!(dom.bounding_components_many(&[morto]), vec![0.0; 4]);
    }


    #[test]
    fn query_id_duplicado_retorna_primeiro_na_ordem_documental() {
        let dom = parse_html_to_dom("<div id='same'></div><span id='same'></span>");
        let first = dom.query("div").unwrap();
        assert_eq!(dom.query("#same"), Some(first));
    }


    /// A consulta por índice devolve em ordem DOCUMENTAL, não em ordem de
    /// arena — e as duas divergem assim que um `appendChild` reordena. É a
    /// armadilha que o comentário do `query_idx` sempre alertou, e agora que os
    /// índices respondem de verdade, é ela que precisa de teste.
    #[test]
    fn consulta_por_indice_respeita_a_ordem_documental() {
        let mut dom = parse_html_to_dom(
            "<div id='host'><p class='x' id='a'>a</p><p class='x' id='b'>b</p></div>",
        );
        let host = dom.query("#host").unwrap();
        let a = dom.query("#a").unwrap();
        // move o PRIMEIRO para o fim: ordem de arena continua a,b; a documental
        // vira b,a.
        dom.append_child(host, a);
        let ids: Vec<String> = dom
            .query_all(".x")
            .into_iter()
            .map(|n| dom.get_attr(n, "id").unwrap_or_default().to_string())
            .collect();
        assert_eq!(ids, vec!["b", "a"], "ordem documental depois do reparent");

        // Um nó REMOVIDO não pode aparecer, mesmo com a entrada de índice viva
        // (o índice é superconjunto por design — ver a auditoria).
        let b = dom.query("#b").unwrap();
        dom.remove_node(b);
        let ids: Vec<String> = dom
            .query_all(".x")
            .into_iter()
            .map(|n| dom.get_attr(n, "id").unwrap_or_default().to_string())
            .collect();
        assert_eq!(ids, vec!["a"], "o removido não entra");

        // E um nó com DUAS classes da lista entra uma vez só.
        let mut dom2 = parse_html_to_dom("<p class='x y' id='u'></p><p class='y' id='v'></p>");
        let ids: Vec<String> = dom2
            .query_all(".x, .y")
            .into_iter()
            .map(|n| dom2.get_attr(n, "id").unwrap_or_default().to_string())
            .collect();
        assert_eq!(ids, vec!["u", "v"], "sem duplicata e em ordem");
        let _ = &mut dom2;
    }


    #[test]
    fn document_element_nao_e_a_raiz_document() {
        let dom = parse_html_to_dom("<html><body><p>x</p></body></html>");
        let html = dom.document_element().expect("html top-level");
        assert_eq!(dom.node_type(dom.root_id()), 9);
        assert_eq!(dom.node_type(html), 1);
        assert_eq!(dom.tag_name(html), Some("html"));
    }

    #[test]
    fn get_element_by_id_nao_interpreta_id_como_css() {
        let dom = parse_html_to_dom("<div id='a.b'>literal</div><p id='a'>outro</p>");
        let literal = dom.get_element_by_id("a.b").expect("id literal");
        assert_eq!(dom.tag_name(literal), Some("div"));
        assert_eq!(dom.text_content(literal).as_deref(), Some("literal"));
    }
