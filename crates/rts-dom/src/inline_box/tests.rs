//! Testes movidos de `inline_box.rs` na modularização; nenhuma linha foi
//! alterada. A indentação de 4 espaços é a do `mod` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use crate::table::tests::{geometria, rect, textos};

    /// A palavra que não cabe TRANSBORDA — `overflow-wrap: normal` é o inicial, e
    /// esta é a metade da prova que fixa o antes.
    #[test]
    fn palavra_longa_sem_overflow_wrap_transborda_o_container() {
        let (dom, list) = geometria(
            "<div style='width:40px'><span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(
            s.w > 40.0,
            "sem overflow-wrap a palavra tem de sair da caixa: {s:?}"
        );
    }

    /// E a MESMA com `overflow-wrap: break-word` fica dentro, em várias linhas.
    #[test]
    fn palavra_longa_com_break_word_parte_e_cabe_no_container() {
        let (dom, list) = geometria(
            "<div style='width:40px;overflow-wrap:break-word'>\
             <span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w <= 41.0, "a palavra partida cabe na caixa: {s:?}");
        assert!(s.h > 20.0, "e ocupa mais do que uma linha: {s:?}");
    }

    /// O nome LEGADO faz o mesmo: `word-wrap` é alias de `overflow-wrap` (MDN), e
    /// é a grafia que 8 das 13 folhas do corpus escrevem.
    #[test]
    fn word_wrap_legado_quebra_como_overflow_wrap() {
        let (dom, list) = geometria(
            "<div style='width:40px;word-wrap:break-word'>\
             <span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w <= 41.0, "o alias legado tem de quebrar igual: {s:?}");
    }

    /// `break-all` parte no meio de uma palavra CURTA — a que caberia sozinha na
    /// linha seguinte e que por isso `break-word` deixaria descer inteira. É a
    /// diferença entre os dois valores, e é o que este teste fixa.
    #[test]
    fn break_all_parte_uma_palavra_que_break_word_deixaria_descer() {
        let estreito = "width:60px;font-size:16px";
        let (d1, l1) = geometria(
            &format!("<div style='{estreito};overflow-wrap:break-word'>aaaa <span>bbbb</span></div>"),
            800.0,
        );
        let (d2, l2) = geometria(
            &format!("<div style='{estreito};word-break:break-all'>aaaa <span>bbbb</span></div>"),
            800.0,
        );
        let com_word = rect(&d1, &l1, "span", 0);
        let com_all = rect(&d2, &l2, "span", 0);
        assert!(
            com_all.h > com_word.h,
            "break-all reparte a palavra em duas linhas onde break-word a desce \
             inteira: all={com_all:?} word={com_word:?}"
        );
    }

    /// `keep-all` NÃO parte: é sobre texto CJK e, em texto latino, o que pede é
    /// exatamente o comportamento inicial.
    #[test]
    fn keep_all_nao_parte_a_palavra() {
        let (dom, list) = geometria(
            "<div style='width:40px;word-break:keep-all'>\
             <span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w > 40.0, "keep-all tem de deixar transbordar: {s:?}");
    }

    /// A elipse só aparece quando as TRÊS condições se juntam: `ellipsis`,
    /// transbordo escondido e linha que não quebra.
    #[test]
    fn elipse_aparece_so_com_ellipsis_overflow_hidden_e_nowrap() {
        let conteudo = "uma frase bastante comprida para nao caber";
        let completo = "width:80px;text-overflow:ellipsis;overflow:hidden;white-space:nowrap";
        let (_d, l) = geometria(&format!("<div style='{completo}'>{conteudo}</div>"), 800.0);
        assert!(
            textos(&l).iter().any(|t| t.ends_with('…')),
            "com as três condições a linha acaba em reticências: {:?}",
            textos(&l)
        );

        // e cada uma em falta desliga-a.
        for faltando in [
            "width:80px;overflow:hidden;white-space:nowrap",
            "width:80px;text-overflow:ellipsis;white-space:nowrap",
            "width:80px;text-overflow:ellipsis;overflow:hidden",
        ] {
            let (_d, l) = geometria(&format!("<div style='{faltando}'>{conteudo}</div>"), 800.0);
            assert!(
                !textos(&l).iter().any(|t| t.contains('…')),
                "sem uma das condições não há elipse ({faltando}): {:?}",
                textos(&l)
            );
        }
    }

    /// E a linha com elipse CABE na caixa: o orçamento tira a largura das
    /// próprias reticências antes de cortar, senão elas ficavam de fora.
    #[test]
    fn a_linha_com_elipse_nao_transborda_a_caixa() {
        let (dom, list) = geometria(
            "<div style='width:80px;text-overflow:ellipsis;overflow:hidden;\
             white-space:nowrap'><span>uma frase bastante comprida para nao caber</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w <= 81.0, "a linha cortada cabe na caixa: {s:?}");
    }

    /// Um `<img width height>` sem pixels carregados OCUPA a sua caixa. É o que o
    /// browser faz enquanto a imagem não chegou da rede — e sem rede nunca chega,
    /// que é a situação de todo o harness de paridade.
    #[test]
    fn imagem_sem_pixels_ocupa_a_caixa_que_declara() {
        let (dom, list) = geometria("<div><img width='252' height='252'></div>", 800.0);
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 252.0).abs() < 0.5, "largura = {}", img.w);
        assert!((img.h - 252.0).abs() < 0.5, "altura = {}", img.h);
    }

    /// E a caixa CONTA para a largura intrínseca de quem encolhe ao conteúdo.
    ///
    /// É a cadeia inteira do defeito medido contra o Chrome: a `<figure>` do
    /// MediaWiki é `display:table`, encolhe ao conteúdo, e com a imagem a medir
    /// zero ficava com 10px em vez de 260 — a `<figcaption>` ao lado passava a
    /// quebrar a um carácter por linha, 700px de altura onde o Chrome tem 107.
    /// A imagem é que estava errada; a legenda só pagava.
    #[test]
    fn a_figura_que_encolhe_ao_conteudo_mede_a_imagem_sem_pixels() {
        let html = "<figure style='display:table'>\
            <img width='252' height='252'>\
            <figcaption style='display:table-caption'>aa bb cc dd</figcaption>\
            </figure>";
        let (dom, list) = geometria(html, 800.0);
        let fig = rect(&dom, &list, "figure", 0);
        assert!(
            fig.w >= 252.0,
            "a figura encolheu a {} em volta de uma imagem de 252",
            fig.w
        );
    }

    /// Sem pixels a caixa existe mas NADA é pintado: uma reserva vazia é o que o
    /// browser mostra, e pintar um retângulo ali seria inventar conteúdo.
    #[test]
    fn imagem_sem_pixels_reserva_a_caixa_mas_nao_pinta_nada() {
        let (_dom, list) = geometria("<div><img width='40' height='40'></div>", 800.0);
        let pintadas = list
            .materialized()
            .iter()
            .filter(|i| matches!(i, crate::layout::DisplayItem::Image { .. }))
            .count();
        assert_eq!(pintadas, 0, "pintou {pintadas} imagem(ns) sem pixels");
    }

    /// Uma imagem larga num contentor estreito TRANSBORDA — não encolhe.
    ///
    /// Medido no Chrome: `<div style='width:50px'><img width='100' height='101'>`
    /// dá 100x101, e assim a 500, 100, 50 e 10px de contentor. Encolher era o que
    /// aqui se fazia (50x50 aos 50px, 10x10 aos 10px), e dentro de uma tabela
    /// fechava um ciclo: a célula estreitava porque a imagem encolhia, e a imagem
    /// encolhia porque a célula estreitara.
    #[test]
    fn imagem_larga_em_container_estreito_transborda_como_no_chrome() {
        for largura in [500, 100, 50, 10] {
            let html = format!(
                "<div style='width:{largura}px'><img width='100' height='101'></div>"
            );
            let (dom, list) = geometria(&html, 800.0);
            let img = rect(&dom, &list, "img", 0);
            assert!(
                (img.w - 100.0).abs() < 0.5 && (img.h - 101.0).abs() < 0.5,
                "contentor de {largura}px encolheu a imagem: {img:?}"
            );
        }
    }

    /// E um `max-width` DECLARADO continua a encolher: é ele quem manda encolher
    /// no CSS, e esta é a metade que prova que o corte mudou de sítio em vez de
    /// desaparecer.
    ///
    /// A altura NÃO acompanha aqui, e é o comportamento certo: a razão só se
    /// preserva quando a outra dimensão é `auto`, e este `<img>` declara as duas.
    /// (Sem rede não há pixels, logo não há razão intrínseca para exercitar o
    /// outro ramo — é o que todo este harness mede.)
    #[test]
    fn max_width_declarado_encolhe_a_largura() {
        let (dom, list) = geometria(
            "<div style='width:500px'><img style='max-width:50px' width='100' height='200'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 50.0).abs() < 0.5, "largura = {}", img.w);
        assert!((img.h - 200.0).abs() < 0.5, "altura declarada = {}", img.h);
    }

    /// A base de uma PERCENTAGEM é a largura do bloco contentor — a margem do
    /// próprio elemento NÃO entra nela.
    ///
    /// É a receita do `.mw-file-element` da Wikipédia: `margin:3px` e
    /// `max-width:calc(100% - (2 * 3px) - (2 * 1px))` num contentor de 258px. O
    /// `100%` vale 258, o `calc` dá 250, e a imagem de 250 fica intacta. Descontar
    /// as margens antes de resolver — o que `layout_image` fazia — punha o `100%`
    /// a valer 252 e cortava a imagem para 244: 6px exatos, em 30 imagens da
    /// página.
    #[test]
    fn a_percentagem_de_max_width_resolve_contra_o_contentor_e_nao_menos_a_margem() {
        let html = "<div style='width:258px'>            <img style='margin:3px;max-width:calc(100% - (2 * 3px) - (2 * 1px))'              width='250' height='167'></div>";
        let (dom, list) = geometria(html, 800.0);
        let img = rect(&dom, &list, "img", 0);
        assert!(
            (img.w - 250.0).abs() < 0.5,
            "o 100% resolveu contra {} em vez de 258: {img:?}",
            img.w + 8.0
        );
    }

    /// Um `height:auto` DECLARADO vence o atributo `height` do HTML — e a altura
    /// sai da RAZÃO, não do zero.
    ///
    /// Duas regras num teste porque é o par que as torna verdadeiras: o atributo
    /// é um presentational hint e perde para qualquer declaração, mas os dois
    /// atributos juntos continuam a dar a razão de aspecto (HTML, "dimension
    /// attributes"). Tirar o atributo e não pôr a razão dava altura ZERO — a
    /// miniatura da Wikipédia media 252x2, só as bordas.
    #[test]
    fn height_auto_declarado_vence_o_atributo_mas_a_razao_sobrevive() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='height:auto' width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 250.0).abs() < 0.5, "largura = {}", img.w);
        assert!(
            (img.h - 167.0).abs() < 0.5,
            "a altura tem de vir da razão 250:167, não do zero: {}",
            img.h
        );
    }

    /// E com `width` declarado MENOR, a razão dos atributos escala a altura: é a
    /// prova de que o 167 acima veio da razão e não de o atributo ter sobrevivido.
    #[test]
    fn a_razao_dos_atributos_escala_a_altura_quando_a_largura_muda() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='width:125px;height:auto'              width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 125.0).abs() < 0.5, "largura = {}", img.w);
        assert!(
            (img.h - 83.5).abs() < 0.5,
            "metade da largura é metade da altura: {}",
            img.h
        );
    }

    /// A caixa de um replaced é a BORDER-BOX, que é o que `getBoundingClientRect`
    /// devolve. `.mw-file-element{border:1px solid}` dá 250 de conteúdo e 252 de
    /// caixa — os 2px que sobravam depois de a base da percentagem ser corrigida.
    #[test]
    fn a_borda_entra_na_caixa_do_replaced() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='border:1px solid #000'              width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 252.0).abs() < 0.5, "250 + 1 + 1 = {}", img.w);
        assert!((img.h - 169.0).abs() < 0.5, "167 + 1 + 1 = {}", img.h);
    }

    /// Uma largura SEM estilo de borda não ocupa nada: o inicial de
    /// `border-style` é `none`, e sem estilo o Chrome não desenha nem reserva.
    #[test]
    fn largura_de_borda_sem_estilo_nao_entra_na_caixa() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='border-width:10px'              width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 250.0).abs() < 0.5, "largura = {}", img.w);
    }

    /// A receita COMPLETA da Wikipédia, medida contra o Chrome: contentor de
    /// 258px, `margin:3px`, `border:1px`, `height:auto` e
    /// `max-width:calc(100% - (2 * 3px) - (2 * 1px))` sobre um `<img>` de 250x167.
    ///
    /// A largura é a do Chrome ao pixel (252). A ALTURA diverge — o Chrome dá 252,
    /// porque offline a imagem nunca carrega e ele cai num quadrado; nós damos 169
    /// pela razão dos atributos, que é o que a spec manda e o que continua certo
    /// no dia em que a imagem chegar. Divergência conhecida, fixada aqui para que
    /// mude por decisão e não por acidente.
    #[test]
    fn a_miniatura_da_wikipedia_mede_a_largura_do_chrome() {
        let html = "<div style='width:258px'><a>            <img style='margin:3px;border:1px solid #000;height:auto;             max-width:calc(100% - (2 * 3px) - (2 * 1px))' width='250' height='167'></a></div>";
        let (dom, list) = geometria(html, 800.0);
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 252.0).abs() < 0.5, "o Chrome dá 252: {}", img.w);
        assert!((img.h - 169.0).abs() < 0.5, "167 + as duas bordas: {}", img.h);
    }

    /// Dentro de um `<picture>`, é o `<source>` escolhido que dá as dimensões — e
    /// a escolha depende do VIEWPORT.
    ///
    /// É o rodapé da Wikipédia, ao pixel: com o viewport de 1280 o
    /// `media="(min-width: 500px)"` casa e a caixa é a do `<source>` (84×29, que
    /// é o que o Chrome mede); com 400 não casa, e responde o `<img>` de
    /// fallback. O mesmo documento, duas respostas, que é o que `<picture>` é.
    #[test]
    fn o_source_escolhido_do_picture_da_as_dimensoes_e_o_viewport_decide() {
        let html = "<picture>            <source media='(min-width: 500px)' srcset='/b.svg' width='84' height='29'>            <img src='/a.svg' width='25' height='25'></picture>";
        let (d1, l1) = geometria(html, 1280.0);
        let largo = rect(&d1, &l1, "img", 0);
        assert!(
            (largo.w - 84.0).abs() < 0.5 && (largo.h - 29.0).abs() < 0.5,
            "o source que casa tem de mandar: {largo:?}"
        );
        let (d2, l2) = geometria(html, 400.0);
        let estreito = rect(&d2, &l2, "img", 0);
        assert!(
            (estreito.w - 25.0).abs() < 0.5 && (estreito.h - 25.0).abs() < 0.5,
            "sem source que case, responde o <img>: {estreito:?}"
        );
    }

    /// Um `media` que o avaliador NÃO SABE ler salta a `<source>` em vez de a
    /// escolher por engano.
    ///
    /// É a honestidade que o `MediaQuery` já tem — uma feature desconhecida torna
    /// a query sempre-falsa — e vale a pena fixá-la aqui: a alternativa (ignorar
    /// o `media` que não se entende) escolhia uma caixa que o browser não teria
    /// escolhido, e o erro ficava silencioso.
    #[test]
    fn um_media_que_nao_sabemos_avaliar_salta_a_source() {
        let html = "<picture>            <source media='(orientation: landscape)' srcset='/b.svg' width='84' height='29'>            <img src='/a.svg' width='25' height='25'></picture>";
        let (dom, list) = geometria(html, 1280.0);
        let img = rect(&dom, &list, "img", 0);
        assert!(
            (img.w - 25.0).abs() < 0.5,
            "uma condição que não entendemos não pode ganhar: {img:?}"
        );
    }

    /// Uma `<source>` SEM `media` casa sempre — é a forma que só oferece outro
    /// formato — e a PRIMEIRA que casa é a que ganha, na ordem do documento.
    #[test]
    fn a_primeira_source_que_casa_ganha_e_sem_media_casa_sempre() {
        let html = "<picture>            <source media='(min-width: 5000px)' srcset='/x.svg' width='10' height='10'>            <source srcset='/b.svg' width='84' height='29'>            <source srcset='/c.svg' width='99' height='99'>            <img src='/a.svg' width='25' height='25'></picture>";
        let (dom, list) = geometria(html, 1280.0);
        let img = rect(&dom, &list, "img", 0);
        assert!(
            (img.w - 84.0).abs() < 0.5,
            "a primeira que casa é a segunda source: {img:?}"
        );
    }

    /// Um `<img>` FORA de um `<picture>` não é afetado: um `<source>` que não é
    /// irmão dele não lhe diz respeito.
    #[test]
    fn uma_source_fora_do_picture_nao_dimensiona_a_imagem() {
        let html = "<div>            <source srcset='/b.svg' width='84' height='29'>            <img src='/a.svg' width='25' height='25'></div>";
        let (dom, list) = geometria(html, 1280.0);
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 25.0).abs() < 0.5, "{img:?}");
    }

    /// Um `display:inline` com `padding:0` mede o TEXTO, não o contentor.
    ///
    /// É a receita do `.mw-heading` da Wikipédia, e o `padding:0` era sozinho o
    /// que a partia: um zero declarado contava como "há padding", devolvia o
    /// `<h3>` ao caminho de bloco e ele saía com os 752px do contentor em vez dos
    /// ~185 do seu texto. São 51 cabeçalhos na página, 100% errados, todos assim.
    #[test]
    fn cabecalho_inline_com_padding_zero_mede_o_texto_e_nao_o_contentor() {
        let regra = "<style>.mw-heading h3{display:inline;border:0;margin:0;                     padding:0;color:inherit;font:inherit}</style>";
        let html = format!(
            "{regra}<div style='width:600px' class='mw-heading'><h3>abc</h3></div>"
        );
        let (dom, list) = geometria(&html, 800.0);
        let h3 = rect(&dom, &list, "h3", 0);
        assert!(
            h3.w < 100.0,
            "o cabeçalho tomou a largura do contentor: {h3:?}"
        );
    }

    /// E um padding REAL continua a criar caixa: é o par que prova que a pergunta
    /// mudou de "declarou?" para "ocupa espaço?" em vez de desaparecer.
    #[test]
    fn padding_declarado_com_valor_continua_a_criar_caixa() {
        let zero = "<div style='width:400px'><span style='padding:0'>abc</span></div>";
        let real = "<div style='width:400px'><span style='padding:4px'>abc</span></div>";
        let (d1, l1) = geometria(zero, 800.0);
        let (d2, l2) = geometria(real, 800.0);
        let a = rect(&d1, &l1, "span", 0);
        let b = rect(&d2, &l2, "span", 0);
        assert!(
            (b.w - a.w - 8.0).abs() < 0.5,
            "os 4px de cada lado têm de aparecer: zero={a:?} real={b:?}"
        );
    }

    /// A mesma pergunta para a BORDA, que hoje não disparava por acidente e não
    /// por desenho — os dois lados respondem agora à mesma regra.
    #[test]
    fn borda_zero_nao_cria_caixa_e_borda_real_cria() {
        let zero = "<div style='width:400px'><span style='border:0'>abc</span></div>";
        let real = "<div style='width:400px'><span style='border:2px solid #000'>abc</span></div>";
        let (d1, l1) = geometria(zero, 800.0);
        let (d2, l2) = geometria(real, 800.0);
        let a = rect(&d1, &l1, "span", 0);
        let b = rect(&d2, &l2, "span", 0);
        assert!((a.w - 22.08).abs() < 1.0, "borda zero não ocupa: {a:?}");
        assert!(b.w > a.w, "borda real ocupa: zero={a:?} real={b:?}");
    }

    /// A `hlist` do MediaWiki NÃO regride, e o número é o do Chrome.
    ///
    /// Medido em `chrome_extract.mjs` sobre o CSS verdadeiro: o `<ul>` interior
    /// mede **89,3** — a largura do seu conteúdo — e não os 600 do contentor. O
    /// teste que já existia fixava só `w > 0`, o que passa nos dois mundos; este
    /// fixa o valor, que é o que separa "tem caixa" de "tem a caixa certa".
    /// (88,3 aqui contra 89,3 lá é a métrica aproximada do medidor de teste.)
    #[test]
    fn a_hlist_do_mediawiki_mede_o_conteudo_e_nao_o_contentor() {
        let html = "<style>.hlist ul{margin:0;padding:0}            .hlist li{margin:0;display:inline}.hlist ul ul{display:inline}</style>            <div style='width:600px'><div class='hlist'><ul><li>alfa            <ul><li>bravo</li><li>charlie</li></ul></li><li>delta</li></ul></div></div>";
        let (dom, list) = geometria(html, 800.0);
        let ul = rect(&dom, &list, "ul", 1);
        assert!(
            ul.w > 50.0 && ul.w < 150.0,
            "o ul interior é a largura do seu conteúdo (~89 no Chrome): {ul:?}"
        );
    }

    /// Um `inline-block` dentro de um pai INLINE continua a ter caixa.
    ///
    /// É a forma do menu principal da Wikipédia — um `<ul>` que passou a inline
    /// com `<li style="display:inline-block">` dentro — e o que ela expôs não é
    /// geometria errada, é **conteúdo invisível**: o `<li>` não recebia retângulo
    /// nenhum. O caminho nunca tinha sido exercitado porque o pai nunca era
    /// inline; foi a correção dos cabeçalhos que o tornou alcançável.
    ///
    /// 22,08 aqui contra os 22,2 medidos no Chrome é a métrica aproximada do
    /// medidor de teste — o que se fixa é que a caixa EXISTE e mede o texto, e
    /// não a largura do contentor.
    #[test]
    fn inline_block_dentro_de_pai_inline_continua_a_ter_caixa() {
        let (dom, list) = geometria(
            "<div style='width:600px'><ul style='display:inline'>             <li style='display:inline-block'>abc</li></ul></div>",
            800.0,
        );
        let li = rect(&dom, &list, "li", 0);
        assert!(li.w > 0.0, "o inline-block ficou sem caixa: {li:?}");
        assert!(
            li.w < 100.0,
            "e mede o texto, não o contentor de 600: {li:?}"
        );
    }

    /// O mesmo com um `<span>`, que é a outra forma em que o defeito aparecia na
    /// página — um pai inline cujos filhos desapareciam.
    #[test]
    fn os_filhos_de_um_inline_nao_desaparecem() {
        let (dom, list) = geometria(
            "<div style='width:600px'><span style='display:inline'>             <span style='display:inline-block'>abc</span></span></div>",
            800.0,
        );
        let filho = rect(&dom, &list, "span", 1);
        assert!(filho.w > 0.0, "o filho perdeu a caixa: {filho:?}");
    }

    /// Um `display:inline-block` FLUI na linha, ao lado do texto.
    ///
    /// Media-se no Chrome a caixa ao lado da palavra; aqui ela descia para a
    /// linha seguinte, encostada à esquerda. A causa era uma cópia local da
    /// pergunta "é de bloco?" escrita como "não é inline?" — e o `inline-block`
    /// é o valor que as duas leituras separam.
    #[test]
    fn inline_block_flui_na_linha_do_texto_em_vez_de_descer() {
        let (dom, list) = geometria(
            "<div style='width:600px'>abc <span style='display:inline-block;             width:20px;height:10px'></span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.x > 10.0, "o inline-block foi para o início da linha: {s:?}");
        assert_eq!(s.y, 0.0, "e desceu de linha: {s:?}");
    }

    /// Dois inline-blocks SEM texto à volta continuam lado a lado — a corrida de
    /// irmãos, que é o caso dos botões, não regride.
    #[test]
    fn dois_inline_blocks_irmaos_ficam_lado_a_lado() {
        let ib = "display:inline-block;width:20px;height:10px";
        let (dom, list) = geometria(
            &format!("<div style='width:600px'><span style='{ib}'></span>                      <span style='{ib}'></span></div>"),
            800.0,
        );
        let a = rect(&dom, &list, "span", 0);
        let b = rect(&dom, &list, "span", 1);
        assert_eq!(a.y, b.y, "empilharam: a={a:?} b={b:?}");
        assert!(b.x > a.x, "e não avançaram: a={a:?} b={b:?}");
    }

    /// Um `display:inline-block` declarado vence a TAG.
    ///
    /// `.mw-list-item{display:inline-block}` sobre um `<li>` batia no
    /// `block::lookup("li")` e voltava ao caminho de bloco: os itens do menu
    /// empilhados, cada um com a largura do contentor. São 27 dos 55
    /// inline-blocks da página da Wikipédia.
    #[test]
    fn inline_block_declarado_vence_a_tag_de_bloco() {
        let (dom, list) = geometria(
            "<ul style='width:600px'><li style='display:inline-block'>abc</li>             <li style='display:inline-block'>def</li></ul>",
            800.0,
        );
        let a = rect(&dom, &list, "li", 0);
        let b = rect(&dom, &list, "li", 1);
        assert_eq!(a.y, b.y, "os itens empilharam: a={a:?} b={b:?}");
        assert!(a.w < 100.0, "e tomaram a largura do contentor: {a:?}");
    }

    /// Um `<img>` que não declara dimensão nenhuma e não tem pixels continua sem
    /// caixa. O par com os testes acima é o que prova que a caixa vem do que se
    /// DECLARA, e não de o elemento ser um `<img>`.
    #[test]
    fn imagem_sem_dimensao_nenhuma_continua_sem_caixa() {
        let (dom, list) = geometria("<div><img></div>", 800.0);
        let ids = dom.query_all("img");
        let idx = dom.resolve(ids[0]).unwrap();
        let r = list.geometry_now().rects.get(&idx).copied();
        assert!(r.is_none_or(|r| r.w == 0.0), "caixa inventada: {r:?}");
    }
