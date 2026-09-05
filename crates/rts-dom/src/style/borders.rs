//! BORDAS POR LADO (`border-top`, `border-left-color`, …) e `outline`.
//!
//! O modelo tinha UMA borda uniforme (`border_width`/`border_style`/
//! `border_color`), e a folha real da Wikipédia usa `border-bottom` 17 vezes e
//! `border-top` 16 — uma barra separadora é quase sempre um lado só. Pintar essa
//! declaração com a borda uniforme daria uma moldura fechada onde a página pede
//! uma linha: ERRADO de forma mais visível do que ignorá-la, que era o estado
//! anterior.
//!
//! ## Como o valor por lado convive com o uniforme
//!
//! Os campos por lado são um SEGUNDO nível, não uma substituição: `border_widths`
//! (um [`Edges`], que já mescla lado a lado na cascade) e `border_<lado>_style` /
//! `border_<lado>_color`. Quem lê pede [`resolved_sides`], que faz o fallback para
//! a borda uniforme lado a lado — assim `border: 1px solid red; border-top: none`
//! dá o que o browser dá, e `border: 1px solid red` sozinho continua a passar pelo
//! caminho uniforme que já existia.
//!
//! Rejeitado: dar quatro campos completos a cada lado e apagar o uniforme. Isso
//! obrigava a reescrever o paint, o `has_box`, o `measure` e os 232 testes por uma
//! propriedade que 90% das páginas declara uniforme.
//!
//! ## `outline` (<https://developer.mozilla.org/en-US/docs/Web/CSS/outline>)
//!
//! Uma borda que NÃO ocupa espaço: desenha-se por fora da caixa e não entra no box
//! model. É por isso que vive aqui e não no `Edges` — nada no layout a soma.
//! `outline-offset` é aceite e guardado. Cortes: o outline é sempre RETANGULAR
//! (não segue o `border-radius`, ao contrário do Chrome moderno) e `auto` vale
//! `solid`, porque não temos o anel de foco próprio da plataforma.

use super::props::ComputedStyle;
use super::aplica::set_if;
use super::values::{BorderStyle, Dimension, Rgba, Side};

/// Um dos quatro lados, na ordem do CSS (top/right/bottom/left).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SideName {
    Top,
    Right,
    Bottom,
    Left,
}

impl SideName {
    /// O lado nomeado por um sufixo de propriedade (`"top"` → `Top`).
    pub fn parse(v: &str) -> Option<SideName> {
        Some(match v {
            "top" => SideName::Top,
            "right" => SideName::Right,
            "bottom" => SideName::Bottom,
            "left" => SideName::Left,
            _ => return None,
        })
    }
}

/// A borda EFETIVA de um lado, já resolvida: largura em pontos, estilo e cor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SideBorder {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Rgba,
}

impl SideBorder {
    /// `true` se este lado pinta alguma coisa (largura > 0 E estilo visível). O
    /// default do CSS para `border-style` é `none` — sem estilo declarado, uma
    /// largura sozinha não desenha nada (fiel ao Chrome).
    pub fn paints(self) -> bool {
        self.width > 0.0 && self.style.is_visible()
    }
}

/// Escreve a largura de UM lado (em `border_widths`).
pub fn set_side_width(css: &mut ComputedStyle, side: SideName, w: Option<f32>) {
    set_side_width_dim(css, side, w.map(Dimension::Px))
}

/// Como [`set_side_width`], mas com a DIMENSÃO como veio (`.3em`, `1rem`): a
/// borda em `em` resolve contra a fonte do elemento em `resolved_sides`, e é o
/// que dá ao caret `.dropdown-toggle::after` do Bootstrap (só bordas de `.3em`)
/// os seus 9,6px em vez de `medium` (`claude-borda-em`).
pub fn set_side_width_dim(css: &mut ComputedStyle, side: SideName, w: Option<Dimension>) {
    let v = match w {
        Some(d) => Side::Len(d),
        None => Side::Unset,
    };
    match side {
        SideName::Top => css.border_widths.top = v,
        SideName::Right => css.border_widths.right = v,
        SideName::Bottom => css.border_widths.bottom = v,
        SideName::Left => css.border_widths.left = v,
    }
}

/// Escreve o estilo de UM lado.
pub fn set_side_style(css: &mut ComputedStyle, side: SideName, s: Option<BorderStyle>) {
    match side {
        SideName::Top => css.border_top_style = s,
        SideName::Right => css.border_right_style = s,
        SideName::Bottom => css.border_bottom_style = s,
        SideName::Left => css.border_left_style = s,
    }
}

/// Escreve a cor de UM lado.
pub fn set_side_color(css: &mut ComputedStyle, side: SideName, c: Option<Rgba>) {
    match side {
        SideName::Top => css.border_top_color = c,
        SideName::Right => css.border_right_color = c,
        SideName::Bottom => css.border_bottom_color = c,
        SideName::Left => css.border_left_color = c,
    }
}

/// O shorthand `border-<lado>: <width> || <style> || <color>` — os três em
/// qualquer ordem, qualquer um omitível (MDN). Igual ao `border` uniforme, mas
/// escrevendo só no lado nomeado.
///
/// `border-top: none` (só o estilo) tem de APAGAR a borda daquele lado, e é por
/// isso que o estilo omitido cai em `None` explicitamente: sem isso, um
/// `border: 1px solid` anterior sobreviveria no lado que a página desligou.
pub fn apply_side_shorthand(css: &mut ComputedStyle, side: SideName, val: &str) {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for tok in val.split_whitespace() {
        if let Some(s) = BorderStyle::parse(tok) {
            style = Some(s);
        } else if let Some(w) = parse_width_dim(tok) {
            width = Some(w);
        } else if let Some(c) = super::color::parse_color(tok) {
            color = Some(c);
        }
    }
    // A spec: o shorthand SETA as três longhands, e o que foi omitido vai ao
    // valor inicial (width=medium, style=none, color=currentColor). Seguimos isso
    // para o ESTILO — que é o que decide se a linha aparece — e para a largura;
    // a cor omitida fica a herdar (`currentColor` sem um campo próprio para ela).
    set_side_style(css, side, Some(style.unwrap_or(BorderStyle::None)));
    set_side_width_dim(css, side, Some(width.unwrap_or(Dimension::Px(3.0))));
    if color.is_some() {
        set_side_color(css, side, color);
    }
}

/// Largura de borda como DIMENSÃO: `thin`/`medium`/`thick`, zero, ou qualquer
/// comprimento — `.3em`, `1rem` incluídos, que `parse_width_token` (só px)
/// deixava cair. Quem resolve `em` é `resolved_sides`, contra a fonte do nó.
pub fn parse_width_dim(tok: &str) -> Option<Dimension> {
    let low = tok.trim().to_ascii_lowercase();
    match low.as_str() {
        "thin" => return Some(Dimension::Px(1.0)),
        "medium" => return Some(Dimension::Px(3.0)),
        "thick" => return Some(Dimension::Px(5.0)),
        _ => {}
    }
    let num = low.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%').trim();
    if num.parse::<f32>().map(|n| n == 0.0).unwrap_or(false) {
        return Some(Dimension::Px(0.0));
    }
    super::lengths::parse_dimension_pub(&low).filter(|d| !matches!(d, Dimension::Auto | Dimension::Percent(_)))
}

/// Largura de borda de um token: `thin`/`medium`/`thick` (os valores do Chrome:
/// 1/3/5 px) ou um comprimento absoluto.
pub fn parse_width_token(tok: &str) -> Option<f32> {
    let low = tok.trim().to_ascii_lowercase();
    match low.as_str() {
        "thin" => return Some(1.0),
        "medium" => return Some(3.0),
        "thick" => return Some(5.0),
        _ => {}
    }
    // ZERO é uma largura válida, e o `parse_len` recusa-a: o `parse_px` filtra
    // `> 0`, portanto `border-width: 0` e `border-top-width: 0px` devolviam
    // `None` e a declaração caía — o lado ficava `Unset` e herdava a borda
    // uniforme, dando largura a um lado que o autor mandou apagar. É o mesmo
    // defeito do shorthand multi-valor e da mesma origem, e aparece em toda
    // forma `0 200px 100px 0`, que é como se escreve um triângulo.
    //
    // A correção fica AQUI e não no `parse_px`: aquele serve `width`/`height`,
    // onde o filtro `> 0` distingue "não declarado" de "zero" para outros
    // consumidores. Mudá-lo seria consertar uma borda e mexer no box model todo.
    let num = low
        .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%')
        .trim();
    if num.parse::<f32>().map(|n| n == 0.0).unwrap_or(false) {
        return Some(0.0);
    }
    super::lengths::parse_len_pub(&low)
}

/// `true` se TODOS os tokens do shorthand `border: <width> <style> <color>`
/// classificam em largura, estilo ou cor.
///
/// CSS 2.1 não deixa "os componentes válidos aplicam-se, o inválido ignora-se
/// sozinho": um valor que não corresponde à gramática da propriedade invalida
/// a declaração INTEIRA, que fica como se não tivesse sido escrita — a
/// cascata mantém o que já lá estava. `border: red solid -1px` tinha o
/// inverso: `-1px` não classifica em nada (largura negativa é inválida em
/// qualquer unidade), mas o resto do shorthand corria na mesma e escrevia
/// style=solid/color=red por cima de um `border-width: 8px` de uma regra
/// anterior — a parte INVÁLIDA "vencia" ao apagar a válida em vez de reprovar
/// a linha toda (`border-width-010` do WPT).
pub fn shorthand_tokens_all_valid(val: &str) -> bool {
    val.split_whitespace().all(|tok| {
        BorderStyle::parse(tok).is_some()
            || parse_width_token(tok).is_some()
            || parse_width_dim(tok).is_some()
            || super::color::parse_color(tok).is_some()
    })
}

/// `true` se o nome é uma longhand de borda POR LADO
/// (`border-<lado>-<width|style|color>`). Reconhecer pela FORMA — e não com doze
/// braços literais no `match` do parse — mantém o dispatch por nome num sítio só
/// e não deixa esquecer uma combinação.
pub fn is_longhand(prop: &str) -> bool {
    split_longhand(prop).is_some()
}

/// Parte `border-top-width` em `(Top, "width")`. `None` se não é uma longhand
/// por lado (inclui `border-width`, que é a uniforme e tem braço próprio).
fn split_longhand(prop: &str) -> Option<(SideName, &str)> {
    let rest = prop.strip_prefix("border-")?;
    let (side, what) = rest.split_once('-')?;
    // Só as três longhands reais. `border-top-left-radius` casaria a forma
    // (`top` + resto) e ficaria engolido em silêncio — escrito no raio ÚNICO,
    // arredondaria também os outros três cantos. A recusa continua certa, e agora
    // o canto tem para onde ir: `style::radius` tem um campo por canto.
    matches!(what, "width" | "style" | "color").then_some(())?;
    Some((SideName::parse(side)?, what))
}

/// Aplica uma longhand por lado já reconhecida por [`is_longhand`].
pub fn apply_longhand(css: &mut ComputedStyle, prop: &str, val: &str) {
    let Some((side, what)) = split_longhand(prop) else {
        return;
    };
    match what {
        "width" => set_side_width_dim(css, side, parse_width_dim(val)),
        "style" => set_side_style(css, side, BorderStyle::parse(val)),
        "color" => set_side_color(css, side, super::color::parse_color(val)),
        // Inalcançável: o `split_longhand` acima já recusou tudo o que não é uma
        // das três. Os cantos são de `style::radius`, que tem um campo por canto.
        _ => {}
    }
}

/// `outline: <width> || <style> || <color>` — a mesma classificação por token do
/// `border`, para os campos de outline. `auto` conta como `solid` (não há anel de
/// foco de plataforma para imitar).
pub fn apply_outline_shorthand(css: &mut ComputedStyle, val: &str) {
    let mut style = None;
    for tok in val.split_whitespace() {
        if tok.eq_ignore_ascii_case("auto") {
            style = Some(BorderStyle::Solid);
        } else if let Some(s) = BorderStyle::parse(tok) {
            style = Some(s);
        } else if let Some(w) = parse_width_token(tok) {
            set_if(&mut css.outline_width, Some(w));
        } else if let Some(c) = super::color::parse_color(tok) {
            set_if(&mut css.outline_color, Some(c));
        }
    }
    set_if(&mut css.outline_style, Some(style.unwrap_or(BorderStyle::None)));
    if css.outline_width.is_none() {
        set_if(&mut css.outline_width, Some(3.0)); // `medium`, o inicial da spec
    }
}

/// As quatro bordas EFETIVAS (top, right, bottom, left), com o fallback para a
/// borda uniforme por lado. É o que o paint consome.
pub fn resolved_sides(css: &ComputedStyle) -> [SideBorder; 4] {
    // REINSTAURADO: `medium` (3px) É o inicial de `border-*-width` (CSS2.1
    // §border-width), e é seguro pôr aqui porque `paints()`/`used_widths`
    // continuam a zerar a largura USADA sempre que o estilo não é visível —
    // um lado sem `border-style` nenhum tem `us = BorderStyle::None`, `paints()`
    // dá falso, e o `unwrap_or(3.0)` nunca chega a pintar nem a ocupar espaço.
    //
    // O quadrado cinzento de `border-width-002` que motivou o revert anterior
    // media 128,128,128 — a cor exata do fallback `border_color.unwrap_or
    // (0x808080FF)` que este mesmo lote acabou de substituir por `currentColor`
    // (ver `uc` abaixo). E a causa de ele aparecer sem largura nenhuma
    // declarada era o OUTRO bug deste lote: `parse_width_token` não conhecia
    // `in`/`cm`/`mm`/`q`, então `border-width: 0.5in 0.25in` caía a `None` nos
    // quatro lados e o `unwrap_or(3.0)` de então aplicava-se a TODOS — um
    // quadrado uniforme de 3+3=6px em vez do retângulo assimétrico esperado.
    // Com as duas causas já corrigidas (unidades absolutas no shorthand +
    // `currentColor`), a shorthand de dois valores dá `Side::Len` explícito
    // aos quatro lados e nunca toca neste `uw` — o cenário que quebrou já não
    // existe. Fica para quem mediu depois confirmar com um binário fresco.
    let uw = css.border_width.unwrap_or(3.0);
    let us = css.border_style.unwrap_or(BorderStyle::None);
    // `currentColor` é o inicial de `border-*-color` (CSS Backgrounds 3
    // §border-color), e sem um `color` declarado isso é PRETO — não um
    // cinzento de fallback. Era `unwrap_or(0x808080FF)` sem olhar para
    // `css.color`, e por isso um `border-width:0.5in` sem `border-color`
    // pintava cinzento (128,128,128) onde a referência do WPT pinta preto: a
    // geometria já batia (96×96 no sítio certo), só a cor divergia — cluster
    // de 9216px, 35 fixtures (`border-width-001` e vizinhos). O padrão certo
    // já existe em `pintura.rs:391` para `outline`: `.or(css.color)` antes do
    // preto inicial.
    let uc = css.border_color.or(css.color).unwrap_or(0x000000FF);
    // `em`/`rem` num lado resolvem contra a fonte DESTE nó (a computada é px
    // depois da cascade); `%` não existe em bordas e a viewport não entra.
    let fonte = match css.font_size {
        Some(Dimension::Px(f)) => f,
        _ => 16.0,
    };
    let rc = super::ResolveCtx {
        parent_content_w: 0.0,
        node_font_size: fonte,
        root_font_size: super::root_font_size(),
        viewport_w: 0.0,
        viewport_h: 0.0,
    };
    let one = |w: Side, s: Option<BorderStyle>, c: Option<Rgba>| SideBorder {
        width: w.px().or_else(|| w.resolve(&rc)).unwrap_or(uw).max(0.0),
        style: s.unwrap_or(us),
        color: c.unwrap_or(uc),
    };
    [
        one(
            css.border_widths.top,
            css.border_top_style,
            css.border_top_color,
        ),
        one(
            css.border_widths.right,
            css.border_right_style,
            css.border_right_color,
        ),
        one(
            css.border_widths.bottom,
            css.border_bottom_style,
            css.border_bottom_color,
        ),
        one(
            css.border_widths.left,
            css.border_left_style,
            css.border_left_color,
        ),
    ]
}

/// Repõe os campos POR LADO — o que o shorthand `border` tem de fazer antes de
/// escrever os seus três valores.
///
/// `border-left: 20px solid; border: 6px solid` dá 6px nos quatro lados, porque
/// o shorthand curto escreve as DOZE longhands e não só as três uniformes. Sem
/// esta limpeza, o lado declarado antes sobrevivia e a caixa saía 14px mais
/// larga do que no Chrome (medido em `claude-border-lados`).
///
/// ⚠️ CORTE: a reposição vale dentro do MESMO bloco de declarações, que é onde a
/// ordem é conhecida. Entre regras diferentes, a cascade mescla campo a campo e
/// um `border-left` de uma regra menos específica sobrevive a um `border` de uma
/// mais específica. Distingui-los exige guardar "declarado como inicial" em vez
/// de `None` em toda a struct — a mesma dívida que o shorthand `background` tem,
/// e pela mesma razão.
pub fn clear_sides(css: &mut ComputedStyle) {
    css.border_widths = super::values::Edges::default();
    css.border_top_style = None;
    css.border_right_style = None;
    css.border_bottom_style = None;
    css.border_left_style = None;
    css.border_top_color = None;
    css.border_right_color = None;
    css.border_bottom_color = None;
    css.border_left_color = None;
}

/// As larguras USADAS das quatro bordas, na ordem (top, right, bottom, left) —
/// o que o BOX MODEL soma à caixa.
///
/// Não é o mesmo que a largura declarada, e a diferença é uma regra da spec:
/// **`border-style: none` faz a largura usada ser ZERO**, por mais que o autor
/// declare `border-right-width: 30px`. Sem isto, um lado que não pinta ocupava
/// espaço na mesma — medido no corpus (`claude-border-lados`), é a diferença
/// entre a caixa de 200px que o Chrome dá e uma de 230px.
///
/// É a mesma função que decide a PINTURA ([`SideBorder::paints`]), e de
/// propósito: uma borda que ocupa espaço mas não pinta, ou o contrário, é a
/// forma mais barata de o layout e o render discordarem sobre a mesma caixa.
pub fn used_widths(css: &ComputedStyle) -> [f32; 4] {
    let s = resolved_sides(css);
    [
        if s[0].paints() { s[0].width } else { 0.0 },
        if s[1].paints() { s[1].width } else { 0.0 },
        if s[2].paints() { s[2].width } else { 0.0 },
        if s[3].paints() { s[3].width } else { 0.0 },
    ]
}

/// `true` se algum lado foi declarado à parte — o gatilho para o paint sair do
/// caminho uniforme (um `DisplayItem::Border`) e emitir uma barra por lado.
pub fn has_per_side(css: &ComputedStyle) -> bool {
    css.border_widths.any_set()
        || css.border_top_style.is_some()
        || css.border_right_style.is_some()
        || css.border_bottom_style.is_some()
        || css.border_left_style.is_some()
        || css.border_top_color.is_some()
        || css.border_right_color.is_some()
        || css.border_bottom_color.is_some()
        || css.border_left_color.is_some()
}

// ── Os shorthands de CAIXA: `border-width`, `border-style`, `border-color` ────

/// Reparte 1 a 4 valores pelos quatro lados, na regra dos shorthands de caixa:
/// 1 = todos; 2 = vertical / horizontal; 3 = topo / horizontal / baixo;
/// 4 = topo / direita / baixo / esquerda. `None` se não há valores ou há mais
/// de quatro (valor malformado — nada é escrito).
///
/// **Não é a regra dos cantos de raio**, que copiam a DIAGONAL — ver
/// `style::radius`. As duas formas parecem-se e a diferença é silenciosa, por
/// isso vivem em módulos separados com a regra escrita em cada um.
fn quatro_lados(val: &str) -> Option<([String; 4], bool)> {
    let t = super::lengths::split_top_ws(val);
    let g = |i: usize| t[i].clone();
    let lados = match t.len() {
        1 => [g(0), g(0), g(0), g(0)],
        2 => [g(0), g(1), g(0), g(1)],
        3 => [g(0), g(1), g(2), g(1)],
        4 => [g(0), g(1), g(2), g(3)],
        _ => return None,
    };
    // O segundo membro diz se houve UM valor só — e portanto se o campo
    // UNIFORME também deve ser escrito. Vem daqui em vez de ser recontado por
    // quem chama: dois sítios a repartir o mesmo valor são dois sítios para
    // discordarem sobre quantos valores ele tinha.
    Some((lados, t.len() == 1))
}

/// A ordem em que `quatro_lados` devolve os lados.
const ORDEM: [SideName; 4] = [
    SideName::Top,
    SideName::Right,
    SideName::Bottom,
    SideName::Left,
];

/// `border-width: <1 a 4 larguras>`.
///
/// ## Porque isto existe: uma declaração de quatro valores era DESCARTADA
///
/// O braço do parse fazia `css.border_width = parse_len(val)`, e o `parse_len`
/// lê UM comprimento — `border-width: 100px 0 0 159154.92px` devolvia `None` e a
/// declaração caía inteira, em silêncio. Não é um caso de borda: é como se
/// desenha um TRIÂNGULO em CSS (conteúdo 0x0, três lados a zero e um enorme, a
/// caixa É a borda), e o gráfico de setores da Wikipédia faz isso. Medido: 201
/// 954 px, **24,9% de todo o erro de largura da página**, em 36 elementos.
///
/// O consumidor por lado já estava certo ([`used_widths`]) — só nunca recebia os
/// valores.
///
/// O campo UNIFORME (`border_width`) continua a ser escrito no caso de UM valor,
/// que é o que ele sempre significou; com dois ou mais não há largura uniforme
/// que o descreva, e escrever lá uma das quatro seria dar uma resposta errada a
/// quem lê o campo em vez de nenhuma.
pub fn apply_width_shorthand(css: &mut ComputedStyle, val: &str) {
    let Some((v, uniforme)) = quatro_lados(val) else {
        return;
    };
    for (i, lado) in ORDEM.into_iter().enumerate() {
        set_side_width(css, lado, parse_width_token(&v[i]));
    }
    if uniforme {
        set_if(&mut css.border_width, parse_width_token(&v[0]));
    }
}

/// `border-style: <1 a 4 estilos>` — a mesma repartição, o mesmo motivo.
///
/// Vale por si mesmo e não só por simetria: um triângulo com as quatro larguras
/// certas e sem estilo continua invisível E sem ocupar espaço, porque
/// [`used_widths`] zera a largura de um lado que não pinta. Corrigir a largura
/// sem corrigir o estilo não teria movido nada.
pub fn apply_style_shorthand(css: &mut ComputedStyle, val: &str) {
    let Some((v, uniforme)) = quatro_lados(val) else {
        return;
    };
    for (i, lado) in ORDEM.into_iter().enumerate() {
        set_side_style(css, lado, BorderStyle::parse(&v[i]));
    }
    if uniforme {
        set_if(&mut css.border_style, BorderStyle::parse(&v[0]));
    }
}

/// `border-color: <1 a 4 cores>` — idem.
///
/// ⚠️ Uma cor pode ter ESPAÇOS dentro (`rgb(0, 0, 0)` quando escrita com
/// espaços, `rgb(0 0 0)`), e é por isso que a repartição usa o `split_top_ws`,
/// que respeita parênteses: um `split_whitespace` transformaria uma cor em três
/// lados.
pub fn apply_color_shorthand(css: &mut ComputedStyle, val: &str) {
    let Some((v, uniforme)) = quatro_lados(val) else {
        return;
    };
    for (i, lado) in ORDEM.into_iter().enumerate() {
        set_side_color(css, lado, super::color::parse_color(&v[i]));
    }
    if uniforme {
        set_if(&mut css.border_color, super::color::parse_color(&v[0]));
    }
}
