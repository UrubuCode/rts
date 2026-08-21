//! Quebra de palavra e elipse: `overflow-wrap`, `word-break`, `word-wrap`
//! legado e `text-overflow: ellipsis`.
//!
//! Movido de `inline_box.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.

    use super::*;
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
