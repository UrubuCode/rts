//! As propriedades de TABELA e a posição do marcador de lista: `border-collapse`,
//! `border-spacing`, `table-layout` e `list-style-position`.
//!
//! Vivem juntas num ficheiro só porque partilham o consumidor — [`crate::table`]
//! e [`crate::listitem`] — e nenhuma delas é lida por mais nada. A alternativa
//! era espalhá-las por `values.rs` (que já tem os tipos de toda a gente) e por
//! `text.rs`; ficariam ao lado de propriedades com que não têm relação nenhuma.
//!
//! **`border-collapse` não é cosmética.** É ela que decide se há
//! `border-spacing` entre cada duas colunas ou zero, e 2px por coluna acumulam:
//! numa régua com tolerância de 1px, uma tabela `collapse` medida como
//! `separate` conta como errada em todas as suas células mesmo com o algoritmo
//! de colunas certo. Foi esse o custo medido que fez estas propriedades
//! existirem.

use crate::style::Dimension;

/// `border-collapse` — se as bordas das células adjacentes se fundem
/// (<https://developer.mozilla.org/en-US/docs/Web/CSS/border-collapse>).
///
/// `Separate` é o default do CSS; `Collapse` é o que quase toda a folha real
/// declara, e o que anula o `border-spacing`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BorderCollapse {
    #[default]
    Separate,
    Collapse,
}

impl BorderCollapse {
    pub fn parse(v: &str) -> Option<BorderCollapse> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "separate" => BorderCollapse::Separate,
            "collapse" => BorderCollapse::Collapse,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            BorderCollapse::Separate => "separate",
            BorderCollapse::Collapse => "collapse",
        }
    }
}

/// `table-layout` — se as larguras das colunas vêm do CONTEÚDO (`auto`) ou só da
/// primeira linha (`fixed`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TableLayout {
    #[default]
    Auto,
    Fixed,
}

impl TableLayout {
    pub fn parse(v: &str) -> Option<TableLayout> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "auto" => TableLayout::Auto,
            "fixed" => TableLayout::Fixed,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            TableLayout::Auto => "auto",
            TableLayout::Fixed => "fixed",
        }
    }
}

/// `border-spacing` — o vão entre células, horizontal e vertical.
///
/// Um struct e não dois campos separados porque o CSS os declara juntos numa
/// propriedade só (`border-spacing: 2px 4px`) e a forma de um valor tem de casar
/// com a forma da declaração: dois campos independentes deixariam representável
/// o estado "só o vertical foi declarado", que a sintaxe não produz.
///
/// HERDÁVEL — a spec põe-na na `<table>` e é lida lá, mas herda como qualquer
/// propriedade de tabela.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BorderSpacing {
    pub h: Dimension,
    pub v: Dimension,
}

impl Default for BorderSpacing {
    fn default() -> Self {
        // 2px é o valor da folha de estilo do browser para uma tabela
        // `separate` — não é zero, e é a diferença que se vê ao lado do Chrome.
        BorderSpacing {
            h: Dimension::Px(2.0),
            v: Dimension::Px(2.0),
        }
    }
}

impl BorderSpacing {
    /// `border-spacing: <len>` (os dois eixos) ou `<len> <len>` (h e depois v).
    pub fn parse(v: &str) -> Option<BorderSpacing> {
        let mut it = v.split_whitespace();
        let h = crate::style::lengths::parse_dimension(it.next()?)?;
        let v = match it.next() {
            Some(s) => crate::style::lengths::parse_dimension(s)?,
            None => h,
        };
        Some(BorderSpacing { h, v })
    }
}

/// `list-style-position` — se o marcador fica FORA da caixa de conteúdo do item
/// (`outside`, o default) ou dentro dela, como primeira coisa da linha.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
}

impl ListStylePosition {
    pub fn parse(v: &str) -> Option<ListStylePosition> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "outside" => ListStylePosition::Outside,
            "inside" => ListStylePosition::Inside,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            ListStylePosition::Outside => "outside",
            ListStylePosition::Inside => "inside",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_spacing_de_um_valor_vale_para_os_dois_eixos() {
        let s = BorderSpacing::parse("4px").unwrap();
        assert_eq!(s.h, Dimension::Px(4.0));
        assert_eq!(s.v, Dimension::Px(4.0));
    }

    #[test]
    fn border_spacing_de_dois_valores_e_horizontal_e_depois_vertical() {
        let s = BorderSpacing::parse("2px 8px").unwrap();
        assert_eq!(s.h, Dimension::Px(2.0));
        assert_eq!(s.v, Dimension::Px(8.0));
    }
}
