//! A gramática de `@media` (lote P, §5.P) e `window.matchMedia`.
//!
//! Antes disto o motor só entendia `min-width`/`max-width` em px/em/rem e um
//! `always_false` para qualquer outra coisa — o suficiente para o Bootstrap,
//! que não usa `orientation`/`prefers-*`/listas OR. Este módulo troca isso por
//! uma lista de queries (`,` = OR), cada uma com `not`/`only`, um tipo
//! (`screen`/`all`/`print`) e uma conjunção (`and`) de features — incluindo a
//! forma de intervalo (`(400px <= width <= 800px)`), que a spec deixa
//! equivalente a duas comparações.
//!
//! `MediaContext` é o que substituiu o `f32` solto (só `viewport_w`) nas duas
//! chamadas de `Stylesheet::matched_for_*` — a árvore de features precisa da
//! altura, e `prefers-color-scheme`/`prefers-reduced-motion` vêm do HOST, não
//! do viewport. `window.matchMedia` (lote P item 2) usa o MESMO avaliador: uma
//! verdade só entre a folha e a fachada.

/// `prefers-color-scheme` — um campo no `Dom`, não uma constante: o host (ou um
/// teste) pode mudar. Default `Light`, como um browser sem preferência de SO.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrefersColorScheme {
    #[default]
    Light,
    Dark,
}

impl PrefersColorScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            PrefersColorScheme::Light => "light",
            PrefersColorScheme::Dark => "dark",
        }
    }
}

/// O que uma media query é avaliada CONTRA: o viewport da passada de layout
/// mais o que só o host sabe. `Copy` — é passado por valor a cada cascade.
#[derive(Clone, Copy, Debug)]
pub struct MediaContext {
    pub width: f32,
    pub height: f32,
    pub prefers_color_scheme: PrefersColorScheme,
    /// `prefers-reduced-motion: reduce` quando `true`.
    pub prefers_reduced_motion: bool,
}

impl Default for MediaContext {
    fn default() -> Self {
        MediaContext {
            width: 0.0,
            height: 0.0,
            prefers_color_scheme: PrefersColorScheme::Light,
            prefers_reduced_motion: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum MediaType {
    All,
    Screen,
    Print,
    /// Um tipo desconhecido (`tty`, `braille`…) — a query nunca casa, como a
    /// spec manda para tipos não reconhecidos que não sejam `all`.
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Orientation {
    Portrait,
    Landscape,
}

/// Uma feature dentro de UMA condição `(feature: valor)` ou de intervalo.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Feature {
    MinWidth(f32),
    MaxWidth(f32),
    Width(f32),
    MinHeight(f32),
    MaxHeight(f32),
    Height(f32),
    Orientation(Orientation),
    MinAspectRatio(f32),
    MaxAspectRatio(f32),
    AspectRatio(f32),
    MinResolution(f32),
    MaxResolution(f32),
    Resolution(f32),
    Hover(bool),
    PointerFine(bool),
    PrefersColorScheme(PrefersColorScheme),
    PrefersReducedMotion(bool),
    /// A forma de intervalo (`400px <= width <= 800px`) — um único par
    /// min/max, porque a spec a define como "as duas comparações", e um par
    /// devolve isso sem duplicar o `Vec<Feature>` da condição.
    WidthRange(f32, f32),
    HeightRange(f32, f32),
    /// Reconhecida sintaticamente mas sem consumidor — nunca casa (conservador,
    /// a mesma escolha que `always_false` fazia para tudo antes deste módulo).
    Unsupported,
}

impl Feature {
    fn matches(self, ctx: &MediaContext) -> bool {
        match self {
            Feature::MinWidth(v) => ctx.width >= v,
            Feature::MaxWidth(v) => ctx.width <= v,
            Feature::Width(v) => (ctx.width - v).abs() < 0.5,
            Feature::MinHeight(v) => ctx.height >= v,
            Feature::MaxHeight(v) => ctx.height <= v,
            Feature::Height(v) => (ctx.height - v).abs() < 0.5,
            Feature::Orientation(o) => {
                let landscape = ctx.width >= ctx.height;
                matches!(
                    (o, landscape),
                    (Orientation::Landscape, true) | (Orientation::Portrait, false)
                )
            }
            Feature::MinAspectRatio(v) => ctx.height > 0.0 && ctx.width / ctx.height >= v - 1e-4,
            Feature::MaxAspectRatio(v) => ctx.height > 0.0 && ctx.width / ctx.height <= v + 1e-4,
            Feature::AspectRatio(v) => ctx.height > 0.0 && (ctx.width / ctx.height - v).abs() < 1e-3,
            // resolução: headless não tem DPI real — 1dppx (96dpi) é o único
            // valor honesto de responder, como um monitor não-retina.
            Feature::MinResolution(v) => 1.0 >= v - 1e-4,
            Feature::MaxResolution(v) => 1.0 <= v + 1e-4,
            Feature::Resolution(v) => (1.0 - v).abs() < 1e-3,
            // `hover: hover`/`pointer: fine` — headless não tem apontador real;
            // responder o par "tem mouse" é o que um desktop normal reporta, e
            // é o par que os testes de `@media (hover:hover)`/`(pointer:fine)`
            // do corpus assumem (a alternativa, `none`/`coarse`, é o telemóvel).
            Feature::Hover(wants_hover) => wants_hover,
            Feature::PointerFine(wants_fine) => wants_fine,
            Feature::PrefersColorScheme(s) => s == ctx.prefers_color_scheme,
            Feature::PrefersReducedMotion(reduce) => reduce == ctx.prefers_reduced_motion,
            Feature::WidthRange(min, max) => ctx.width >= min && ctx.width <= max,
            Feature::HeightRange(min, max) => ctx.height >= min && ctx.height <= max,
            Feature::Unsupported => false,
        }
    }
}

/// Uma query ÚNICA (um termo entre vírgulas): `not`/`only`, tipo, condições AND.
#[derive(Clone, PartialEq, Debug)]
struct SingleQuery {
    negate: bool,
    media_type: MediaType,
    features: Vec<Feature>,
}

impl SingleQuery {
    fn matches(&self, ctx: &MediaContext) -> bool {
        let type_ok = match self.media_type {
            MediaType::All => true,
            MediaType::Screen => true, // este motor É o "screen" — nunca imprime
            MediaType::Print | MediaType::Unknown => false,
        };
        let cond_ok = self.features.iter().all(|f| f.matches(ctx));
        let base = type_ok && cond_ok;
        if self.negate { !base } else { base }
    }
}

/// A condição de um `@media`, avaliada contra um [`MediaContext`]. Substitui a
/// v1 (só min/max-width) por uma lista `,`-separada (OR) de [`SingleQuery`].
#[derive(Clone, PartialEq, Debug, Default)]
pub struct MediaQuery {
    queries: Vec<SingleQuery>,
}

impl MediaQuery {
    /// Parseia a condição após `@media` (sem o `@media` nem as chaves finais).
    pub fn parse(cond: &str) -> MediaQuery {
        let queries = super::selector::split_top_level_commas(cond)
            .into_iter()
            .map(parse_single_query)
            .collect();
        MediaQuery { queries }
    }

    /// `true` se QUALQUER termo da lista OR casa o contexto.
    pub fn matches(&self, ctx: &MediaContext) -> bool {
        if self.queries.is_empty() {
            return true; // `@media {}` — condição vazia é sempre verdadeira
        }
        self.queries.iter().any(|q| q.matches(ctx))
    }

    /// Combina com uma query EXTERNA (aninhamento `@media` dentro de `@media`):
    /// AND — cada termo externo é combinado com cada termo interno (produto),
    /// porque `@media A { @media B { … } }` só é `true` quando A E B o são; a
    /// distribuição sobre o OR de cada lista é a única forma de manter isso com
    /// listas de termos em vez de um único predicado.
    fn and(self, outer: MediaQuery) -> MediaQuery {
        if outer.queries.is_empty() {
            return self;
        }
        if self.queries.is_empty() {
            return outer;
        }
        let mut combined = Vec::with_capacity(self.queries.len() * outer.queries.len());
        for inner in &self.queries {
            for out in &outer.queries {
                let mut features = inner.features.clone();
                features.extend(out.features.iter().cloned());
                combined.push(SingleQuery {
                    negate: false, // `not` já foi resolvido nos termos de origem
                    media_type: narrower_type(inner.media_type, out.media_type),
                    features,
                });
            }
        }
        MediaQuery { queries: combined }
    }
}

fn narrower_type(a: MediaType, b: MediaType) -> MediaType {
    match (a, b) {
        (MediaType::All, other) | (other, MediaType::All) => other,
        (x, y) if x == y => x,
        _ => MediaType::Unknown, // dois tipos concretos diferentes nunca casam juntos
    }
}

/// Combina a query desta camada com a da camada envolvente (usado por
/// `rules.rs` no lowering de `@media` aninhado).
pub(in crate::style::stylesheet) fn combine(outer: Option<MediaQuery>, inner: MediaQuery) -> Option<MediaQuery> {
    Some(match outer {
        Some(outer) => inner.and(outer),
        None => inner,
    })
}

fn parse_single_query(term: &str) -> SingleQuery {
    let mut t = term.trim();
    let mut negate = false;
    if let Some(rest) = strip_ci_word(t, "not") {
        negate = true;
        t = rest.trim();
    } else if let Some(rest) = strip_ci_word(t, "only") {
        t = rest.trim();
    }
    // Uma condição pura, sem tipo: começa por `(`.
    let (media_type, rest) = if t.starts_with('(') {
        (MediaType::All, t)
    } else {
        let (word, rest) = split_first_word(t);
        let ty = match word.to_ascii_lowercase().as_str() {
            "" | "all" => MediaType::All,
            "screen" => MediaType::Screen,
            "print" => MediaType::Print,
            _ => MediaType::Unknown,
        };
        (ty, rest.trim())
    };
    let rest = strip_ci_word(rest, "and").map(|r| r.trim()).unwrap_or(rest);
    let features = parse_conditions(rest);
    SingleQuery {
        negate,
        media_type,
        features,
    }
}

/// `(min-width: 768px) and (orientation: landscape)` → as duas [`Feature`]s.
/// Cada parêntese de topo é uma condição; `and` entre elas é o único
/// combinador que a v1 aceita fora de listas (o `or` de nível de feature —
/// `@media (400px <= width <= 800px) or (…)` — não existe na spec: `or` só
/// existe como a vírgula entre media-queries inteiras).
fn parse_conditions(s: &str) -> Vec<Feature> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' => {
                if depth == 0 {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(st) = start.take() {
                        out.push(parse_condition(&s[st..i]));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// O conteúdo de UM parêntese: `feature: valor`, ou a forma de intervalo
/// `400px <= width <= 800px` / `width >= 400px` / `width < 800px`.
fn parse_condition(inner: &str) -> Feature {
    let inner = inner.trim();
    if let Some(f) = parse_range(inner) {
        return f;
    }
    let Some((feat, val)) = inner.split_once(':') else {
        // feature booleana sem valor: `(monochrome)` etc — não modelada.
        return Feature::Unsupported;
    };
    let feat = feat.trim().to_ascii_lowercase();
    let val = val.trim();
    match feat.as_str() {
        "min-width" => parse_len(val).map(Feature::MinWidth),
        "max-width" => parse_len(val).map(Feature::MaxWidth),
        "width" => parse_len(val).map(Feature::Width),
        "min-height" => parse_len(val).map(Feature::MinHeight),
        "max-height" => parse_len(val).map(Feature::MaxHeight),
        "height" => parse_len(val).map(Feature::Height),
        "orientation" => match val.to_ascii_lowercase().as_str() {
            "landscape" => Some(Feature::Orientation(Orientation::Landscape)),
            "portrait" => Some(Feature::Orientation(Orientation::Portrait)),
            _ => None,
        },
        "min-aspect-ratio" => parse_ratio(val).map(Feature::MinAspectRatio),
        "max-aspect-ratio" => parse_ratio(val).map(Feature::MaxAspectRatio),
        "aspect-ratio" => parse_ratio(val).map(Feature::AspectRatio),
        "min-resolution" => parse_resolution(val).map(Feature::MinResolution),
        "max-resolution" => parse_resolution(val).map(Feature::MaxResolution),
        "resolution" => parse_resolution(val).map(Feature::Resolution),
        "hover" => Some(Feature::Hover(val.eq_ignore_ascii_case("hover"))),
        "pointer" => Some(Feature::PointerFine(val.eq_ignore_ascii_case("fine"))),
        "prefers-color-scheme" => match val.to_ascii_lowercase().as_str() {
            "light" => Some(Feature::PrefersColorScheme(PrefersColorScheme::Light)),
            "dark" => Some(Feature::PrefersColorScheme(PrefersColorScheme::Dark)),
            _ => None,
        },
        "prefers-reduced-motion" => match val.to_ascii_lowercase().as_str() {
            "reduce" => Some(Feature::PrefersReducedMotion(true)),
            "no-preference" => Some(Feature::PrefersReducedMotion(false)),
            _ => None,
        },
        _ => None,
    }
    .unwrap_or(Feature::Unsupported)
}

/// `400px <= width <= 800px` / `width >= 400px` / `800px > width`. Só
/// `width`/`height` (as duas ranges que o corpus exercita); qualquer outra
/// feature em forma de intervalo é `Unsupported`.
fn parse_range(s: &str) -> Option<Feature> {
    let ops = ["<=", ">=", "<", ">"];
    let mut found: Option<(usize, &str)> = None;
    for op in ops {
        if let Some(idx) = s.find(op) {
            if found.map(|(fi, _)| idx < fi).unwrap_or(true) {
                found = Some((idx, op));
            }
        }
    }
    let (idx, op) = found?;
    let left = s[..idx].trim();
    let right = s[idx + op.len()..].trim();
    // forma dupla: `A op width op2 B`
    let mut rest_ops = ops.iter().filter(|o| right.contains(**o));
    if let Some(op2) = rest_ops.next() {
        let idx2 = right.find(op2)?;
        let mid = right[..idx2].trim();
        let far = right[idx2 + op2.len()..].trim();
        if !mid.eq_ignore_ascii_case("width") && !mid.eq_ignore_ascii_case("height") {
            return None;
        }
        let a = parse_len(left)?;
        let b = parse_len(far)?;
        return Some(range_pair(mid, op, a, op2, b));
    }
    // forma simples: `width >= 400px` ou `400px <= width`
    if left.eq_ignore_ascii_case("width") || left.eq_ignore_ascii_case("height") {
        let v = parse_len(right)?;
        return Some(single_range(left, op, v));
    }
    if right.eq_ignore_ascii_case("width") || right.eq_ignore_ascii_case("height") {
        let v = parse_len(left)?;
        return Some(single_range(right, flip_op(op), v));
    }
    None
}

/// `op` vem sempre de `ops` (`<=`, `>=`, `<`, `>`) — o `_` é inalcançável, mas
/// devolver `op` ali exigiria uma lifetime que a chamada não tem (o `&str` de
/// entrada não é `'static`); `">"` reaproveitado no lugar não muda o resultado
/// porque o ramo nunca corre.
fn flip_op(op: &str) -> &'static str {
    match op {
        "<=" => ">=",
        ">=" => "<=",
        "<" => ">",
        ">" => "<",
        _ => ">",
    }
}

fn single_range(dim: &str, op: &str, v: f32) -> Feature {
    let is_width = dim.eq_ignore_ascii_case("width");
    match op {
        ">=" | ">" => {
            if is_width {
                Feature::MinWidth(v)
            } else {
                Feature::MinHeight(v)
            }
        }
        _ => {
            if is_width {
                Feature::MaxWidth(v)
            } else {
                Feature::MaxHeight(v)
            }
        }
    }
}

/// `A <= width <= B` — a única forma dupla útil no corpus. Os dois operadores
/// têm de olhar na mesma direção (`<=`/`<` nos dois, a forma que a spec define
/// como "as duas comparações" fazem sentido juntas); qualquer outra combinação
/// (`>=`/`<=` misturados) não é um intervalo simples e cai em `Unsupported`.
fn range_pair(dim: &str, op1: &str, a: f32, op2: &str, b: f32) -> Feature {
    let is_width = dim.eq_ignore_ascii_case("width");
    let consistent = matches!(op1, "<=" | "<") && matches!(op2, "<=" | "<");
    if !consistent {
        return Feature::Unsupported;
    }
    if is_width {
        Feature::WidthRange(a, b)
    } else {
        Feature::HeightRange(a, b)
    }
}

fn parse_len(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    if let Some(n) = low.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = low.strip_suffix("rem").or_else(|| low.strip_suffix("em")) {
        return n.trim().parse::<f32>().ok().map(|x| x * 16.0);
    }
    low.parse().ok()
}

/// `dppx`/`x` (o único que este motor responde de forma honesta, 1dppx); `dpi`
/// convertido (96dpi = 1dppx).
fn parse_resolution(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    if let Some(n) = low.strip_suffix("dppx").or_else(|| low.strip_suffix('x')) {
        return n.trim().parse().ok();
    }
    if let Some(n) = low.strip_suffix("dpi") {
        return n.trim().parse::<f32>().ok().map(|d| d / 96.0);
    }
    None
}

/// `16/9` ou `1.777`.
fn parse_ratio(v: &str) -> Option<f32> {
    if let Some((w, h)) = v.split_once('/') {
        let w: f32 = w.trim().parse().ok()?;
        let h: f32 = h.trim().parse().ok()?;
        if h == 0.0 {
            return None;
        }
        return Some(w / h);
    }
    v.trim().parse().ok()
}

fn strip_ci_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    if s.len() < word.len() {
        return None;
    }
    let (head, rest) = s.split_at(word.len());
    if head.eq_ignore_ascii_case(word) && rest.chars().next().map(|c| c.is_whitespace()).unwrap_or(true) {
        Some(rest)
    } else {
        None
    }
}

fn split_first_word(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}
