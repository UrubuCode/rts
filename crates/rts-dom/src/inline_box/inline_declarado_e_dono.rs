//! Testes movidos de `inline_box.rs` na modularização; nenhuma linha foi
//! alterada. A indentação de 4 espaços é a do `mod` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    //! Um elemento cujo `display:inline` e DECLARADO continua a ser dono dos
    //! seus fragmentos mesmo quando declara padding.
    //!
    //! A forma e a `hlist` do MediaWiki — `.hlist ul{padding:0}` sobre
    //! `.hlist ul ul{display:inline}` — e o que a disparava era o `padding`
    //! valer como "declarado" independentemente de ser zero.

    use crate::table::tests::geometria;

    fn caixa(html: &str, sel: &str, n: usize) -> Option<crate::layout::Rect> {
        let (dom, list) = geometria(html, 800.0);
        let id = *dom.query_all(sel).get(n)?;
        let idx = dom.resolve(id)?;
        list.geometry_now().rects.get(&idx).copied()
    }

    /// O `padding` declarado num inline NAO lhe tira a caixa — nem quando e
    /// zero, que era a forma que a pagina real usava.
    #[test]
    fn ul_inline_com_padding_declarado_continua_a_ter_caixa() {
        let regras = "<style>.h li{display:inline}.h ul ul{display:inline}</style>";
        let lista = "<div class='h'><ul><li>a<ul><li>b</li><li>c</li></ul></li></ul></div>";
        let sem_padding = caixa(&format!("{regras}{lista}"), "ul", 1);
        assert!(
            sem_padding.is_some_and(|r| r.w > 0.0),
            "sem padding a caixa ja existia: {sem_padding:?}"
        );
        for padding in ["padding:0", "padding:2px 0"] {
            let com = caixa(
                &format!("{regras}<style>.h ul{{{padding}}}</style>{lista}"),
                "ul",
                1,
            );
            assert!(
                com.is_some_and(|r| r.w > 0.0),
                "`{padding}` tirou a caixa ao inline: {com:?}"
            );
        }
    }

    /// E a caixa que ele ganha e a UNIAO dos filhos, nao uma caixa inventada:
    /// contem os `li` que ja estavam certos.
    #[test]
    fn a_caixa_do_inline_contem_os_fragmentos_dos_filhos() {
        let html = "<style>.h li{display:inline}.h ul ul{display:inline}\
                    .h ul{padding:0}</style>\
                    <div class='h'><ul><li>a<ul><li>bbbb</li><li>cccc</li></ul></li></ul></div>";
        let ul = caixa(html, "ul", 1).expect("o ul interior tem caixa");
        let li = caixa(html, "li", 1).expect("o li neto tem caixa");
        assert!(
            ul.x <= li.x + 0.01 && ul.x + ul.w >= li.x + li.w - 0.01,
            "a caixa do ul tem de conter o li: ul={ul:?} li={li:?}"
        );
    }
