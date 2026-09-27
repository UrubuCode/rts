//! A parsed selector back to text — what the inspector shows as a rule's
//! selector.
//!
//! # Why a serialiser and not the source text kept on the `Rule`
//!
//! Keeping the text would put one more string on every rule of every page —
//! Bootstrap alone has ~2 700 — for a question only the inspector asks. The
//! parsed form already holds everything the text said that the cascade uses,
//! so the text is rebuilt on demand and costs nothing to a program that never
//! opens DevTools. What it cannot give back is what the parser normalised away
//! (letter case of a tag, redundant whitespace, `odd` written as `2n+1`).

use super::{AttrOp, Combinator, ComplexSelector, PseudoClass, PseudoElement, SimpleSelector};

impl ComplexSelector {
    /// The selector as CSS text: `div > p.card::before`.
    pub fn to_css_text(&self) -> String {
        let mut out = String::new();
        for (i, compound) in self.compounds.iter().enumerate() {
            if i > 0 {
                out.push_str(match self.combinators.get(i - 1) {
                    Some(Combinator::Child) => " > ",
                    Some(Combinator::NextSibling) => " + ",
                    Some(Combinator::SubsequentSibling) => " ~ ",
                    _ => " ",
                });
            }
            if compound.parts.is_empty() {
                out.push('*');
            }
            for part in &compound.parts {
                simple(part, &mut out);
            }
        }
        out.push_str(match self.pseudo_element {
            Some(PseudoElement::Before) => "::before",
            Some(PseudoElement::After) => "::after",
            Some(PseudoElement::Marker) => "::marker",
            None => "",
        });
        out
    }

    /// Specificity as the `(a, b, c)` triple CSS and CDP both name — ids,
    /// classes/attributes/pseudo-classes, types.
    pub fn specificity_triple(&self) -> (u32, u32, u32) {
        let packed = self.specificity();
        ((packed >> 16) & 0xFF, (packed >> 8) & 0xFF, packed & 0xFF)
    }
}

fn simple(part: &SimpleSelector, out: &mut String) {
    match part {
        SimpleSelector::Tag(tag) => out.push_str(tag),
        SimpleSelector::Class(class) => {
            out.push('.');
            out.push_str(class);
        }
        SimpleSelector::Id(id) => {
            out.push('#');
            out.push_str(id);
        }
        SimpleSelector::Universal => out.push('*'),
        SimpleSelector::Attr { name, op, value } => {
            out.push('[');
            out.push_str(name);
            let op = match op {
                AttrOp::Exists => None,
                AttrOp::Equals => Some("="),
                AttrOp::Prefix => Some("^="),
                AttrOp::Suffix => Some("$="),
                AttrOp::Contains => Some("*="),
                AttrOp::Word => Some("~="),
                AttrOp::DashPrefix => Some("|="),
            };
            if let Some(op) = op {
                out.push_str(op);
                out.push('"');
                out.push_str(value);
                out.push('"');
            }
            out.push(']');
        }
        SimpleSelector::Pseudo(pseudo) => pseudo_class(pseudo, out),
    }
}

fn pseudo_class(pseudo: &PseudoClass, out: &mut String) {
    let plain = match pseudo {
        PseudoClass::FirstChild => ":first-child",
        PseudoClass::LastChild => ":last-child",
        PseudoClass::OnlyChild => ":only-child",
        PseudoClass::Empty => ":empty",
        PseudoClass::Root => ":root",
        PseudoClass::FirstOfType => ":first-of-type",
        PseudoClass::LastOfType => ":last-of-type",
        PseudoClass::OnlyOfType => ":only-of-type",
        PseudoClass::Checked => ":checked",
        PseudoClass::Disabled => ":disabled",
        PseudoClass::Enabled => ":enabled",
        PseudoClass::Required => ":required",
        PseudoClass::Hover => ":hover",
        PseudoClass::Focus => ":focus",
        PseudoClass::FocusWithin => ":focus-within",
        PseudoClass::FocusVisible => ":focus-visible",
        PseudoClass::Active => ":active",
        PseudoClass::Visited => ":visited",
        PseudoClass::Link => ":link",
        PseudoClass::ReadWrite => ":read-write",
        PseudoClass::ReadOnly => ":read-only",
        PseudoClass::Target => ":target",
        PseudoClass::Scope => ":scope",
        PseudoClass::Default => ":default",
        PseudoClass::PlaceholderShown => ":placeholder-shown",
        PseudoClass::NthChild(a, b) => return nth(":nth-child", *a, *b, out),
        PseudoClass::NthOfType(a, b) => return nth(":nth-of-type", *a, *b, out),
        PseudoClass::Lang(lang) => {
            out.push_str(":lang(");
            out.push_str(lang);
            out.push(')');
            return;
        }
        PseudoClass::Not(list) => return list_of(":not", list, out),
        PseudoClass::Is(list) => return list_of(":is", list, out),
        PseudoClass::Where(list) => return list_of(":where", list, out),
        PseudoClass::Has(list) => {
            out.push_str(":has(");
            for (i, (combinator, selector)) in list.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(match combinator {
                    Combinator::Child => "> ",
                    Combinator::NextSibling => "+ ",
                    Combinator::SubsequentSibling => "~ ",
                    _ => "",
                });
                out.push_str(&selector.to_css_text());
            }
            out.push(')');
            return;
        }
    };
    out.push_str(plain);
}

fn list_of(name: &str, list: &[ComplexSelector], out: &mut String) {
    out.push_str(name);
    out.push('(');
    let texts: Vec<String> = list.iter().map(ComplexSelector::to_css_text).collect();
    out.push_str(&texts.join(", "));
    out.push(')');
}

/// `An+B` in its shortest form: `3`, `2n`, `n+1`, `-n+3`, `2n-1`.
fn nth(name: &str, a: i32, b: i32, out: &mut String) {
    out.push_str(name);
    out.push('(');
    match a {
        0 => {}
        1 => out.push('n'),
        -1 => out.push_str("-n"),
        _ => out.push_str(&format!("{a}n")),
    }
    if a == 0 {
        out.push_str(&b.to_string());
    } else if b > 0 {
        out.push_str(&format!("+{b}"));
    } else if b < 0 {
        out.push_str(&b.to_string());
    }
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::super::parse_selector;

    /// A selector read back from its text parses to the same selector — the
    /// inspector shows text a user can paste into a stylesheet.
    #[test]
    fn a_selector_round_trips_through_its_text() {
        for source in [
            "div > p.card",
            "#main .item + li ~ span",
            "a[href^=\"http\"]:hover",
            "li:nth-child(2n+1)",
            "p::before",
            "ul :not(.x, .y)",
            "*",
        ] {
            let parsed = parse_selector(source).expect(source);
            let text = parsed.to_css_text();
            let again = parse_selector(&text).expect(&text);
            assert_eq!(parsed, again, "{source} -> {text}");
        }
    }

    #[test]
    fn specificity_reads_as_the_css_triple() {
        let parsed = parse_selector("#a .b c").unwrap();
        assert_eq!(parsed.specificity_triple(), (1, 1, 1));
    }
}
