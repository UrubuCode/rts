//! Scroll: clamp ao conteúdo e o rect que `bounding_rect_scrolled` devolve
//! depois de rolar — a régua Rust do lote G (`PLAN.md` §4.G).

    use super::*;

    /// `<div height:100px overflow:auto>` com um filho de `height:300px` é
    /// exactamente o cenário do PLAN: 300 de conteúdo, 100 visível, teto 200.
    fn caixa_com_scroll() -> Dom {
        parse_html_to_dom(
            "<div id=caixa style='height:100px;overflow:auto'>\
             <div id=filho style='height:300px'>conteudo</div>\
             </div>",
        )
    }

    #[test]
    fn set_scroll_clampa_ao_conteudo() {
        let mut dom = caixa_com_scroll();
        let caixa = dom.query("#caixa").unwrap();
        // sem ter rolado ainda, o offset é zero.
        assert_eq!(dom.scroll_of(caixa), (0.0, 0.0));
        dom.set_scroll(caixa, 0.0, 500.0);
        let (x, y) = dom.scroll_of(caixa);
        assert_eq!(x, 0.0);
        assert_eq!(y, 200.0, "300 de conteúdo - 100 visível = 200 de teto");
    }

    /// Um `id` que não é uma região rolável (sem `overflow`) não ganha offset
    /// nenhum — é o "sem conteúdo maior que a caixa, não há o que rolar" que
    /// um browser também responde.
    #[test]
    fn set_scroll_em_no_sem_overflow_e_no_op() {
        let mut dom = parse_html_to_dom("<div id=simples style='height:50px'>x</div>");
        let simples = dom.query("#simples").unwrap();
        dom.set_scroll(simples, 10.0, 10.0);
        assert_eq!(dom.scroll_of(simples), (0.0, 0.0));
    }

    /// `bounding_rect_scrolled` do FILHO reflete o offset da região: rolar
    /// 150 sobe o filho 150 na tela — a mesma pergunta que
    /// `getBoundingClientRect` responde num browser depois de um scroll.
    #[test]
    fn bounding_rect_scrolled_reflete_offset_da_regiao() {
        let mut dom = caixa_com_scroll();
        let caixa = dom.query("#caixa").unwrap();
        let filho = dom.query("#filho").unwrap();
        let antes = dom.bounding_rect_scrolled(filho).expect("filho tem caixa");
        dom.set_scroll(caixa, 0.0, 150.0);
        let depois = dom.bounding_rect_scrolled(filho).expect("filho tem caixa");
        assert!(
            (depois.y - (antes.y - 150.0)).abs() < 0.01,
            "filho devia subir exactamente o offset: antes={antes:?} depois={depois:?}"
        );
        // X não mudou (só rolou Y).
        assert!((depois.x - antes.x).abs() < 0.01);
    }

    /// `scroll_extent`: fora de uma região rolável, `scrollWidth`/`scrollHeight`
    /// respondem a MESMA caixa que `clientWidth`/`clientHeight` — sem conteúdo
    /// a rolar, é isso que um browser também responde. Dentro da região do
    /// cenário padrão, a altura de conteúdo (300) excede a visível (100).
    #[test]
    fn scroll_extent_distingue_regiao_de_elemento_comum() {
        let dom = caixa_com_scroll();
        let caixa = dom.query("#caixa").unwrap();
        let (_scroll_w, scroll_h, _client_w, client_h) = dom.scroll_extent(caixa);
        assert!(scroll_h > client_h, "conteúdo (300) deve exceder o visível (100): {scroll_h} vs {client_h}");
        assert!((client_h - 100.0).abs() < 1.0);

        let simples_dom = parse_html_to_dom("<div id=s style='height:50px'>x</div>");
        let s = simples_dom.query("#s").unwrap();
        let (sw, sh, cw, ch) = simples_dom.scroll_extent(s);
        assert!((sw - cw).abs() < 0.01, "sem overflow, scrollWidth==clientWidth");
        assert!((sh - ch).abs() < 0.01, "sem overflow, scrollHeight==clientHeight");
    }

    /// `set_scroll` que MUDA o offset empurra um evento `"scroll"` cru para a
    /// fila do próprio nó — o mesmo mecanismo que o clique do backend usa
    /// (`push_raw_event`), drenado pela fachada TS via `pumpEventCallbacks`.
    /// Um `set_scroll` que NÃO muda nada (mesmo valor, ou clampado ao mesmo
    /// valor já corrente) não dispara duas vezes.
    #[test]
    fn set_scroll_dispara_scroll_uma_vez_por_mudanca() {
        let mut dom = caixa_com_scroll();
        let caixa = dom.query("#caixa").unwrap();
        dom.set_scroll(caixa, 0.0, 150.0);
        let (alvo, tipo) = dom.poll_raw_event().expect("um evento scroll pendente");
        assert_eq!(alvo, caixa);
        assert_eq!(tipo, "scroll");
        assert!(dom.poll_raw_event().is_none(), "só um evento por mudança");

        // repetir o MESMO valor não dispara de novo.
        dom.set_scroll(caixa, 0.0, 150.0);
        assert!(dom.poll_raw_event().is_none());
    }

    /// `page_scroll`/`set_page_scroll`: mesma forma para a página — clampa ao
    /// `content_height` do documento e dispara `"scroll"` no `<body>` (o alvo
    /// que `window.addEventListener` já usa para eventos de janela).
    #[test]
    fn set_page_scroll_clampa_e_dispara_no_body() {
        let mut dom = parse_html_to_dom(
            "<div style='height:2000px'>alto</div>",
        );
        assert_eq!(dom.page_scroll(), (0.0, 0.0));
        dom.set_page_scroll(0.0, 999_999.0);
        let (_, y) = dom.page_scroll();
        // viewport default do Dom headless é 800 de altura (dom/arvore.rs) —
        // o teto é content_height - 800, bem menor que o pedido.
        assert!(y > 0.0 && y < 999_999.0, "clampado ao conteúdo: {y}");
        let (alvo, tipo) = dom.poll_raw_event().expect("um evento scroll pendente");
        assert_eq!(tipo, "scroll");
        let body = dom.query("body").unwrap();
        assert_eq!(alvo, body, "o scroll de página despacha no <body>");
    }
