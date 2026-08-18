//! SELETORES CSS: simples, compostos (`p.card#x`), combinadores (`div > p`,
//! `+`, `~`), atributo (`[a=v]` e operadores) e pseudo-classes — estruturais
//! (`:first-child`, `:nth-of-type`), de estado (`:hover`, `:focus`, `:link`) e
//! funcionais (`:not()`, `:is()`, `:where()`, `:lang()`).
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
    /// Pseudo-classe (`:first-child`, `:hover`, `:not(...)`, `:lang(x)`, …). A
    /// especificidade NÃO é fixa em 10: `:not`/`:is` tomam a do argumento e
    /// `:where` vale zero — por isso delega em [`PseudoClass::specificity`].
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
    // A família `-of-type` conta só os irmãos com a MESMA tag. É a diferença
    // toda em relação às de cima: `a:only-of-type` casa o único `<a>` entre
    // irmãos de tags variadas, onde `:only-child` não casaria nenhum.
    FirstOfType,
    LastOfType,
    OnlyOfType,
    /// `:nth-of-type(an+b)` — mesmo (a, b) do `:nth-child`, contado por tag.
    NthOfType(i32, i32),
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
    /// `:focus` — ESTADO VIVO: o elemento com o foco de teclado. O DOM só tem UM
    /// foco vivo, o `focused_input` que o loop de input seta ao clicar dentro da
    /// caixa de um campo; então `:focus` casa esse nó e mais nenhum.
    ///
    /// NÃO propaga aos ancestrais (ao contrário de `:hover`): quem faz isso é
    /// `:focus-within`, que é outra pseudo e não está implementada.
    Focus,
    /// `:focus-within` — o elemento focado OU um ancestral dele. É a versão do
    /// `:focus` que propaga, como o `:hover` propaga.
    FocusWithin,
    /// `:focus-visible` — o browser mostra o anel de foco quando o foco veio do
    /// teclado, e esconde-o quando veio do rato. Aqui não há essa distinção (o
    /// backend entrega um só `focus_input`), então casa o MESMO que `:focus`.
    ///
    /// Aproximar em vez de nunca casar é a escolha deliberada: a regra existe
    /// quase sempre para desenhar o anel de foco, e não casar nunca tira o
    /// indicador de foco à navegação por teclado — perder o indicador é pior do
    /// que mostrá-lo também depois de um clique.
    FocusVisible,
    /// `:active` — o elemento entre o mousedown e o mouseup sobre ele. NUNCA casa:
    /// o DOM não guarda esse estado (`set_hovered` é o único estado de ponteiro
    /// que o backend entrega). Casar sempre seria pior que casar nunca — deixaria
    /// o estilo de "botão premido" colado em todo botão da página.
    Active,
    /// `:visited` — NUNCA casa: não há histórico de navegação. É também o que o
    /// browser faz na prática por privacidade (só um punhado de propriedades de
    /// cor é aplicável a `:visited`), portanto "nunca" é o desvio menor.
    Visited,
    /// `:link` — um hiperligação AINDA não visitada. Como `:visited` nunca casa,
    /// isto é simplesmente "é um `<a>`/`<area>` com `href`".
    Link,
    /// `:read-write` — editável pelo utilizador: `input`/`textarea` sem `readonly`
    /// nem `disabled`, ou qualquer elemento com `contenteditable` diferente de
    /// `"false"`.
    ReadWrite,
    /// `:read-only` — o COMPLEMENTO de `:read-write` (spec: todo elemento que não
    /// é editável, incluindo um `<p>` qualquer — não só campos de formulário).
    /// Seguimos a spec e não a intuição: uma folha real usa `:read-only` para
    /// desenhar o campo bloqueado, e restringir a formulários casaria menos do
    /// que o browser casa, o que se paga na cascade e não aqui.
    ReadOnly,
    /// `:lang(x)` — o idioma do elemento é `x` ou um seu subtipo (`en` casa
    /// `en-US`). O idioma vem do atributo `lang` do próprio nó ou do ancestral
    /// mais próximo que o tenha. NÃO consultamos `<meta http-equiv>` nem o
    /// cabeçalho Content-Language: nenhum dos dois chega ao DOM.
    Lang(String),
    /// `:not(a, b)` — casa se NENHUM dos seletores do argumento casa o elemento.
    /// Especificidade = a do argumento MAIS específico (o `:not` em si vale 0).
    Not(Vec<ComplexSelector>),
    /// `:is(a, b)` (e o alias antigo `:matches()`) — casa se ALGUM casa.
    /// Especificidade = a do argumento mais específico.
    Is(Vec<ComplexSelector>),
    /// `:where(a, b)` — casa igual ao `:is`, mas contribui ZERO para a
    /// especificidade. É a única diferença entre os dois, e é o ponto todo dele:
    /// `:where(.a) .b` perde para `.c .b`, enquanto `:is(.a) .b` ganha.
    Where(Vec<ComplexSelector>),
}

/// A especificidade é uma TRIPLA (ids, classes, tags), não um número — e é
/// guardada num `u32` com um byte por componente, `0xII_CC_TT`. A ordem
/// numérica do `u32` é então exatamente a ordem lexicográfica da tripla, que é o
/// que a cascade quer, e nenhum consumidor precisa de saber disto: a
/// especificidade continua a ser uma chave opaca de ordenação.
///
/// A alternativa que estava aqui — somar 100/10/1 num só número — está errada e
/// a spec diz porquê: os componentes NÃO se convertem uns nos outros. Com a soma
/// plana, dez tags (10) empatavam com uma classe e onze classes (110) venciam um
/// id. Um seletor que casa com o peso errado é pior do que um que não casa,
/// porque a regra vencedora aparece longe da causa. É também o que o Blink faz
/// (`CSSSelector::Specificity`, máscaras `0xff0000`/`0x00ff00`/`0x0000ff`).
const ESPEC_ID: u32 = 0x01_00_00;
const ESPEC_CLASSE: u32 = 0x00_01_00;
const ESPEC_TAG: u32 = 0x00_00_01;

/// Soma duas especificidades COMPONENTE A COMPONENTE, saturando cada byte em
/// 255. Saturar em vez de deixar transbordar: um transbordo levaria uma classe a
/// mais para o campo dos ids, invertendo a cascade justamente no seletor mais
/// carregado. 255 de um componente é inalcançável em folhas reais, portanto o
/// erro que a saturação introduz é um empate entre dois seletores absurdos.
fn soma_especificidade(a: u32, b: u32) -> u32 {
    let comp = |desloc: u32| {
        let s = ((a >> desloc) & 0xFF) + ((b >> desloc) & 0xFF);
        s.min(0xFF) << desloc
    };
    comp(16) | comp(8) | comp(0)
}

impl PseudoClass {
    /// A especificidade desta pseudo-classe: peso de classe para quase todas; as
    /// funcionais tomam a do argumento mais específico, e `:where` vale zero.
    fn specificity(&self) -> u32 {
        match self {
            PseudoClass::Where(_) => 0,
            PseudoClass::Not(list) | PseudoClass::Is(list) => {
                list.iter().map(ComplexSelector::specificity).max().unwrap_or(0)
            }
            _ => ESPEC_CLASSE,
        }
    }

    /// Os seletores do argumento, quando é uma pseudo funcional. Fatia vazia para
    /// as restantes — é por aqui que o índice de regras e o stylesheet varrem o
    /// que está DENTRO de um `:is()`/`:not()`, em vez de o ignorarem.
    pub fn sub_selectors(&self) -> &[ComplexSelector] {
        match self {
            PseudoClass::Not(l) | PseudoClass::Is(l) | PseudoClass::Where(l) => l,
            _ => &[],
        }
    }
}

/// Visita todo [`SimpleSelector`] de `sel`, incluindo os que estão DENTRO de
/// `:not()`/`:is()`/`:where()`, em profundidade.
///
/// Existe porque as varreduras derivadas (classes citadas, "usa atributo?",
/// "depende da posição?") são perguntas sobre o que a regra PODE observar, e um
/// simples aninhado é observado na mesma. Ignorá-lo fazia `.a:is(.b)` não
/// invalidar quando `b` mudava — uma falha longe da causa.
pub fn visit_simples<'a>(sel: &'a ComplexSelector, f: &mut impl FnMut(&'a SimpleSelector)) {
    for compound in &sel.compounds {
        for part in &compound.parts {
            f(part);
            if let SimpleSelector::Pseudo(pc) = part {
                for sub in pc.sub_selectors() {
                    visit_simples(sub, f);
                }
            }
        }
    }
}

/// `true` se este compound contém `:hover`, inclusive dentro de uma pseudo
/// funcional (`.a:is(:hover)`). O alcance da invalidação de hover pergunta isto
/// por compound, e um `:hover` aninhado invalida tanto como um solto.
pub fn compound_has_hover(compound: &CompoundSelector) -> bool {
    compound.parts.iter().any(|part| match part {
        SimpleSelector::Pseudo(PseudoClass::Hover) => true,
        SimpleSelector::Pseudo(pc) => pc
            .sub_selectors()
            .iter()
            .any(|s| s.compounds.iter().any(compound_has_hover)),
        _ => false,
    })
}

/// Visita `sel` e todos os seletores aninhados nas pseudo funcionais. Usado por
/// quem precisa dos COMBINADORES (e não só dos simples).
pub fn visit_selectors<'a>(sel: &'a ComplexSelector, f: &mut impl FnMut(&'a ComplexSelector)) {
    f(sel);
    for compound in &sel.compounds {
        for part in &compound.parts {
            if let SimpleSelector::Pseudo(pc) = part {
                for sub in pc.sub_selectors() {
                    visit_selectors(sub, f);
                }
            }
        }
    }
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
    /// Bytes ESTIMADOS das strings deste simples — a parte do seletor que
    /// realmente aloca. Usado por [`crate::metrics::footprint`].
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                SimpleSelector::Tag(s) | SimpleSelector::Class(s) | SimpleSelector::Id(s) => {
                    s.capacity()
                }
                SimpleSelector::Attr { name, value, .. } => name.capacity() + value.capacity(),
                SimpleSelector::Pseudo(pc) => match pc {
                    PseudoClass::Lang(s) => s.capacity(),
                    _ => pc.sub_selectors().iter().map(ComplexSelector::estimated_bytes).sum(),
                },
                SimpleSelector::Universal => 0,
            }
    }

    fn specificity(&self) -> u32 {
        match self {
            SimpleSelector::Id(_) => ESPEC_ID,
            SimpleSelector::Class(_) | SimpleSelector::Attr { .. } => ESPEC_CLASSE,
            SimpleSelector::Pseudo(pc) => pc.specificity(),
            SimpleSelector::Tag(_) => ESPEC_TAG,
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
                        + c.parts.iter().map(SimpleSelector::estimated_bytes).sum::<usize>()
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

    /// Peso da cascade: a tripla (ids, classes, tags) empacotada — ver
    /// [`ESPEC_ID`]. Opaca para quem chama; só se compara.
    pub fn specificity(&self) -> u32 {
        self.compounds
            .iter()
            .flat_map(|c| c.parts.iter())
            .map(SimpleSelector::specificity)
            .fold(0, soma_especificidade)
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
/// Não-ASCII é aceite: a spec (CSS Syntax §4.2) trata todo ponto de código
/// acima de U+0080 como caracter de identificador, e a folha da Wikipédia usa-os
/// (`.page-Wikipédia_Página_principal`, `.animangá`). Cortar no `é` partia o
/// seletor em dois e descartava a regra — a falha aparecia como estilo em falta
/// numa página inteira, longe daqui.
fn take_ident(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c as u32 >= 0x80))
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
/// → `None` (a regra inteira é descartada).
///
/// PSEUDO-ELEMENTOS (`::before`, `::after`, `::marker`) caem aqui e são
/// recusados de propósito: gerá-los exige criar caixas que não existem na
/// árvore, o que é trabalho do layout e não do seletor. Recusar descarta a
/// regra — que é o certo enquanto não há caixa: aceitar o seletor e aplicar as
/// declarações ao elemento REAL pintaria o `content` do `::after` dentro do
/// próprio elemento, e um erro visível é pior que uma regra ausente.
fn parse_pseudo_selector(s: &str) -> Option<(SimpleSelector, &str)> {
    let after_colon = &s[1..];
    // `:nth-child(...)`/`:nth-of-type(...)` — captura o argumento entre parênteses.
    for (nome, por_tipo) in [("nth-child(", false), ("nth-of-type(", true)] {
        let Some(rest) = after_colon.strip_prefix(nome) else { continue };
        let close = rest.find(')')?;
        let (a, b) = parse_nth(&rest[..close])?;
        let pc = if por_tipo { PseudoClass::NthOfType(a, b) } else { PseudoClass::NthChild(a, b) };
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
        let Some(rest) = strip_func_name(after_colon, nome) else { continue };
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
    if let Some(rest) = strip_func_name(after_colon, "lang") {
        let (arg, after) = take_balanced_paren(rest)?;
        let lang = arg.trim().trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase();
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
        // Ainda por fazer: `:target` (não há fragmento de URL no DOM) e `:has()`
        // (relacional — casa um ancestral pelo que está ABAIXO dele, o que o
        // matcher da direita-para-a-esquerda não faz e a invalidação não sabe
        // seguir). Recusar descarta a regra.
        _ => return None,
    };
    Some((SimpleSelector::Pseudo(pc), rest))
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
        SimpleSelector::Class(c) => class_attr
            .is_some_and(|raw| raw.split_whitespace().any(|class| class == c)),
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
