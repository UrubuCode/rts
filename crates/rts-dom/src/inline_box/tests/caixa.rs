//! Quando um inline TEM caixa própria: padding, borda, e `inline-block`
//! declarado ou herdado da tag.
//!
//! Movido de `inline_box.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.

    use super::*;

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
