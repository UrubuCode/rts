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
//! `initial`/`unset` estão em `style::parse::apply_css_wide_keyword`, que usa a
//! tabela de valores iniciais TIPADA (`style::initial`; a de `style::initial` é
//! de strings, para serializar). `revert`/`revert-layer` (lote J,
//! `style::stylesheet::revert`) precisam de saber de que ORIGEM/LAYER veio cada
//! declaração candidata — informação que só a lista de regras casadas tem, não
//! um `ComputedStyle` isolado — por isso ficam marcados aqui (`revert_props`/
//! `revert_layer_props`, o mesmo padrão de `inherit_props`) e resolvidos lá.

use super::props::ComputedStyle;

/// Remove o marcador de `inherit` de uma propriedade quando uma declaração
/// posterior da mesma camada fornece outro valor. Sem isto, `color: inherit;
/// color: red` seria novamente sobrescrito pelo pai na passada de herança.
pub(crate) fn clear_inherit_marker(dst: &mut ComputedStyle, name: &str) {
    let Some(existing) = dst.inherit_props.take() else {
        return;
    };
    let remaining: Vec<String> = existing
        .iter()
        .filter(|property| property.as_str() != name)
        .cloned()
        .collect();
    dst.inherit_props = (!remaining.is_empty()).then(|| std::sync::Arc::new(remaining));
}

/// Marca uma propriedade que foi explicitamente definida como `initial`.
pub(crate) fn mark_initial_property(dst: &mut ComputedStyle, name: &str) {
    let mut names = dst.initial_props.as_deref().cloned().unwrap_or_default();
    if !names.iter().any(|property| property == name) {
        names.push(name.to_string());
    }
    dst.initial_props = Some(std::sync::Arc::new(names));
}

pub(crate) fn clear_initial_marker(dst: &mut ComputedStyle, name: &str) {
    let Some(existing) = dst.initial_props.take() else {
        return;
    };
    let remaining: Vec<String> = existing
        .iter()
        .filter(|property| property.as_str() != name)
        .cloned()
        .collect();
    dst.initial_props = (!remaining.is_empty()).then(|| std::sync::Arc::new(remaining));
}

/// Remove o marcador de `inherit` quando um campo computado posterior vence a
/// mesma propriedade na cascade.
pub(crate) fn clear_inherit_for_field(dst: &mut ComputedStyle, field: &str) {
    let names: &[&str] = match field {
        "color" => &["color"],
        "font_size" => &["font-size", "font"],
        "bold" => &["font-weight", "font"],
        "italic" => &["font-style", "font"],
        "line_height" => &["line-height", "font"],
        "font_family" => &["font-family", "font"],
        "text_align" => &["text-align"],
        "white_space" => &["white-space"],
        "text_transform" => &["text-transform"],
        "letter_spacing" => &["letter-spacing"],
        "text_decoration" => &["text-decoration", "text-decoration-line"],
        "font_stretch" => &["font-stretch", "font"],
        "word_spacing" => &["word-spacing"],
        "visibility" => &["visibility"],
        "tab_size" => &["tab-size"],
        "line_break" => &["line-break"],
        "text_decoration_skip_ink" => &["text-decoration-skip-ink"],
        "caret_color" => &["caret-color"],
        "text_wrap" => &["text-wrap"],
        "hyphens" => &["hyphens"],
        "direction" => &["direction"],
        "word_break" => &["word-break"],
        "overflow_wrap" => &["overflow-wrap", "word-wrap"],
        "text_indent" => &["text-indent"],
        "list_style_type" => &["list-style-type", "list-style"],
        "list_style_position" => &["list-style-position", "list-style"],
        "pointer_events" => &["pointer-events"],
        _ => &[],
    };
    for name in names {
        clear_inherit_marker(dst, name);
    }
}

/// Remove o marcador de `initial` quando um campo computado posterior vence a
/// mesma propriedade na cascade.
pub(crate) fn clear_initial_for_field(dst: &mut ComputedStyle, field: &str) {
    let names: &[&str] = match field {
        "color" => &["color"],
        "font_size" => &["font-size", "font"],
        "bold" => &["font-weight", "font"],
        "italic" => &["font-style", "font"],
        "line_height" => &["line-height", "font"],
        "font_family" => &["font-family", "font"],
        "text_align" => &["text-align"],
        "white_space" => &["white-space"],
        "text_transform" => &["text-transform"],
        "letter_spacing" => &["letter-spacing"],
        "text_decoration" => &["text-decoration", "text-decoration-line"],
        "font_stretch" => &["font-stretch", "font"],
        "word_spacing" => &["word-spacing"],
        "visibility" => &["visibility"],
        "tab_size" => &["tab-size"],
        "line_break" => &["line-break"],
        "text_decoration_skip_ink" => &["text-decoration-skip-ink"],
        "caret_color" => &["caret-color"],
        "text_wrap" => &["text-wrap"],
        "hyphens" => &["hyphens"],
        "direction" => &["direction"],
        "word_break" => &["word-break"],
        "overflow_wrap" => &["overflow-wrap", "word-wrap"],
        "text_indent" => &["text-indent"],
        "list_style_type" => &["list-style-type", "list-style"],
        "list_style_position" => &["list-style-position", "list-style"],
        "pointer_events" => &["pointer-events"],
        _ => &[],
    };
    for name in names {
        clear_initial_marker(dst, name);
    }
}

/// Reaplica `initial` depois da herança genérica. Alguns iniciais são `None`
/// no modelo (por exemplo a família de fonte), logo só o marcador consegue
/// impedir a cópia silenciosa do pai.
pub(crate) fn apply_initial_keywords(dst: &mut ComputedStyle) {
    let Some(names) = dst.initial_props.clone() else {
        return;
    };
    for name in names.iter() {
        let _ = crate::style::parse::apply_css_wide_keyword(dst, name, "initial");
    }
}

/// No elemento raiz, `inherit`/`unset` de uma propriedade herdável resolve para
/// o inicial porque não existe elemento pai. A operação remove o marcador antigo
/// antes de aplicar o inicial, inclusive quando o campo já tinha um valor de uma
/// declaração anterior no mesmo snapshot.
pub(crate) fn apply_root_inherit_as_initial(dst: &mut ComputedStyle) {
    let Some(names) = dst.inherit_props.clone() else {
        return;
    };
    for name in names.iter() {
        let _ = crate::style::parse::apply_css_wide_keyword(dst, name, "initial");
    }
}

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

/// `true` se `style` tem a propriedade nomeada `name` DECLARADA (o campo que
/// `copy_property` copiaria não é o "vazio" de um `ComputedStyle::default()`).
///
/// Não é uma segunda lista de nomes: aplica `copy_property` contra um alvo em
/// branco e compara o resultado com o vazio — reusa a MESMA tabela em vez de a
/// repetir invertida. Usado só pelo `revert`/`revert-layer` do lote J
/// (`style::stylesheet::revert`), para decidir se uma regra candidata a
/// "origem/layer anterior" realmente TOCA a propriedade antes de a copiar —
/// caminho frio, corre só quando alguma declaração da folha usa um dos dois
/// keywords.
pub(crate) fn property_is_set(style: &ComputedStyle, name: &str) -> bool {
    let mut probe = ComputedStyle::default();
    copy_property(&mut probe, style, name);
    probe != ComputedStyle::default()
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

    #[test]
    fn css_wide_initial_e_unset_resolvem_na_passada_de_heranca() {
        let pai = parse_inline("color:blue;width:100px");
        let mut filho = parse_inline("color:unset;width:unset");
        assert_eq!(filho.color, None);
        assert_eq!(filho.width, Some(crate::style::Dimension::Auto));
        filho.inherit_from(&pai);
        assert_eq!(filho.color, pai.color, "unset herda propriedade herdável");
        assert_eq!(filho.width, Some(crate::style::Dimension::Auto));

        let mut inicial = parse_inline("color:initial;width:initial");
        inicial.inherit_from(&pai);
        assert_eq!(inicial.color, Some(0x000000ff), "initial não herda color");
        assert_eq!(inicial.width, Some(crate::style::Dimension::Auto));

        let parent_font = parse_inline("font-family:Arial");
        let mut initial_font = parse_inline("font-family:initial");
        initial_font.inherit_from(&parent_font);
        assert_eq!(
            initial_font.font_family, None,
            "initial não deve herdar a família do pai"
        );
    }

    #[test]
    fn declaracao_posterior_remove_marcador_de_inherit() {
        let pai = parse_inline("color:blue");
        let mut filho = parse_inline("color:inherit;color:red");
        filho.inherit_from(&pai);
        assert_eq!(filho.color, Some(0xff0000ff));
    }
}
