//! Testes movidos de `inline_box.rs` na modularização; nenhuma linha foi
//! alterada. A indentação de 4 espaços é a do `mod` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use crate::table::tests::{geometria, rect};

    /// Um aglomerado SEM whitespace desce inteiro para a linha seguinte.
    ///
    /// `<i>y</i><b>z</b>` não tem espaço entre os dois: em CSS não há ali
    /// oportunidade de quebra nenhuma, e o browser move os dois juntos. Partir
    /// o aglomerado punha o `y` no fim de uma linha e o `z` no início da outra,
    /// e a caixa do pai — que é a UNIÃO dos fragmentos, e essa estava certa —
    /// passava a ser um retângulo com a largura da linha inteira.
    ///
    /// É a forma exata do pior desvio de largura da Wikipédia: as referências
    /// (`<sup class="mw-ref"><a><span>[1]</span></a></sup>`) saíam com 752x41
    /// onde o Chrome dá 21x15. O que estava errado era o sítio do corte, não a
    /// união.
    ///
    /// Medida com o `ApproxMeasurer`: 8px por carácter a 16px. "aaaaa" mede 40,
    /// o espaço 8, e cada letra 8 — o aglomerado `yz` pede 8+16=24 sobre os 40
    /// já postos, logo 64 numa caixa de 60: desce, e desce inteiro.
    #[test]
    fn aglomerado_sem_espacos_desce_inteiro_para_a_linha_seguinte() {
        let (dom, list) = geometria(
            "<p style='width:60px'>aaaaa <sup><i>y</i><b>z</b></sup></p>",
            800.0,
        );
        let i = rect(&dom, &list, "i", 0);
        let b = rect(&dom, &list, "b", 0);
        let sup = rect(&dom, &list, "sup", 0);
        assert_eq!(i.y, b.y, "o aglomerado não é partido: i={i:?} b={b:?}");
        assert!(
            i.x < b.x,
            "e mantém a ordem na mesma linha: i={i:?} b={b:?}"
        );
        assert!(
            sup.w < 30.0,
            "a caixa do pai é a do aglomerado, não a da linha: {sup:?}"
        );
    }
