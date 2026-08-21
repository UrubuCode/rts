//! O VOCABULÁRIO do seletor e a ESPECIFICIDADE: simples, atributo, pseudo-classe, combinador
//!
//! Extraído de `selector.rs` sem alterar uma linha.

use super::*;

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
    Attr {
        name: String,
        op: AttrOp,
        value: String,
    },
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
pub(in crate::style::selector) const ESPEC_TAG: u32 = 0x00_00_01;

/// Soma duas especificidades COMPONENTE A COMPONENTE, saturando cada byte em
/// 255. Saturar em vez de deixar transbordar: um transbordo levaria uma classe a
/// mais para o campo dos ids, invertendo a cascade justamente no seletor mais
/// carregado. 255 de um componente é inalcançável em folhas reais, portanto o
/// erro que a saturação introduz é um empate entre dois seletores absurdos.
pub(in crate::style::selector) fn soma_especificidade(a: u32, b: u32) -> u32 {
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
            PseudoClass::Not(list) | PseudoClass::Is(list) => list
                .iter()
                .map(ComplexSelector::specificity)
                .max()
                .unwrap_or(0),
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
    /// O PSEUDO-ELEMENTO no fim do seletor (`p::before`), se há.
    ///
    /// Fica fora dos compounds de propósito: um pseudo-elemento não é mais um
    /// teste sobre o elemento — os compounds continuam a casar o elemento
    /// ORIGINANTE (o `<p>`), e isto diz que as declarações não vão para ele mas
    /// para uma caixa gerada. É também o que mantém o índice de regras a
    /// funcionar sem uma linha de mudança: a chave continua a sair do compound
    /// alvo.
    pub pseudo_element: Option<PseudoElement>,
}

/// Os pseudo-elementos que geram caixa aqui.
// `Hash` porque a tabela de contadores é indexada por `(nó, pseudo)`: a caixa
// gerada não tem `NodeIdx` próprio, e o par é a única chave que a identifica.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PseudoElement {
    /// `::before` — caixa gerada ANTES do conteúdo do elemento.
    Before,
    /// `::after` — caixa gerada DEPOIS do conteúdo.
    After,
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
                    _ => pc
                        .sub_selectors()
                        .iter()
                        .map(ComplexSelector::estimated_bytes)
                        .sum(),
                },
                SimpleSelector::Universal => 0,
            }
    }

    pub(in crate::style::selector) fn specificity(&self) -> u32 {
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
    pub(in crate::style::selector) fn parse_one(s: &str) -> Option<(SimpleSelector, &str)> {
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
        // Um seletor com PSEUDO-ELEMENTO casa uma caixa gerada, e o
        // `querySelector` não devolve caixas geradas — no browser
        // `document.querySelector("p::before")` é sempre `null`. Descartá-lo
        // aqui, e não no matcher, é o que mantém a cascata a funcionar: a
        // cascata usa o mesmo matcher e PRECISA de casar estes seletores contra
        // o elemento originante.
        .filter(|sel| sel.pseudo_element.is_none())
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
