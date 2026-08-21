//! O que a display list de uma página REAL contém na primeira tela.
//!
//! Existe porque geometria e PINTURA são perguntas diferentes: o relatório pelo
//! `rts:dom` já dizia que 10 369 elementos têm caixa, e a janela continuava
//! branca. Este exemplo olha para o que o backend receberia.
//!
//!   cargo run -q -p rts-dom --example page_paint -- pagina.html pagina.css

use rts_dom::layout::{self, DisplayItem};

struct Medidor;

impl layout::TextMeasurer for Medidor {
    fn text_width(&self, text: &str, size: f32, _mono: bool, _bold: bool, _italic: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }

    fn line_height(&self, size: f32) -> f32 {
        size * 1.3
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let html = std::fs::read_to_string(&args[1]).expect("html");
    let css = args.get(2).and_then(|p| std::fs::read_to_string(p).ok());
    let mut dom = rts_dom::parse_html_to_dom(&html);
    if let Some(css) = &css {
        dom.add_stylesheet(css);
    }
    let medidor = Medidor;
    let ctx = layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &medidor,
    };
    let t0 = std::time::Instant::now();
    let list = layout::layout_document(&dom, &ctx);
    let primeiro = t0.elapsed();
    // O SEGUNDO layout é o que um frame paga: se o cache do `rts-dom` acerta,
    // custa quase nada; se não, a janela paga o layout inteiro a cada frame — e
    // uma página real fica branca porque quase nunca chega a pintar.
    let t1 = std::time::Instant::now();
    let _ = layout::layout_cached(&dom, &ctx);
    let segundo_frio = t1.elapsed();
    let t2 = std::time::Instant::now();
    let _ = layout::layout_cached(&dom, &ctx);
    let terceiro = t2.elapsed();
    println!("layout: primeiro={primeiro:?} | cached-1={segundo_frio:?} | cached-2={terceiro:?}");

    // A ORDEM dos marcadores de clip, que é o que decide se o resto da página é
    // pintado ou recortado a nada.
    if std::env::var_os("RTS_CLIP_ORDER").is_some() {
        let mut prof = 0i32;
        list.walk(|item, dx, dy| match item {
            DisplayItem::BeginClip { rect, .. } => {
                prof += 1;
                println!(
                    "  {}BeginClip ({:.0},{:.0}) {:.0}x{:.0}",
                    "  ".repeat(prof as usize),
                    rect.x + dx,
                    rect.y + dy,
                    rect.w,
                    rect.h
                );
            }
            DisplayItem::EndClip { .. } => {
                println!("  {}EndClip", "  ".repeat(prof.max(1) as usize));
                prof -= 1;
            }
            DisplayItem::Text { text, .. } => {
                println!(
                    "  {}txt(prof={prof}) {:?}",
                    "  ".repeat((prof + 1).max(1) as usize),
                    text.chars().take(20).collect::<String>()
                );
            }
            _ => {}
        });
    }
    {
        let (mut abre, mut fecha) = (0usize, 0usize);
        list.walk(|item, _, _| match item {
            DisplayItem::BeginClip { .. } => abre += 1,
            DisplayItem::EndClip { .. } => fecha += 1,
            _ => {}
        });
        println!("clips: abre={abre} fecha={fecha}");
        // Quantos itens ficam DENTRO do primeiro clip, e de que tamanho ele é:
        // um clip de 1x1 que engole a página é a diferença entre uma página
        // pintada e uma tela branca.
        let (mut prof, mut dentro, mut primeiro) = (0i32, 0usize, None);
        list.walk(|item, dx, dy| match item {
            DisplayItem::BeginClip { rect, .. } => {
                if primeiro.is_none() {
                    primeiro = Some((rect.x + dx, rect.y + dy, rect.w, rect.h));
                }
                prof += 1;
            }
            DisplayItem::EndClip { .. } => prof -= 1,
            _ => {
                if prof > 0 && primeiro.is_some() {
                    dentro += 1;
                }
            }
        });
        println!("primeiro clip: {primeiro:?} | itens sob algum clip: {dentro}");
        // QUEM é o nó que recorta — a diferença entre "o clip está no sítio
        // errado" e "a regra casou com o elemento errado".
        let mut achou = false;
        list.walk(|item, _, _| {
            if achou {
                return;
            }
            if let DisplayItem::BeginClip { node, rect, .. } = item {
                achou = true;
                let no = dom.node(*node);
                let tag = match &no.kind {
                    rts_dom::NodeKind::Element { tag } => tag.clone(),
                    _ => "?".into(),
                };
                println!(
                    "  recorta: <{tag}> node={node:?} rect={:.0}x{:.0} filhos={}",
                    rect.w,
                    rect.h,
                    no.children.len()
                );
            }
        });
    }
    {
        // Onde, na ORDEM DE PINTURA, cada marcador cai.
        let mut pos = 0usize;
        let mut marcas = Vec::new();
        list.walk(|item, _, _| {
            match item {
                DisplayItem::BeginClip { rect, node, .. } if marcas.len() < 6 => marcas.push(
                    format!("#{pos} Begin no={node:?} {:.0}x{:.0}", rect.w, rect.h),
                ),
                DisplayItem::EndClip { filhos_dentro } if marcas.len() < 6 => {
                    marcas.push(format!("#{pos} End (filhos_dentro={filhos_dentro})"))
                }
                _ => {}
            }
            pos += 1;
        });
        // onde fecha o PRIMEIRO clip (profundidade volta a zero)
        let (mut prof, mut p2, mut fecha_em) = (0i32, 0usize, None);
        list.walk(|item, _, _| {
            match item {
                DisplayItem::BeginClip { .. } => prof += 1,
                DisplayItem::EndClip { filhos_dentro } => {
                    prof -= 1;
                    if prof == 0 && fecha_em.is_none() {
                        fecha_em = Some(p2);
                        println!("  o End que fecha tem filhos_dentro={filhos_dentro}");
                    }
                }
                _ => {}
            }
            p2 += 1;
        });
        println!("primeiro clip fecha no item {fecha_em:?}");
        println!("total de itens pintados: {pos}");
        for m in marcas {
            println!("  {m}");
        }
    }
    {
        // A ESTRUTURA da lista de topo: onde os marcadores estão nos itens
        // próprios, e onde os filhos entram.
        println!(
            "topo: items={} children={}",
            list.items.len(),
            list.children.len()
        );
        for (i, it) in list.items.iter().enumerate().take(2000) {
            match it {
                DisplayItem::BeginClip { rect, node, .. } => {
                    println!("  items[{i}] Begin no={node:?} {:.0}x{:.0}", rect.w, rect.h)
                }
                DisplayItem::EndClip { filhos_dentro } => {
                    println!("  items[{i}] End (filhos_dentro={filhos_dentro})")
                }
                _ => {}
            }
        }
        let ats: Vec<usize> = list.children.iter().take(10).map(|c| c.at).collect();
        println!("  at dos primeiros filhos: {ats:?}");
    }
    let (mut textos, mut rects, mut outros) = (0usize, 0usize, 0usize);
    let (mut textos_na_tela, mut rects_na_tela) = (0usize, 0usize);
    let mut amostra = Vec::new();
    list.walk(|item, dx, dy| match item {
        DisplayItem::Text {
            x, y, text, color, ..
        } => {
            textos += 1;
            let (x, y) = (x + dx, y + dy);
            if y < 800.0 && x < 1280.0 {
                textos_na_tela += 1;
                if amostra.len() < 12 && !text.trim().is_empty() {
                    amostra.push(format!(
                        "  texto y={:.0} x={:.0} cor=#{:08X} {:?}",
                        y,
                        x,
                        color,
                        text.chars().take(28).collect::<String>()
                    ));
                }
            }
        }
        DisplayItem::SolidRect { rect, .. } => {
            rects += 1;
            if rect.y + dy < 800.0 {
                rects_na_tela += 1;
            }
        }
        _ => outros += 1,
    });
    println!(
        "itens: texto={textos} rect={rects} outros={outros} | na primeira tela: texto={textos_na_tela} rect={rects_na_tela}"
    );
    println!(
        "altura do conteúdo: {:.0} | canvas: #{:08X}",
        list.content_height, list.canvas_background
    );
    for linha in amostra {
        println!("{linha}");
    }
}
