//! O MATCH puro de um compound — o que navega a árvore vive no `Dom`
//!
//! Extraído de `selector.rs` sem alterar uma linha.

use super::*;

/// `true` se um COMPOUND (`p.card#x`) casa UM elemento dado tag/id/classes + um
/// resolvedor de atributo e de pseudo-classe estrutural (que o `Dom` fornece, pois
/// pseudos/`[attr]` dependem da posição/atributos do nó). Puro: não navega a árvore
/// (os combinadores são tratados fora, no `Dom`).
pub fn compound_matches(
    compound: &CompoundSelector,
    tag: &str,
    id: Option<&str>,
    classes: &[&str],
    attr: &impl Fn(&str) -> Option<String>,
    pseudo: &impl Fn(&PseudoClass) -> bool,
) -> bool {
    compound.parts.iter().all(|p| match p {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(t) => t == tag,
        SimpleSelector::Id(i) => id == Some(i.as_str()),
        SimpleSelector::Class(c) => classes.contains(&c.as_str()),
        SimpleSelector::Attr { name, op, value } => attr(name)
            .map(|v| attr_op_matches(*op, &v, value))
            .unwrap_or(false),
        SimpleSelector::Pseudo(pc) => pseudo(pc),
    })
}

/// Matcher usado pelo DOM no caminho quente: lê classes e atributos por empréstimo,
/// sem criar `Vec<&str>` ou `String` para cada candidato de regra.
pub fn compound_matches_borrowed<'a, F, P>(
    compound: &CompoundSelector,
    tag: &str,
    id: Option<&str>,
    class_attr: Option<&str>,
    attr: &F,
    pseudo: &P,
) -> bool
where
    F: Fn(&str) -> Option<&'a str>,
    P: Fn(&PseudoClass) -> bool,
{
    compound.parts.iter().all(|p| match p {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(t) => t == tag,
        SimpleSelector::Id(i) => id == Some(i.as_str()),
        SimpleSelector::Class(c) => {
            class_attr.is_some_and(|raw| raw.split_whitespace().any(|class| class == c))
        }
        SimpleSelector::Attr { name, op, value } => attr(name)
            .map(|actual| attr_op_matches(*op, actual, value))
            .unwrap_or(false),
        SimpleSelector::Pseudo(pc) => pseudo(pc),
    })
}

/// Aplica o operador de um seletor de atributo `[attr OP value]` ao valor real.
fn attr_op_matches(op: AttrOp, actual: &str, expected: &str) -> bool {
    match op {
        AttrOp::Exists => true, // a presença já foi checada (attr() devolveu Some)
        AttrOp::Equals => actual == expected,
        AttrOp::Prefix => !expected.is_empty() && actual.starts_with(expected),
        AttrOp::Suffix => !expected.is_empty() && actual.ends_with(expected),
        AttrOp::Contains => !expected.is_empty() && actual.contains(expected),
        AttrOp::Word => actual.split_whitespace().any(|w| w == expected),
        AttrOp::DashPrefix => actual == expected || actual.starts_with(&format!("{expected}-")),
    }
}
