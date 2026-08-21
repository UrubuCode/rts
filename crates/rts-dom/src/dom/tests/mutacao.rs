//! Mutação da árvore: criar, inserir, mover, clonar, remover, atributos, `innerHTML`.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn create_text_e_insert_before() {
        let mut dom = parse_html_to_dom("<ul><li>b</li></ul>");
        let ul = dom.query("ul").unwrap();
        let li_b = dom.query("li").unwrap();
        // createElement + insertBefore(novo, li_b) → novo vira PRIMEIRO filho.
        let li_a = dom.create_element("li");
        dom.insert_before(ul, li_a, Some(li_b));
        assert_eq!(dom.first_child(ul), Some(li_a));
        assert_eq!(dom.next_sibling(li_a), Some(li_b));
        // createTextNode + appendChild dentro do li_a.
        let txt = dom.create_text_node("a");
        dom.append_child(li_a, txt);
        assert_eq!(dom.node_type(txt), 3); // Text
        assert_eq!(dom.first_child(li_a), Some(txt));
        // insert_before com reference None → anexa ao fim.
        let li_c = dom.create_element("li");
        dom.insert_before(ul, li_c, None);
        assert_eq!(dom.last_child(ul), Some(li_c));
    }


    #[test]
    fn inner_html_get_serializa() {
        // innerHTML (get): reconstrói o HTML dos filhos.
        let dom = parse_html_to_dom("<div><p class='x'>oi <b>forte</b></p></div>");
        let div = dom.query("div").unwrap();
        assert_eq!(
            dom.inner_html(div).unwrap(),
            "<p class=\"x\">oi <b>forte</b></p>"
        );
        // outerHTML inclui o próprio div.
        assert_eq!(
            dom.outer_html(div).unwrap(),
            "<div><p class=\"x\">oi <b>forte</b></p></div>"
        );
        // entidades re-encodadas no texto.
        let d2 = parse_html_to_dom("<p>a &lt; b &amp; c</p>");
        let p = d2.query("p").unwrap();
        assert_eq!(d2.inner_html(p).unwrap(), "a &lt; b &amp; c");
    }


    #[test]
    fn inner_html_set_substitui() {
        // innerHTML (set): parseia e troca os filhos.
        let mut dom = parse_html_to_dom("<div><span>velho</span></div>");
        let div = dom.query("div").unwrap();
        dom.set_inner_html(div, "<p>novo</p><b>!</b>");
        // os filhos novos estão lá; o velho sumiu.
        assert_eq!(dom.inner_html(div).unwrap(), "<p>novo</p><b>!</b>");
        // a árvore real reflete (query acha o <p> novo).
        let p = dom.query("p").unwrap();
        assert_eq!(dom.text_content(p).unwrap(), "novo");
        assert!(dom.query("span").is_none()); // o velho foi descartado
    }


    #[test]
    fn inner_html_set_desanexa_dos_indices() {
        // Regressão: o atalho O(1) por índice (`.classe`/`#id`) não pode achar nó
        // descartado pelo set_inner_html/set_text (o `parent` dos filhos velhos é
        // zerado para o `is_attached` do índice falhar).
        let mut dom = parse_html_to_dom("<div id=a><span class=x id=velho>old</span></div>");
        let div = dom.query("#a").unwrap();
        dom.set_inner_html(div, "<b>new</b>");
        assert!(dom.query(".x").is_none());
        assert!(dom.query("#velho").is_none());
        // set_text idem.
        let mut d2 = parse_html_to_dom("<div id=a><span class=x>old</span></div>");
        let div2 = d2.query("#a").unwrap();
        d2.set_text(div2, "txt");
        assert!(d2.query(".x").is_none());
    }


    #[test]
    fn inner_html_round_trip() {
        // parse → serialize → parse estável (subset).
        let html = "<section id=\"s\"><h2>T</h2><p>texto <i>it</i> fim</p></section>";
        let dom = parse_html_to_dom(html);
        let sec = dom.query("#s").unwrap();
        let serial = dom.outer_html(sec).unwrap();
        assert_eq!(serial, html);
    }


    #[test]
    fn node_utils_contains_e_nodevalue() {
        let dom = parse_html_to_dom("<div id=\"a\"><p>oi</p></div>");
        let div = dom.query("#a").unwrap();
        let p = dom.query("p").unwrap();
        assert!(dom.contains(div, p)); // div contém p
        assert!(dom.contains(div, div)); // contém a si mesmo
        assert!(!dom.contains(p, div)); // p NÃO contém o div
        assert!(dom.has_child_nodes(div));
        // nodeValue: só Text/Comment; Element = None.
        let txt = dom.first_child(p).unwrap();
        assert_eq!(dom.node_value(txt).as_deref(), Some("oi"));
        assert_eq!(dom.node_value(div), None);
    }


    #[test]
    fn normalize_funde_textos_adjacentes() {
        let mut dom = parse_html_to_dom("<div id=\"a\"></div>");
        let div = dom.query("#a").unwrap();
        for s in ["a", "b", "", "c"] {
            let t = dom.create_text_node(s);
            dom.append_child(div, t);
        }
        assert_eq!(dom.child_nodes(div).len(), 4);
        dom.normalize(div);
        let kids = dom.child_nodes(div);
        assert_eq!(kids.len(), 1, "4 textos (1 vazio) → 1 fundido");
        assert_eq!(dom.node_value(kids[0]).as_deref(), Some("abc"));
    }


    #[test]
    fn atributos_remove_has_e_names() {
        let mut dom = parse_html_to_dom("<div id=\"a\" class=\"c\" hidden>x</div>");
        let div = dom.query("#a").unwrap();
        // hidden é booleano (valor "") mas PRESENTE — has_attr o detecta.
        assert!(dom.has_attr(div, "hidden"));
        assert!(!dom.has_attr(div, "title"));
        assert_eq!(dom.attr_names(div), vec!["id", "class", "hidden"]);
        dom.remove_attr(div, "hidden");
        assert!(!dom.has_attr(div, "hidden"));
        assert_eq!(dom.attr_names(div), vec!["id", "class"]);
    }


    #[test]
    fn clone_node_deep_e_shallow() {
        let mut dom = parse_html_to_dom("<div id=\"a\"><p>oi</p><span>tchau</span></div>");
        let a = dom.query("#a").unwrap();
        // shallow: clone sem filhos.
        let shallow = dom.clone_node(a, false).unwrap();
        assert_eq!(dom.child_nodes(shallow).len(), 0);
        assert_eq!(dom.node_name(shallow).as_deref(), Some("div"));
        // deep: com a subárvore.
        let deep = dom.clone_node(a, true).unwrap();
        assert_eq!(dom.child_elements(deep).len(), 2); // p + span
        assert_eq!(dom.text_content(deep).as_deref(), Some("oitchau"));
        // o clone é SOLTO (sem pai).
        assert_eq!(dom.parent_of(deep), None);
    }


    #[test]
    fn mutacao_rica() {
        let mut dom = parse_html_to_dom("<div id=\"a\"><p id=\"p\">x</p></div>");
        let a = dom.query("#a").unwrap();
        let p = dom.query("#p").unwrap();
        // prepend: novo elemento no início.
        let h = dom.create_element("h1");
        dom.prepend_child(a, h);
        assert_eq!(dom.first_element_child(a), Some(h)); // h1 antes do p
        // before/after: irmão de p.
        let b = dom.create_element("b");
        dom.insert_adjacent(p, b, false); // b antes de p
        let i = dom.create_element("i");
        dom.insert_adjacent(p, i, true); // i depois de p
        // ordem: h1, b, p, i.
        let kids = dom.child_elements(a);
        let names: Vec<String> = kids.iter().map(|&k| dom.node_name(k).unwrap()).collect();
        assert_eq!(names, vec!["h1", "b", "p", "i"]);
        // replaceWith: troca p por um span.
        let s = dom.create_element("span");
        dom.replace_with(p, s);
        assert!(dom.query("#p").is_none()); // p saiu
        // clearChildren: esvazia.
        dom.clear_children(a);
        assert_eq!(dom.child_nodes(a).len(), 0);
    }


    #[test]
    fn clone_indexado_e_achavel() {
        // BUG: o clone não entrava nos índices id/class → querySelector não achava.
        let mut dom = parse_html_to_dom("<div id=\"src\" class=\"card\">x</div>");
        let src = dom.query("#src").unwrap();
        let clone = dom.clone_node(src, true).unwrap();
        // muda o id do clone e anexa à raiz.
        dom.set_attr(clone, "id", "copy");
        // anexa à própria raiz #document.
        dom.append_child(dom.root_id(), clone);
        // agora querySelector acha o clone pela classe (índice) e pelo novo id.
        assert!(dom.query(".card").is_some());
        assert_eq!(dom.get_elements_by_class_name("card").len(), 2); // original + clone
    }


    #[test]
    fn replace_with_atomico_nao_destroi_em_ciclo() {
        // BUG CRITICAL: replaceWith(node, ancestral) destruía node sem inserir.
        let mut dom =
            parse_html_to_dom("<div id=\"out\"><div id=\"in\"><p id=\"p\">x</p></div></div>");
        let out = dom.query("#out").unwrap();
        let p = dom.query("#p").unwrap();
        // tentar substituir p por 'out' (ancestral de p) — guarda de ciclo aborta o
        // insert; p NÃO deve ser destruído.
        dom.replace_with(p, out);
        assert!(
            dom.query("#p").is_some(),
            "p preservado quando a inserção aborta"
        );
        // replaceWith por si mesmo é no-op (não remove).
        dom.replace_with(p, p);
        assert!(dom.query("#p").is_some());
        // caso normal: substitui p por um span novo.
        let s = dom.create_element("span");
        dom.set_attr(s, "id", "s");
        dom.replace_with(p, s);
        assert!(dom.query("#p").is_none());
        assert!(dom.query("#s").is_some());
    }


    #[test]
    fn after_com_proximo_irmao_mantem_ordem() {
        // BUG: after(other) com other já sendo o próximo irmão jogava other pro fim.
        let mut dom = parse_html_to_dom(
            "<div id=\"a\"><b id=\"b\">1</b><i id=\"i\">2</i><u id=\"u\">3</u></div>",
        );
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        let i = dom.query("#i").unwrap();
        // b.after(i) — i JÁ é o próximo irmão de b; deve manter a ordem b,i,u.
        dom.insert_adjacent(b, i, true);
        let names: Vec<String> = dom
            .child_elements(a)
            .iter()
            .map(|&k| dom.node_name(k).unwrap())
            .collect();
        assert_eq!(names, vec!["b", "i", "u"]); // ordem preservada, i não foi pro fim
    }


    #[test]
    fn create_comment_node() {
        let mut dom = parse_html_to_dom("<div></div>");
        let c = dom.create_comment("nota");
        assert_eq!(dom.node_type(c), 8);
        assert_eq!(dom.node_value(c).as_deref(), Some("nota"));
    }


    #[test]
    fn node_type_e_name() {
        let mut dom = parse_html_to_dom("<p>oi</p>");
        let p = dom.query("p").unwrap();
        let txt = dom.first_child(p).unwrap();
        assert_eq!(dom.node_type(p), 1); // Element
        assert_eq!(dom.node_name(p).as_deref(), Some("p"));
        assert_eq!(dom.node_type(txt), 3); // Text
        assert_eq!(dom.node_name(txt).as_deref(), Some("#text"));
        let c = dom.create_text_node("x");
        assert_eq!(dom.node_type(c), 3);
    }


    #[test]
    fn set_text_substitui_conteudo() {
        let mut dom = parse_html_to_dom("<p>antes <b>x</b></p>");
        let p = dom.query("p").unwrap();
        dom.set_text(p, "depois");
        let p = idx(&dom, p);
        assert_eq!(dom.node(p).children.len(), 1);
        assert_eq!(
            dom.node(dom.node(p).children[0]).kind,
            NodeKind::Text("depois".into())
        );
    }


    #[test]
    fn set_attr_cria_e_atualiza() {
        let mut dom = parse_html_to_dom("<div>x</div>");
        let div = dom.query("div").unwrap();
        dom.set_attr(div, "class", "card");
        let d = idx(&dom, div);
        assert_eq!(dom.node(d).attr("class"), Some("card"));
        dom.set_attr(div, "class", "card ativo"); // atualiza, não duplica
        assert_eq!(dom.node(d).attr("class"), Some("card ativo"));
        assert_eq!(dom.node(d).attrs.len(), 1);
    }


    #[test]
    fn create_e_append_child() {
        let mut dom = parse_html_to_dom("<ul></ul>");
        let ul = dom.query("ul").unwrap();
        let li = dom.create_element("li");
        dom.set_text(li, "novo item");
        dom.append_child(ul, li);
        let (ul, li) = (idx(&dom, ul), idx(&dom, li));
        assert_eq!(dom.node(ul).children, vec![li]);
        assert_eq!(dom.node(li).parent, Some(ul));
        assert_eq!(tag(&dom, li), "li");
    }


    #[test]
    fn append_move_de_pai_e_remove() {
        let mut dom = parse_html_to_dom("<div><span>x</span></div><section></section>");
        let div = dom.query("div").unwrap();
        let span = dom.query("span").unwrap();
        let section = dom.query("section").unwrap();
        // move o span do div para o section
        dom.append_child(section, span);
        let (di, si, se) = (idx(&dom, div), idx(&dom, span), idx(&dom, section));
        assert!(dom.node(di).children.is_empty());
        assert_eq!(dom.node(se).children, vec![si]);
        assert_eq!(dom.node(si).parent, Some(se));
        // remove o span de vez
        dom.remove_node(span);
        assert!(dom.node(se).children.is_empty());
        assert_eq!(dom.node(si).parent, None);
    }


    #[test]
    fn append_nao_cria_ciclo() {
        let mut dom = parse_html_to_dom("<div><span>x</span></div>");
        let div = dom.query("div").unwrap();
        let span = dom.query("span").unwrap();
        // tentar pôr o div (ancestral) dentro do span deve ser ignorado.
        dom.append_child(span, div);
        let (di, si) = (idx(&dom, div), idx(&dom, span));
        assert_eq!(dom.node(di).parent, Some(body_idx(&dom))); // intacto sob o <body>
        assert!(dom.node(si).children.contains(&di) == false);
    }


    #[test]
    fn nodeid_versionado_stale_apos_reparse() {
        // INVARIANTE 2: um NodeId de uma árvore anterior NÃO resolve na nova.
        let dom1 = parse_html_to_dom("<div id='x'>a</div>");
        let id_velho = dom1.query("#x").unwrap();
        let dom2 = parse_html_to_dom("<div id='x'>b</div>");
        // mesmo seletor, árvore nova → gen diferente.
        let id_novo = dom2.query("#x").unwrap();
        assert_ne!(id_velho.generation, id_novo.generation);
        // o id velho é stale na árvore nova: resolve → None (não aplica a nó errado).
        assert_eq!(dom2.resolve(id_velho), None);
        assert!(dom2.resolve(id_novo).is_some());
    }


    #[test]
    fn nodeid_abi_roundtrip() {
        let id = NodeId {
            generation: 7,
            idx: 42,
        };
        let v = id.to_abi();
        assert!(v >= 0);
        assert_eq!(NodeId::from_abi(v), Some(id));
        // sentinela -1 e negativos → None.
        assert_eq!(NodeId::from_abi(-1), None);
        assert_eq!(NodeId::from_abi(-999), None);
    }
