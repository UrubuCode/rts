//! Os RAIOS POR CANTO (`border-top-left-radius` e as sete companhias).
//!
//! São **1 064 declarações** nas folhas medidas — o maior item que faltava, e
//! quase todo do WhatsApp Web, que escreve os quatro cantos separados e na forma
//! lógica (`border-start-start-radius`). Até aqui caíam todas no contador de
//! ignoradas, com uma recusa DELIBERADA em `style::borders`: o modelo tinha um
//! raio só para os quatro cantos, e escrever um canto declarado nesse campo
//! arredondaria os outros três. Essa recusa continua a ser a resposta certa e não
//! foi levantada — o que mudou é que agora há onde guardar o canto.
//!
//! ## O que ESTE módulo faz, e o que continua por fazer
//!
//! Guarda os quatro cantos em campos próprios e responde-os no computed. **Não
//! pinta.** A lista de display tem UM raio por retângulo
//! (`DisplayItem::SolidRect { radius: f32 }`, em `layout.rs`), e o egui lê-o em
//! `frame/render.rs`; pintar por canto é mudar esse item para quatro valores e o
//! consumidor com ele — outra tarefa, outro dono.
//!
//! ## A regra que não pode quebrar
//!
//! `corner_radius` — o campo único — continua a responder EXATAMENTE o que
//! respondia. Quem já o lê (o fundo, a borda, a barra de rolagem, a tabela) não
//! pode receber resposta diferente por causa deste lote, e por isso o shorthand
//! `border-radius` escreve os dois: o campo único como sempre escreveu, e os
//! quatro cantos por cima. Um canto declarado sozinho NÃO toca o campo único —
//! é a recusa de sempre, agora com o valor guardado ao lado em vez de perdido.

use super::lengths::{parse_len_pub, split_top_ws};
use super::props::ComputedStyle;

/// Os quatro cantos, na ordem em que o shorthand os escreve.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

impl Corner {
    /// O canto nomeado pelo sufixo de uma longhand, física ou LÓGICA.
    ///
    /// As lógicas assumem LTR horizontal, que é o mesmo corte de
    /// `padding-inline-start` e de `style::logical` — um segundo comportamento
    /// para "que lado é `start`" seria pior que o corte.
    fn parse(sufixo: &str) -> Option<Corner> {
        Some(match sufixo {
            "top-left" | "start-start" => Corner::TopLeft,
            "top-right" | "start-end" => Corner::TopRight,
            "bottom-right" | "end-end" => Corner::BottomRight,
            "bottom-left" | "end-start" => Corner::BottomLeft,
            _ => return None,
        })
    }
}

fn set(css: &mut ComputedStyle, c: Corner, v: Option<f32>) {
    match c {
        Corner::TopLeft => css.corner_tl = v,
        Corner::TopRight => css.corner_tr = v,
        Corner::BottomRight => css.corner_br = v,
        Corner::BottomLeft => css.corner_bl = v,
    }
}

fn get(css: &ComputedStyle, c: Corner) -> Option<f32> {
    match c {
        Corner::TopLeft => css.corner_tl,
        Corner::TopRight => css.corner_tr,
        Corner::BottomRight => css.corner_br,
        Corner::BottomLeft => css.corner_bl,
    }
}

/// A componente HORIZONTAL de um raio: `10px` ou o primeiro de `10px 20px`.
///
/// Um canto do CSS é uma ELIPSE — dois raios, e a folha real do WhatsApp usa a
/// forma de dois valores. O modelo tem um número por canto, portanto fica o
/// horizontal. Um canto elíptico sairá circular; está dito aqui em vez de ser
/// descoberto por quem comparar com o Chrome.
fn primeiro_raio(val: &str) -> Option<f32> {
    let toks = split_top_ws(val);
    parse_len_pub(toks.first()?)
}

/// O shorthand `border-radius`, na parte que ESTE módulo responde: os quatro
/// cantos. Chamado a seguir ao braço que escreve `corner_radius`, sem lhe tocar.
///
/// `<1 a 4 valores> [/ <1 a 4 valores>]`. A ordem é TL, TR, BR, BL, e o omitido
/// copia o canto DIAGONALMENTE oposto — não o adjacente, que é a regra dos
/// shorthands de caixa e não a dos cantos. A parte depois da `/` são os raios
/// verticais e é descartada pelo mesmo motivo que `primeiro_raio` descarta o
/// segundo valor.
pub fn apply_shorthand(css: &mut ComputedStyle, val: &str) {
    let horizontais = val.split('/').next().unwrap_or(val);
    let t = split_top_ws(horizontais);
    let g = |i: usize| parse_len_pub(&t[i]);
    let (tl, tr, br, bl) = match t.len() {
        1 => (g(0), g(0), g(0), g(0)),
        2 => (g(0), g(1), g(0), g(1)),
        3 => (g(0), g(1), g(2), g(1)),
        4 => (g(0), g(1), g(2), g(3)),
        _ => return,
    };
    css.corner_tl = tl;
    css.corner_tr = tr;
    css.corner_br = br;
    css.corner_bl = bl;
}

/// Tenta aplicar uma longhand de canto. `false` = o nome não é de nenhuma.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    let Some(sufixo) = prop.strip_prefix("border-").and_then(|r| r.strip_suffix("-radius")) else {
        return false;
    };
    let Some(canto) = Corner::parse(sufixo) else { return false };
    set(css, canto, primeiro_raio(val));
    true
}

/// O valor DECLARADO de uma longhand de canto (`""` se o elemento não a
/// declarou — o inicial vive em `style::initial`, como o de todas). `None` = o
/// nome não é de um canto.
pub fn get_property(css: &ComputedStyle, prop: &str) -> Option<String> {
    let sufixo = prop.strip_prefix("border-").and_then(|r| r.strip_suffix("-radius"))?;
    let canto = Corner::parse(sufixo)?;
    // O canto responde o que o canto tem; se só o shorthand foi declarado, foi
    // ele que escreveu os quatro, portanto a resposta já está no campo do canto.
    Some(get(css, canto).map(|v| format!("{v}px")).unwrap_or_default())
}
