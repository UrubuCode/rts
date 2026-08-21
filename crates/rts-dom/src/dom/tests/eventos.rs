//! Listeners, `dispatchEvent`, bubbling e a fila de polling.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn listener_cb_registra_e_coleta_com_bubbling() {
        // addEventListener(type, fn): o Dom guarda o word opaco; dispatch_event_collect
        // devolve os pares (nó, cb) na ordem DOM (alvo → ancestrais).
        let mut dom = parse_html_to_dom("<div id=pai><button id=b>x</button></div>");
        let pai = dom.query("#pai").unwrap();
        let b = dom.query("#b").unwrap();
        dom.add_event_listener_cb(b, "click", 111);
        dom.add_event_listener_cb(pai, "click", 222);
        // duplicata do MESMO cb é no-op (spec DOM).
        dom.add_event_listener_cb(b, "click", 111);
        assert_eq!(dom.dispatch_event_collect(b, "click", true), 2);
        assert_eq!(dom.last_dispatch_at(0).unwrap().1, 111); // alvo primeiro
        assert_eq!(dom.last_dispatch_at(1).unwrap().1, 222); // depois o pai
        // sem bubbling: só o alvo.
        assert_eq!(dom.dispatch_event_collect(b, "click", false), 1);
        // removeEventListener limpa os callbacks do tipo.
        dom.remove_event_listener(b, "click");
        assert_eq!(dom.dispatch_event_collect(b, "click", false), 0);
    }


    #[test]
    fn eventos_add_dispatch_poll() {
        // addEventListener marca o nó; dispatchEvent enfileira; poll consome (#1760).
        let mut dom = parse_html_to_dom("<div id=\"a\"><button id=\"b\">x</button></div>");
        let b = dom.query("#b").unwrap();
        dom.add_event_listener(b, "click");
        assert!(dom.has_listener(b, "click"));
        assert!(!dom.has_listener(b, "mousedown"));
        // dispatch no botão → 1 listener (só o botão escuta).
        assert_eq!(dom.dispatch_event(b, "click", true), 1);
        let (target, t) = dom.poll_event().unwrap();
        assert_eq!(target, b);
        assert_eq!(t, "click");
        assert!(dom.poll_event().is_none()); // fila esvaziou
    }


    #[test]
    fn eventos_bubbling() {
        // dispatch no filho borbulha para o pai que também escuta (#1760).
        let mut dom = parse_html_to_dom("<div id=\"a\"><button id=\"b\">x</button></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        dom.add_event_listener(a, "click"); // o PAI escuta
        dom.add_event_listener(b, "click"); // o filho também
        // dispatch no filho: notifica filho E pai (bubbling) → 2.
        assert_eq!(dom.dispatch_event(b, "click", true), 2);
        // ordem: alvo primeiro, depois o ancestral (target → bubble).
        let (first, _) = dom.poll_event().unwrap();
        let (second, _) = dom.poll_event().unwrap();
        assert_eq!(first, b);
        assert_eq!(second, a);
    }


    #[test]
    fn eventos_arvore_profunda_e_filtro_por_tipo() {
        // VALIDADO no Chrome: árvore de 6 níveis, bubbling seletivo + filtro de tipo.
        let mut dom = parse_html_to_dom(
            "<section id=\"sec\"><article id=\"art\"><div id=\"box\"><p id=\"par\"><a id=\"link\">t</a></p></div></article></section>",
        );
        let sec = dom.query("#sec").unwrap();
        let box_ = dom.query("#box").unwrap();
        let link = dom.query("#link").unwrap();
        dom.add_event_listener(sec, "click");
        dom.add_event_listener(box_, "click");
        dom.add_event_listener(link, "click");
        dom.add_event_listener(box_, "mouseover");
        // click no link borbulha por link→box→sec (pula art/par que não escutam).
        assert_eq!(dom.dispatch_event(link, "click", true), 3);
        let chain: Vec<NodeId> = std::iter::from_fn(|| dom.poll_event().map(|(n, _)| n)).collect();
        assert_eq!(chain, vec![link, box_, sec]); // ordem target→bubble
        // mouseover no link: só o box escuta esse TIPO (apesar do bubbling).
        assert_eq!(dom.dispatch_event(link, "mouseover", true), 1);
        assert_eq!(dom.poll_event().unwrap().0, box_);
        // remove o do box → re-dispatch click enfileira só link+sec.
        dom.remove_event_listener(box_, "click");
        assert_eq!(dom.dispatch_event(link, "click", true), 2);
    }


    #[test]
    fn eventos_no_solto_sem_bubbling() {
        // dispatch num nó SOLTO (sem pai): só ele, sem bubbling.
        let mut dom = parse_html_to_dom("<div></div>");
        let solto = dom.create_element("button");
        dom.add_event_listener(solto, "click");
        assert_eq!(dom.dispatch_event(solto, "click", true), 1); // só ele, não tem pai
    }


    #[test]
    fn eventos_bubbles_false_so_o_alvo() {
        // bubbles=false: só o alvo é notificado, mesmo com o pai escutando (#1760).
        let mut dom = parse_html_to_dom("<div id=\"a\"><button id=\"b\">x</button></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        dom.add_event_listener(a, "focus");
        dom.add_event_listener(b, "focus");
        // focus não borbulha (bubbles=false): só o botão, não o pai.
        assert_eq!(dom.dispatch_event(b, "focus", false), 1);
        assert_eq!(dom.poll_event().unwrap().0, b);
        assert!(dom.poll_event().is_none());
    }


    #[test]
    fn eventos_tipo_case_sensitive() {
        // tipos de evento são CASE-SENSITIVE (spec DOM: click ≠ CLICK).
        let mut dom = parse_html_to_dom("<div id=\"a\">x</div>");
        let a = dom.query("#a").unwrap();
        dom.add_event_listener(a, "click");
        assert!(dom.has_listener(a, "click"));
        assert!(!dom.has_listener(a, "CLICK")); // case diferente não casa
        assert_eq!(dom.dispatch_event(a, "Click", true), 0); // não dispara
        assert_eq!(dom.dispatch_event(a, "click", true), 1); // o exato dispara
    }


    #[test]
    fn eventos_remove_e_sem_listener() {
        let mut dom = parse_html_to_dom("<div id=\"a\">x</div>");
        let a = dom.query("#a").unwrap();
        dom.add_event_listener(a, "click");
        dom.remove_event_listener(a, "click");
        assert!(!dom.has_listener(a, "click"));
        // dispatch sem ninguém escutando → 0 enfileirados.
        assert_eq!(dom.dispatch_event(a, "click", true), 0);
        assert!(dom.poll_event().is_none());
    }
