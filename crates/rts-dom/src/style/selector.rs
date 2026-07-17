//! SELETORES CSS: simples, compostos (`p.card#x`), combinadores (`div > p`,
//! `+`, `~`), atributo (`[a=v]` e operadores) e pseudo-classes ESTRUTURAIS.
//! O matching que precisa da ÁRVORE (combinadores/pseudo por posição) vive no
//! `Dom` (`matches_complex`); aqui fica o parse + o match puro de um compound.

/// Um seletor SIMPLES atômico — um único teste sobre UM elemento. Vários simples no
/// mesmo elemento formam um [`CompoundSelector`] (`p.card#x`). Egui-free.
#[derive(Clone, PartialEq, Debug)]
pub enum SimpleSelector {
    /// `p`, `div` — casa pela tag (minúsculas). Especificidade 1.
    Tag(String),
    /// `.card` — casa se a classe está no `class=""`. Especificidade 10.
    Class(String),
    /// `#header` — casa pelo `id`. Especificidade 100.
    Id(String),
    /// `*` — casa qualquer elemento. Especificidade 0.
    Universal,
    /// `[attr]` / `[attr=v]` / `[attr^=v]` / `[attr$=v]` / `[attr*=v]` / `[attr~=v]`
    /// / `[attr|=v]`. Especificidade 10 (como classe).
    Attr { name: String, op: AttrOp, value: String },
    /// Pseudo-classe ESTRUTURAL (`:first-child`/`:last-child`/`:only-child`/
    /// `:empty`/`:root`/`:nth-child(...)`). Sem estado (sem `:hover`). Especif. 10.
    Pseudo(PseudoClass),
}

/// O operador de um seletor de atributo `[attr OP value]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttrOp {
    /// `[attr]` — só presença.
    Exists,
    /// `[attr=v]` — igual exato.
    Equals,
    /// `[attr^=v]` — começa com.
    Prefix,
    /// `[attr$=v]` — termina com.
    Suffix,
    /// `[attr*=v]` — contém substring.
    Contains,
    /// `[attr~=v]` — v é uma das palavras (lista separada por espaço).
    Word,
    /// `[attr|=v]` — igual a v OU começa com `v-` (lang).
    DashPrefix,
}

/// Uma pseudo-classe estrutural (resolvida pela POSIÇÃO na árvore, sem estado).
#[derive(Clone, PartialEq, Debug)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    /// `:nth-child(an+b)` — guarda (a, b). `odd`=2n+1, `even`=2n.
    NthChild(i32, i32),
    // Pseudo-classes de "estado" que num DOM viram presença de ATRIBUTO (não há UI
    // viva headless): mapeiam direto para o atributo correspondente.
    /// `:checked` — `checked`/`selected` presente.
    Checked,
    /// `:disabled` — `disabled` presente.
    Disabled,
    /// `:enabled` — elemento de form SEM `disabled`.
    Enabled,
    /// `:required` — `required` presente.
    Required,
    /// `:hover` — ESTADO VIVO: casa se o elemento é o nó sob o cursor OU um
    /// ancestral dele (spec: hover propaga pelos ancestrais). O nó hovered vem
    /// do backend (hit-test do mouse) via `Dom::set_hovered`; headless casa
    /// nunca (hovered = nenhum). Especificidade 10 (classe), como no browser.
    Hover,
}

/// O combinador ENTRE dois compounds numa cadeia (`A > B`): a relação de B com A.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Combinator {
    /// `A B` (espaço) — B é descendente de A.
    Descendant,
    /// `A > B` — B é filho DIRETO de A.
    Child,
    /// `A + B` — B é o irmão imediatamente após A.
    NextSibling,
    /// `A ~ B` — B é um irmão posterior a A.
    SubsequentSibling,
}

/// Um seletor COMPOSTO — vários simples no MESMO elemento (`p.card#x` = tag p +
/// classe card + id x, todos no mesmo nó). Vazio nunca (ao menos 1 simples).
#[derive(Clone, PartialEq, Debug)]
pub struct CompoundSelector {
    pub parts: Vec<SimpleSelector>,
}

/// O seletor de uma regra: uma sequência de compounds ligados por combinadores
/// (`div > p.card a` = 3 compounds). O ÚLTIMO compound é o ALVO (o elemento que a
/// regra estiliza); os anteriores são contexto a casar subindo/lateralmente na
/// árvore. `Selector` é o alias usado pelo resto do crate.
pub type Selector = ComplexSelector;

#[derive(Clone, PartialEq, Debug)]
pub struct ComplexSelector {
    /// Os compounds em ordem de documento (esquerda→direita). O último é o alvo.
    pub compounds: Vec<CompoundSelector>,
    /// Os combinadores ENTRE os compounds: `combinators[i]` liga `compounds[i]` a
    /// `compounds[i+1]`. Tamanho = `compounds.len() - 1`.
    pub combinators: Vec<Combinator>,
}

impl SimpleSelector {
    fn specificity(&self) -> u32 {
        match self {
            SimpleSelector::Id(_) => 100,
            SimpleSelector::Class(_) | SimpleSelector::Attr { .. } | SimpleSelector::Pseudo(_) => 10,
            SimpleSelector::Tag(_) => 1,
            SimpleSelector::Universal => 0,
        }
    }

    /// Parseia UM simples a partir do início de `s`, devolvendo (simples, resto).
    /// `None` se não reconhece. Usado em loop pelo parser de compound.
    fn parse_one(s: &str) -> Option<(SimpleSelector, &str)> {
        let s = s.trim_start();
        if s.is_empty() {
            return None;
        }
        let first = s.chars().next()?;
        match first {
            '*' => Some((SimpleSelector::Universal, &s[1..])),
            '.' => {
                let (ident, rest) = take_ident(&s[1..]);
                (!ident.is_empty()).then(|| (SimpleSelector::Class(ident.to_string()), rest))
            }
            '#' => {
                let (ident, rest) = take_ident(&s[1..]);
                (!ident.is_empty()).then(|| (SimpleSelector::Id(ident.to_string()), rest))
            }
            '[' => parse_attr_selector(s),
            ':' => parse_pseudo_selector(s),
            c if c.is_ascii_alphabetic() => {
                let (ident, rest) = take_ident(s);
                Some((SimpleSelector::Tag(ident.to_ascii_lowercase()), rest))
            }
            _ => None,
        }
    }
}

/// Parseia um seletor CSS completo (compostos + combinadores + atributo + pseudo)
/// para um [`ComplexSelector`]. `None` se vazio/inválido. Porta pública usada pelo
/// parser de regras (que já quebra a vírgula antes).
pub fn parse_selector(s: &str) -> Option<ComplexSelector> {
    ComplexSelector::parse(s)
}

/// Parseia uma LISTA de seletores separada por vírgula (`p, a, .x`) — o que
/// querySelector/matches/closest aceitam. Cada item inválido é PULADO (a lista não
/// é descartada inteira por um item ruim, fiel ao forgiving parsing de querySelector
/// não — na verdade querySelector lança se algum é inválido; aqui pulamos por
/// robustez headless). Divide a vírgula no TOP-LEVEL (fora de `[...]` e `(...)`).
pub fn parse_selector_list(s: &str) -> Vec<ComplexSelector> {
    split_top_level_commas(s)
        .into_iter()
        .filter_map(|part| ComplexSelector::parse(part.trim()))
        .collect()
}

/// Divide `s` nas vírgulas de TOP-LEVEL (ignora vírgulas dentro de `[...]` ou
/// `(...)`, ex: `[a="x,y"]`, `:nth-child(2n, 1)`).
pub(crate) fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth_br, mut depth_par, mut start) = (0i32, 0i32, 0usize);
    let mut in_quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if in_quote.is_none() => in_quote = Some(c),
            q if Some(q) == in_quote => in_quote = None,
            '[' if in_quote.is_none() => depth_br += 1,
            ']' if in_quote.is_none() => depth_br -= 1,
            '(' if in_quote.is_none() => depth_par += 1,
            ')' if in_quote.is_none() => depth_par -= 1,
            ',' if in_quote.is_none() && depth_br == 0 && depth_par == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

impl ComplexSelector {
    /// Parseia um seletor completo (compostos + combinadores). `None` se inválido.
    pub(crate) fn parse(s: &str) -> Option<ComplexSelector> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut compounds = Vec::new();
        let mut combinators = Vec::new();
        let mut rest = s;
        let mut pending_combinator: Option<Combinator> = None;
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            // combinador explícito (>, +, ~) antes do próximo compound?
            let explicit = match rest.chars().next() {
                Some('>') => Some(Combinator::Child),
                Some('+') => Some(Combinator::NextSibling),
                Some('~') => Some(Combinator::SubsequentSibling),
                _ => None,
            };
            if let Some(c) = explicit {
                // combinador DUPLO (`>>`, `> +`) é inválido → descarta a regra.
                if pending_combinator.is_some() {
                    return None;
                }
                pending_combinator = Some(c);
                rest = &rest[1..];
                continue;
            }
            // parseia um compound (1+ simples consecutivos).
            let (compound, after) = parse_compound(rest)?;
            if !compounds.is_empty() {
                // o combinador é o explícito pendente OU descendente (espaço).
                combinators.push(pending_combinator.take().unwrap_or(Combinator::Descendant));
            } else if pending_combinator.is_some() {
                return None; // combinador no início é inválido
            }
            compounds.push(compound);
            rest = after;
        }
        if compounds.is_empty() || pending_combinator.is_some() {
            return None;
        }
        Some(ComplexSelector { compounds, combinators })
    }

    /// Peso da cascade: soma das especificidades de todos os simples de todos os
    /// compounds (id=100, classe/attr/pseudo=10, tag=1, universal=0).
    pub fn specificity(&self) -> u32 {
        self.compounds
            .iter()
            .flat_map(|c| c.parts.iter())
            .map(SimpleSelector::specificity)
            .sum()
    }
}

/// Parseia um COMPOUND (sequência de simples sem espaço entre eles): `p.card#x`.
fn parse_compound(s: &str) -> Option<(CompoundSelector, &str)> {
    let mut parts = Vec::new();
    let mut rest = s;
    loop {
        // para o compound no primeiro whitespace ou combinador.
        if rest.is_empty() {
            break;
        }
        let c = rest.chars().next().unwrap();
        if c.is_whitespace() || c == '>' || c == '+' || c == '~' {
            break;
        }
        let (simple, after) = SimpleSelector::parse_one(rest)?;
        // VALIDAÇÃO (Selectors L4 §4.2): um type/universal (tag ou `*`) só pode ser o
        // PRIMEIRO simples do compound. `p*`/`*p`/`p.x*`/`a:hover b` (tipo após pseudo)
        // são inválidos → o browser descarta a regra. Rejeitamos (None).
        if matches!(simple, SimpleSelector::Tag(_) | SimpleSelector::Universal) && !parts.is_empty() {
            return None;
        }
        parts.push(simple);
        rest = after;
    }
    (!parts.is_empty()).then(|| (CompoundSelector { parts }, rest))
}

/// Pega o identificador CSS do início de `s` (letra/dígito/`-`/`_`), devolve
/// (ident, resto).
fn take_ident(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// Parseia `[name op value]` a partir de `[...`. Devolve (Attr, resto após `]`).
fn parse_attr_selector(s: &str) -> Option<(SimpleSelector, &str)> {
    // acha o `]` que fecha — FORA de aspas (`[a="x]y"]` tem `]` literal no valor).
    let mut close = None;
    let mut in_quote: Option<char> = None;
    for (i, c) in s.char_indices().skip(1) {
        match c {
            '"' | '\'' if in_quote.is_none() => in_quote = Some(c),
            q if Some(q) == in_quote => in_quote = None,
            ']' if in_quote.is_none() => {
                close = Some(i);
                break;
            }
            _ => {}
        }
    }
    let close = close?;
    let inner = &s[1..close];
    let rest = &s[close + 1..];
    let inner = inner.trim();
    // acha o operador (=, ^=, $=, *=, ~=, |=) ou só presença.
    let (name, op, value) = if let Some(eq) = inner.find('=') {
        let (before, after) = inner.split_at(eq);
        let value = after[1..].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
        let (name, op) = match before.chars().last() {
            Some('^') => (&before[..before.len() - 1], AttrOp::Prefix),
            Some('$') => (&before[..before.len() - 1], AttrOp::Suffix),
            Some('*') => (&before[..before.len() - 1], AttrOp::Contains),
            Some('~') => (&before[..before.len() - 1], AttrOp::Word),
            Some('|') => (&before[..before.len() - 1], AttrOp::DashPrefix),
            _ => (before, AttrOp::Equals),
        };
        (name.trim().to_ascii_lowercase(), op, value)
    } else {
        (inner.to_ascii_lowercase(), AttrOp::Exists, String::new())
    };
    if name.is_empty() {
        return None;
    }
    Some((SimpleSelector::Attr { name, op, value }, rest))
}

/// Parseia `:pseudo` ou `:pseudo(args)` a partir de `:...`. Pseudo desconhecida
/// (ex: `:hover` — com estado) → `None` (a regra inteira é descartada).
fn parse_pseudo_selector(s: &str) -> Option<(SimpleSelector, &str)> {
    let after_colon = &s[1..];
    // `:nth-child(...)` — captura o argumento entre parênteses.
    if let Some(rest) = after_colon.strip_prefix("nth-child(") {
        let close = rest.find(')')?;
        let arg = &rest[..close];
        let (a, b) = parse_nth(arg)?;
        return Some((SimpleSelector::Pseudo(PseudoClass::NthChild(a, b)), &rest[close + 1..]));
    }
    let (ident, rest) = take_ident(after_colon);
    let pc = match ident {
        "first-child" => PseudoClass::FirstChild,
        "last-child" => PseudoClass::LastChild,
        "only-child" => PseudoClass::OnlyChild,
        "empty" => PseudoClass::Empty,
        "root" => PseudoClass::Root,
        "checked" => PseudoClass::Checked,
        "disabled" => PseudoClass::Disabled,
        "enabled" => PseudoClass::Enabled,
        "required" => PseudoClass::Required,
        "hover" => PseudoClass::Hover,
        _ => return None, // :focus/:not()/etc não suportados
    };
    Some((SimpleSelector::Pseudo(pc), rest))
}

/// Parseia o argumento de `:nth-child()`: `odd`/`even`/`N`/`an+b`/`an-b`/`an`/`n`.
/// Devolve (a, b) tal que casa quando `index = a*k + b` p/ algum k>=0 (1-based).
fn parse_nth(arg: &str) -> Option<(i32, i32)> {
    let a = arg.trim().to_ascii_lowercase();
    match a.as_str() {
        "odd" => return Some((2, 1)),
        "even" => return Some((2, 0)),
        _ => {}
    }
    if !a.contains('n') {
        // só um número: casa exatamente esse índice.
        return a.parse::<i32>().ok().map(|b| (0, b));
    }
    // forma `an+b` / `an-b` / `an` / `n` / `-n`.
    let (coef, rest) = a.split_once('n')?;
    let a_val: i32 = match coef.trim() {
        "" | "+" => 1,
        "-" => -1,
        c => c.parse().ok()?,
    };
    let b_val: i32 = match rest.trim() {
        "" => 0,
        b => b.replace(' ', "").parse().ok()?,
    };
    Some((a_val, b_val))
}

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
