//! Parse de DECLARAÇÕES CSS (`prop: valor; ...`) → [`ComputedStyle`]/[`DeclBlock`].
//! É o parser do `style=""` inline E do corpo `{ ... }` de cada regra de
//! stylesheet (reusado por `stylesheet.rs`). Shorthands (`margin`, `border`,
//! `font`, `gap`) expandem para os campos da tabela (`props.rs`) aqui — por isso o
//! dispatch nome→campo é um match explícito, não gerado (1 nome ≠ 1 campo).
//! Ignora propriedade/valor desconhecido sem panicar (robustez de parser real).

use super::color::parse_color;
use super::lengths::{
    parse_dimension, parse_dimension_signed, parse_edges, parse_gap_pair, parse_inset, parse_len,
    parse_px, parse_side, parse_signed_px, split_top_ws,
};
use super::props::ComputedStyle;
use super::stylesheet::DeclBlock;
use super::values::{
    AlignItems, BorderStyle, Dimension, DisplayKind, Edges, FlexDirection, FloatSide,
    JustifyContent, LineHeight, Position, Side, TextAlign, TextTransform, Visibility,
    WhiteSpace,
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
    let _phase = crate::metrics::phases::scope("parse-decls");
    let mut block = DeclBlock::default();
    for decl in style.split(';') {
        let mut it = decl.splitn(2, ':');
        let (prop, val_raw) = match (it.next(), it.next()) {
            (Some(p), Some(v)) => (p.trim().to_ascii_lowercase(), v.trim()),
            _ => continue,
        };
        crate::bump!(css_declarations);
        // Destaca o sufixo `!important` (case-insensitive) do valor; a camada de
        // destino depende dele.
        let (val, important) = split_important(val_raw);
        // CUSTOM PROPERTY (`--nome: valor`): guarda o valor CRU no bloco — a
        // cascade por elemento resolve (#1779). Importância ignorada (v1).
        if prop.starts_with("--") {
            crate::bump!(css_custom_declarations);
            block.custom.push((prop.clone(), val.to_string()));
            continue;
        }
        // Valor com `var()`: NÃO parseia agora — vira declaração PENDENTE, que a
        // cascade resolve POR ELEMENTO (contra as custom props dele) na posição
        // desta regra.
        if val.contains("var(") {
            crate::bump!(css_var_refs);
            block.pending.push((prop.clone(), val.to_string(), important));
            continue;
        }
        // `inherit` — vale para QUALQUER propriedade e não se parece com nenhum
        // valor: guarda-se o NOME, e a passada de herança copia o campo do pai
        // (ver `style::inherit_kw`). Antes disto, a declaração era descartada em
        // silêncio, o que deixava vencer uma regra menos específica.
        let css = if important { &mut block.important } else { &mut block.normal };
        if val.eq_ignore_ascii_case("inherit") {
            let mut nomes = css.inherit_props.as_deref().cloned().unwrap_or_default();
            if !nomes.contains(&prop) {
                nomes.push(prop.clone());
            }
            css.inherit_props = Some(std::sync::Arc::new(nomes));
            continue;
        }
        match prop.as_str() {
            "color" => css.color = parse_color(val),
            "background-color" => css.bg = parse_color(val),
            // SHORTHAND `background` — cor + imagem/gradiente + position/size/
            // repeat, em qualquer ordem (ver `style::background`, que também lista
            // o que ficou de fora). Antes daqui só a forma "o valor INTEIRO é uma
            // cor ou um gradiente" era lida, então `background: #fff url(x)
            // no-repeat` — a forma da folha real — não pintava fundo nenhum.
            "background" => {
                let bg = crate::style::background::parse_background(val);
                if let Some(c) = bg.color {
                    css.bg = Some(c);
                }
                if let Some(g) = bg.gradient {
                    css.gradient = Some(g);
                }
                if let Some(i) = bg.image {
                    css.bg_image = Some(i);
                }
                if let Some(p) = bg.position {
                    css.bg_position = Some(p);
                }
                if let Some(s) = bg.size {
                    css.bg_size = Some(s);
                }
                if let Some(r) = bg.repeat {
                    css.bg_repeat = Some(r);
                }
            }
            "background-image" => {
                // Um gradiente É a imagem de fundo e o motor pinta-o; uma `url()`
                // fica guardada crua (não é buscada — ver `style::background`).
                if let Some(g) = crate::style::effects::LinearGradient::parse(val) {
                    css.gradient = Some(g);
                } else {
                    css.bg_image = Some(val.trim().to_string());
                }
            }
            // A máscara é RECONHECIDA, não interpretada: guardamos a url crua só
            // para saber que a forma da caixa vem de fora. O prefixo `-webkit-` é
            // o que a folha real traz ao lado da propriedade padrão (a Wikipédia
            // declara as duas), e ignorá-lo deixava metade das páginas de fora.
            "mask-image" | "-webkit-mask-image" => {
                css.mask_image = Some(val.trim().to_string())
            }
            // `filter` e `clip-path` guardados CRUS, para o paint. O prefixo
            // `-webkit-` está ao lado do nome padrão porque a folha real declara
            // os dois na mesma regra, e reconhecer só um deixaria a metade que a
            // página escreveu primeiro a decidir o resultado. Ver os campos em
            // `props.rs` para o motivo de não serem tipados aqui.
            "filter" | "-webkit-filter" => css.filter = Some(val.trim().to_string()),
            "clip-path" | "-webkit-clip-path" => css.clip_path = Some(val.trim().to_string()),
            "background-repeat" => css.bg_repeat = crate::style::BgRepeat::parse(val),
            "background-position" => css.bg_position = crate::style::BgPosition::parse(val),
            "background-size" => css.bg_size = crate::style::BgSize::parse(val),
            "box-shadow" => css.box_shadow = crate::style::effects::BoxShadow::parse(val),
            "grid-template-columns" => {
                css.grid_columns = parse_grid_columns(val);
                css.grid_template_columns =
                    crate::style::GridTrack::parse_list(val).map(std::sync::Arc::new);
            }
            "grid-template-rows" => {
                css.grid_template_rows =
                    crate::style::GridTrack::parse_list(val).map(std::sync::Arc::new);
            }
            "grid-auto-rows" => {
                css.grid_auto_rows = crate::style::GridTrack::parse_one(val);
            }
            "justify-items" => {
                css.grid_justify_items = crate::style::AlignItems::parse(val);
            }
            "grid-template-areas" => {
                css.grid_template_areas =
                    crate::style::GridAreas::parse(val).map(std::sync::Arc::new);
            }
            "grid-area" => {
                css.grid_area = crate::style::grid_areas::parse_grid_area_name(val);
            }
            "grid" | "grid-template" => {
                // shorthand `grid-template: [áreas] rows / columns`. As linhas de
                // área vêm INTERCALADAS com os tamanhos das linhas, então tirá-las
                // primeiro é o que deixa o resto na forma `rows / cols` que o mesmo
                // código já lia — em vez de um segundo parser para a forma com áreas.
                css.grid_template_areas =
                    crate::style::GridAreas::parse(val).map(std::sync::Arc::new);
                let tracks = crate::style::grid_areas::strip_quoted(val);
                if let Some((rows, cols)) = tracks.split_once('/') {
                    css.grid_template_rows =
                        crate::style::GridTrack::parse_list(rows).map(std::sync::Arc::new);
                    css.grid_template_columns =
                        crate::style::GridTrack::parse_list(cols).map(std::sync::Arc::new);
                    css.grid_columns = parse_grid_columns(cols);
                }
            }
            "transform" => css.transform = crate::style::effects::Transform::parse(val),
            "aspect-ratio" => css.aspect_ratio = parse_aspect_ratio(val),
            "opacity" => {
                // `opacity: <0..1>` (clampa fora do intervalo, como o browser).
                css.opacity = val.trim().parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
            }
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
            "visibility" => css.visibility = Visibility::parse(val),
            "text-transform" => css.text_transform = TextTransform::parse(val),
            "letter-spacing" => {
                // `normal` = 0; senão um comprimento (px/em/rem — resolve p/ px cedo
                // seria ideal, mas letter-spacing quase sempre vem em px/em pequenos;
                // usa parse_len que cobre px). `normal`/inválido → None.
                // `normal` = 0. NEGATIVO é legal e usa-se para apertar títulos
                // (`letter-spacing: -1px`); o `parse_len` recusa-o por servir
                // larguras, daí o caminho com sinal.
                css.letter_spacing = if val.trim().eq_ignore_ascii_case("normal") {
                    Some(0.0)
                } else {
                    parse_signed_px(val)
                };
            }
            "text-decoration" | "text-decoration-line" => {
                apply_text_decoration(css, val, prop != "text-decoration-line")
            }
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
            // Props LÓGICAS (Tailwind v4 usa `px-N`→padding-inline, `py-N`→padding-block
            // em TUDO): inline = left+right, block = top+bottom (modo horizontal LTR).
            "padding-inline" => {
                let s = parse_side(val, false);
                css.padding.left = s;
                css.padding.right = s;
            }
            "padding-block" => {
                let s = parse_side(val, false);
                css.padding.top = s;
                css.padding.bottom = s;
            }
            "padding-inline-start" | "padding-inline-end" => {
                // LTR: start=left, end=right. Sem distinguir aqui, aplica no lado certo.
                let s = parse_side(val, false);
                if prop.as_str() == "padding-inline-start" { css.padding.left = s; } else { css.padding.right = s; }
            }
            // margin aceita `auto` (centralização); padding não.
            "margin" => css.margin = parse_edges(val, true),
            "margin-top" => css.margin.top = parse_side(val, true),
            "margin-right" => css.margin.right = parse_side(val, true),
            "margin-bottom" => css.margin.bottom = parse_side(val, true),
            "margin-left" => css.margin.left = parse_side(val, true),
            "margin-inline" => {
                let s = parse_side(val, true);
                css.margin.left = s;
                css.margin.right = s;
            }
            "margin-block" => {
                let s = parse_side(val, true);
                css.margin.top = s;
                css.margin.bottom = s;
            }
            // LTR: start=left, end=right (o mesmo corte do `padding-inline-*` —
            // `direction:rtl` é aceite mas o layout não inverte; ver `style::text`).
            "margin-inline-start" | "margin-inline-end" => {
                let s = parse_side(val, true);
                if prop.as_str() == "margin-inline-start" {
                    css.margin.left = s;
                } else {
                    css.margin.right = s;
                }
            }
            "margin-block-start" | "margin-block-end" => {
                let s = parse_side(val, true);
                if prop.as_str() == "margin-block-start" {
                    css.margin.top = s;
                } else {
                    css.margin.bottom = s;
                }
            }
            // shorthand `border: <width> <style> <color>` (qualquer ordem, qualquer
            // omitível). Setar os 3 de uma vez. (Por-lado fica para fase 2.)
            "border" => apply_border_shorthand(css, val),
            "border-width" => crate::style::borders::apply_width_shorthand(css, val),
            "border-style" => crate::style::borders::apply_style_shorthand(css, val),
            "border-color" => crate::style::borders::apply_color_shorthand(css, val),
            // O campo UNICO continua a responder o que sempre respondeu (quem o
            // le nao pode mudar de resposta por causa dos cantos); os quatro
            // cantos sao escritos por cima, sem lhe tocar. Ver `style::radius`.
            "border-radius" => {
                css.corner_radius = parse_len(val);
                crate::style::radius::apply_shorthand(css, val);
            }
            // ── Bordas POR LADO: `border-top: 1px solid #ccc` e as 12 longhands.
            // Uma barra separadora é quase sempre um lado só; pintá-la com a borda
            // uniforme daria uma moldura fechada (ver `style::borders`).
            "border-top" | "border-right" | "border-bottom" | "border-left" => {
                if let Some(side) = crate::style::SideName::parse(&prop["border-".len()..]) {
                    crate::style::borders::apply_side_shorthand(css, side, val);
                }
            }
            _ if crate::style::borders::is_longhand(&prop) => {
                crate::style::borders::apply_longhand(css, &prop, val)
            }
            // `outline`: uma borda que não ocupa espaço (fora do box model).
            "outline" => crate::style::borders::apply_outline_shorthand(css, val),
            "outline-width" => css.outline_width = crate::style::borders::parse_width_token(val),
            "outline-style" => {
                css.outline_style = if val.trim().eq_ignore_ascii_case("auto") {
                    Some(BorderStyle::Solid)
                } else {
                    BorderStyle::parse(val)
                }
            }
            "outline-color" => css.outline_color = parse_color(val),
            "outline-offset" => css.outline_offset = parse_signed_px(val),
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
            "align-self" => {
                // `auto` = herda o align-items do container (campo fica None).
                css.align_self = if val.eq_ignore_ascii_case("auto") {
                    None
                } else {
                    AlignItems::parse(val)
                }
            }
            "order" => css.order = val.trim().parse::<i32>().ok(),
            "flex-grow" => css.flex_grow = val.trim().parse::<f32>().ok().filter(|v| *v >= 0.0),
            "flex-shrink" => {
                css.flex_shrink = val.trim().parse::<f32>().ok().filter(|v| *v >= 0.0)
            }
            "flex-basis" => css.flex_basis = parse_dimension(val),
            // shorthand `flex`: none | auto | <grow> [<shrink>] [<basis>] — o
            // `.col` do Bootstrap é `flex: 1 0 0%`.
            "flex" => apply_flex_shorthand(css, val),
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
            "z-index" => css.z_index = val.trim().parse::<i32>().ok(),
            "top" => css.inset_top = parse_inset(val),
            "right" => css.inset_right = parse_inset(val),
            "bottom" => css.inset_bottom = parse_inset(val),
            "left" => css.inset_left = parse_inset(val),
            // ── Texto / listas / fluxo (ver `style::text` p/ o que cada uma faz) ──
            "vertical-align" => css.vertical_align = crate::style::VerticalAlign::parse(val),
            "clear" => css.clear = crate::style::Clear::parse(val),
            "word-break" => css.word_break = crate::style::WordBreak::parse(val),
            // `word-wrap` é o nome legado de `overflow-wrap` (MDN: alias).
            "overflow-wrap" | "word-wrap" => {
                css.overflow_wrap = crate::style::OverflowWrap::parse(val)
            }
            "direction" => css.direction = crate::style::Direction::parse(val),
            // `text-indent` aceita negativo (o truque de esconder texto atrás da
            // margem, comum em logos com fundo).
            "text-indent" => css.text_indent = parse_dimension_signed(val),
            "list-style-type" => css.list_style_type = crate::style::ListStyleType::parse(val),
            "list-style-position" => {
                css.list_style_position = crate::style::ListStylePosition::parse(val)
            }
            // ── Tabela ────────────────────────────────────────────────────────
            "border-collapse" => css.border_collapse = crate::style::BorderCollapse::parse(val),
            "border-spacing" => css.border_spacing = crate::style::BorderSpacing::parse(val),
            "table-layout" => css.table_layout = crate::style::TableLayout::parse(val),
            "list-style-image" => css.list_style_image = Some(val.trim().to_string()),
            // `list-style: <type> || <position> || <image>` — os três em qualquer
            // ordem, e agora os três GUARDADOS: a posição deixou de ser descartada
            // quando o marcador passou a ser desenhado.
            //
            // A ordem dos ramos importa: `none` é um valor válido de `type` E o
            // ficheiro não tem como saber a qual dos dois o autor se referia, por
            // isso o `type` é tentado ANTES da posição — `outside`/`inside` não
            // são valores de `type`, portanto não há ambiguidade no outro sentido.
            "list-style" => {
                for tok in val.split_whitespace() {
                    if tok.to_ascii_lowercase().starts_with("url(") {
                        css.list_style_image = Some(tok.to_string());
                    } else if let Some(t) = crate::style::ListStyleType::parse(tok) {
                        css.list_style_type = Some(t);
                    } else if let Some(p) = crate::style::ListStylePosition::parse(tok) {
                        css.list_style_position = Some(p);
                    }
                }
            }
            // `cursor` — guardado cru; quem o usa é o backend de janela.
            "cursor" => css.cursor = Some(val.trim().to_ascii_lowercase()),
            // `flex-flow: <direction> || <wrap>` (MDN) — só expande.
            "flex-flow" => {
                for tok in val.split_whitespace() {
                    if let Some(d) = FlexDirection::parse(tok) {
                        css.flex_direction = Some(d);
                    } else if tok.eq_ignore_ascii_case("wrap")
                        || tok.eq_ignore_ascii_case("wrap-reverse")
                    {
                        css.flex_wrap = Some(true);
                    } else if tok.eq_ignore_ascii_case("nowrap") {
                        css.flex_wrap = Some(false);
                    }
                }
            }
            "transition" => css.transition = crate::anim::TransitionSpec::parse(val),
            "animation" => css.animation = crate::anim::AnimationSpec::parse(val),
            // Uma propriedade que nenhum braço reconhece é CSS que a página
            // escreveu e o motor ignora em silêncio. Contá-la é o que transforma
            // "o layout não bate com o Chrome" numa lista de nomes a implementar.
            // GRUPOS de propriedades resolvidos por módulo, antes de desistir. Um
            // grupo aqui em vez de treze braços literais mantém a lista de nomes
            // do lado de quem os aplica — uma segunda lista neste `match` era o
            // sítio óbvio para uma delas ficar de fora.
            _ if crate::style::timing::try_apply(css, &prop, val) => {}
            _ if crate::style::logical::try_apply(css, &prop, val) => {}
            _ if crate::style::vocab::try_apply(css, &prop, val) => {}
            _ if crate::style::radius::try_apply(css, &prop, val) => {}
            _ if crate::style::grid_lines::try_apply(css, &prop, val) => {}
            // RECONHECIDA e deliberadamente não modelada: conta noutra coluna,
            // para a coluna das desconhecidas continuar a ser a lista do que
            // falta fazer e não uma mistura com o que foi recusado.
            _ if crate::style::inert::is_inert(&prop) => {
                crate::bump!(css_declarations_inert);
            }
            _ => {
                crate::bump!(css_declarations_unknown);
                crate::note!("propriedade-ignorada", prop.clone());
            }
        }
    }
    block
}

/// Separa o sufixo `!important` (case-insensitive, com espaços) de um valor CSS.
/// Devolve `(valor_sem_important, é_important)`. `"red !important"` → `("red", true)`.
/// Aplica `text-decoration` / `text-decoration-line`. `com_cor` distingue os
/// dois: o SHORTHAND também traz a cor (`underline dotted red`), e o parser da
/// linha já ignora os tokens que não são de linha — por isso a cor não tem onde
/// ser lida a não ser aqui. `-line` não aceita cor, mas nenhum valor de linha
/// parseia como cor, então partilhar o corpo não engana nenhum dos dois.
///
/// É `pub(super)` porque `style::vocab` a chama para as grafias prefixadas
/// (`-webkit-text-decoration`, 6 folhas do corpus), que nunca chegam ao `match`
/// deste ficheiro — ele casa por literal e não vê o prefixo. Uma segunda cópia
/// lá seria duas respostas à mesma pergunta, com a cor a ser lida só numa delas.
pub(super) fn apply_text_decoration(css: &mut ComputedStyle, val: &str, com_cor: bool) {
    css.text_decoration = crate::style::values::TextDecoration::parse(val);
    if com_cor {
        if let Some(c) = val.split_whitespace().find_map(parse_color) {
            css.text_decoration_color = Some(c);
        }
    }
}

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




/// Parseia `display: block|flex|inline|inline-block|none` para [`DisplayKind`].
/// Extrai o Nº DE COLUNAS de `grid-template-columns`: de `repeat(N, ...)` pega N; de
/// uma lista de trilhas (`1fr 1fr 1fr`, `200px 200px`) conta os itens de topo. `None`
/// para valores que não dão um número (auto/subgrid/…). Cobre o padrão Tailwind
/// `grid-cols-N` (= `repeat(N, minmax(0,1fr))`).
fn parse_grid_columns(v: &str) -> Option<i32> {
    let v = v.trim();
    let low = v.to_ascii_lowercase();
    if let Some(i) = low.find("repeat(") {
        let inner = &v[i + "repeat(".len()..];
        // o 1º argumento (antes da 1ª vírgula de topo) é a contagem.
        let count = inner.split(',').next()?.trim();
        return count.parse::<i32>().ok().filter(|n| *n >= 1);
    }
    // lista de trilhas separadas por espaço de TOPO (respeita parênteses de minmax()).
    let n = split_top_ws(v).len() as i32;
    (n >= 1).then_some(n)
}

/// Parseia `aspect-ratio`: `<w> / <h>` (ex. `16 / 9`) ou um número único (`1.5`).
/// `auto`/inválido → `None`. Devolve a razão largura/altura.
fn parse_aspect_ratio(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some((w, h)) = v.split_once('/') {
        let w = w.trim().parse::<f32>().ok()?;
        let h = h.trim().parse::<f32>().ok()?;
        return (h != 0.0 && w > 0.0).then_some(w / h);
    }
    v.parse::<f32>().ok().filter(|r| *r > 0.0)
}

/// Valores não suportados (table, …) → `None` (cai no default da tag).
fn parse_display(v: &str) -> Option<DisplayKind> {
    match v.trim().to_ascii_lowercase().as_str() {
        "block" | "flow-root" => Some(DisplayKind::Block),
        "flex" | "inline-flex" => Some(DisplayKind::Flex),
        "inline" => Some(DisplayKind::Inline),
        // `inline-block` tem variante PRÓPRIA desde que ela existe: colapsá-la em
        // `Inline` fazia o computed responder `inline` onde o browser responde
        // `inline-block` (8 desvios do corpus). Para o LAYOUT continua a valer o
        // mesmo código — `DisplayKind::to_display_code` mapeia as duas no mesmo —,
        // portanto isto corrige a resposta sem mudar a disposição.
        "inline-block" => Some(DisplayKind::InlineBlock),
        "grid" | "inline-grid" => Some(DisplayKind::Grid),
        "none" => Some(DisplayKind::None),
        // `list-item` — o `<li>`. Bloco MAIS um marcador; ver `crate::listitem`.
        "list-item" => Some(DisplayKind::ListItem),
        // Os valores de TABELA. `inline-table` cai em `Table` porque a diferença
        // é só como a caixa participa do fluxo do PAI (inline vs bloco), e por
        // dentro é a mesma repartição de colunas; tratá-lo como caixa inline é
        // um refino, não um algoritmo à parte.
        "table" | "inline-table" => Some(DisplayKind::Table),
        "table-row-group" | "table-header-group" | "table-footer-group" => {
            Some(DisplayKind::TableRowGroup)
        }
        "table-row" => Some(DisplayKind::TableRow),
        "table-cell" => Some(DisplayKind::TableCell),
        "table-caption" => Some(DisplayKind::TableCaption),
        // `table-column`/`table-column-group` (`<col>`/`<colgroup>`) NÃO geram
        // caixa nenhuma no CSS — só carregam largura para as colunas. Devolver
        // `None` aqui os faria cair no default da tag (bloco) e pintar uma caixa
        // vazia que o Chrome não tem; `None` (o display) é o que os apaga.
        "table-column" | "table-column-group" => Some(DisplayKind::None),
        _ => None,
    }
}

/// Aplica o shorthand `border: <width> <style> <color>` — os 3 em QUALQUER ORDEM,
/// qualquer um omitível (MDN). Classifica cada token: keyword de estilo → style;
/// largura (px/keyword) → width; senão tenta cor. Defaults CSS: style=none (se não
/// vier, a borda não aparece — o render checa `is_visible`), width=medium(3),
/// color=currentColor (aqui deixamos `border_color` como veio / herdado).
fn apply_border_shorthand(css: &mut ComputedStyle, val: &str) {
    // O curto escreve as DOZE longhands, não só as três uniformes: um lado
    // declarado antes dele é reposto (ver `borders::clear_sides`).
    crate::style::borders::clear_sides(css);
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


/// Aplica o shorthand `flex: none | auto | <grow> [<shrink>] [<basis>]`.
/// Mapeamentos da spec: `none` = 0 0 auto; `auto` = 1 1 auto; UM número =
/// grow=N shrink=1 basis=0% (o `.col { flex: 1 0 0% }` já vem com os três).
fn apply_flex_shorthand(css: &mut ComputedStyle, val: &str) {
    let v = val.trim();
    if v.eq_ignore_ascii_case("none") {
        css.flex_grow = Some(0.0);
        css.flex_shrink = Some(0.0);
        css.flex_basis = Some(Dimension::Auto);
        return;
    }
    if v.eq_ignore_ascii_case("auto") {
        css.flex_grow = Some(1.0);
        css.flex_shrink = Some(1.0);
        css.flex_basis = Some(Dimension::Auto);
        return;
    }
    let toks: Vec<&str> = v.split_whitespace().collect();
    // separa os NÚMEROS iniciais (grow [shrink]) de uma dimensão final (basis).
    let mut nums: Vec<f32> = Vec::new();
    let mut basis: Option<Dimension> = None;
    for t in &toks {
        if basis.is_none() && nums.len() < 2 {
            if let Ok(n) = t.parse::<f32>() {
                nums.push(n.max(0.0));
                continue;
            }
        }
        if basis.is_none() {
            basis = parse_dimension(t);
        }
    }
    match (nums.len(), basis) {
        // `flex: 200px` — só a basis.
        (0, Some(b)) => css.flex_basis = Some(b),
        (0, None) => {} // inválido: ignora (robustez)
        (n, b) => {
            css.flex_grow = Some(nums[0]);
            css.flex_shrink = Some(if n >= 2 { nums[1] } else { 1.0 });
            // UM número sem basis → basis 0% (spec); com basis explícita, usa-a.
            css.flex_basis = Some(b.unwrap_or(Dimension::Percent(0.0)));
        }
    }
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
