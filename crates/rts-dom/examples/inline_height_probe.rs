//! Onde é que a nossa página fica mais alta do que a do Chrome.
//!
//! O harness de paridade diz QUANTO (130 577px contra 69 930px) e por elemento,
//! mas não diz PORQUÊ, porque só compara retângulos. Este exemplo olha para o
//! que produziu a altura: por parágrafo, quantas LINHAS de texto emitimos e
//! quantas seriam precisas para o texto que ele tem na largura que ele tem.
//!
//! A razão entre as duas separa as duas explicações possíveis — medimos o texto
//! largo demais (linhas a mais pelo mesmo motivo em todo o lado) ou partimos o
//! parágrafo em pedaços que não deviam estar em linhas próprias.
//!
//!   cargo run -q -p rts-dom --example inline_height_probe -- scripts/parity/pagina.combinada.html

use rts_dom::layout::{self, DisplayItem};

struct Medidor;

impl layout::TextMeasurer for Medidor {
    fn text_width(&self, text: &str, size: f32, _mono: bool, _bold: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }
    fn line_height(&self, size: f32) -> f32 {
        size * 1.3
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let html = std::fs::read_to_string(&args[1]).expect("html");
    let dom = rts_dom::parse_html_to_dom(&html);
    let medidor = Medidor;
    let ctx = layout::LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &medidor };
    let list = layout::layout_document(&dom, &ctx);
    println!("altura do documento: {:.0}", list.content_height);

    // Os itens de texto, ordenados por y, para se poderem contar as linhas
    // distintas dentro de um retângulo.
    // Pela ÁRVORE (`walk`) e não por `list.items`: desde a saída em árvore, os
    // itens de uma subárvore reusada vivem no fragmento dela. Ler `items` direto
    // dá 63 itens para a Wikipédia inteira e a conclusão errada de que a página
    // não pinta.
    let mut textos: Vec<(f32, f32, f32, usize)> = Vec::new();
    let mut por_tipo = std::collections::BTreeMap::new();
    list.walk(|it, dx, dy| {
        let nome = match it {
            DisplayItem::Text { x, y, text, size, .. } => {
                textos.push((x + dx, y + dy, *size, text.chars().count()));
                "Text"
            }
            DisplayItem::SolidRect { .. } => "SolidRect",
            DisplayItem::Border { .. } => "Border",
            DisplayItem::Image { .. } => "Image",
            _ => "outro",
        };
        *por_tipo.entry(nome).or_insert(0usize) += 1;
    });
    textos.sort_by(|a, b| a.1.total_cmp(&b.1));
    println!("itens de texto: {}", textos.len());
    println!("itens por tipo: {por_tipo:?}");
    if let Some((_, y, _, _)) = textos.last() {
        println!("y do ultimo texto: {y:.0}");
    }

    let geo = list.geometry();
    let mut paragrafos: Vec<(f32, rts_dom::layout::Rect)> = dom
        .query_all("p")
        .iter()
        .filter_map(|id| dom.resolve(*id))
        .filter_map(|idx| geo.rects.get(&idx).map(|r| (r.h, *r)))
        .collect();
    paragrafos.sort_by(|a, b| b.0.total_cmp(&a.0));

    println!("\nos 10 <p> mais altos — linhas emitidas contra linhas necessárias:");
    for (_, r) in paragrafos.iter().take(10) {
        let dentro: Vec<_> = textos
            .iter()
            .filter(|(x, y, _, _)| {
                *y >= r.y - 1.0 && *y < r.y + r.h + 1.0 && *x >= r.x - 1.0 && *x < r.x + r.w + 1.0
            })
            .collect();
        let mut ys: Vec<f32> = dentro.iter().map(|(_, y, _, _)| *y).collect();
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        let chars: usize = dentro.iter().map(|(_, _, _, n)| *n).sum();
        let size = dentro.first().map(|(_, _, s, _)| *s).unwrap_or(16.0);
        // largura do texto pelo mesmo medidor, dividida pela largura da caixa.
        let precisas = (chars as f32 * size * 0.5 / r.w).ceil().max(1.0);
        println!(
            "  h={:6.0} w={:4.0} | itens={:3} linhas={:3} chars={:5} | necessarias~{:3} | inflacao {:.1}x",
            r.h,
            r.w,
            dentro.len(),
            ys.len(),
            chars,
            precisas,
            ys.len() as f32 / precisas,
        );
    }
}
