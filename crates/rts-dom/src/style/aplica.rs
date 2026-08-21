//! Escrever no `ComputedStyle` — e a regra de quando NÃO escrever.
//!
//! ## A regra
//!
//! Uma declaração cujo valor não parseia **é deitada fora**, e o que estava
//! declarado antes fica a valer (CSS Syntax 3 §5.4). O Blink fá-lo no parser:
//! `CSSParserImpl::ConsumeDeclaration` descarta-a e ela nunca chega ao
//! `CSSPropertyValueSet`, portanto não há nada para escrever por cima.
//!
//! O nosso dispatch é um `match` de nome→campo em que cada braço faz
//! `css.campo = parse(val)`, e um `parse` que falha responde `None`. Atribuir
//! esse `None` **apaga a declaração anterior** — o contrário exato do que
//! descartar significa.
//!
//! ## Porque isto importa mais do que parece
//!
//! O CSS moderno escreve-se em escadas de progressive enhancement:
//!
//! ```css
//! color: #1a73e8;
//! color: color-mix(in oklch, var(--a), var(--b));
//! ```
//!
//! O primeiro degrau é para quem não sabe o segundo. Sem esta guarda, cada
//! escada dessas apaga precisamente o degrau que sabemos pintar, e a página fica
//! **pior** do que se a linha nova não existisse. Medido antes de haver guarda
//! nenhuma: `color:red; color:notacolor` respondia preto, `width:100px;
//! width:-5px` respondia não-declarado, `display:flex; display:bogus` perdia o
//! flex.
//!
//! ## Porque é aqui e não em cada braço
//!
//! Os braços são ~136, em seis ficheiros, dois dos quais já estão acima do teto
//! de linhas do repositório. `set_if(&mut css.campo, parse(val))` **substitui** a
//! linha em vez de acrescentar uma, portanto a regra entra sem fazer crescer
//! ficheiro nenhum — e fica escrita num sítio só, que é o que impede a próxima
//! propriedade de nascer sem ela.

use super::values::{Edges, Side};

/// Escreve `novo` em `dst` **só se o parse deu resultado**.
///
/// Um `None` é o parse a dizer "não reconheci este valor", e não o autor a dizer
/// "sem valor" — as duas coisas são indistinguíveis no tipo, e é por isso que a
/// distinção tem de estar em quem escreve.
pub(crate) fn set_if<T>(dst: &mut Option<T>, novo: Option<T>) {
    if novo.is_some() {
        *dst = novo;
    }
}

/// Para os parsers em que `None` significa DUAS coisas.
///
/// `BoxShadow::parse`, `Transform::parse`, `parse_text_shadow` e
/// `GridTrack::parse_list` respondem `None` tanto ao valor que não reconhecem
/// como ao `none` explícito — e `none` ali é uma declaração VÁLIDA cujo efeito é
/// limpar. Um `set_if` cego trataria as duas como recusa e um
/// `box-shadow: 0 0 5px red; box-shadow: none` deixava a sombra de pé.
///
/// Isto é o mesmo defeito que o módulo fecha, virado do avesso: lá o `None` de
/// recusa era escrito como se fosse valor; aqui o `None` de valor seria
/// descartado como se fosse recusa. Meia correção seria pior que o defeito, e a
/// suíte não apanhava — passou verde com o `box-shadow: none` já partido.
///
/// Não é uma regra geral sobre a palavra `none`: aplica-se APENAS aos arms cujo
/// parser mistura os dois sentidos. Onde `none` é um valor a sério
/// (`display:none`, `float:none`) o parser responde `Some` e o `set_if` basta.
pub(crate) fn set_ou_limpa<T>(dst: &mut Option<T>, val: &str, novo: Option<T>) {
    if novo.is_some() {
        *dst = novo;
    } else if val.trim().eq_ignore_ascii_case("none") {
        *dst = None;
    }
}

/// O mesmo, para um lado de margin/padding.
///
/// [`Side`] não é um `Option`: o "não declarado" dele é a variante `Unset`, que
/// é também o que `parse_side` responde ao que recusa. Daí uma função própria em
/// vez de um `set_if` genérico.
///
/// Entrou antes das outras por necessidade e não por escolha: ao passar o
/// `padding` negativo a ser RECUSADO, um `padding-left:8px; padding-left:-4px`
/// ficava sem padding nenhum. A correção do sinal teria criado o defeito que
/// este módulo existe para fechar.
pub(crate) fn set_side(dst: &mut Side, novo: Side) {
    if novo != Side::Unset {
        *dst = novo;
    }
}

/// O shorthand de caixa (`margin: 8px 4px`), e a regra dele é MAIS FORTE que a
/// dos outros dois: um shorthand com **um** componente inválido é inválido por
/// inteiro (CSS Cascade 5 §3.2), e não "válido nos lados que deram".
///
/// Importa porque um shorthand escreve os QUATRO lados. `padding:8px;
/// padding:-4px` não perderia só um lado — o segundo `parse_edges` devolve
/// quatro `Unset` e apagava a caixa toda. Por isso a decisão é tomada em
/// [`super::lengths::parse_edges`], que responde `None` quando qualquer token
/// cai, e aqui só se escreve o que veio inteiro.
pub(crate) fn set_edges(dst: &mut Edges, novo: Option<Edges>) {
    if let Some(e) = novo {
        *dst = e;
    }
}

#[cfg(test)]
mod tests {
    use crate::style::parse::parse_inline;
    use crate::style::values::{Dimension, Side};

    /// O defeito, na forma em que foi medido: a declaração inválida escrevia o
    /// `None` do parse por cima da válida que veio antes.
    ///
    /// Três propriedades de três tipos diferentes, porque o defeito era do
    /// DISPATCH e não de um parser: uma cor, um comprimento e um keyword.
    #[test]
    fn declaracao_invalida_nao_apaga_a_anterior() {
        let vermelho = parse_inline("color:red").color;
        assert!(vermelho.is_some(), "a base do teste tem de parsear");
        assert_eq!(parse_inline("color:red;color:notacolor").color, vermelho);

        assert_eq!(
            parse_inline("width:100px;width:-5px").width,
            Some(Dimension::Px(100.0))
        );
        assert_eq!(
            parse_inline("display:flex;display:bogus").display,
            parse_inline("display:flex").display
        );
    }

    /// E a metade sem a qual isto seria uma correção pior que o defeito: uma
    /// declaração VÁLIDA continua a vencer a anterior.
    #[test]
    fn declaracao_valida_continua_a_vencer_a_anterior() {
        assert_eq!(
            parse_inline("width:100px;width:50px").width,
            Some(Dimension::Px(50.0))
        );
        assert_eq!(
            parse_inline("color:red;color:blue").color,
            parse_inline("color:blue").color
        );
    }

    /// Um shorthand com UM componente inválido é inválido por INTEIRO, e não
    /// válido nos lados que deram.
    ///
    /// Importa mais que o longhand porque o shorthand escreve os quatro lados de
    /// uma vez: aplicar os que deram apagaria os outros três.
    #[test]
    fn shorthand_de_caixa_com_um_componente_invalido_cai_inteiro() {
        assert_eq!(
            parse_inline("margin:8px 4px;margin:8px lixo").margin.top,
            Side::Len(Dimension::Px(8.0))
        );
        assert_eq!(
            parse_inline("margin:8px 4px;margin:8px lixo").margin.right,
            Side::Len(Dimension::Px(4.0)),
            "o lado que o segundo shorthand NAO tocaria também tem de sobreviver"
        );
        assert_eq!(
            parse_inline("padding:8px;padding:-4px").padding.left,
            Side::Len(Dimension::Px(8.0))
        );
    }

    /// O defeito virado do avesso, e o que a suíte deixou passar verde: onde
    /// `none` é um valor VÁLIDO cujo efeito é limpar, ele tem de continuar a
    /// limpar.
    ///
    /// `BoxShadow::parse("none")` e `Transform::parse("none")` respondem `None`
    /// tal como respondem a lixo — um `set_if` cego deixava a sombra de pé.
    #[test]
    fn none_explicito_continua_a_limpar_onde_e_um_valor() {
        assert!(
            parse_inline("box-shadow:0 0 5px red").box_shadow.is_some(),
            "a base do teste tem de parsear"
        );
        assert!(
            parse_inline("box-shadow:0 0 5px red;box-shadow:none")
                .box_shadow
                .is_none(),
            "`none` é uma declaração válida e o efeito dela é limpar"
        );
        assert!(
            parse_inline("transform:scale(2);transform:none")
                .transform
                .is_none()
        );
    }

    /// E o lixo, nessas mesmas propriedades, continua a NÃO limpar — que é o que
    /// distingue as duas metades do teste anterior.
    #[test]
    fn lixo_nao_limpa_o_que_o_none_limpa() {
        assert!(
            parse_inline("box-shadow:0 0 5px red;box-shadow:xpto")
                .box_shadow
                .is_some(),
            "um valor não reconhecido é descartado, não é um `none`"
        );
    }
}
