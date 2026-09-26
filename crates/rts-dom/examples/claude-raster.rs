//! O RASTERIZADOR headless do nosso lado da régua de pintura: parseia uma
//! fixture, faz layout a 1280x800 e escreve um PNG RGBA da `DisplayList` — sem egui, sem wgpu, sem
//! janela. É a metade que faltava ao `06-reguas-e-saude-do-codigo.md`
//! finding 2: as réguas existentes comparam CAIXAS, nunca a IMAGEM.
//!
//!   cargo run -q -p rts-dom --example claude-raster -- pagina.html saida.png
//!
//! # O que pinta e o que salta — decidido aqui, não na régua
//!
//! Pinta: `SolidRect` (cantos quadrados — arredondar exigiria um rasterizador
//! de arco, e a régua já tolera 8/255 por canal na borda de um retângulo,
//! que é onde um canto reto contra um arredondado diverge), `Border` (quatro
//! tiras retas), `GradientRect` linear (interpolação por pixel ao longo de
//! `angle_deg`, a MESMA fórmula que `rts-egui` usa no mesh de 4 vértices —
//! ver `crates/rts-egui/src/frame/render/pintura.rs`), `Shadow` (achatada:
//! preenche o rect deslocado com a cor, sem blur — o blur é um desfoque
//! gaussiano e não vale o código para uma régua que já ignora texto),
//! `BeginClip`/`EndClip` (pilha de rects, interseção).
//!
//! Text is laid out with `rts_text`'s `RealMeasurer` and painted glyph by
//! glyph from the same faces (plan `docs/superpowers/plans/2026-09-26-text-crate.md`,
//! F3; `claude-raster/text.rs` says how an item's face is chosen). Only text
//! whose family resolves to no face is masked, and the count of those is the
//! last line printed: `rts-raster: N text items painted, M masked`.
//!
//! SALTA: `Image` — aponta para um handle do
//! `HandleTable`, que este exemplo não tem (não há `Engine` nem `Registry`
//! aqui, só `Dom`+layout). `Pixels` de uma imagem que NÃO carregou (abaixo)
//! continua mascarado; `Pixels` de uma que carregou PINTA (lote
//! imagens-no-raster: `fill_pixels`, já existia para `<canvas>`, `<img>`
//! nunca tinha pixels antes deste lote). Ambas as omissões contam para a
//! MÁSCARA que o comparador (`scripts/css_pintura_comparar.mjs`) recebe: os
//! rects de texto e de imagem sem pixels do nosso lado saem também num
//! `.mask.json` ao lado do PNG.
//!
//! # Imagens (lote imagens-no-raster)
//!
//! ANTES do layout, `carregar_imagens` (implementação em
//! `claude-paint-dump.rs`, chamada aqui — descodificador ÚNICO em
//! `rts_dom::imagem`, movido de `rts-dom-bridge`; só o `std::fs::read` se
//! repete nos dois exemplos, ver o porquê no cabeçalho de `imagem/mod.rs`)
//! carrega PNG local (relativo ao HTML) e `data:image/png;base64,…` — o
//! mesmo que `dom.ts::loadResources` faz numa página a correr. NÃO carrega
//! `http(s)` nem `data:image/svg+xml` (este motor só descodifica PNG,
//! PLAN.md lote V-img) — essas continuam mascaradas como antes.
//!
//! # Zero dependências novas no CRATE (para a ESCRITA de PNG)
//!
//! `rts-dom` já tem uma dependência agora (`flate2`, para a LEITURA de PNG
//! em `src/imagem/png.rs`, lote imagens-no-raster) — mas a ESCRITA do PNG de
//! saída continua escrita à mão neste FICHEIRO (bloco IDAT com deflate
//! "stored", sem compressão): a crate `png` faria os dois sentidos, e
//! trazê-la só para a escrita, quando a leitura já não precisa dela, seria
//! uma segunda dependência para o mesmo problema que a primeira já resolve.

#[path = "claude-raster/canvas.rs"]
mod canvas;
#[path = "claude-raster/png.rs"]
mod png;
#[path = "claude-raster/text.rs"]
mod text;

use canvas::{Canvas, H, W, rect_intersect, transformed_bbox};
use png::write_png;
use rts_dom::layout;
use rts_dom::paint::{DisplayItem, DisplayList, Mat2d, Rect};
use rts_dom::Dom;
use std::path::Path;
use text::{Outcome, TextPainter, TextRun};

/// Ver o cabeçalho e a doc em `claude-paint-dump.rs` — a mesma função,
/// duplicada de propósito (glue de I/O, não lógica: o DESCODIFICADOR é um
/// só, `rts_dom::imagem`) porque `examples/` não tem um módulo partilhado
/// entre dois binários sem o truque de `#[path]` num ficheiro sem `main`, que
/// nenhum outro exemplo deste crate usa.
fn carregar_imagens(dom: &mut Dom, base_dir: &Path) {
    for id in dom.query_all("img") {
        let Some(idx) = dom.resolve(id) else { continue };
        let Some(src) = dom.node(idx).attr("src").map(str::to_string) else { continue };
        let decoded = if src.starts_with("data:") {
            rts_dom::imagem::bytes_da_data_url(&src).and_then(|b| rts_dom::imagem::png::decodificar(&b))
        } else if src.starts_with("http://") || src.starts_with("https://") {
            None
        } else {
            std::fs::read(base_dir.join(&src)).ok().and_then(|b| rts_dom::imagem::png::decodificar(&b))
        };
        if let Some((rgba, w, h)) = decoded {
            dom.set_pixel_data(id, rgba, w, h);
        }
    }
    carregar_fundos(dom, base_dir);
}

/// O mesmo carregamento acima, para `background-image: url(...)` — o achado
/// do lote `background-image-pintado`: nenhum elemento além de `<img>` tinha
/// os seus pixels carregados, então `layout::fundo_imagem` nunca tinha o que
/// pintar. `dom.query_all("*")` (em vez de uma tag) porque um fundo pode
/// estar em QUALQUER elemento, não só numa lista fechada de tags.
fn carregar_fundos(dom: &mut Dom, base_dir: &Path) {
    for id in dom.query_all("*") {
        let Some(css) = dom.computed_style(id) else { continue };
        let Some(url) = css.bg_image.as_deref().and_then(rts_dom::style::background::bg_image_url) else {
            continue;
        };
        let decoded = if url.starts_with("data:") {
            rts_dom::imagem::bytes_da_data_url(&url).and_then(|b| rts_dom::imagem::png::decodificar(&b))
        } else if url.starts_with("http://") || url.starts_with("https://") {
            None
        } else {
            std::fs::read(base_dir.join(&url)).ok().and_then(|b| rts_dom::imagem::png::decodificar(&b))
        };
        if let Some((rgba, w, h)) = decoded {
            dom.set_pixel_data(id, rgba, w, h);
        }
    }
}

/// O primeiro fragmento de `<meta name="fixar-hash" content="alvo">`, se a
/// fixture declarar um — mesma leitura textual de `lista()` em
/// `examples/claude-css-runner.ts`, sem depender de um parser de atributos
/// (o HTML aqui já é confiável, escrito à mão nas fixtures do corpus).
fn meta_fixar_hash(fonte: &str) -> Option<String> {
    let marca = fonte.find("name=\"fixar-hash\"")?;
    let c = fonte[marca..].find("content=\"")? + marca + "content=\"".len();
    let fim = fonte[c..].find('"')? + c;
    let primeiro = fonte[c..fim].split(',').next()?.trim();
    if primeiro.is_empty() {
        None
    } else {
        Some(primeiro.to_string())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(entrada), Some(saida)) = (args.get(1), args.get(2)) else {
        eprintln!("uso: claude-raster <pagina.html> <saida.png>");
        std::process::exit(2);
    };
    let html = std::fs::read_to_string(entrada).unwrap_or_else(|e| {
        eprintln!("não li {entrada}: {e}");
        std::process::exit(2);
    });
    let mut dom: Dom = rts_dom::parse_html_to_dom(&html);
    // `<meta name="fixar-hash" content="alvo">` (mesmo mecanismo de
    // `examples/claude-css-runner.ts`, régua de N): este rasterizador não
    // executa `<script>`, então uma fixture sobre `:target` (só marcado pelo
    // Blink depois de a URL NAVEGAR para o fragmento) precisa de um jeito
    // honesto de dizer qual fragmento estava ativo — chamar o mesmo
    // `Dom::set_location_hash` que `window.location.hash =` chamaria.
    if let Some(hash) = meta_fixar_hash(&html) {
        dom.set_location_hash(&hash);
    }
    let base_dir = Path::new(entrada).parent().unwrap_or_else(|| Path::new("."));
    carregar_imagens(&mut dom, base_dir);
    let measurer = rts_text::adapter::RealMeasurer::new(text::font_store());
    let ctx = layout::LayoutCtx { viewport_w: W as f32, viewport_h: H as f32, measurer: &measurer };
    let list: DisplayList = layout::layout_document(&dom, &ctx);
    // Um `<img>` com `src` e SEM pixels a esta altura: `carregar_imagens`
    // (acima, antes do layout) já tentou PNG local e `data:image/png` — o que
    // sobra aqui é `http(s)` (sem busca síncrona neste exemplo) e
    // `data:image/svg+xml` (este motor só descodifica PNG). O Blink pinta
    // essas na mesma; a área vai para a máscara como o texto — o instrumento
    // diz o que não vê em vez de o contar como diferença.
    let mut mascara_de_imagens: Vec<[f32; 4]> = Vec::new();
    for id in dom.query_all("img") {
        let Some(idx) = dom.resolve(id) else { continue };
        if dom.pixel_data_of(idx).is_some() || dom.node(idx).attr("src").is_none() {
            continue;
        }
        // `bounding_rect` e não `node_rects`: as caixas vivem nas subárvores
        // reusadas da lista, e é ele que as resolve.
        if let Some(r) = layout::bounding_rect(&dom, idx, &ctx) {
            mascara_de_imagens.push([r.x, r.y, r.w, r.h]);
        }
    }

    let mut canvas = Canvas::new(list.canvas_background);
    let mut clip_stack: Vec<Rect> = Vec::new();
    // Matrizes ACUMULADAS (não as cruas de cada `PushTransform`): o topo já é
    // `outer.then(inner)`, pronta a aplicar a um ponto local sem recompor a
    // pilha inteira por item — `transform`s aninhados (um elemento
    // transformado dentro doutro) compõem assim (ver a doc de
    // `DisplayItem::PushTransform`).
    let mut xform_stack: Vec<Mat2d> = Vec::new();
    let mut mask: Vec<[f32; 4]> = mascara_de_imagens; // [x,y,w,h] dos rects ignorados (texto, imagens)
    let mut pintados = 0usize;
    let mut painter = TextPainter::new(&measurer);
    let (mut texto_pintado, mut masked) = (0usize, 0usize);
    let mut saltados_imagem = 0usize;

    list.walk(|item, dx, dy| {
        let clip = clip_stack.last().copied();
        let mat = xform_stack.last().copied();
        match item {
            DisplayItem::SolidRect { rect, color, .. } => {
                match mat {
                    Some(m) => canvas.fill_rect_mat(*rect, &m, *color, clip),
                    None => canvas.fill_rect(shift(*rect, dx, dy), *color, clip),
                }
                pintados += 1;
            }
            DisplayItem::Border { rect, width, color, .. } => {
                match mat {
                    Some(m) => canvas.stroke_rect_mat(*rect, &m, *width, *color, clip),
                    None => canvas.stroke_rect(shift(*rect, dx, dy), *width, *color, clip),
                }
                pintados += 1;
            }
            DisplayItem::GradientRect { rect, c0, c1, angle_deg, .. } => {
                match mat {
                    Some(m) => canvas.fill_gradient_mat(*rect, &m, *c0, *c1, *angle_deg, clip),
                    None => canvas.fill_gradient(shift(*rect, dx, dy), *c0, *c1, *angle_deg, clip),
                }
                pintados += 1;
            }
            DisplayItem::Shadow { rect, dx: sdx, dy: sdy, color, .. } => {
                // O deslocamento da sombra (`sdx`/`sdy`) é sobre o RECT
                // original, antes da matriz — a mesma ordem do CSS (a sombra
                // desloca a caixa, DEPOIS o `transform` pinta o resultado).
                let r = Rect::new(rect.x + sdx, rect.y + sdy, rect.w, rect.h);
                match mat {
                    Some(m) => canvas.fill_rect_mat(r, &m, *color, clip),
                    None => canvas.fill_rect(shift(r, dx, dy), *color, clip),
                }
                pintados += 1;
            }
            DisplayItem::Text { x, y, text, size, mono, is_ahem, family, color, bold, italic, letter_spacing, orientation, .. } => {
                // Only the origin goes through a `transform`, as it always
                // has here: glyphs are not rotated or skewed.
                let (mx, my) = match mat {
                    Some(m) => m.apply(*x, *y),
                    None => (*x + dx, *y + dy),
                };
                let run = TextRun {
                    x: mx,
                    y: my,
                    text,
                    size: *size,
                    color: *color,
                    mono: *mono,
                    is_ahem: *is_ahem,
                    family: family.as_deref(),
                    bold: *bold,
                    italic: *italic,
                    letter_spacing: *letter_spacing,
                    orientation: *orientation,
                };
                match painter.paint(&mut canvas, &run, clip) {
                    Outcome::Painted => {
                        texto_pintado += 1;
                        pintados += 1;
                    }
                    Outcome::NoFace(r) => {
                        mask.push([r.x, r.y, r.w, r.h]);
                        masked += 1;
                    }
                }
            }
            DisplayItem::Quad { pts, color } => {
                let pts = match mat {
                    Some(m) => pts.map(|(x, y)| m.apply(x, y)),
                    None => pts.map(|(x, y)| (x + dx, y + dy)),
                };
                canvas.fill_quad(pts, *color, clip);
                pintados += 1;
            }
            // Uma imagem não se pinta aqui (sem handle table) — e por isso
            // também não se COMPARA: a área vai para a máscara como o texto,
            // senão a régua mede o que o exemplo não tem em vez do que o motor
            // faz (`claude-object-fit` dava 1,95 % só disto).
            // `Pixels` viajam DENTRO da lista: pintam-se, escalados à caixa por
            // vizinho mais próximo (o `object-fit: fill` que o layout emite).
            DisplayItem::Pixels { rect, data, w, h } if mat.is_none() && *w > 0 && *h > 0 => {
                let r = shift(*rect, dx, dy);
                canvas.fill_pixels(r, data, *w, *h, clip);
                pintados += 1;
            }
            DisplayItem::Image { rect, .. } | DisplayItem::Pixels { rect, .. } => {
                let r = match mat {
                    Some(m) => {
                        let (x0, y0, x1, y1) = transformed_bbox(*rect, &m);
                        Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
                    }
                    None => shift(*rect, dx, dy),
                };
                mask.push([r.x, r.y, r.w, r.h]);
                saltados_imagem += 1;
            }
            DisplayItem::BeginClip { rect, .. } => {
                let r = shift(*rect, dx, dy);
                let novo = match clip {
                    Some(c) => rect_intersect(c, r),
                    None => r,
                };
                clip_stack.push(novo);
            }
            DisplayItem::EndClip { .. } => {
                clip_stack.pop();
            }
            DisplayItem::PushTransform { mat: novo } => {
                // `dx`/`dy` é o deslocamento que uma subárvore REUSADA (via
                // `ChildRef`) soma às coordenadas — dobra-se na parte de
                // TRANSLAÇÃO da matriz (`e`/`f`) em vez de deslocar o `rect`
                // à parte, a mesma regra que `itens::translate_item` já
                // aplica a um `PushTransform` mutado por uma subárvore
                // reusada, só que calculada aqui em vez de gravada na lista.
                let efetiva = Mat2d { e: novo.e + dx, f: novo.f + dy, ..*novo };
                let acumulada = match xform_stack.last() {
                    Some(base) => base.then(efetiva),
                    None => efetiva,
                };
                xform_stack.push(acumulada);
            }
            DisplayItem::PopTransform => {
                xform_stack.pop();
            }
        }
    });

    write_png(saida, W as u32, H as u32, &canvas.px).unwrap_or_else(|e| {
        eprintln!("não escrevi {saida}: {e}");
        std::process::exit(2);
    });

    // A máscara vive ao lado do PNG com o MESMO nome — o comparador a lê sem
    // precisar de um segundo argumento na linha de comandos.
    let mask_path = format!("{saida}.mask.json");
    let mask_json: Vec<String> = mask
        .iter()
        .map(|[x, y, w, h]| format!("[{x:.2},{y:.2},{w:.2},{h:.2}]"))
        .collect();
    std::fs::write(&mask_path, format!("[{}]", mask_json.join(","))).unwrap_or_else(|e| {
        eprintln!("não escrevi {mask_path}: {e}");
        std::process::exit(2);
    });

    // `pintados` já existia (só ia para o `eprintln!` de baixo) — expõe-se aqui
    // como um segundo sidecar, ao lado de `<saida>.mask.json`, para que quem
    // compara dois lados de uma régua (`scripts/wpt_reftests.mjs`) saiba se ESTE
    // lado desenhou alguma coisa sem reabrir o PNG: "0" é o sinal barato de
    // "nada pintado", distinto de "pintou e calhou de dar a mesma cor de fundo".
    let pintados_path = format!("{saida}.pintados");
    std::fs::write(&pintados_path, pintados.to_string()).unwrap_or_else(|e| {
        eprintln!("não escrevi {pintados_path}: {e}");
        std::process::exit(2);
    });

    // The masked text count, beside `.pintados` for the same reason: the WPT
    // runner records it per test without parsing stderr. It is the ruler for
    // how much of the corpus this instrument still cannot see (plan, F3).
    let masked_path = format!("{saida}.mascarados");
    std::fs::write(&masked_path, masked.to_string()).unwrap_or_else(|e| {
        eprintln!("não escrevi {masked_path}: {e}");
        std::process::exit(2);
    });

    eprintln!(
        "rts-raster: {pintados} itens pintados, {saltados_imagem} imagem (mascaradas, sem handle table aqui)"
    );
    eprintln!("rts-raster: {texto_pintado} text items painted, {masked} masked");
}

fn shift(r: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(r.x + dx, r.y + dy, r.w, r.h)
}
