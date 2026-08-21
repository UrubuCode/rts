//! Transições e `@keyframes` — a interpolação no tempo.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn transition_interpola_no_tempo() {
        // transition: o DOM é dono do loop — advance(now) interpola a mudança (#1776).
        let mut dom = parse_html_to_dom(
            "<div id=\"box\" style=\"background:#000000;transition:0.5s linear\">x</div>",
        );
        let box_id = dom.query("#box").unwrap();
        let bi = dom.resolve(box_id).unwrap();
        // frame 0: estabelece o baseline (background preto). Sem animação ainda.
        assert!(!dom.advance(0.0));
        // o JS muda o background para branco (via setStyleProp → atributo style).
        dom.set_style_property(box_id, "background", "white");
        // frame em t=0: detecta a mudança, inicia a transição. Ainda preto (t=0).
        assert!(dom.advance(0.0)); // há animação ativa
        let at0 = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(at0, 0x000000FF, "início = preto");
        // metade do tempo (250ms de 500): cinza (#808080).
        dom.advance(250.0);
        let mid = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(mid, 0x808080FF, "meio = cinza, got 0x{mid:08X}");
        // fim (500ms): branco, e a animação encerra.
        let still = dom.advance(500.0);
        let end = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(end, 0xFFFFFFFF, "fim = branco");
        assert!(!still, "animação encerrou");
    }


    #[test]
    fn keyframes_anima_no_tempo() {
        // @keyframes roda SOZINHA no tempo (sem gatilho) — fase 2 do #1776.
        let mut dom = parse_html_to_dom(
            "<style>@keyframes pulse{0%{background:#000000}50%{background:#ff0000}100%{background:#000000}}\
             #box{animation:pulse 1s linear}</style><div id=\"box\">x</div>",
        );
        let bi = dom.resolve(dom.query("#box").unwrap()).unwrap();
        // t=0: começa no 0% (preto). advance estabelece o start.
        dom.advance(0.0);
        assert_eq!(
            dom.computed_style_idx(bi).unwrap().bg,
            Some(0x000000FF),
            "0%"
        );
        // t=250ms (25% da animação): entre 0% e 50% → metade do caminho preto→vermelho.
        dom.advance(250.0);
        let q = dom.computed_style_idx(bi).unwrap().bg.unwrap();
        assert_eq!(q, 0x800000FF, "25% = meio de preto→vermelho, got 0x{q:08X}");
        // t=500ms (50%): vermelho puro.
        dom.advance(500.0);
        assert_eq!(
            dom.computed_style_idx(bi).unwrap().bg,
            Some(0xFF0000FF),
            "50%"
        );
        // t=750ms (75%): meio de vermelho→preto de volta.
        dom.advance(750.0);
        assert_eq!(
            dom.computed_style_idx(bi).unwrap().bg,
            Some(0x800000FF),
            "75%"
        );
        // a animação fica ativa (retorna true durante o curso).
        assert!(dom.advance(400.0));
    }


    #[test]
    fn keyframes_from_to_e_iteracoes() {
        // sintaxe from/to + iterações finitas (termina no estado final).
        let mut dom = parse_html_to_dom(
            "<style>@keyframes grow{from{width:100px}to{width:300px}}#b{animation:grow 1s linear 1}</style><div id=\"b\">x</div>",
        );
        let bi = dom.resolve(dom.query("#b").unwrap()).unwrap();
        dom.advance(0.0);
        assert_eq!(
            dom.computed_style_idx(bi).unwrap().width,
            Some(crate::style::Dimension::Px(100.0)),
            "from"
        );
        dom.advance(500.0);
        assert_eq!(
            dom.computed_style_idx(bi).unwrap().width,
            Some(crate::style::Dimension::Px(200.0)),
            "meio"
        );
        // depois de 1 iteração (1s), a animação encerra (não retorna ativa).
        let active = dom.advance(1100.0);
        assert!(!active, "1 iteração terminou");
    }
