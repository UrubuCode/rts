//! Parse de DECLARAÇÕES CSS (`prop: valor; ...`) → [`ComputedStyle`]/[`DeclBlock`].
//! É o parser do `style=""` inline E do corpo `{ ... }` de cada regra de
//! stylesheet (reusado por `stylesheet.rs`). Shorthands (`margin`, `border`,
//! `font`, `gap`) expandem para os campos da tabela (`props.rs`) aqui — por isso o
//! dispatch nome→campo é um match explícito, não gerado (1 nome ≠ 1 campo).
//! Ignora propriedade/valor desconhecido sem panicar (robustez de parser real).

use super::color::parse_color;
use super::props::ComputedStyle;
use super::stylesheet::DeclBlock;
use super::values::{
    AlignItems, BorderStyle, Dimension, DisplayKind, Edges, FlexDirection, FloatSide,
    JustifyContent, LineHeight, Position, Side, TextAlign, TextTransform, WhiteSpace,
};

/// Parseia um `style="prop: valor; ..."` para um [`ComputedStyle`] (só a camada
/// NORMAL — atalho retrocompatível; `!important` inline é raro). Para a cascade
/// completa com `!important`, use [`parse_inline_block`].
pub fn parse_inline(style: &str) -> ComputedStyle {
    parse_inline_block(style).normal
}

/// Parseia um bloco de declarações CSS (`"prop: valor; outra: x !important"`)
/// separando as camadas normal/important (MDN estágio 1). Ignora
/// propriedades/valores desconhecidos sem panicar (robustez de parser real).
pub fn parse_inline_block(style: &str) -> DeclBlock {
    let mut block = DeclBlock::default();
    for decl in style.split(';') {
        let mut it = decl.splitn(2, ':');
        let (prop, val_raw) = match (it.next(), it.next()) {
            (Some(p), Some(v)) => (p.trim().to_ascii_lowercase(), v.trim()),
            _ => continue,
        };
        // Destaca o sufixo `!important` (case-insensitive) do valor; a camada de
        // destino depende dele.
        let (val, important) = split_important(val_raw);
        let css = if important { &mut block.important } else { &mut block.normal };
        match prop.as_str() {
            "color" => css.color = parse_color(val),
            "background-color" | "background" => css.bg = parse_color(val),
            "font-size" => css.font_size = parse_dimension(val),
            "font-weight" => css.bold = Some(is_bold(val)),
            "font-style" => {
                css.italic =
                    Some(val.eq_ignore_ascii_case("italic") || val.eq_ignore_ascii_case("oblique"))
            }
            // ── Texto/fonte (#1749) ────────────────────────────────────────────────
            "text-align" => css.text_align = TextAlign::parse(val),
            "line-height" => css.line_height = LineHeight::parse(val),
            "white-space" => css.white_space = WhiteSpace::parse(val),
            "text-transform" => css.text_transform = TextTransform::parse(val),
            "font-family" => css.font_family = parse_font_family(val),
            "font" => apply_font_shorthand(css, val),
            // ── overflow (#1744): scroll container interno. `overflow` define os dois
            // eixos; `-x`/`-y` cada um. Reusa o enum do módulo scrollbar.
            "overflow" => {
                let o = crate::scrollbar::Overflow::parse_str(val);
                css.overflow_x = o;
                css.overflow_y = o;
            }
            "overflow-x" => css.overflow_x = crate::scrollbar::Overflow::parse_str(val),
            "overflow-y" => css.overflow_y = crate::scrollbar::Overflow::parse_str(val),
            // ── Box model: shorthand 1/2/3/4 valores + longhands por lado. ─────────
            "padding" => css.padding = parse_edges(val, false),
            "padding-top" => css.padding.top = parse_side(val, false),
            "padding-right" => css.padding.right = parse_side(val, false),
            "padding-bottom" => css.padding.bottom = parse_side(val, false),
            "padding-left" => css.padding.left = parse_side(val, false),
            // margin aceita `auto` (centralização); padding não.
            "margin" => css.margin = parse_edges(val, true),
            "margin-top" => css.margin.top = parse_side(val, true),
            "margin-right" => css.margin.right = parse_side(val, true),
            "margin-bottom" => css.margin.bottom = parse_side(val, true),
            "margin-left" => css.margin.left = parse_side(val, true),
            // shorthand `border: <width> <style> <color>` (qualquer ordem, qualquer
            // omitível). Setar os 3 de uma vez. (Por-lado fica para fase 2.)
            "border" => apply_border_shorthand(css, val),
            "border-width" => css.border_width = parse_len(val),
            "border-style" => css.border_style = BorderStyle::parse(val),
            "border-color" => css.border_color = parse_color(val),
            "border-radius" => css.corner_radius = parse_len(val),
            "width" => css.width = parse_dimension(val),
            // `box-sizing: border-box | content-box` — border-box faz o `width`
            // incluir padding+border (3 cards de 32% cabem). Default content-box.
            "box-sizing" => css.border_box = Some(val.eq_ignore_ascii_case("border-box")),
            // `display` — o eixo/fluxo dos filhos, do CSS (não mais só do defineBlock).
            "display" => css.display = parse_display(val),
            // `flex-wrap` — combina com display:flex para promover a FlexWrap.
            "flex-wrap" => css.flex_wrap = Some(val.eq_ignore_ascii_case("wrap")),
            // ── Flexbox: alinhamento + gap + direção ──────────────────────────────
            "justify-content" => css.justify = JustifyContent::parse(val),
            "align-items" => css.align_items = AlignItems::parse(val),
            "flex-direction" => css.flex_direction = FlexDirection::parse(val),
            "column-gap" => css.gap = parse_dimension(val),
            "row-gap" => css.row_gap = parse_dimension(val),
            // `gap: <row> <col>` (1 valor = ambos; 2 = row col).
            "gap" => {
                let (rg, cg) = parse_gap_pair(val);
                css.row_gap = rg;
                css.gap = cg;
            }
            "height" => css.height = parse_dimension(val),
            "min-width" => css.min_width = parse_dimension(val),
            "max-width" => css.max_width = parse_dimension(val),
            "min-height" => css.min_height = parse_dimension(val),
            "max-height" => css.max_height = parse_dimension(val),
            // `position` + offsets (top/right/bottom/left). Os offsets aceitam
            // negativos (deslocam para fora) — parse_dimension rejeita <0, então
            // px negativo entra por parse direto.
            "float" => css.float_side = FloatSide::parse(val),
            "position" => css.position = Position::parse(val),
            "top" => css.inset_top = parse_inset(val),
            "right" => css.inset_right = parse_inset(val),
            "bottom" => css.inset_bottom = parse_inset(val),
            "left" => css.inset_left = parse_inset(val),
            "transition" => css.transition = crate::anim::TransitionSpec::parse(val),
            "animation" => css.animation = crate::anim::AnimationSpec::parse(val),
            _ => {}
        }
    }
    block
}

/// Separa o sufixo `!important` (case-insensitive, com espaços) de um valor CSS.
/// Devolve `(valor_sem_important, é_important)`. `"red !important"` → `("red", true)`.
fn split_important(val: &str) -> (&str, bool) {
    let v = val.trim();
    // Acha `!important` no fim, tolerante a espaço entre `!` e `important` não — a
    // spec exige `!important` colado (espaço só antes do `!`).
    let lower = v.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix("!important") {
        let cut = stripped.len();
        return (v[..cut].trim_end(), true);
    }
    (v, false)
}

/// `font-size` em px (aceita "18px" ou "18"). Ignora unidades não-px por ora
/// (em/%/rem chegam na fase de unidades). Só valores > 0.
fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim();
    let num = v.strip_suffix("px").unwrap_or(v);
    num.trim().parse::<f32>().ok().filter(|n| *n > 0.0)
}

/// `width` como [`Dimension`], cobrindo as unidades de comprimento usuais:
/// `auto`; `60%` → Percent; `1.5em` → Em; `2rem` → Rem; `50vw`/`80vh` → Vw/Vh;
/// `280`/`280px` → Px. Unidades relativas resolvem TARDE no render (risco 5).
/// Número inválido / unidade desconhecida → `None`. Ordem do match importa: testa
/// sufixos de 3/2 letras (`rem`) ANTES dos de 1 (`%`) e do px implícito.
fn parse_dimension(v: &str) -> Option<Dimension> {
    let v = v.trim();
    if v.eq_ignore_ascii_case("auto") {
        return Some(Dimension::Auto);
    }
    // `calc(...)` — expressão linear reduzida no parse (resolve tarde).
    let low_full = v.to_ascii_lowercase();
    if let Some(inner) = low_full.strip_prefix("calc(").and_then(|r| r.strip_suffix(')')) {
        return parse_calc(inner).map(Dimension::Calc);
    }
    // (sufixo, construtor, clamp_max) — `%`/`vw`/`vh` em 0..=100; resto sem teto.
    let num = |s: &str| s.trim().parse::<f32>().ok().filter(|n| *n >= 0.0);
    let low = v.to_ascii_lowercase();
    // sufixos de 2+ letras primeiro (rem antes de em; px por último implícito).
    if let Some(n) = low.strip_suffix("rem").and_then(num) {
        return Some(Dimension::Rem(n));
    }
    if let Some(n) = low.strip_suffix("em").and_then(num) {
        return Some(Dimension::Em(n));
    }
    if let Some(n) = low.strip_suffix("vw").and_then(num) {
        return Some(Dimension::Vw(n.clamp(0.0, 100.0)));
    }
    if let Some(n) = low.strip_suffix("vh").and_then(num) {
        return Some(Dimension::Vh(n.clamp(0.0, 100.0)));
    }
    if let Some(n) = low.strip_suffix('%').and_then(num) {
        return Some(Dimension::Percent(n.clamp(0.0, 100.0)));
    }
    // px explícito ou número puro.
    num(low.strip_suffix("px").unwrap_or(&low)).map(Dimension::Px)
}

/// Parseia `display: block|flex|inline|inline-block|none` para [`DisplayKind`].
/// Valores não suportados (grid, table, …) → `None` (cai no default da tag).
fn parse_display(v: &str) -> Option<DisplayKind> {
    match v.trim().to_ascii_lowercase().as_str() {
        "block" | "flow-root" => Some(DisplayKind::Block),
        "flex" | "inline-flex" => Some(DisplayKind::Flex),
        "inline" | "inline-block" => Some(DisplayKind::Inline),
        "none" => Some(DisplayKind::None),
        _ => None, // grid/table/etc — não suportado nesta fase.
    }
}

/// Aplica o shorthand `border: <width> <style> <color>` — os 3 em QUALQUER ORDEM,
/// qualquer um omitível (MDN). Classifica cada token: keyword de estilo → style;
/// largura (px/keyword) → width; senão tenta cor. Defaults CSS: style=none (se não
/// vier, a borda não aparece — o render checa `is_visible`), width=medium(3),
/// color=currentColor (aqui deixamos `border_color` como veio / herdado).
fn apply_border_shorthand(css: &mut ComputedStyle, val: &str) {
    for tok in val.split_whitespace() {
        if let Some(style) = BorderStyle::parse(tok) {
            css.border_style = Some(style);
        } else if let Some(w) = parse_border_width_token(tok) {
            css.border_width = Some(w);
        } else if let Some(c) = parse_color(tok) {
            css.border_color = Some(c);
        }
        // token irreconhecível: ignora (robustez).
    }
    // `border: 2px red` sem estilo → o CSS exige style p/ aparecer; mas o shorthand
    // `border` RESETA o style para o default `solid`? Não — a spec diz que o
    // shorthand SETA todos os 3, e se o estilo for omitido vira `none`. Porém o uso
    // real quase sempre traz o estilo. Para fidelidade: se nenhum estilo veio no
    // shorthand, fica `none` (não pinta) — mas só se o width veio (senão é no-op).
    if css.border_style.is_none() && css.border_width.is_some() {
        css.border_style = Some(BorderStyle::None);
    }
}

/// Largura de borda de um token: `thin`/`medium`/`thick` ou um comprimento px.
fn parse_border_width_token(tok: &str) -> Option<f32> {
    match tok.to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => parse_px(tok),
    }
}

/// Um offset de posicionamento (`top`/`left`/…): como [`parse_dimension`], MAS
/// aceita valores NEGATIVOS em px (deslocar para fora é comum em badges/tooltips);
/// as unidades relativas continuam ≥ 0 via parse_dimension.
fn parse_inset(v: &str) -> Option<Dimension> {
    let t = v.trim();
    // negativo: só a forma px/número (o parse_dimension rejeita <0).
    if t.starts_with('-') {
        let num = t.strip_suffix("px").unwrap_or(t);
        return num.trim().parse::<f32>().ok().map(Dimension::Px);
    }
    parse_dimension(t)
}

/// Parseia o shorthand `gap: <row-gap> <column-gap>` → `(row_gap, column_gap)`.
/// 1 valor = ambos iguais; 2 valores = row primeiro (ordem CSS). Reusa parse_dimension.
fn parse_gap_pair(val: &str) -> (Option<Dimension>, Option<Dimension>) {
    let parts: Vec<&str> = val.split_whitespace().collect();
    match parts.as_slice() {
        [a] => {
            let d = parse_dimension(a);
            (d, d)
        }
        [r, c] => (parse_dimension(r), parse_dimension(c)),
        _ => (None, None),
    }
}

/// Parseia o shorthand de margin/padding (1/2/3/4 valores) para [`Edges`], com o
/// mapeamento exato do CSS:
/// - 1: todos os lados
/// - 2: `top/bottom` | `left/right` (vertical | horizontal)
/// - 3: `top` | `left/right` | `bottom`
/// - 4: `top` | `right` | `bottom` | `left` (horário)
/// `allow_auto` habilita o keyword `auto` (margin). Tokens inválidos → Unset.
fn parse_edges(val: &str, allow_auto: bool) -> Edges {
    let toks: Vec<Side> = val
        .split_whitespace()
        .map(|t| parse_side(t, allow_auto))
        .collect();
    match toks.as_slice() {
        [a] => Edges::all(*a),
        [v, h] => Edges { top: *v, right: *h, bottom: *v, left: *h },
        [t, h, b] => Edges { top: *t, right: *h, bottom: *b, left: *h },
        [t, r, b, l] => Edges { top: *t, right: *r, bottom: *b, left: *l },
        _ => Edges::default(), // 0 ou >4: ignora (robustez).
    }
}

/// Parseia UM lado de margin/padding: um COMPRIMENTO em qualquer unidade
/// (px/%/em/rem/vw/vh — resolve tarde, como width), `auto` (se `allow_auto`), ou
/// `Unset` se inválido. NEGATIVO permitido (margem negativa é o gutter `.row` do
/// Bootstrap; padding negativo é clampado ≥ 0 no consumo do layout).
fn parse_side(tok: &str, allow_auto: bool) -> Side {
    let t = tok.trim();
    if allow_auto && t.eq_ignore_ascii_case("auto") {
        return Side::Auto;
    }
    match parse_dimension_signed(t) {
        // `auto` já foi tratado acima (só margin); um Auto aqui é inválido
        // (padding: auto não existe) — vira Unset, nunca `Len(Auto)`.
        Some(Dimension::Auto) | None => Side::Unset,
        Some(d) => Side::Len(d),
    }
}

// ── calc() — expressão LINEAR de comprimentos, reduzida no parse ────────────────
// `calc(1.375rem + 1.5vw)` / `calc(100% - 2rem)` / `calc(2 * (1rem + 4px))`.
// Gramática (recursive descent): expr := term (('+'|'-') term)* ;
// term := atom (('*'|'/') atom)* ; atom := comprimento | número | '(' expr ')'.
// Multiplicação/divisão só com ESCALAR de um dos lados (regra da spec). O
// resultado é um [`CalcLen`] (combinação das 6 bases) que resolve TARDE.

/// Um valor intermediário do parser de calc: comprimento (combinação linear) ou
/// número puro (escalar de multiplicação).
enum CalcVal {
    Len(crate::style::CalcLen),
    Num(f32),
}

/// Parseia o MIOLO de um `calc(...)` (sem o `calc(` e `)`), ou `None` se inválido.
fn parse_calc(inner: &str) -> Option<crate::style::CalcLen> {
    let toks: Vec<char> = inner.chars().collect();
    let mut pos = 0usize;
    let v = calc_expr(&toks, &mut pos)?;
    // sobrou lixo → inválido.
    while pos < toks.len() {
        if !toks[pos].is_whitespace() {
            return None;
        }
        pos += 1;
    }
    match v {
        CalcVal::Len(l) => Some(l),
        CalcVal::Num(_) => None, // calc(2) não é um comprimento
    }
}

fn calc_ws(t: &[char], p: &mut usize) {
    while *p < t.len() && t[*p].is_whitespace() {
        *p += 1;
    }
}

fn calc_expr(t: &[char], p: &mut usize) -> Option<CalcVal> {
    let mut acc = calc_term(t, p)?;
    loop {
        calc_ws(t, p);
        let op = match t.get(*p) {
            Some('+') => 1.0f32,
            Some('-') => -1.0f32,
            _ => return Some(acc),
        };
        *p += 1;
        let rhs = calc_term(t, p)?;
        acc = match (acc, rhs) {
            (CalcVal::Len(a), CalcVal::Len(b)) => CalcVal::Len(a.add(b.scale(op))),
            (CalcVal::Num(a), CalcVal::Num(b)) => CalcVal::Num(a + op * b),
            _ => return None, // comprimento ± número é inválido na spec
        };
    }
}

fn calc_term(t: &[char], p: &mut usize) -> Option<CalcVal> {
    let mut acc = calc_atom(t, p)?;
    loop {
        calc_ws(t, p);
        let mul = match t.get(*p) {
            Some('*') => true,
            Some('/') => false,
            _ => return Some(acc),
        };
        *p += 1;
        let rhs = calc_atom(t, p)?;
        acc = match (acc, rhs, mul) {
            (CalcVal::Len(a), CalcVal::Num(k), true) => CalcVal::Len(a.scale(k)),
            (CalcVal::Num(k), CalcVal::Len(a), true) => CalcVal::Len(a.scale(k)),
            (CalcVal::Num(a), CalcVal::Num(b), true) => CalcVal::Num(a * b),
            (CalcVal::Len(a), CalcVal::Num(k), false) if k != 0.0 => CalcVal::Len(a.scale(1.0 / k)),
            (CalcVal::Num(a), CalcVal::Num(b), false) if b != 0.0 => CalcVal::Num(a / b),
            _ => return None, // len*len, num/len, divisão por zero: inválidos
        };
    }
}

fn calc_atom(t: &[char], p: &mut usize) -> Option<CalcVal> {
    calc_ws(t, p);
    if t.get(*p) == Some(&'(') {
        *p += 1;
        let v = calc_expr(t, p)?;
        calc_ws(t, p);
        if t.get(*p) != Some(&')') {
            return None;
        }
        *p += 1;
        return Some(v);
    }
    // número (com sinal) + unidade opcional.
    let start = *p;
    if matches!(t.get(*p), Some('-') | Some('+')) {
        *p += 1;
    }
    while matches!(t.get(*p), Some(c) if c.is_ascii_digit() || *c == '.') {
        *p += 1;
    }
    if *p == start {
        return None;
    }
    let num: f32 = t[start..*p].iter().collect::<String>().parse().ok()?;
    // unidade (letras/%).
    let ustart = *p;
    while matches!(t.get(*p), Some(c) if c.is_ascii_alphabetic() || *c == '%') {
        *p += 1;
    }
    let unit: String = t[ustart..*p].iter().collect::<String>().to_ascii_lowercase();
    use crate::style::CalcLen;
    Some(match unit.as_str() {
        "" => CalcVal::Num(num),
        "px" => CalcVal::Len(CalcLen { px: num, ..Default::default() }),
        "%" => CalcVal::Len(CalcLen { pct: num, ..Default::default() }),
        "em" => CalcVal::Len(CalcLen { em: num, ..Default::default() }),
        "rem" => CalcVal::Len(CalcLen { rem: num, ..Default::default() }),
        "vw" => CalcVal::Len(CalcLen { vw: num, ..Default::default() }),
        "vh" => CalcVal::Len(CalcLen { vh: num, ..Default::default() }),
        _ => return None, // unidade desconhecida (ch/vmin/…): calc inválido
    })
}

/// Como [`parse_dimension`], mas aceita valores NEGATIVOS (margens/offsets). O
/// `%`/`vw`/`vh` não são clampados a 0..=100 aqui (o sinal importa).
fn parse_dimension_signed(v: &str) -> Option<Dimension> {
    let v = v.trim();
    let (neg, abs) = match v.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, v),
    };
    let d = parse_dimension(abs)?;
    if !neg {
        return Some(d);
    }
    Some(match d {
        Dimension::Auto => Dimension::Auto,
        Dimension::Px(x) => Dimension::Px(-x),
        Dimension::Percent(x) => Dimension::Percent(-x),
        Dimension::Em(x) => Dimension::Em(-x),
        Dimension::Rem(x) => Dimension::Rem(-x),
        Dimension::Vw(x) => Dimension::Vw(-x),
        Dimension::Vh(x) => Dimension::Vh(-x),
        Dimension::Calc(c) => Dimension::Calc(c.scale(-1.0)),
    })
}

/// Um comprimento de TEXTO/BORDA (`font-size`/`border-width`/`border-radius`, que
/// são `f32` px no modelo): aceita `px`/número E `rem` (× o root FIXO de 16px —
/// igual ao browser sem `html{font-size}` custom; o Bootstrap define TODA a
/// tipografia em rem). ⚠️ CORTE documentado: `em`/`%` de font-size dependem do
/// font do PAI (só a cascade sabe) — ficam de fora até o campo virar `Dimension`.
fn parse_len(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    if let Some(n) = low.strip_suffix("rem") {
        return n.trim().parse::<f32>().ok().filter(|x| *x > 0.0).map(|x| x * 16.0);
    }
    parse_px(&low)
}

/// `font-weight`: `bold`/`bolder` ou peso numérico ≥ 600 → negrito.
fn is_bold(v: &str) -> bool {
    let v = v.trim();
    if v.eq_ignore_ascii_case("bold") || v.eq_ignore_ascii_case("bolder") {
        return true;
    }
    v.parse::<u32>().map(|w| w >= 600).unwrap_or(false)
}

/// `font-family: A, B, C` → o NOME da 1ª família (sem aspas). É o que guardamos
/// (o backend resolve a fonte real; o motor só precisa saber se é monoespaçada).
fn parse_font_family(v: &str) -> Option<String> {
    let first = v.split(',').next()?.trim().trim_matches(|c| c == '"' || c == '\'');
    (!first.is_empty()).then(|| first.to_string())
}

/// `true` se o nome de família indica fonte MONOESPAÇADA (o backend usa para
/// escolher o atlas mono). Reconhece a keyword genérica `monospace` e nomes comuns.
pub fn is_mono_family(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("mono") || n.contains("courier") || n.contains("consol") || n == "menlo"
}

/// `font: [style] [weight] size[/line-height] family` (shorthand). Parseia os
/// tokens posicionais: o `size` é o 1º comprimento; `/line-height` segue o size; o
/// resto antes do size são style/weight; o resto depois do size é a família.
/// ⚠️ CORTE: a spec diz que o shorthand RESETA as longhands omitidas ao valor
/// inicial (font sem `italic` zera o italic). Aqui só SETAMOS o que vem (não
/// resetamos o omitido) — `font-weight:bold; font:16px X` mantém o bold. E o size em
/// `em/rem/%` não resolve (parse_px só px), igual à longhand font-size.
fn apply_font_shorthand(css: &mut ComputedStyle, val: &str) {
    // separa o `size/line-height` (tem `/`) do resto.
    let tokens: Vec<&str> = val.split_whitespace().collect();
    let mut size_idx = None;
    for (i, t) in tokens.iter().enumerate() {
        // o token de size é o 1º que começa com dígito (ex: 16px, 1.2em, 16px/1.5).
        if t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            size_idx = Some(i);
            break;
        }
    }
    let Some(si) = size_idx else { return };
    // antes do size: style/weight.
    for t in &tokens[..si] {
        if t.eq_ignore_ascii_case("italic") || t.eq_ignore_ascii_case("oblique") {
            css.italic = Some(true);
        } else if is_bold(t) {
            css.bold = Some(true);
        }
    }
    // o size (e line-height opcional após `/`).
    let size_tok = tokens[si];
    let (sz, lh) = match size_tok.split_once('/') {
        Some((s, l)) => (s, Some(l)),
        None => (size_tok, None),
    };
    // px direto; se for relativo (em/rem/%), parse_px falha e fica None (herda) —
    // mesma limitação da longhand font-size (documentada).
    css.font_size = parse_dimension(sz);
    if let Some(l) = lh {
        css.line_height = LineHeight::parse(l);
    }
    // depois do size: a família.
    if si + 1 < tokens.len() {
        css.font_family = parse_font_family(&tokens[si + 1..].join(" "));
    }
}
