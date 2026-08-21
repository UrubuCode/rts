//! O keyword `inherit` — `background-color: inherit`, `color: inherit`.
//!
//! É um valor que TODA propriedade aceita e que não se parece com nenhum outro:
//! não descreve uma cor nem um comprimento, diz "seja o que for que o pai
//! computou". Por isso não cabe no parse de valor de cada propriedade — quando o
//! parse corre, o pai ainda não foi computado.
//!
//! ## Como é representado
//!
//! A declaração não escreve valor nenhum: guarda o NOME da propriedade numa
//! lista (`ComputedStyle::inherit_props`), e a passada de herança — que já
//! existe e já tem o pai na mão — copia o campo de lá. Uma lista de nomes, e não
//! um valor-sentinela por campo, porque o sentinela teria de existir em cada um
//! dos ~70 tipos da tabela e sobreviver a todo `merge_over`.
//!
//! ## Porque importa mais do que o corpus mostra
//!
//! Uma fixture falhava por isto. A folha real da Wikipédia declara `inherit` **43
//! vezes** (14 delas `color: inherit`), e até aqui a declaração era descartada em
//! silêncio — o que não é o mesmo que não existir: um `a { color: blue }` seguido
//! de `.nav a { color: inherit }` ficava azul onde o browser dá a cor do pai.
//!
//! ⚠️ NÃO cobre `initial`, `unset` e `revert`, que são os outros três keywords
//! largos do CSS (5 usos na mesma folha). `initial` precisa da tabela de valores
//! iniciais TIPADA — a de `style::initial` é de strings, para serializar — e
//! `revert` precisa de saber de que camada da cascade veio cada declaração, que
//! o nosso modelo não guarda. Ficam por fazer, ditos.

use super::props::ComputedStyle;

/// Copia de `src` para `dst` o campo da propriedade `name`. É o único sítio que
/// mapeia nome CSS → campo para COPIAR (o parse mapeia nome → valor parseado), e
/// cobre as propriedades que o modelo tem: uma que não esteja aqui deixa o campo
/// como está, que é o comportamento anterior a este módulo.
pub fn copy_property(dst: &mut ComputedStyle, src: &ComputedStyle, name: &str) {
    match name {
        "color" => dst.color = src.color,
        "background-color" | "background" => {
            dst.bg = src.bg;
            dst.gradient = src.gradient;
        }
        // O SHORTHAND, e por isso cinco campos numa entrada só: `font` controla
        // `font-style`, `font-variant`, `font-weight`, `font-stretch`,
        // `font-size`, `line-height` e `font-family`, e um `font: inherit` que
        // herdasse só o tamanho seria meia correção com cara de correção. Das
        // sete, `variant` e `stretch` não existem no modelo — quando existirem,
        // acrescentam-se aqui, e é esta linha que diz onde.
        //
        // Um uso na folha da Wikipédia, e atinge 51 elementos: a regra é
        // `.mw-heading h1,…,h6{…font:inherit}`, e sem ela cada cabeçalho
        // MULTIPLICA o tamanho do pai. A classe `mw-heading3` está no `<div>`,
        // que fica com 117% de 16 = 18,72; o `<h3>` que herda isso mede 18,72, e
        // o que NÃO herda aplica os seus 117% *sobre* os 18,72 e chega a 21,90.
        // É essa multiplicação que o `font:inherit` existe para cortar.
        "font" => {
            dst.font_size = src.font_size;
            dst.font_family = src.font_family.clone();
            dst.bold = src.bold;
            dst.italic = src.italic;
            dst.line_height = src.line_height;
        }
        "font-size" => dst.font_size = src.font_size,
        "font-family" => dst.font_family = src.font_family.clone(),
        "font-weight" => dst.bold = src.bold,
        "font-style" => dst.italic = src.italic,
        "line-height" => dst.line_height = src.line_height,
        "letter-spacing" => dst.letter_spacing = src.letter_spacing,
        "text-align" => dst.text_align = src.text_align,
        "text-transform" => dst.text_transform = src.text_transform,
        "text-decoration" | "text-decoration-line" => dst.text_decoration = src.text_decoration,
        "text-indent" => dst.text_indent = src.text_indent,
        "white-space" => dst.white_space = src.white_space,
        "word-break" => dst.word_break = src.word_break,
        "overflow-wrap" | "word-wrap" => dst.overflow_wrap = src.overflow_wrap,
        "direction" => dst.direction = src.direction,
        "visibility" => dst.visibility = src.visibility,
        "cursor" => dst.cursor = src.cursor.clone(),
        "list-style-type" => dst.list_style_type = src.list_style_type,
        "list-style-image" => dst.list_style_image = src.list_style_image.clone(),
        "display" => dst.display = src.display,
        "border-color" => dst.border_color = src.border_color,
        "border-width" => dst.border_width = src.border_width,
        "border-style" => dst.border_style = src.border_style,
        "opacity" => dst.opacity = src.opacity,
        "width" => dst.width = src.width,
        "height" => dst.height = src.height,
        // Uma propriedade fora desta lista não é um erro: `inherit` nela fica sem
        // efeito, exatamente como antes de este módulo existir. Acrescentar uma
        // linha aqui é o que a faz passar a funcionar.
        _ => {}
    }
}

impl ComputedStyle {
    /// Aplica as declarações `inherit` deste nó contra o estilo já computado do
    /// PAI. Chamado pela passada de herança, depois de ela copiar o que herda por
    /// omissão — a ordem importa: `inherit` é explícito e vence.
    pub(crate) fn apply_inherit_keyword(&mut self, parent: &ComputedStyle) {
        let Some(nomes) = self.inherit_props.clone() else {
            return;
        };
        for nome in nomes.iter() {
            copy_property(self, parent, nome);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::copy_property;
    use crate::style::parse::parse_inline;

    /// `font: inherit` copia os CINCO longhands que o shorthand controla, e não
    /// só o tamanho.
    ///
    /// Um por asserção porque o defeito que isto fecha é exatamente o de herdar
    /// um e esquecer os outros: `copy_property` não conhecia `font` de todo, o
    /// `_ => {}` engolia-o, e o cabeçalho ficava com o tamanho multiplicado.
    #[test]
    fn font_inherit_copia_os_cinco_longhands_do_shorthand() {
        let pai = parse_inline(
            "font-size:18.72px;font-family:Georgia;font-weight:bold;\
             font-style:italic;line-height:2",
        );
        let mut filho = parse_inline("font:inherit");
        assert!(filho.font_size.is_none(), "o parse não escreve valor nenhum");
        copy_property(&mut filho, &pai, "font");
        assert_eq!(filho.font_size, pai.font_size, "font-size");
        assert_eq!(filho.font_family, pai.font_family, "font-family");
        assert_eq!(filho.bold, pai.bold, "font-weight");
        assert_eq!(filho.italic, pai.italic, "font-style");
        assert_eq!(filho.line_height, pai.line_height, "line-height");
    }

    /// E o `font-size:inherit` continua a copiar SÓ o tamanho: o shorthand não
    /// pode alargar o longhand.
    #[test]
    fn font_size_inherit_nao_arrasta_a_familia_junto() {
        let pai = parse_inline("font-size:18.72px;font-family:Georgia");
        let mut filho = parse_inline("font-size:inherit");
        copy_property(&mut filho, &pai, "font-size");
        assert_eq!(filho.font_size, pai.font_size);
        assert!(
            filho.font_family.is_none(),
            "o longhand herdou o que não é dele: {:?}",
            filho.font_family
        );
    }

    /// Uma propriedade que a lista NÃO conhece continua sem efeito, em vez de
    /// falhar: é dívida documentada e o teste diz que é deliberada.
    #[test]
    fn uma_propriedade_fora_da_lista_fica_sem_efeito() {
        let pai = parse_inline("padding:8px");
        let mut filho = parse_inline("padding:inherit");
        copy_property(&mut filho, &pai, "padding");
        assert!(
            !filho.padding.any_set(),
            "o `padding:inherit` passou a funcionar sem ninguém o implementar"
        );
    }
}
