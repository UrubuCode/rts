    use super::*;

    /// Uma cor cinzenta média sobrevive ao `contrast`: é o ponto fixo da
    /// fórmula, e é o que apanha um deslocamento trocado de sinal.
    ///
    /// A tolerância de 1 não é folga: o ponto fixo é 127,5 e um `u8` não o tem.
    /// `0x80` é 0,502, portanto `contrast(2)` afasta-o meio nível do meio — e
    /// exigir igualdade exata era uma afirmação falsa sobre a aritmética, não
    /// um teste mais apertado.
    #[test]
    fn contraste_fixa_o_meio_cinzento() {
        let c = filtro("contrast(2)").aplicar(0x8080_80FF);
        assert!(((c >> 24) & 0xFF).abs_diff(0x80) <= 1, "{c:08X}");
        assert_eq!(c & 0xFF, 0xFF, "o contraste não toca no alpha");
    }

    #[test]
    fn invert_1_troca_preto_e_branco() {
        let f = filtro("invert(1)");
        assert_eq!(f.aplicar(0x0000_00FF), 0xFFFF_FFFF);
        assert_eq!(f.aplicar(0xFFFF_FFFF), 0x0000_00FF);
    }

    /// `invert(100%)` é o mesmo que `invert(1)` — a percentagem não é um caso
    /// à parte, é a mesma fração.
    #[test]
    fn percentagem_e_fracao_dao_o_mesmo() {
        assert_eq!(filtro("invert(100%)"), filtro("invert(1)"));
        assert_eq!(filtro("brightness(50%)"), filtro("brightness(0.5)"));
    }

    /// O cinzento de um vermelho puro é a luminância dele, não a média dos
    /// canais — o erro clássico, e o que distingue a matriz certa da inventada.
    #[test]
    fn grayscale_usa_luminancia_e_nao_media() {
        let c = filtro("grayscale(1)").aplicar(0xFF00_00FF);
        let r = (c >> 24) & 0xFF;
        assert_eq!(r, 54, "0.2126*255 = 54; a média daria 85");
        assert_eq!((c >> 16) & 0xFF, r, "cinzento: os três canais iguais");
        assert_eq!((c >> 8) & 0xFF, r);
    }

    #[test]
    fn brightness_escala_e_satura_no_topo() {
        assert_eq!(filtro("brightness(0)").aplicar(0xFFFF_FFFF), 0x0000_00FF);
        // 0x80 * 2 = 0x100, que não cabe: satura em 0xFF em vez de dar a volta.
        assert_eq!(filtro("brightness(2)").aplicar(0x8080_80FF), 0xFFFF_FFFF);
    }

    /// `opacity()` mexe no alpha e SÓ no alpha — não escurece o RGB.
    #[test]
    fn opacity_so_toca_no_alpha() {
        let c = filtro("opacity(0.5)").aplicar(0x1122_33FF);
        assert_eq!(c & 0xFFFF_FF00, 0x1122_3300);
        assert_eq!(c & 0xFF, 128);
    }

    /// A ordem de leitura é a ordem de aplicação, e o par que o mostra tem de
    /// ser um que NÃO comute.
    ///
    /// `grayscale`+`invert` parecia o par óbvio e não serve: os pesos da
    /// luminância somam 1, logo `lum(1-c) = 1-lum(c)` e os dois sentidos dão
    /// exatamente a mesma cor. Um teste escrito com eles passaria com a
    /// composição trocada. `brightness`+`invert` não comuta — `2(1-c)` contra
    /// `1-2c` — e é por isso que é este o par aqui.
    #[test]
    fn a_cadeia_aplica_na_ordem_de_leitura() {
        let cinza = 0x8080_80FF;
        // clarear primeiro passa de 1,0 e o inverso satura em 0.
        assert_eq!(
            (filtro("brightness(2) invert(1)").aplicar(cinza) >> 24) & 0xFF,
            0
        );
        // inverter primeiro dá 0,498, e o dobro disso quase satura por cima.
        assert_eq!(
            (filtro("invert(1) brightness(2)").aplicar(cinza) >> 24) & 0xFF,
            254
        );
    }

    /// O cinzento de um vermelho, invertido, é o inverso do cinzento dele — a
    /// identidade que o teste da ordem NÃO pode usar, e que vale à mesma como
    /// verificação da composição em si.
    #[test]
    fn grayscale_e_invert_compoem_sem_perder_o_valor() {
        let c = filtro("grayscale(1) invert(1)").aplicar(0xFF00_00FF);
        assert_eq!((c >> 24) & 0xFF, 201, "255 - 54");
    }

    /// O ponto central da regra da casa: `blur` não desfoca aqui, e por isso a
    /// cadeia inteira não pinta. Aplicar só o `brightness` daria um elemento
    /// nítido e mais claro — nem o pedido, nem o anterior.
    #[test]
    fn blur_na_cadeia_recusa_a_cadeia_toda() {
        let f = filtro("blur(4px) brightness(1.5)");
        assert!(
            f.e_identidade(),
            "cadeia com blur tem de deixar a cor intacta"
        );
        assert!(filtro("drop-shadow(0 1px 2px rgba(0,0,0,.4))").e_identidade());
        assert!(filtro("blur(4px)").e_identidade());
    }

    /// Um `var()` por resolver chega aqui como texto: é inexprimível pela mesma
    /// porta que o `blur`, e não um número a fingir de zero.
    #[test]
    fn var_por_resolver_nao_vira_zero() {
        assert!(filtro("brightness(var(--b))").e_identidade());
        assert!(filtro("var(--f)").e_identidade());
    }

    #[test]
    fn none_e_vazio_sao_identidade() {
        assert!(filtro("none").e_identidade());
        assert!(filtro("").e_identidade());
        assert!(filtro("   ").e_identidade());
    }

    /// `drop-shadow` traz um `rgba(...)` dentro: se o parser fechasse na
    /// primeira `)`, sobrava lixo — e lixo também dá identidade, o que tornaria
    /// este bug invisível. Daí testar o PARSER e não só o resultado.
    #[test]
    fn parenteses_aninhados_nao_partem_o_parse() {
        let fs = funcoes("drop-shadow(0 0 2px rgba(0,0,0,.5)) invert(1)").unwrap();
        assert_eq!(fs.len(), 2);
        assert_eq!(fs[0].0, "drop-shadow");
        assert_eq!(fs[1], ("invert".to_owned(), "1".to_owned()));
    }

    #[test]
    fn hue_rotate_de_360_nao_muda_nada_de_visivel() {
        let c = filtro("hue-rotate(360deg)").aplicar(0x3366_99FF);
        for desloc in [24, 16, 8] {
            let (a, b) = ((c >> desloc) & 0xFF, (0x3366_99FFu32 >> desloc) & 0xFF);
            assert!(a.abs_diff(b) <= 1, "erro de arredondamento, não de matriz");
        }
    }

    const CAIXA: Rect = Rect {
        x: 10.0,
        y: 20.0,
        w: 100.0,
        h: 50.0,
    };

    #[test]
    fn inset_de_um_valor_encolhe_os_quatro_lados() {
        let r = clip_retangulo("inset(10px)", CAIXA).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (20.0, 30.0, 80.0, 30.0));
    }

    /// A ordem do CSS é cima/direita/baixo/esquerda, e a percentagem de cima é
    /// da ALTURA — trocar os eixos passaria num caixa quadrada e só nela.
    #[test]
    fn inset_percentagem_usa_o_eixo_certo() {
        let r = clip_retangulo("inset(10% 25%)", CAIXA).unwrap();
        assert_eq!(r.y, 20.0 + 5.0, "10% de 50 de altura");
        assert_eq!(r.x, 10.0 + 25.0, "25% de 100 de largura");
        assert_eq!(r.w, 50.0);
        assert_eq!(r.h, 40.0);
    }

    /// Lados que se cruzam dão área ZERO. Um `w` negativo não é "nada pintado",
    /// é um retângulo ao contrário no backend.
    #[test]
    fn inset_que_se_cruza_da_area_zero_e_nao_negativa() {
        let r = clip_retangulo("inset(60px)", CAIXA).unwrap();
        assert_eq!((r.w, r.h), (0.0, 0.0));
    }

    /// As formas que não sabemos fazer não recortam NADA — e em particular não
    /// recortam pela envolvente, que é a aproximação errada com aparência de
    /// certa.
    #[test]
    fn formas_inexprimiveis_nao_recortam() {
        for v in [
            "polygon(50% 0, 100% 100%, 0 100%)",
            "circle(40% at 50% 50%)",
            "ellipse(50% 40%)",
            "path('M0 0 L10 10')",
            "inset(10px round 8px)",
            "none",
            "var(--c)",
        ] {
            assert!(clip_retangulo(v, CAIXA).is_none(), "{v} não devia recortar");
        }
    }

    /// `em` precisa do estilo do elemento, que aqui não existe. Adivinhar 16px
    /// daria um recorte errado — recusar deixa o elemento inteiro.
    #[test]
    fn unidade_que_nao_sabemos_resolver_recusa() {
        assert!(clip_retangulo("inset(1em)", CAIXA).is_none());
        assert!(clip_retangulo("inset(2rem 1px)", CAIXA).is_none());
    }

    /// `clip: rect(0,0,0,0)` (CSS2.1) recorta a caixa a NADA — a área em que
    /// o WPT `visufx/clip-004..006` etc. falhavam: 9216px = 96×96 (1in²)
    /// pintados onde o browser não pinta nada.
    #[test]
    fn clip_legado_todo_zero_da_area_zero() {
        use crate::style::values::Dimension;
        use crate::style::vocab::Clip;
        let zero = Some(Dimension::Px(0.0));
        let clip = Clip::Rect {
            top: zero,
            right: zero,
            bottom: zero,
            left: zero,
        };
        let r = clip_legacy_retangulo(clip, CAIXA).unwrap();
        assert_eq!((r.w, r.h), (0.0, 0.0));
    }

    /// Um lado `auto` é a borda DESSA caixa, não a origem — `right: auto` vai
    /// até `caixa.w`, não até 0.
    #[test]
    fn clip_legado_auto_e_a_borda_da_propria_caixa() {
        use crate::style::values::Dimension;
        use crate::style::vocab::Clip;
        let clip = Clip::Rect {
            top: Some(Dimension::Px(5.0)),
            right: None,
            bottom: None,
            left: Some(Dimension::Px(5.0)),
        };
        let r = clip_legacy_retangulo(clip, CAIXA).unwrap();
        assert_eq!((r.x, r.y), (CAIXA.x + 5.0, CAIXA.y + 5.0));
        assert_eq!((r.w, r.h), (CAIXA.w - 5.0, CAIXA.h - 5.0));
    }

    /// `clip: auto` não recorta nada — é o valor inicial da propriedade.
    #[test]
    fn clip_legado_auto_geral_nao_recorta() {
        use crate::style::vocab::Clip;
        assert!(clip_legacy_retangulo(Clip::Auto, CAIXA).is_none());
    }
