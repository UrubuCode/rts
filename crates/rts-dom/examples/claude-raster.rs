//! O RASTERIZADOR headless do nosso lado da régua de pintura: parseia uma
//! fixture, faz layout a 1280x800 com o `ApproxMeasurer` (o mesmo do
//! `claude-paint-dump.rs`, para que os dois lados desta régua meçam o MESMO
//! layout) e escreve um PNG RGBA da `DisplayList` — sem egui, sem wgpu, sem
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
//! SALTA: `Text` — o medidor é aproximado (`0.5×size` por caractere, sem
//! fonte real) e pintar caixas por essa métrica compararia um erro de medição
//! contra um glifo real do Blink, o que a fixture de texto nunca vai bater
//! nem que o rasterizador esteja perfeito. `Image`/`Pixels` — a primeira
//! aponta para um handle do `HandleTable`, que este exemplo não tem (não há
//! `Engine` nem `Registry` aqui, só `Dom`+layout); a segunda carrega bytes
//! próprios e É pintável, mas nenhuma fixture do corpus a usa hoje, então fica
//! para quando uma precisar. Ambas as omissões contam para a MÁSCARA que o
//! comparador (`scripts/css_pintura_comparar.mjs`) recebe: os rects de texto
//! do nosso lado saem também num `.mask.json` ao lado do PNG.
//!
//! # Zero dependências novas no CRATE
//!
//! `rts-dom` continua com zero `[dependencies]` — o PNG é escrito à mão neste
//! FICHEIRO (bloco IDAT com deflate "stored", sem compressão) porque o
//! ficheiro é um exemplo e não faz parte do crate publicado, e a alternativa
//! (a crate `png`) não está no `Cargo.lock`: trazê-la custaria uma
//! dependência nova (mesmo que só de dev) para ~40 linhas de CRC32/Adler32.

use rts_dom::layout::{self, DisplayItem, DisplayList, Rect, TextMeasurer};
use rts_dom::Dom;
use std::io::Write;

const W: usize = 1280;
const H: usize = 800;

/// Um pixel RGBA reto (sem alpha pré-multiplicado) num buffer W×H.
struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    fn new(bg: u32) -> Canvas {
        let mut px = vec![0u8; W * H * 4];
        let (r, g, b, a) = argb_bytes(bg);
        for i in 0..(W * H) {
            px[i * 4] = r;
            px[i * 4 + 1] = g;
            px[i * 4 + 2] = b;
            px[i * 4 + 3] = a;
        }
        Canvas { px }
    }

    /// Alpha-blend de UM pixel. `clip` é a interseção corrente das
    /// `BeginClip` abertas — `None` fora de qualquer rect pintável.
    fn blend(&mut self, x: i32, y: i32, color: u32, clip: Option<Rect>) {
        if x < 0 || y < 0 || x as usize >= W || y as usize >= H {
            return;
        }
        if let Some(c) = clip {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            if fx < c.x || fy < c.y || fx > c.x + c.w || fy > c.y + c.h {
                return;
            }
        }
        let (r, g, b, a) = argb_bytes(color);
        if a == 0 {
            return;
        }
        let i = (y as usize * W + x as usize) * 4;
        if a == 255 {
            self.px[i] = r;
            self.px[i + 1] = g;
            self.px[i + 2] = b;
            self.px[i + 3] = 255;
            return;
        }
        let af = a as f32 / 255.0;
        for (k, s) in [r, g, b].into_iter().enumerate() {
            let d = self.px[i + k] as f32;
            self.px[i + k] = (s as f32 * af + d * (1.0 - af)).round() as u8;
        }
        self.px[i + 3] = 255;
    }

    fn fill_rect(&mut self, r: Rect, color: u32, clip: Option<Rect>) {
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.w).ceil() as i32;
        let y1 = (r.y + r.h).ceil() as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, color, clip);
            }
        }
    }

    /// Borda como quatro tiras — não um retângulo vazado, para não assumir
    /// que `width` é igual nos quatro lados (a `DisplayList` já colapsou para
    /// um valor só neste item; ver o comentário em `display.rs`).
    fn stroke_rect(&mut self, r: Rect, width: f32, color: u32, clip: Option<Rect>) {
        let w = width.max(1.0);
        self.fill_rect(Rect::new(r.x, r.y, r.w, w), color, clip); // topo
        self.fill_rect(Rect::new(r.x, r.y + r.h - w, r.w, w), color, clip); // fundo
        self.fill_rect(Rect::new(r.x, r.y, w, r.h), color, clip); // esquerda
        self.fill_rect(Rect::new(r.x + r.w - w, r.y, w, r.h), color, clip); // direita
    }

    /// Gradiente linear por pixel. `angle_deg` na convenção do CSS (0 = para
    /// cima, 90 = para a direita) — a mesma leitura que `pintura.rs` faz no
    /// backend egui, para que o mesh e este pixel-a-pixel concordem.
    fn fill_gradient(&mut self, r: Rect, c0: u32, c1: u32, angle_deg: f32, clip: Option<Rect>) {
        let rad = angle_deg.to_radians();
        let (dx, dy) = (rad.sin(), -rad.cos()); // direção do gradiente
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.w).ceil() as i32;
        let y1 = (r.y + r.h).ceil() as i32;
        // Projeta os 4 cantos na direção do gradiente para achar [tmin, tmax] —
        // o intervalo real que o rect ocupa, não [0,1] cru.
        let corners = [(r.x, r.y), (r.x + r.w, r.y), (r.x, r.y + r.h), (r.x + r.w, r.y + r.h)];
        let ts: Vec<f32> = corners.iter().map(|(cx, cy)| cx * dx + cy * dy).collect();
        let tmin = ts.iter().cloned().fold(f32::INFINITY, f32::min);
        let tmax = ts.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let span = (tmax - tmin).max(0.0001);
        for y in y0..y1 {
            for x in x0..x1 {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let t = ((fx * dx + fy * dy) - tmin) / span;
                let t = t.clamp(0.0, 1.0);
                self.blend(x, y, lerp_color(c0, c1, t), clip);
            }
        }
    }
}

fn argb_bytes(c: u32) -> (u8, u8, u8, u8) {
    // A `DisplayList` guarda RGBA em u32 (ver comentário em `display.rs`:
    // "cor é u32 RGBA"). Byte mais significativo = R.
    (
        ((c >> 24) & 0xff) as u8,
        ((c >> 16) & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        (c & 0xff) as u8,
    )
}

fn lerp_color(c0: u32, c1: u32, t: f32) -> u32 {
    let (r0, g0, b0, a0) = argb_bytes(c0);
    let (r1, g1, b1, a1) = argb_bytes(c1);
    let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u32;
    (l(r0, r1) << 24) | (l(g0, g1) << 16) | (l(b0, b1) << 8) | l(a0, a1)
}

fn rect_intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Escreve um PNG RGBA8 mínimo: sem paleta, sem interlace, um bloco IDAT
/// deflate "stored" (sem compressão — o ficheiro fica maior que um PNG real,
/// mas continua um PNG válido; qualquer leitor de PNG o decodifica).
fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bpc, RGBA, sem filtro/interlace especial
    write_chunk(&mut out, b"IHDR", &ihdr);

    // Um filtro-byte 0 (None) por linha, depois os RGBA da linha.
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0u8);
        let row = &rgba[y * w as usize * 4..(y + 1) * w as usize * 4];
        raw.extend_from_slice(row);
    }
    let zlib = deflate_stored(&raw);
    write_chunk(&mut out, b"IDAT", &zlib);
    write_chunk(&mut out, b"IEND", &[]);

    std::fs::File::create(path)?.write_all(&out)
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// zlib: cabeçalho de 2 bytes, N blocos deflate "stored" (BFINAL/BTYPE=00,
/// cada um até 65535 bytes crus), depois Adler-32.
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // CMF/FLG: deflate, janela 32K, sem dicionário
    let chunks = data.chunks(65535).collect::<Vec<_>>();
    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i + 1 == chunks.len();
        out.push(if is_last { 1 } else { 0 });
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Um item de texto não pintado, para a máscara de "área ignorada" que o
/// comparador soma. Usa o MESMO `ApproxMeasurer` do layout — a régua de N já
/// diz porquê: um medidor diferente aqui daria uma máscara que não cobre o
/// que o layout de facto colocou onde.
fn text_mask_rect(x: f32, y: f32, text: &str, size: f32, mono: bool) -> Rect {
    let w = layout::ApproxMeasurer.text_width(text, size, mono, false, false);
    Rect::new(x, y - size, w, size * 1.3)
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
    let dom: Dom = rts_dom::parse_html_to_dom(&html);
    let ctx = layout::LayoutCtx {
        viewport_w: W as f32,
        viewport_h: H as f32,
        measurer: &layout::ApproxMeasurer,
    };
    let list: DisplayList = layout::layout_document(&dom, &ctx);

    let mut canvas = Canvas::new(list.canvas_background);
    let mut clip_stack: Vec<Rect> = Vec::new();
    let mut mask: Vec<[f32; 4]> = Vec::new(); // [x,y,w,h] dos rects de texto ignorados
    let mut pintados = 0usize;
    let mut saltados_texto = 0usize;
    let mut saltados_imagem = 0usize;

    list.walk(|item, dx, dy| {
        let clip = clip_stack.last().copied();
        match item {
            DisplayItem::SolidRect { rect, color, .. } => {
                canvas.fill_rect(shift(*rect, dx, dy), *color, clip);
                pintados += 1;
            }
            DisplayItem::Border { rect, width, color, .. } => {
                canvas.stroke_rect(shift(*rect, dx, dy), *width, *color, clip);
                pintados += 1;
            }
            DisplayItem::GradientRect { rect, c0, c1, angle_deg, .. } => {
                canvas.fill_gradient(shift(*rect, dx, dy), *c0, *c1, *angle_deg, clip);
                pintados += 1;
            }
            DisplayItem::Shadow { rect, dx: sdx, dy: sdy, color, .. } => {
                let r = shift(*rect, dx + sdx, dy + sdy);
                canvas.fill_rect(r, *color, clip);
                pintados += 1;
            }
            DisplayItem::Text { x, y, text, size, mono, .. } => {
                let r = text_mask_rect(x + dx, y + dy, text, *size, *mono);
                mask.push([r.x, r.y, r.w, r.h]);
                saltados_texto += 1;
            }
            DisplayItem::Image { rect, .. } | DisplayItem::Pixels { rect, .. } => {
                let _ = shift(*rect, dx, dy);
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

    eprintln!(
        "rts-raster: {pintados} itens pintados, {saltados_texto} texto (mascarado), \
         {saltados_imagem} imagem (saltados, sem handle table aqui)"
    );
}

fn shift(r: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(r.x + dx, r.y + dy, r.w, r.h)
}
