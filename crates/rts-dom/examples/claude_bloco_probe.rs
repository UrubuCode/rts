//! Um bloco dentro de um bloco emite pintura?
//!
//! O caso mínimo da regressão que apagou a página: `<div><p>texto</p></div>`
//! media a altura certa e não pintava nada. Existe como exemplo e não como
//! teste para poder ser corrido contra binários de commits diferentes durante
//! uma bisseção.
use rts_dom::layout::{self, DisplayItem};

struct M;
impl layout::TextMeasurer for M {
    fn text_width(&self, t: &str, s: f32, _b: bool, _i: bool) -> f32 { t.chars().count() as f32 * s * 0.5 }
    fn line_height(&self, s: f32) -> f32 { s * 1.3 }
}

fn conta(html: &str) -> (usize, usize) {
    let dom = rts_dom::parse_html_to_dom(html);
    let m = M;
    let ctx = layout::LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &m };
    let list = layout::layout_document(&dom, &ctx);
    let (mut textos, mut rects) = (0, 0);
    list.walk(|item, _, _| match item {
        DisplayItem::Text { .. } => textos += 1,
        DisplayItem::SolidRect { .. } => rects += 1,
        _ => {}
    });
    (textos, rects)
}

fn main() {
    for (nome, html) in [
        ("<p> sozinho", "<p>so um p</p>"),
        ("texto solto em div", "<div>texto solto</div>"),
        ("<div><p>texto</p></div>", "<div><p>Um paragrafo com texto.</p></div>"),
        ("div>div com fundo", "<div><div style='background:#ff0000;height:20px'>x</div></div>"),
        ("div>div>p", "<div><div><p>fundo</p></div></div>"),
    ] {
        let (t, r) = conta(html);
        println!("{nome:28} -> textos={t} rects={r}");
    }
}
