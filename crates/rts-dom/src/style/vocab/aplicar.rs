//! O braço de `parse.rs`: nome de propriedade → campo
//!
//! Extraído de `vocab.rs` sem alterar uma linha.

use super::*;

/// `font-stretch` em PERCENTAGEM (100 = `normal`), que é a forma em que a spec
/// define os keywords e a forma em que o computed responde. `None` se o valor não
/// é nem keyword nem percentagem.
fn parse_font_stretch(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    let pct = match low.as_str() {
        "ultra-condensed" => 50.0,
        "extra-condensed" => 62.5,
        "condensed" => 75.0,
        "semi-condensed" => 87.5,
        "normal" => 100.0,
        "semi-expanded" => 112.5,
        "expanded" => 125.0,
        "extra-expanded" => 150.0,
        "ultra-expanded" => 200.0,
        _ => return low.strip_suffix('%')?.trim().parse::<f32>().ok(),
    };
    Some(pct)
}

/// `zoom: <número> | <percentagem> | normal` → o fator (1.0 = sem zoom).
fn parse_zoom(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    if low == "normal" {
        return Some(1.0);
    }
    if let Some(p) = low.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|n| n / 100.0);
    }
    low.parse::<f32>().ok()
}

/// Tenta aplicar uma propriedade deste lote. `false` = o nome não é de nenhuma
/// delas, e o `parse` conta-a como ignorada.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    // O prefixo de fornecedor é um alias do mesmo nome — exceto onde o valor
    // também difere, e nenhuma deste lote é desse caso.
    let name = prop
        .strip_prefix("-webkit-")
        .or_else(|| prop.strip_prefix("-moz-"))
        .unwrap_or(prop);
    match name {
        // ── COM EFEITO REAL: caem em mecanismos que já são consumidos ──────────
        // Os dois eixos de `background-position` em separado. O campo é o mesmo
        // que o shorthand escreve, portanto o render já os pinta.
        "background-position-x" => {
            let mut p = css.bg_position.unwrap_or_default();
            let Some(x) = parse_dimension_or_keyword(val, true) else {
                return true;
            };
            p.x = x;
            set_if(&mut css.bg_position, Some(p));
        }
        "background-position-y" => {
            let mut p = css.bg_position.unwrap_or_default();
            let Some(y) = parse_dimension_or_keyword(val, false) else {
                return true;
            };
            p.y = y;
            set_if(&mut css.bg_position, Some(p));
        }
        // `place-content: <align> <justify>` e `place-self: <align> <justify>` —
        // só expandem para os campos que já existem (o mesmo que `flex-flow` faz).
        "place-content" => {
            let t = split_top_ws(val);
            if let Some(j) = t.last().and_then(|s| JustifyContent::parse(s)) {
                set_if(&mut css.justify, Some(j));
            }
            if let Some(a) = t.first().and_then(|s| JustifyContent::parse(s)) {
                set_if(&mut css.align_content, Some(a));
            }
        }
        // `place-items: <align> <justify>` — o par que `place-content` e
        // `place-self` já expandem ao lado, sobre os campos que o container usa
        // para os ITENS. Um valor só vale para os dois eixos.
        "place-items" => {
            let t = split_top_ws(val);
            if let Some(a) = t.first().and_then(|s| AlignItems::parse(s)) {
                set_if(&mut css.align_items, Some(a));
            }
            if let Some(j) = t.last().and_then(|s| AlignItems::parse(s)) {
                set_if(&mut css.grid_justify_items, Some(j));
            }
        }
        "place-self" => {
            let t = split_top_ws(val);
            if let Some(a) = t.first().and_then(|s| AlignItems::parse(s)) {
                set_if(&mut css.align_self, Some(a));
            }
            if let Some(j) = t.last().and_then(|s| AlignItems::parse(s)) {
                set_if(&mut css.justify_self, Some(j));
            }
        }

        // ── GUARDADAS, SEM GEOMETRIA (o motivo está no tipo de cada uma) ───────
        // `align-content` reusa o vocabulário de `JustifyContent` em vez de um
        // enum próprio com as mesmas seis variantes. O corte: `stretch`/`normal`
        // chegam como `FlexStart`, porque é o que o layout faz hoje com `stretch`
        // no eixo cruzado (ver a nota em `AlignItems::Stretch`).
        "align-content" => set_if(&mut css.align_content, JustifyContent::parse(val)),
        // `justify-self` reusa `AlignItems` pelo mesmo motivo que
        // `grid_justify_items` já reusa: é o mesmo conjunto de posições.
        "justify-self" => set_if(&mut css.justify_self, AlignItems::parse(val)),
        "text-overflow" => set_if(&mut css.text_overflow, TextOverflow::parse(val)),
        // `clip` — ver [`Clip`] para porque é guardada sem recortar. Só chega
        // aqui pelo nome nu; não tem forma prefixada em folha nenhuma do corpus.
        "clip" => set_if(&mut css.clip, Clip::parse(val)),
        "text-wrap" | "text-wrap-mode" => set_if(&mut css.text_wrap, TextWrap::parse(val)),
        "object-fit" => set_if(&mut css.object_fit, ObjectFit::parse(val)),
        // `object-position` tem a MESMA gramática de `background-position` — reusa
        // o parser dela em vez de um segundo parser de posição.
        "object-position" => set_if(&mut css.object_position, BgPosition::parse(val)),
        "unicode-bidi" => set_if(&mut css.unicode_bidi, UnicodeBidi::parse(val)),
        "hyphens" => set_if(&mut css.hyphens, Hyphens::parse(val)),
        "scrollbar-width" => set_if(&mut css.scrollbar_width, ScrollbarWidth::parse(val)),
        "caption-side" => set_if(&mut css.caption_side, CaptionSide::parse(val)),
        "pointer-events" => set_if(&mut css.pointer_events, PointerEvents::parse(val)),
        // `transform-origin` tem a mesma gramatica de `background-position` —
        // reusa o parser dela, como o `object-position` ao lado. O layout
        // continua a rodar em torno do centro; ver o comentario do campo.
        "transform-origin" => set_if(&mut css.transform_origin, BgPosition::parse(val)),
        // Aliases do WebKit para propriedades que ja existem. O `-webkit-` sozinho
        // ja foi tirado no topo; estes tres tem NOME diferente, nao so prefixo, e
        // sao a sintaxe da flexbox de 2009 que o `google.css` ainda escreve.
        "box-orient" => {
            css.flex_direction = match val.trim() {
                "vertical" => Some(super::values::FlexDirection::Column),
                "horizontal" => Some(super::values::FlexDirection::Row),
                _ => return true,
            }
        }
        // `justify` e o nome antigo de `space-between`; os outros coincidem.
        "box-pack" => {
            css.justify = if val.trim() == "justify" {
                Some(JustifyContent::SpaceBetween)
            } else {
                JustifyContent::parse(val)
            }
        }
        "box-align" => set_if(&mut css.align_items, AlignItems::parse(val)),
        // `-webkit-justify-content` / `-webkit-align-items`: o nome e o mesmo, so
        // o prefixo muda — mas o `parse` casa por literal e nao ve o prefixado.
        "justify-content" => set_if(&mut css.justify, JustifyContent::parse(val)),
        "align-items" => set_if(&mut css.align_items, AlignItems::parse(val)),
        // `-webkit-transform`: so chega aqui prefixado — o nome nu tem braco
        // proprio no `parse`, que corre antes deste modulo.
        "transform" => set_if(&mut css.transform, super::effects::Transform::parse(val)),
        "text-decoration-color" => set_if(&mut css.text_decoration_color, super::color::parse_color(val)),
        // `-webkit-text-decoration` / `-moz-text-decoration`: só o prefixo muda,
        // o valor é o mesmo shorthand (é a forma que o WebKit antigo exigia, e
        // 6 das 13 folhas do corpus ainda a escrevem ao lado da nua). O nome nu
        // tem braço próprio no `parse`, que corre antes deste módulo — chamar a
        // MESMA função em vez de repetir o corpo é o que impede as duas grafias
        // de responderem coisas diferentes.
        "text-decoration" => super::parse::apply_text_decoration(css, val, true),
        // As propriedades INDIVIDUAIS de transformacao (`rotate: 45deg`), que a
        // spec define como aplicadas DEPOIS do `transform`. Escrevem no mesmo
        // `Transform` que o shorthand escreve — e por isso ja sao PINTADAS, sem
        // nada por ligar. Um campo proprio por eixo seria uma segunda descricao
        // da mesma transformacao, com o layout a ter de as compor.
        //
        // O CORTE: composicao por ORDEM DE DECLARACAO, nao pela ordem da spec.
        // `transform: rotate(10deg)` depois de `rotate: 45deg` da 55 graus aqui e
        // 10 no browser (o shorthand substitui). Uma folha que declare as duas
        // formas no mesmo elemento e rara; uma que declare so uma sai certa.
        "rotate" => {
            let Some(d) = super::effects::parse_angle_deg(&val.trim()) else {
                return true;
            };
            let mut t = css
                .transform
                .unwrap_or_else(super::effects::Transform::identity);
            t.ops.push(crate::layout::TransformOp::Rotate { deg: d });
            set_if(&mut css.transform, Some(t));
        }
        "scale" => {
            let t2 = split_top_ws(val);
            let Some(sx) = t2.first().and_then(|s| s.parse::<f32>().ok()) else {
                return true;
            };
            let sy = t2.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(sx);
            let mut t = css
                .transform
                .unwrap_or_else(super::effects::Transform::identity);
            t.ops.push(crate::layout::TransformOp::Scale { sx, sy });
            set_if(&mut css.transform, Some(t));
        }
        "font-stretch" => set_if(&mut css.font_stretch, parse_font_stretch(val)),
        "zoom" => set_if(&mut css.zoom, parse_zoom(val)),
        "word-spacing" => {
            // `normal` é 0, e o NEGATIVO vale — como no `letter-spacing` ao
            // lado. Usava um parse de LARGURA, que recusa o sinal: duas
            // propriedades irmãs respondiam ao contrário uma da outra.
            css.word_spacing = if val.trim().eq_ignore_ascii_case("normal") {
                Some(0.0)
            } else {
                super::lengths::parse_signed_px(val)
            }
        }
        // `-webkit-line-clamp: <n>` — corta o bloco a n linhas. `none` = sem
        // limite. Guardada; quem contaria as linhas é o fluxo inline.
        "line-clamp" => {
            css.line_clamp = if val.trim().eq_ignore_ascii_case("none") {
                None
            } else {
                val.trim().parse::<i32>().ok().filter(|n| *n > 0)
            }
        }
        "column-width" => set_if(&mut css.column_width, parse_dimension(val)),
        _ => return false,
    }
    true
}

/// Um valor de eixo de `background-position-x|y`: comprimento/percentagem ou o
/// keyword do eixo. `horizontal` escolhe qual dos dois conjuntos de keywords vale.
fn parse_dimension_or_keyword(v: &str, horizontal: bool) -> Option<Dimension> {
    let low = v.trim().to_ascii_lowercase();
    let pct = match (low.as_str(), horizontal) {
        ("left", true) | ("top", false) => 0.0,
        ("center", _) => 50.0,
        ("right", true) | ("bottom", false) => 100.0,
        _ => return parse_dimension(&low),
    };
    Some(Dimension::Percent(pct))
}
