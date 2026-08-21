//! Fundo, máscara, `filter`, `clip-path` e a grelha
//!
//! Os braços vieram do `match` de `aplica_declaracao` VERBATIM — a forma
//! `try_apply` é a mesma que os seis módulos vizinhos já usam, e a
//! indentação é a mesma nos dois sítios.

use super::*;

pub(in crate::style::parse) fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    match prop {
        "color" => set_if(&mut css.color, parse_color(val)),
        "background-color" => set_if(&mut css.bg, parse_color(val)),
        // SHORTHAND `background` — cor + imagem/gradiente + position/size/
        // repeat, em qualquer ordem (ver `style::background`, que também lista
        // o que ficou de fora). Antes daqui só a forma "o valor INTEIRO é uma
        // cor ou um gradiente" era lida, então `background: #fff url(x)
        // no-repeat` — a forma da folha real — não pintava fundo nenhum.
        "background" => {
            let bg = crate::style::background::parse_background(val);
            if let Some(c) = bg.color {
                set_if(&mut css.bg, Some(c));
            }
            if let Some(g) = bg.gradient {
                set_if(&mut css.gradient, Some(g));
            }
            if let Some(i) = bg.image {
                set_if(&mut css.bg_image, Some(i));
            }
            if let Some(p) = bg.position {
                set_if(&mut css.bg_position, Some(p));
            }
            if let Some(s) = bg.size {
                set_if(&mut css.bg_size, Some(s));
            }
            if let Some(r) = bg.repeat {
                set_if(&mut css.bg_repeat, Some(r));
            }
        }
        "background-image" => {
            // Um gradiente É a imagem de fundo e o motor pinta-o; uma `url()`
            // fica guardada crua (não é buscada — ver `style::background`).
            if let Some(g) = crate::style::effects::LinearGradient::parse(val) {
                set_if(&mut css.gradient, Some(g));
            } else {
                set_if(&mut css.bg_image, Some(val.trim().to_string()));
            }
        }
        // A máscara é RECONHECIDA, não interpretada: guardamos a url crua só
        // para saber que a forma da caixa vem de fora. O prefixo `-webkit-` é
        // o que a folha real traz ao lado da propriedade padrão (a Wikipédia
        // declara as duas), e ignorá-lo deixava metade das páginas de fora.
        "mask-image" | "-webkit-mask-image" => set_if(&mut css.mask_image, Some(val.trim().to_string())),
        // `filter` e `clip-path` guardados CRUS, para o paint. O prefixo
        // `-webkit-` está ao lado do nome padrão porque a folha real declara
        // os dois na mesma regra, e reconhecer só um deixaria a metade que a
        // página escreveu primeiro a decidir o resultado. Ver os campos em
        // `props.rs` para o motivo de não serem tipados aqui.
        "filter" | "-webkit-filter" => set_if(&mut css.filter, Some(val.trim().to_string())),
        "clip-path" | "-webkit-clip-path" => set_if(&mut css.clip_path, Some(val.trim().to_string())),
        "background-repeat" => set_if(&mut css.bg_repeat, crate::style::BgRepeat::parse(val)),
        "background-position" => set_if(&mut css.bg_position, crate::style::BgPosition::parse(val)),
        "background-size" => set_if(&mut css.bg_size, crate::style::BgSize::parse(val)),
        "box-shadow" => set_ou_limpa(&mut css.box_shadow, val, crate::style::effects::BoxShadow::parse(val)),
        "grid-template-columns" => {
            set_if(&mut css.grid_columns, parse_grid_columns(val));
            set_ou_limpa(&mut css.grid_template_columns, val,
                crate::style::GridTrack::parse_list(val).map(std::sync::Arc::new));
        }
        "grid-template-rows" => {
            set_ou_limpa(&mut css.grid_template_rows, val,
                crate::style::GridTrack::parse_list(val).map(std::sync::Arc::new));
        }
        "grid-auto-rows" => {
            set_if(&mut css.grid_auto_rows, crate::style::GridTrack::parse_one(val));
        }
        "justify-items" => {
            set_if(&mut css.grid_justify_items, crate::style::AlignItems::parse(val));
        }
        "grid-template-areas" => {
            css.grid_template_areas = crate::style::GridAreas::parse(val).map(std::sync::Arc::new);
        }
        "grid-area" => {
            set_if(&mut css.grid_area, crate::style::grid_areas::parse_grid_area_name(val));
        }
        "grid" | "grid-template" => {
            // shorthand `grid-template: [áreas] rows / columns`. As linhas de
            // área vêm INTERCALADAS com os tamanhos das linhas, então tirá-las
            // primeiro é o que deixa o resto na forma `rows / cols` que o mesmo
            // código já lia — em vez de um segundo parser para a forma com áreas.
            css.grid_template_areas = crate::style::GridAreas::parse(val).map(std::sync::Arc::new);
            let tracks = crate::style::grid_areas::strip_quoted(val);
            if let Some((rows, cols)) = tracks.split_once('/') {
                css.grid_template_rows =
                    crate::style::GridTrack::parse_list(rows).map(std::sync::Arc::new);
                css.grid_template_columns =
                    crate::style::GridTrack::parse_list(cols).map(std::sync::Arc::new);
                set_if(&mut css.grid_columns, parse_grid_columns(cols));
            }
        }
        _ => return false,
    }
    true
}
