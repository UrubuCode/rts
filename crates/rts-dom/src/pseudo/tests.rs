//! Testes ponta-a-ponta do conteúdo gerado — extraído de `pseudo.rs` (ver o
//! comentário em `mod.rs`) sem alterar uma linha do que já existia, mais os
//! testes novos da família de aspas ao fundo.
//!
//! `pub(crate)` no módulo e no `textos`: os testes ponta a ponta dos
//! contadores vivem em `counters.rs`, ao lado da lógica que provam, e
//! precisam do MESMO helper — reusá-lo é o que impede duas montagens
//! diferentes de `layout_document` a responder à mesma pergunta.

use super::*;
    use crate::dom::parse_html_to_dom;
    use crate::layout::{ApproxMeasurer, DisplayItem, LayoutCtx, layout_document};

    /// Os textos pintados, em ordem de pintura — é o que prova que a caixa
    /// gerada existe e onde ficou.
    pub(crate) fn textos(html: &str) -> Vec<String> {
        let dom = parse_html_to_dom(html);
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let lista = layout_document(&dom, &ctx);
        let mut out = Vec::new();
        lista.walk(|item, _, _| {
            if let DisplayItem::Text { text, .. } = item {
                out.push(text.to_string());
            }
        });
        out
    }

    /// A cor com que cada texto foi pintado.
    fn textos_e_cores(html: &str) -> Vec<(String, u32)> {
        let dom = parse_html_to_dom(html);
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let lista = layout_document(&dom, &ctx);
        let mut out = Vec::new();
        lista.walk(|item, _, _| {
            if let DisplayItem::Text { text, color, .. } = item {
                out.push((text.to_string(), *color));
            }
        });
        out
    }

    #[test]
    fn before_com_content_acrescenta_uma_caixa_antes_do_conteudo() {
        let t = textos("<style>p::before { content:\"→\" }</style><p>oi</p>");
        assert_eq!(t, vec!["→".to_string(), "oi".to_string()]);
    }

    #[test]
    fn after_vem_depois_do_conteudo() {
        let t = textos("<style>p::after { content:\"!\" }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string(), "!".to_string()]);
        // e os dois juntos envolvem o conteúdo.
        let t2 = textos("<style>p::before{content:\"[\"} p::after{content:\"]\"}</style><p>oi</p>");
        assert_eq!(t2, vec!["[".to_string(), "oi".to_string(), "]".to_string()]);
    }

    #[test]
    fn before_nao_muda_a_arvore_de_nos() {
        // O TESTE que prova que não sujámos a árvore: a caixa gerada pinta, mas
        // o programa não a vê em consulta nenhuma — como no browser.
        let dom = parse_html_to_dom("<style>p::before { content:\"→\" }</style><p>oi</p>");
        let p = dom.query("p").unwrap();
        assert_eq!(dom.child_nodes(p).len(), 1); // só o nó de texto "oi"
        assert_eq!(dom.child_elements(p).len(), 0);
        assert_eq!(dom.inner_html(p).unwrap(), "oi");
        // e nenhuma consulta a alcança.
        assert_eq!(dom.query_all("*").len(), dom.query_all("*").len().min(64));
        assert!(dom.query("::before").is_none());
        // mas ela PINTA — senão este teste estaria a provar o nada.
        let t = textos("<style>p::before { content:\"→\" }</style><p>oi</p>");
        assert!(t.contains(&"→".to_string()));
    }

    #[test]
    fn sem_content_nao_ha_caixa() {
        // Uma regra `::before` que só declara cor não gera nada (spec): sem
        // `content` não há caixa.
        let t = textos("<style>p::before { color:#ff0000 }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string()]);
        // `content:none` também não.
        let t2 = textos("<style>p::before { content:none }</style><p>oi</p>");
        assert_eq!(t2, vec!["oi".to_string()]);
    }

    #[test]
    fn display_none_no_pseudo_nao_gera_caixa() {
        let t = textos("<style>p::before { content:\"→\"; display:none }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string()]);
    }

    #[test]
    fn regra_de_pseudo_elemento_nao_estiliza_o_elemento() {
        // O erro que fazia o `::before` ser recusado no parse durante tanto
        // tempo: as declarações são da caixa gerada, não do `<p>`.
        let v =
            textos_e_cores("<style>p::before { content:\"→\"; color:#ff0000 }</style><p>oi</p>");
        let oi = v.iter().find(|(t, _)| t == "oi").unwrap();
        assert_ne!(
            oi.1, 0xFF0000FF,
            "o vermelho era da caixa gerada, não do <p>"
        );
        let seta = v.iter().find(|(t, _)| t == "→").unwrap();
        assert_eq!(seta.1, 0xFF0000FF);
    }

    #[test]
    fn pseudo_herda_a_cor_do_elemento_e_a_propria_vence() {
        // Sem `color` próprio, sai da cor do texto à volta.
        let v = textos_e_cores("<style>p{color:#00ff00} p::before{content:\"→\"}</style><p>oi</p>");
        assert_eq!(v.iter().find(|(t, _)| t == "→").unwrap().1, 0x00FF00FF);
    }

    #[test]
    fn content_vazio_nao_pinta_texto_nenhum() {
        // `content:""` é o caso MAIORITÁRIO da folha real (16 das 80 regras com
        // pseudo-elemento): a caixa existe para levar um `background` ou uma
        // `border`, não texto. Aqui ela não pinta glifo nenhum — o que se fixa é
        // que também não inventa um run vazio a deslocar o que vem a seguir.
        let t = textos("<style>p::before { content:\"\" }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string()]);
    }

    #[test]
    fn display_block_no_pseudo_gera_uma_caixa_de_bloco_propria() {
        // Este teste fixava o OPOSTO até o lote `pintura-e-caixas`
        // (2026-09-04, causa 8 da triagem WPT `flexbox_nested-flex`): dizia
        // que `display:block` num pseudo era sempre tratado como inline, por
        // não haver "ponto de enxerto" no fluxo de bloco para uma caixa
        // gerada — `layout::pseudo_bloco` é esse enxerto agora, um gancho em
        // `layout_children_vertical`.
        //
        // A PROVA por texto sozinha (`textos`) não distingue os dois casos: um
        // `<p>` de uma única linha dá `["→", "oi"]` tratado como inline OU
        // como bloco, porque a ORDEM calha a ser a mesma. O que distingue é a
        // ALTURA — só uma caixa de bloco RESPEITA o `height` declarado do
        // pseudo; tratado como inline essa `height` é descartada (o corte
        // ainda vale para `inline-block`/`position:absolute`, ver
        // `runs.rs::pseudo_run`) e a única altura que sobra é a da LINHA.
        let t = textos(
            "<style>p{margin:0} p::before{content:\"→\";display:block;height:30px}</style><p>oi</p>",
        );
        assert_eq!(t, vec!["→".to_string(), "oi".to_string()], "o texto pinta-se na mesma ordem de sempre");
        let dom = crate::dom::parse_html_to_dom(
            "<style>p{margin:0} p::before{content:\"→\";display:block;height:30px}</style><p>oi</p>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let lista = layout_document(&dom, &ctx);
        let p_id = dom
            .resolve(dom.query_all("p")[0])
            .expect("<p> vivo");
        let h = lista
            .geometry_now()
            .rects
            .get(&p_id)
            .expect("<p> tem geometria")
            .h;
        // 30 (a altura do ::before, PRÓPRIA) + a linha inteira de "oi" — bem
        // acima do que uma ÚNICA linha (~20px) daria se os 30px do `height`
        // tivessem sido descartados, que é o que "tratado como inline" quer
        // dizer na prática.
        assert!(h > 40.0, "::before de bloco devia somar os seus 30px à altura do <p>: h={h}");
    }

    #[test]
    fn content_com_attr_le_o_atributo_do_originante() {
        let t = textos(
            "<style>p::after { content:\" (\" attr(data-nota) \")\" }</style>\
             <p data-nota='n1'>oi</p>",
        );
        // O espaço inicial do `content` SOBREVIVE. Este teste esperava que ele
        // sumisse e dizia porquê: o fluxo inline apagava o espaço em toda
        // fronteira de run, e a expectativa registava o defeito à espera de quem
        // o corrigisse. Corrigido no `collapse_ws`/`wrap_runs`, a expectativa
        // passa a ser a do browser.
        assert_eq!(t, vec!["oi".to_string(), " (n1)".to_string()]);
    }

    #[test]
    fn pseudo_de_elemento_inline_entra_no_meio_da_linha() {
        // O `::after` de um `<a>` dentro de um parágrafo fica ENTRE o texto do
        // link e o que vem a seguir — é o caso do ícone de link externo.
        let t = textos("<style>a::after { content:\"↗\" }</style><p>ver <a>aqui</a> agora</p>");
        // O ícone entra COLADO ao fim do texto do link — a linha pinta-o no
        // mesmo segmento, que é a prova de que ficou dentro da linha e não numa
        // caixa à parte.
        assert!(t.contains(&"aqui↗".to_string()), "ordem: {t:?}");
        let pos_icone = t.iter().position(|s| s.contains('↗')).unwrap();
        let pos_fim = t.iter().position(|s| s.contains("agora")).unwrap();
        assert!(pos_icone < pos_fim, "ordem: {t:?}");
    }

    #[test]
    fn cascata_do_content_segue_a_especificidade() {
        // A regra mais específica dita o `content`, e uma regra que não o
        // declara não o apaga — é o caso maioritário numa folha real.
        let t = textos(
            "<style>p::before{content:\"a\"} .x::before{content:\"b\"} \
             p.x::before{color:#ff0000}</style><p class='x'>oi</p>",
        );
        assert_eq!(t[0], "b");
    }

    #[test]
    fn pseudo_elemento_que_nao_geramos_descarta_a_regra() {
        // `::marker` não é conteúdo gerado por `content`; aplicá-lo ao elemento
        // pintaria o `<p>` inteiro.
        let v = textos_e_cores("<style>p::marker { color:#ff0000 }</style><p>oi</p>");
        assert_ne!(v[0].1, 0xFF0000FF);
    }

    #[test]
    fn content_string_com_escape_hexadecimal_vira_o_caractere() {
        // `"\2192"` é como uma folha real escreve a seta — se não se resolvesse
        // o escape, a página mostrava os dígitos.
        let Some(Content::Pecas(p)) = parse_content(r#""\2192""#) else {
            panic!()
        };
        assert_eq!(p, vec![Peca::Texto("→".to_string())]);
        // e o espaço que termina o código hexadecimal é consumido, não pintado.
        let Some(Content::Pecas(p)) = parse_content(r#""\2192 x""#) else {
            panic!()
        };
        assert_eq!(p, vec![Peca::Texto("→x".to_string())]);
    }

    #[test]
    fn content_none_e_diferente_de_content_desconhecido() {
        // `none` é uma resposta da folha e vence na cascata; `url()` é uma
        // declaração que não sabemos cumprir e é descartada.
        assert_eq!(parse_content("none"), Some(Content::Nenhum));
        assert_eq!(parse_content("normal"), Some(Content::Nenhum));
        assert_eq!(parse_content("url(seta.png)"), None);
        // `counter()` (singular) DEIXOU de estar nesta lista — é o que este
        // trabalho acrescentou. O plural continua nela, e a linha abaixo é o que
        // impede que ele passe a ser aceite por acidente de prefixo: `counters(`
        // começa por `counter` e um `starts_with` desatento aceitá-lo-ia,
        // pintando "3" onde a folha pediu "1.2.3".
        assert_eq!(parse_content("counters(item, '.')"), None);
        // E o estilo dentro de `var()`: a declaração inteira cai, para a cascata
        // poder escolher outra regra em vez de nós inventarmos `decimal`.
        assert_eq!(parse_content("counter(x, var(--y))"), None);
        assert_eq!(parse_content("counter(x, esquisito)"), None);
    }

    #[test]
    fn content_concatena_string_e_attr() {
        let c = parse_content(r#""[" attr(data-x) "]""#).unwrap();
        let attr = |n: &str| (n == "data-x").then(|| "oi".to_string());
        let mut prof = 0i64;
        assert_eq!(texto_de(&c, &attr, None, &[], &mut prof).unwrap(), "[oi]");
        // atributo ausente é string vazia, e os literais ficam.
        let vazio = |_: &str| None;
        assert_eq!(texto_de(&c, &vazio, None, &[], &mut prof).unwrap(), "[]");
    }

    // ── ASPAS (2026-09-16): `quotes`, `open-quote`/`close-quote` ──────────
    //
    // A causa da maior família de falhas WPT `CSS2/generated-content`
    // (`quotes-035`/`quotes-036`/`quotes-applies-to-*`): `open-quote` e
    // `close-quote` eram recusados no `parse_content` e a declaração inteira
    // caía. Os testes de nível (a escolha de par, a saturação, o clamp em
    // zero) vivem em `crate::quotes`; os daqui são PONTA A PONTA, contra o
    // documento inteiro, para provar que a herança e a ordem documental
    // chegam à caixa pintada.

    #[test]
    fn open_e_close_quote_envolvem_o_conteudo_com_o_par_declarado() {
        // quotes-001 do WPT: um só par, um `::before`/`::after` cada.
        let t = textos(
            "<style>div{quotes:\"A\" \"Z\"} div::before{content:open-quote} \
             div::after{content:close-quote}</style><div>x</div>",
        );
        assert_eq!(t, vec!["A".to_string(), "x".to_string(), "Z".to_string()]);
    }

    #[test]
    fn quotes_e_herdado_do_ancestral_nao_do_pseudo() {
        // `quotes` declarado no <div> exterior, lido pelo `::before` de um
        // <span> dois níveis abaixo — se a herança não subisse a árvore, o
        // par tipográfico por omissão sairia em vez de "A".
        let t = textos(
            "<style>#pai{quotes:\"A\" \"Z\"} b::before{content:open-quote}</style>\
             <div id=\"pai\"><span><b>x</b></span></div>",
        );
        assert!(t.contains(&"A".to_string()), "{t:?}");
    }

    #[test]
    fn profundidade_de_aspas_e_global_e_aninha_entre_elementos() {
        // O caso de `quotes-applies-to-001`: dois `open-quote` seguidos (em
        // elementos DIFERENTES, mas o mesmo `quotes` de dois pares) escolhem
        // o par de fora e o de dentro — não o mesmo par repetido.
        let t = textos(
            "<style>#r{quotes:\"P\" \"S\" \"A\" \"S\"} \
             .o::before{content:open-quote} .c::after{content:close-quote}\
             </style>\
             <div id=\"r\"><span class=\"o c\"><span class=\"o c\">x</span></span></div>",
        );
        // "P" (nível 0, exterior) + "A" (nível 1, interior) + "x" + "S" (fecha
        // nível 1) + "S" (fecha nível 0) = PASS ao redor do "x".
        let junto: String = t.concat();
        assert_eq!(junto, "PAxSS", "{t:?}");
    }

    #[test]
    fn no_open_e_no_close_quote_nao_pintam_texto_mas_contam_o_nivel() {
        let t = textos(
            "<style>div{quotes:\"A\" \"Z\"} \
             div::before{content:no-open-quote open-quote} \
             div::after{content:close-quote}</style><div>x</div>",
        );
        // O `no-open-quote` sobe o nível para 1 SEM pintar; o `open-quote`
        // que o segue já lê o nível 1 — só há um par declarado, que satura
        // nele, então o texto ainda é "A", mas o `close-quote` do `::after`
        // tem de descer de volta ao nível 1 e não ao 0 (clamp visível só se
        // uma segunda aspa tentasse fechar a mais).
        assert_eq!(t, vec!["A".to_string(), "x".to_string(), "Z".to_string()]);
    }

    #[test]
    fn quotes_none_apaga_o_texto_das_aspas_sem_apagar_a_caixa() {
        let t = textos(
            "<style>div{quotes:none} div::before{content:open-quote \"x\"}\
             </style><div>y</div>",
        );
        // A aspa em si não pinta nada, mas o literal ao lado sobrevive — é a
        // mesma prova que `um_contador_que_ninguem_criou_vale_zero...` faz
        // para `counter()`.
        assert_eq!(t, vec!["x".to_string(), "y".to_string()]);
    }
