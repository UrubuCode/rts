//! "close a p element" (WHATWG §13.2.6.4.7 "in body"): a abertura de um
//! bloco fecha um `<p>` aberto em BUTTON SCOPE — não só quando o `<p>` é o
//! topo da pilha de abertos. Módulo À PARTE (em vez de crescer
//! `tests/parser.rs`, que já estava perto do teto de 500 linhas do crate)
//! porque o defeito e a régua que o mediu (a fixture
//! `claude-bloco-quebra-inline-dentro-de-paragrafo`, medida no Chrome) são
//! um assunto coeso próprio.

    use super::*;

    #[test]
    fn div_dentro_de_span_dentro_de_p_fecha_o_p_e_o_span() {
        // O bug que este lote corrige: a checagem antiga só olhava o TOPO da
        // pilha de abertos, então via `span` (não `p`) e nunca disparava — o
        // `<div>` nascia ANINHADO dentro do `<span>` dentro do `<p>`. Árvore
        // medida no Chrome (fixture
        // `claude-bloco-quebra-inline-dentro-de-paragrafo.html`), confirmada
        // por `dom.dump()`:
        //   p#paragrafo > ("irmao antes ", span#quebra > "antes")
        //   div#bloco
        //   "depois"
        //   " irmao depois"
        //   p (vazio — o </p> órfão, ver `fechamento_orfao_de_p_cria_p_vazio`)
        //   div#depois_p
        let dom = parse_html_to_dom(
            "<p id=\"paragrafo\">irmao antes <span id=\"quebra\">antes<div id=\"bloco\"></div>depois</span> irmao depois</p><div id=\"depois_p\"></div>",
        );
        let p = dom.query("#paragrafo").unwrap();
        let span = dom.query("#quebra").unwrap();
        let bloco = dom.query("#bloco").unwrap();

        // <div id="bloco"> não é filho do span — fechou o <p> (e o <span>
        // junto, como colateral) e nasceu IRMÃO do <p>, filho do <body>.
        assert_eq!(dom.parent_of(bloco).map(|x| idx(&dom, x)), Some(body_idx(&dom)));
        assert_eq!(dom.next_sibling(p), Some(bloco));

        // o <p> só guarda o que veio ANTES do <div>: o texto e o <span>
        // (cujo conteúdo também para em "antes" — "depois" nunca entrou nele).
        assert_eq!(dom.text_content(p).unwrap(), "irmao antes antes");
        assert_eq!(dom.text_content(span).unwrap(), "antes");

        // "depois" e " irmao depois" (o `</span>` órfão no meio deles é
        // ignorado, não os funde num nó só) vêm DEPOIS do div#bloco, como
        // irmãos dele — não voltam a entrar no <p> fechado.
        let t1 = dom.next_sibling(bloco).expect("texto depois do bloco");
        assert_eq!(dom.text_content(t1).unwrap(), "depois");
        let t2 = dom.next_sibling(t1).expect("segundo texto");
        assert_eq!(dom.text_content(t2).unwrap(), " irmao depois");
    }

    #[test]
    fn ul_dentro_de_p_fecha_o_p() {
        // `<p>a<ul><li>b</ul>`: a abertura de <ul> fecha o <p> aberto (ul
        // está na tabela de fechamento de p), então <ul> nasce IRMÃO do <p>.
        let dom = parse_html_to_dom("<p>a<ul><li>b</ul>");
        let p = dom.query("p").unwrap();
        let ul = dom.query("ul").unwrap();
        assert_eq!(dom.text_content(p).unwrap(), "a");
        assert_eq!(dom.next_sibling(p), Some(ul));
        assert_eq!(dom.parent_of(ul).map(|x| idx(&dom, x)), Some(body_idx(&dom)));
    }

    #[test]
    fn div_dentro_de_button_dentro_de_p_nao_fecha_o_p() {
        // `<p><button><div>`: `button` é um BLOQUEADOR de button scope — a
        // busca do <p> não atravessa ele, então o <div> fica mesmo aninhado
        // (o <p> continua aberto). Isto é o que distingue "close a p
        // element" de um simples "olhe o que está aberto em cima": não é
        // toda abertura de bloco dentro de qualquer coisa que escapa do <p>.
        let dom = parse_html_to_dom("<p><button><div>x</div></button></p>");
        let p = dom.query("p").unwrap();
        let button = dom.query("button").unwrap();
        let div = dom.query("div").unwrap();
        assert_eq!(dom.parent_of(button), Some(p));
        assert_eq!(dom.parent_of(div), Some(button));
    }

    #[test]
    fn fechamento_orfao_de_p_cria_p_vazio() {
        // `</p>` sem `<p>` em button scope (WHATWG): a spec ainda INSERE um
        // `<p>` vazio e fecha-o na hora — diferente de um `</x>` órfão
        // qualquer, que apenas some. Uma folha de estilo com `p + p {…}`
        // conta com esse `<p>` existir.
        let dom = parse_html_to_dom("<div>antes</div></p><div>depois</div>");
        let top = topo(&dom);
        assert_eq!(top.len(), 3, "div, p (vazio) e div");
        assert_eq!(tag(&dom, top[1]), "p");
        assert!(dom.node(top[1]).children.is_empty());
    }

    #[test]
    fn paragrafo_sem_bloco_nao_muda() {
        // Caso comum, sem nada para fechar: um <p> só com texto e inline
        // continua com tudo dentro dele.
        let dom = parse_html_to_dom("<p>oi <b>gente</b> tchau</p>");
        let p_idx = topo(&dom)[0];
        assert_eq!(tag(&dom, p_idx), "p");
        let p = dom.query("p").unwrap();
        assert_eq!(dom.text_content(p).unwrap(), "oi gente tchau");
        assert_eq!(topo(&dom).len(), 1, "nada nasceu fora do p");
    }
