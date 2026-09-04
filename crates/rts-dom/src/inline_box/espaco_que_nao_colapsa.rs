//! Testes movidos de `inline_box.rs` na modularização; nenhuma linha foi
//! alterada. A indentação de 4 espaços é a do `mod` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    //! O NBSP e o inline sem texto visivel: dois defeitos com o mesmo sintoma
    //! (o elemento fica sem caixa) e causas diferentes, por isso testados
    //! separados.
    //!
    //! Todos medem com o `ApproxMeasurer` — cada caractere vale
    //! `tamanho * 0.46` — para que as larguras sejam previsiveis sem falar
    //! sobre uma fonte real.

    use crate::table::tests::{geometria, textos};

    /// Uma caixa que pode nao existir: e a diferenca entre "sem geometria" e
    /// "geometria de largura zero", e os dois casos aparecem aqui.
    fn caixa(html: &str, sel: &str, n: usize) -> Option<crate::layout::Rect> {
        let (dom, list) = geometria(html, 800.0);
        let id = *dom.query_all(sel).get(n)?;
        let idx = dom.resolve(id)?;
        list.geometry_now().rects.get(&idx).copied()
    }

    /// O NBSP OCUPA largura; o espaco normal no mesmo sitio nao ocupa nenhuma.
    ///
    /// E o par que separa os dois caracteres: um `<span>&nbsp;</span>` e
    /// conteudo — o Chrome da-lhe 4,45px a 14,4px de fonte — e um
    /// `<span> </span>` entre duas palavras e o separador que elas ja tinham,
    /// portanto colapsa e o span fica com largura zero.
    #[test]
    fn nbsp_ocupa_largura_e_o_espaco_normal_colapsa() {
        let com_nbsp = caixa("<p>aa <span>&nbsp;</span> bb</p>", "span", 0);
        let com_espaco = caixa("<p>aa <span> </span> bb</p>", "span", 0);
        assert!(
            com_nbsp.is_some_and(|r| r.w > 0.0),
            "o NBSP tem avanco proprio: {com_nbsp:?}"
        );
        assert!(
            com_espaco.is_none_or(|r| r.w == 0.0),
            "o espaco normal entre duas palavras colapsa: {com_espaco:?}"
        );
    }

    /// E o NBSP e PINTADO, em vez de desaparecer no colapso.
    ///
    /// A prova de que nao e so a caixa que voltou: o caractere continua no
    /// texto que vai para o ecra, que e o que faz a linha ter a largura certa.
    #[test]
    fn o_nbsp_sobrevive_ao_texto_pintado() {
        let (_d, list) = geometria("<p>aa<span>&nbsp;</span>bb</p>", 800.0);
        assert!(
            textos(&list).iter().any(|t| t.contains('\u{00A0}')),
            "o NBSP foi engolido pela normalizacao: {:?}",
            textos(&list)
        );
    }

    /// O NBSP NAO e oportunidade de quebra — e a razao de existir.
    ///
    /// Com espaco normal as duas palavras separam-se e a linha parte em duas;
    /// com NBSP nao ha onde partir e a linha transborda inteira, que e o que o
    /// browser faz. A caixa de 60px cabe "aaaa" (29,4) mas nao "aaaa bbbb"
    /// (66,2).
    #[test]
    fn nbsp_nao_oferece_oportunidade_de_quebra() {
        let estreito = "width:60px;font-size:16px";
        let colado = caixa(
            &format!("<p style='{estreito}'><span>aaaa&nbsp;bbbb</span></p>"),
            "span",
            0,
        )
        .expect("o span colado tem caixa");
        let separado = caixa(
            &format!("<p style='{estreito}'><span>aaaa bbbb</span></p>"),
            "span",
            0,
        )
        .expect("o span separado tem caixa");
        assert!(
            colado.h < separado.h,
            "o NBSP tem de manter uma linha so: colado={colado:?} separado={separado:?}"
        );
        assert!(
            colado.w > 60.0,
            "e transborda em vez de partir: {colado:?}"
        );
    }

    /// O avanco do NBSP DESLOCA o que vem a seguir na linha.
    ///
    /// E o erro visivel do colapso, e o que o distingue de uma caixa em falta:
    /// perder o NBSP nao apaga so o span dele, encosta a esquerda tudo o que
    /// vem depois na mesma linha.
    #[test]
    fn o_avanco_do_nbsp_empurra_o_vizinho_da_linha() {
        let com = caixa("<p>a<span>&nbsp;</span><b>b</b></p>", "b", 0)
            .expect("o vizinho tem caixa");
        let sem = caixa("<p>a<span></span><b>b</b></p>", "b", 0)
            .expect("o vizinho tem caixa");
        assert!(
            com.x > sem.x,
            "o NBSP tem de empurrar o vizinho: com={com:?} sem={sem:?}"
        );
    }

    /// Um inline cujo conteudo TODO e `display:none` nao gera fragmento
    /// nenhum — `getBoundingClientRect` no Blink real da 0x0, nao a altura
    /// do strut. E a forma do COinS da Wikipedia — `<span class="Z3988">`
    /// com um unico filho escondido, ~280 por pagina.
    ///
    /// CORRIGIDO (medido no Blink, `claude-sel-has.html`): a versao anterior
    /// deste teste fixava `r.h > 0.0` — a altura do strut vazando para o
    /// rect de um inline sem conteudo nenhum, o mesmo defeito que
    /// `linha.rs::AtomicKind::Marker` tinha para QUALQUER inline vazio.
    #[test]
    fn inline_com_todo_o_conteudo_escondido_tem_caixa_de_largura_zero() {
        let r = caixa(
            "<p>aa <span class='z'><span style='display:none'>Z39.88</span></span> bb</p>",
            "span",
            0,
        );
        let sem_area = r.is_none_or(|r| r.w == 0.0 && r.h == 0.0);
        assert!(sem_area, "{r:?}");
    }

    /// E o texto escondido NAO e pintado.
    ///
    /// A metade que prova que a largura zero veio de o conteudo ser saltado, e
    /// nao de ele ter sido medido a zero: antes deste par, os metadados de
    /// citacao apareciam escritos no meio do paragrafo.
    #[test]
    fn o_texto_de_um_display_none_nao_entra_na_linha() {
        let (_d, list) = geometria(
            "<p>aa <span><span style='display:none'>Z39.88</span></span> bb</p>",
            800.0,
        );
        assert!(
            !textos(&list).iter().any(|t| t.contains("Z39.88")),
            "texto escondido pintado na linha: {:?}",
            textos(&list)
        );
    }

    /// Um inline VAZIO (`Marker`) nao gera fragmento — 0x0 no Blink real.
    ///
    /// CORRIGIDO (medido no Blink): dava `h > 0.0` (a altura do strut).
    #[test]
    fn inline_vazio_nao_tem_caixa() {
        let r = caixa("<p>aa <span></span> bb</p>", "span", 0);
        assert!(r.is_none_or(|r| r.w == 0.0 && r.h == 0.0), "{r:?}");
    }
