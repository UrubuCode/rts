//! O PARSER: o seletor complexo, o compound, o atributo, a pseudo-classe e o `An+B`
//!
//! Extraído de `selector.rs` sem alterar uma linha.

use super::*;

impl ComplexSelector {
    /// Bytes ESTIMADOS deste seletor completo (todos os compounds e suas
    /// strings). Ver [`crate::metrics::footprint`].
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.combinators.capacity() * std::mem::size_of::<Combinator>()
            + self
                .compounds
                .iter()
                .map(|c| {
                    std::mem::size_of::<CompoundSelector>()
                        + c.parts
                            .iter()
                            .map(SimpleSelector::estimated_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    /// Parseia um seletor completo (compostos + combinadores). `None` se inválido.
    pub(crate) fn parse(s: &str) -> Option<ComplexSelector> {
        let _phase = crate::metrics::phases::scope("parse-selector");
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut compounds = Vec::new();
        let mut combinators = Vec::new();
        let mut rest = s;
        let mut pending_combinator: Option<Combinator> = None;
        let mut pseudo_element = None;
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            // Um pseudo-elemento termina o seletor (Selectors L4 §3.3: nada pode
            // segui-lo a não ser outra pseudo-classe, que não suportamos aí).
            // Sobrar texto depois dele é inválido → descarta a regra.
            if let Some((pe, after)) = strip_pseudo_element(rest) {
                if !after.trim().is_empty() {
                    return None;
                }
                // `::before` sozinho é válido e vale `*::before` — o tipo
                // implícito da spec. Recusá-lo perdia a regra que uma folha
                // escreve para dar `box-sizing` a tudo, pseudos incluídos.
                if compounds.is_empty() {
                    compounds.push(CompoundSelector {
                        parts: vec![SimpleSelector::Universal],
                    });
                }
                // `None` = é um `::` que não geramos (`::marker`, `::selection`).
                // A regra é descartada em vez de perder o pseudo-elemento: sem
                // ele, `p::marker { color:red }` pintaria o próprio `<p>`.
                pseudo_element = Some(pe?);
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
        Some(ComplexSelector {
            compounds,
            combinators,
            pseudo_element,
        })
    }

    /// Peso da cascade: a tripla (ids, classes, tags) empacotada — ver
    /// [`ESPEC_ID`]. Opaca para quem chama; só se compara.
    pub fn specificity(&self) -> u32 {
        // O pseudo-elemento pesa como uma TAG, não como uma classe (Selectors
        // L4 §17: conta para o componente C). É o que faz `p::before` vencer
        // `::before` e perder para `.x::before`.
        let base = if self.pseudo_element.is_some() {
            ESPEC_TAG
        } else {
            0
        };
        self.compounds
            .iter()
            .flat_map(|c| c.parts.iter())
            .map(SimpleSelector::specificity)
            .fold(base, soma_especificidade)
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
        // O pseudo-elemento não é um simples deste compound — ele encerra o
        // seletor e quem o consome é o `ComplexSelector::parse`.
        if strip_pseudo_element(rest).is_some() {
            break;
        }
        let (simple, after) = SimpleSelector::parse_one(rest)?;
        // VALIDAÇÃO (Selectors L4 §4.2): um type/universal (tag ou `*`) só pode ser o
        // PRIMEIRO simples do compound. `p*`/`*p`/`p.x*`/`a:hover b` (tipo após pseudo)
        // são inválidos → o browser descarta a regra. Rejeitamos (None).
        if matches!(simple, SimpleSelector::Tag(_) | SimpleSelector::Universal) && !parts.is_empty()
        {
            return None;
        }
        parts.push(simple);
        rest = after;
    }
    (!parts.is_empty()).then(|| (CompoundSelector { parts }, rest))
}

/// Pega o identificador CSS do início de `s` (letra/dígito/`-`/`_`), devolve
/// (ident, resto).
/// Não-ASCII é aceite: a spec (CSS Syntax §4.2) trata todo ponto de código
/// acima de U+0080 como caracter de identificador, e a folha da Wikipédia usa-os
/// (`.page-Wikipédia_Página_principal`, `.animangá`). Cortar no `é` partia o
/// seletor em dois e descartava a regra — a falha aparecia como estilo em falta
/// numa página inteira, longe daqui.
pub(in crate::style::selector) fn take_ident(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c as u32 >= 0x80))
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// Parseia `[name op value]` a partir de `[...`. Devolve (Attr, resto após `]`).
pub(in crate::style::selector) fn parse_attr_selector(s: &str) -> Option<(SimpleSelector, &str)> {
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
        let value = after[1..]
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
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
/// → `None` (a regra inteira é descartada).
///
/// PSEUDO-ELEMENTOS (`::before`, `::after`, `::marker`) caem aqui e são
/// recusados de propósito: gerá-los exige criar caixas que não existem na
/// árvore, o que é trabalho do layout e não do seletor. Recusar descarta a
/// regra — que é o certo enquanto não há caixa: aceitar o seletor e aplicar as
/// declarações ao elemento REAL pintaria o `content` do `::after` dentro do
/// próprio elemento, e um erro visível é pior que uma regra ausente.
pub(in crate::style::selector) fn parse_pseudo_selector(s: &str) -> Option<(SimpleSelector, &str)> {
    let after_colon = &s[1..];
    // `:nth-child(...)`/`:nth-of-type(...)` — captura o argumento entre parênteses.
    for (nome, por_tipo) in [("nth-child(", false), ("nth-of-type(", true)] {
        let Some(rest) = after_colon.strip_prefix(nome) else {
            continue;
        };
        let close = rest.find(')')?;
        let (a, b) = parse_nth(&rest[..close])?;
        let pc = if por_tipo {
            PseudoClass::NthOfType(a, b)
        } else {
            PseudoClass::NthChild(a, b)
        };
        return Some((SimpleSelector::Pseudo(pc), &rest[close + 1..]));
    }
    // As FUNCIONAIS de lista de seletores. O argumento pode conter parênteses
    // (`:not(:nth-child(2))`), por isso o fecho é procurado por equilíbrio e não
    // pelo primeiro `)`.
    for (nome, funcional) in [
        ("not", Funcional::Not),
        ("is", Funcional::Is),
        // `:matches()` é o nome antigo de `:is()` — folhas reais ainda o trazem.
        ("matches", Funcional::Is),
        ("where", Funcional::Where),
    ] {
        let Some(rest) = strip_func_name(after_colon, nome) else {
            continue;
        };
        let (arg, after) = take_balanced_paren(rest)?;
        let partes = split_top_level_commas(arg);
        let mut lista = Vec::with_capacity(partes.len());
        for parte in partes {
            match ComplexSelector::parse(parte.trim()) {
                Some(sel) => lista.push(sel),
                // Selectors L4: `:is()`/`:where()` são FORGIVING (um argumento
                // inválido é só descartado, os outros continuam a valer);
                // `:not()` não é — um argumento inválido invalida o seletor
                // inteiro. Distinção da spec, e é o que separa uma folha que
                // usa uma pseudo que não temos de uma folha mal escrita.
                None if funcional == Funcional::Not => return None,
                None => {}
            }
        }
        let pc = match funcional {
            Funcional::Not => PseudoClass::Not(lista),
            Funcional::Is => PseudoClass::Is(lista),
            Funcional::Where => PseudoClass::Where(lista),
        };
        return Some((SimpleSelector::Pseudo(pc), after));
    }
    // `:has(<lista-relativa>)` — cada item pode ter um combinador EXPLÍCITO
    // líder (`> img`, `+ p`, `~ p`); sem um, o líder é descendente, como um
    // seletor complexo comum começando por espaço implícito (`:has(.b)` é
    // "algum descendente casa `.b`", não "o alvo casa `.b`" — daí não reusar
    // `ComplexSelector::parse` sozinho, que exigiria um combinador ANTES do
    // primeiro compound e o recusaria).
    if let Some(rest) = strip_func_name(after_colon, "has") {
        let (arg, after) = take_balanced_paren(rest)?;
        let mut lista = Vec::new();
        for parte in split_top_level_commas(arg) {
            let parte = parte.trim();
            let (combinador, resto) = match parte.chars().next() {
                Some('>') => (Combinator::Child, parte[1..].trim_start()),
                Some('+') => (Combinator::NextSibling, parte[1..].trim_start()),
                Some('~') => (Combinator::SubsequentSibling, parte[1..].trim_start()),
                _ => (Combinator::Descendant, parte),
            };
            // Argumento inválido descarta o `:has()` inteiro — como `:not()`, e
            // ao contrário do `:is()`/`:where()` (forgiving): um `:has()` mal
            // formado não tem leitura parcial razoável.
            let sel = ComplexSelector::parse(resto)?;
            lista.push((combinador, sel));
        }
        if lista.is_empty() {
            return None;
        }
        return Some((SimpleSelector::Pseudo(PseudoClass::Has(lista)), after));
    }
    if let Some(rest) = strip_func_name(after_colon, "lang") {
        let (arg, after) = take_balanced_paren(rest)?;
        let lang = arg
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_ascii_lowercase();
        if lang.is_empty() {
            return None;
        }
        return Some((SimpleSelector::Pseudo(PseudoClass::Lang(lang)), after));
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
        "first-of-type" => PseudoClass::FirstOfType,
        "last-of-type" => PseudoClass::LastOfType,
        "only-of-type" => PseudoClass::OnlyOfType,
        "focus" => PseudoClass::Focus,
        "focus-within" => PseudoClass::FocusWithin,
        "focus-visible" => PseudoClass::FocusVisible,
        "active" => PseudoClass::Active,
        "visited" => PseudoClass::Visited,
        "link" => PseudoClass::Link,
        "read-only" => PseudoClass::ReadOnly,
        "read-write" => PseudoClass::ReadWrite,
        "target" => PseudoClass::Target,
        "scope" => PseudoClass::Scope,
        "default" => PseudoClass::Default,
        "placeholder-shown" => PseudoClass::PlaceholderShown,
        // Ainda por fazer: `:autofill` (não há preenchimento automático neste
        // motor) e `:modal`/`:focus-visible` real (precisam do `rts-input`,
        // ver `dom/matcher.rs`). Recusar descarta a regra.
        _ => return None,
    };
    Some((SimpleSelector::Pseudo(pc), rest))
}

/// Reconhece um PSEUDO-ELEMENTO no início de `s`.
///
/// Devolve `None` quando `s` não começa por um; `Some((None, resto))` quando é
/// um pseudo-elemento que não sabemos gerar — os dois casos são diferentes e a
/// diferença é a que interessa: o segundo tem de DESCARTAR a regra, porque
/// aplicá-la ao elemento originante pintaria nele o que era para uma caixa
/// gerada.
///
/// Aceita as duas grafias: `::before` (CSS3, a atual) e `:before` (CSS2, ainda
/// muito usada, e sem ambiguidade porque não existe pseudo-CLASSE com este nome).
fn strip_pseudo_element(s: &str) -> Option<(Option<PseudoElement>, &str)> {
    let corpo = s.strip_prefix("::").or_else(|| {
        let apos = s.strip_prefix(':')?;
        let (nome, _) = take_ident(apos);
        // Só a grafia de um colon dos DOIS que a CSS2 tinha; qualquer outro `:`
        // é pseudo-classe e não nos diz respeito aqui.
        (nome.eq_ignore_ascii_case("before") || nome.eq_ignore_ascii_case("after")).then_some(apos)
    })?;
    let (nome, resto) = take_ident(corpo);
    let pe = if nome.eq_ignore_ascii_case("before") {
        Some(PseudoElement::Before)
    } else if nome.eq_ignore_ascii_case("after") {
        Some(PseudoElement::After)
    } else if nome.eq_ignore_ascii_case("marker") {
        // `::marker` (lote O) — a caixa já existe (`listitem::emit_marker`, o
        // `<li>`/`::-webkit-scrollbar*` marcador de lista); o que faltava era
        // uma entrada na cascade para ela ter estilo PRÓPRIO. `content` não se
        // aplica (o marcador não é gerado por `content`), então cai no mesmo
        // ramo de `matched_for_pseudo` sem um `content` vencedor.
        Some(PseudoElement::Marker)
    } else {
        // `::selection`, `::placeholder`, `::first-line`, `::first-letter`…
        // Cada um precisa de maquinaria própria (um intervalo de seleção, uma
        // primeira linha) e nenhum é conteúdo gerado por `content`.
        None
    };
    Some((pe, resto))
}

/// Qual das funcionais de lista de seletores está a ser parseada.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Funcional {
    Not,
    Is,
    Where,
}

/// `Some(resto-a-partir-do-`(`)` se `s` começa por `nome(` (nome case-insensitive,
/// como todo identificador de pseudo em CSS).
fn strip_func_name<'a>(s: &'a str, nome: &str) -> Option<&'a str> {
    let n = nome.len();
    if s.len() > n && s[..n].eq_ignore_ascii_case(nome) && s.as_bytes()[n] == b'(' {
        Some(&s[n..])
    } else {
        None
    }
}

/// Dado `s` a começar em `(`, devolve (conteúdo sem os parênteses, resto após o
/// `)` que fecha). Conta profundidade porque o argumento de `:not()` pode conter
/// outra funcional; `find(')')` cortaria em `:not(:nth-child(2))` no sítio errado.
fn take_balanced_paren(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut in_quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if in_quote.is_none() => in_quote = Some(c),
            q if Some(q) == in_quote => in_quote = None,
            '(' if in_quote.is_none() => depth += 1,
            ')' if in_quote.is_none() => {
                depth -= 1;
                if depth == 0 {
                    return Some((&s[1..i], &s[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
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
