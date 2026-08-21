//! A caixa de uma imagem: dimensões declaradas, razão dos atributos,
//! `max-width`, borda, e o `<source>` que o `<picture>` escolhe.
//!
//! Movido de `inline_box.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.

    use super::*;

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
