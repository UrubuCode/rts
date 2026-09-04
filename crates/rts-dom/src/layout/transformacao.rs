//! A matriz 2D de `transform`: `Mat2d` (a forma `matrix(a,b,c,d,e,f)` do CSS),
//! a lista de funções (`TransformList`) que o parser em `style::effects`
//! preenche na ordem em que aparecem na declaração, e a bounding box de um
//! `Rect` transformado — CSS Transforms 1 §6 (origem) e §7 (a matriz; a
//! composição é lida da DIREITA para a ESQUERDA no PONTO).
//!
//! Vive no LAYOUT e não no `style` porque é aritmética de geometria, não uma
//! pergunta de cascade: `style::effects::Transform` guarda só os OPERANDOS
//! (translate em px/%, escala, ângulos) — dados que sobrevivem à cascade sem
//! precisar do tamanho da caixa — e é aqui, com `box_rect` na mão, que viram
//! uma matriz e depois um retângulo. A mesma separação que `Dimension` já tem
//! entre "o valor declarado" e "o valor resolvido".

use super::Rect;

/// Uma matriz afim 2D, na convenção do `matrix(a,b,c,d,e,f)` do CSS:
/// `x' = a·x + c·y + e`; `y' = b·x + d·y + f`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mat2d {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Mat2d {
    pub const IDENTITY: Mat2d = Mat2d {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(tx: f32, ty: f32) -> Mat2d {
        Mat2d {
            e: tx,
            f: ty,
            ..Mat2d::IDENTITY
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Mat2d {
        Mat2d {
            a: sx,
            d: sy,
            ..Mat2d::IDENTITY
        }
    }

    /// Sentido horário (CSS), como o cálculo antigo em `itens.rs` já fazia:
    /// `rx = dx·cos − dy·sin; ry = dx·sin + dy·cos`.
    pub fn rotate_deg(deg: f32) -> Mat2d {
        let (sin, cos) = deg.to_radians().sin_cos();
        Mat2d {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            ..Mat2d::IDENTITY
        }
    }

    pub fn skew_x_deg(deg: f32) -> Mat2d {
        Mat2d {
            c: deg.to_radians().tan(),
            ..Mat2d::IDENTITY
        }
    }

    pub fn skew_y_deg(deg: f32) -> Mat2d {
        Mat2d {
            b: deg.to_radians().tan(),
            ..Mat2d::IDENTITY
        }
    }

    /// Compõe: o resultado aplicado a um ponto é `self(other(p))` — `other`
    /// corre PRIMEIRO. É a multiplicação de matrizes padrão (`self · other`
    /// como matrizes 3×3 homogéneas, coluna à direita).
    pub fn then(self, other: Mat2d) -> Mat2d {
        Mat2d {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    /// A mesma matriz, mas em torno de `(cx,cy)` em vez da origem:
    /// `T(cx,cy) · self · T(−cx,−cy)` — a forma que `transform-origin` exige
    /// (CSS Transforms 1 §6): translada a origem para o ponto, aplica, desfaz.
    pub fn around(self, cx: f32, cy: f32) -> Mat2d {
        Mat2d::translate(cx, cy).then(self).then(Mat2d::translate(-cx, -cy))
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    /// A bounding box dos 4 cantos transformados — o que
    /// `getBoundingClientRect` devolve para um elemento com `transform`
    /// (CSS Transforms 1: o "transform target" é o quadrilátero, e o rect
    /// reportado é a caixa envolvente ALINHADA AOS EIXOS dele).
    pub fn transform_rect_bbox(&self, r: Rect) -> Rect {
        let pts = [
            self.apply(r.x, r.y),
            self.apply(r.x + r.w, r.y),
            self.apply(r.x, r.y + r.h),
            self.apply(r.x + r.w, r.y + r.h),
        ];
        let min_x = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.0));
        let max_x = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.0));
        let min_y = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.1));
        let max_y = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.1));
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

/// UMA função de `transform`, com os operandos exatamente como declarados —
/// `Translate` guarda px E fração separados porque a fração só resolve contra
/// o tamanho da caixa, que o parser (`style::effects`) não tem.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TransformOp {
    Translate {
        tx: f32,
        ty: f32,
        tx_pct: f32,
        ty_pct: f32,
    },
    Scale {
        sx: f32,
        sy: f32,
    },
    Rotate {
        deg: f32,
    },
    SkewX {
        deg: f32,
    },
    SkewY {
        deg: f32,
    },
    Matrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    },
}

impl TransformOp {
    fn to_matrix(self, box_w: f32, box_h: f32) -> Mat2d {
        match self {
            TransformOp::Translate { tx, ty, tx_pct, ty_pct } => {
                Mat2d::translate(tx + tx_pct * box_w, ty + ty_pct * box_h)
            }
            TransformOp::Scale { sx, sy } => Mat2d::scale(sx, sy),
            TransformOp::Rotate { deg } => Mat2d::rotate_deg(deg),
            TransformOp::SkewX { deg } => Mat2d::skew_x_deg(deg),
            TransformOp::SkewY { deg } => Mat2d::skew_y_deg(deg),
            TransformOp::Matrix { a, b, c, d, e, f } => Mat2d { a, b, c, d, e, f },
        }
    }
}

/// Quantas funções uma declaração `transform` pode compor. Um `Vec` faria
/// `style::effects::Transform` deixar de ser `Copy` — e `ComputedStyle` lê
/// `css.transform` POR VALOR (`if let Some(tf) = css.transform`) em todo o
/// resto do cascade, como qualquer outro campo `[] nome: Tipo;` da tabela.
/// 8 cobre com folga qualquer folha do corpus (a mais longa observada tem 2).
pub const MAX_TRANSFORM_OPS: usize = 8;

/// A lista de funções de UMA declaração `transform`, na ordem em que
/// apareceram no CSS. `resolve` compõe-as against o tamanho da caixa.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct TransformList {
    ops: [Option<TransformOp>; MAX_TRANSFORM_OPS],
    count: u8,
}

impl TransformList {
    /// Acrescenta uma função ao FIM da lista (a próxima a aparecer no CSS).
    /// Sem efeito além do limite — uma declaração real nunca chega lá.
    pub fn push(&mut self, op: TransformOp) {
        let i = self.count as usize;
        if i < MAX_TRANSFORM_OPS {
            self.ops[i] = Some(op);
            self.count += 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// As funções, na ordem em que foram declaradas — para quem quer inspecionar
    /// os OPERANDOS em vez da matriz já composta (testes, `getComputedStyle`).
    pub fn iter(&self) -> impl Iterator<Item = TransformOp> + '_ {
        self.ops.iter().take(self.count as usize).flatten().copied()
    }

    /// A matriz composta, SEM a origem. `f1 f2 … fn` (ordem de escrita) compõe
    /// como `f1∘f2∘…∘fn` — o `then` da direita é aplicado primeiro ao ponto,
    /// que é a leitura que a spec dá à lista (CSS Transforms 1 §7: a lista
    /// aplica-se da direita para a esquerda). Acumular `acc = acc.then(fi)`
    /// da esquerda para a direita dá exactamente essa composição, por
    /// associatividade: depois de `f1,f2,f3`, `acc = (f1∘f2)∘f3 = f1∘f2∘f3`.
    pub fn resolve(&self, box_w: f32, box_h: f32) -> Mat2d {
        let mut acc = Mat2d::IDENTITY;
        for op in self.ops.iter().take(self.count as usize).flatten() {
            acc = acc.then(op.to_matrix(box_w, box_h));
        }
        acc
    }
}

/// Resolve UM eixo de `transform-origin` (já um `Dimension` — o parser reusa a
/// gramática de `background-position`, ver `style/effects.rs`) contra o
/// tamanho da CAIXA (`base` = largura ou altura do border-box), não contra o
/// pai como `Dimension::resolve` faz — é a diferença entre "onde a imagem de
/// fundo começa" e "em volta de que ponto ESTA caixa roda". `viewport_axis` é
/// `viewport_w`/`viewport_h` conforme o eixo (para `vw`/`vh`).
pub(in crate::layout) fn resolve_origin_axis(
    d: crate::style::values::Dimension,
    base: f32,
    font_size: f32,
    root_font_size: f32,
    viewport_axis: f32,
) -> f32 {
    use crate::style::values::Dimension;
    match d {
        Dimension::Px(v) => v,
        Dimension::Percent(p) => base * p / 100.0,
        Dimension::Em(e) => font_size * e,
        Dimension::Rem(r) => root_font_size * r,
        Dimension::Vw(v) | Dimension::Vh(v) => viewport_axis * v / 100.0,
        // `auto`/`max-content`/`calc` não aparecem em `transform-origin` na
        // prática (o corpus não os escreve); o centro é o fallback seguro.
        _ => base * 0.5,
    }
}

/// A matriz FINAL de uma declaração `transform`: a lista de funções, composta e
/// envolvida pela origem (default `50% 50%` — CSS Transforms 1 §6). Junta
/// `TransformList::resolve` + `resolve_origin_axis` + `Mat2d::around` numa
/// chamada só, para `bloco.rs` não repetir os quatro passos na região do
/// `transform` — só o CHAMA e decide o que fazer com a matriz.
pub(in crate::layout) fn matriz_transform(
    tf: crate::style::effects::Transform,
    origin: Option<crate::style::BgPosition>,
    box_rect: Rect,
    font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Mat2d {
    use crate::style::values::Dimension;
    let origin = origin.unwrap_or(crate::style::BgPosition {
        x: Dimension::Percent(50.0),
        y: Dimension::Percent(50.0),
    });
    let root_font = crate::style::root_font_size();
    let ox = box_rect.x + resolve_origin_axis(origin.x, box_rect.w, font_size, root_font, viewport_w);
    let oy = box_rect.y + resolve_origin_axis(origin.y, box_rect.h, font_size, root_font, viewport_h);
    tf.ops.resolve(box_rect.w, box_rect.h).around(ox, oy)
}

/// Aplica `mat` ao retângulo de `id` em `list.node_rects` (se tiver um) e, RECURSIVAMENTE,
/// ao de cada descendente — os descendentes HERDAM a transformação do pai (CSS
/// Transforms 1: o "transform target" inclui a subárvore). Mesmo padrão de
/// `relativo.rs::desloca_node_rects` (percorrer o DOM a partir de `id`), mas a
/// operação é a matriz inteira (bounding box dos 4 cantos) em vez de uma soma.
pub(in crate::layout) fn transforma_node_rects(
    dom: &crate::dom::Dom,
    id: crate::dom::NodeIdx,
    mat: &Mat2d,
    list: &mut super::DisplayList,
) {
    if let Some(r) = list.node_rects.get_mut(&id) {
        *r = mat.transform_rect_bbox(*r);
    }
    for &child in &dom.node(id).children {
        transforma_node_rects(dom, child, mat, list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f32 = 0.001;
    fn perto(a: f32, b: f32) {
        assert!((a - b).abs() < TOL, "{a} != {b}");
    }

    /// `translate(10,20) rotate(45deg)`: a lista compõe-se na ordem de
    /// escrita — `translate` (o 1º) aplica-se por ÚLTIMO ao ponto.
    #[test]
    fn composicao_direita_para_esquerda() {
        let mut l = TransformList::default();
        l.push(TransformOp::Translate { tx: 10.0, ty: 20.0, tx_pct: 0.0, ty_pct: 0.0 });
        l.push(TransformOp::Rotate { deg: 90.0 });
        let m = l.resolve(0.0, 0.0);
        // rotate(90°) manda (1,0) para (0,1); translate soma (10,20) depois.
        let (x, y) = m.apply(1.0, 0.0);
        perto(x, 10.0);
        perto(y, 21.0);
    }

    /// Rotação de 90° em torno do canto TOP-LEFT de uma caixa 100×100 em
    /// (0,0): o canto oposto (100,100) vai para (0,-100)+origem — a mesma
    /// leitura que `claude-transform-origin.html#topo-esq` mede no Chrome
    /// (bounding box `[-100, 150, 100, 100]` para uma caixa que começava em
    /// `top:150,left:0`).
    #[test]
    fn rotate_90_em_torno_do_canto_bate_com_o_chrome() {
        let box_rect = Rect::new(0.0, 150.0, 100.0, 100.0);
        let m = Mat2d::rotate_deg(90.0).around(box_rect.x, box_rect.y);
        let r = m.transform_rect_bbox(box_rect);
        perto(r.x, -100.0);
        perto(r.y, 150.0);
        perto(r.w, 100.0);
        perto(r.h, 100.0);
    }

    /// `matrix(1,0,0.5,1,0,0)` (um shear em X) numa caixa 100×50: a bounding
    /// box cresce em largura pela contribuição do `c` — bate com
    /// `claude-transform-skew-matrix.html#matriz` (`[-12.5, 200, 125, 50]`).
    #[test]
    fn matrix_seis_valores_desloca_a_bounding_box() {
        let box_rect = Rect::new(0.0, 200.0, 100.0, 50.0);
        let cx = box_rect.x + box_rect.w / 2.0;
        let cy = box_rect.y + box_rect.h / 2.0;
        let m = (Mat2d {
            a: 1.0,
            b: 0.0,
            c: 0.5,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        })
        .around(cx, cy);
        let r = m.transform_rect_bbox(box_rect);
        perto(r.x, -12.5);
        perto(r.y, 200.0);
        perto(r.w, 125.0);
        perto(r.h, 50.0);
    }
}
