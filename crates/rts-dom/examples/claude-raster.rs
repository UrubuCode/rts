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
//! nem que o rasterizador esteja perfeito. **Exceto quando a família
//! computada é Ahem** (`DisplayItem::Text::ahem`, decidido no layout por
//! `style::ahem::is_ahem_family`): a Ahem não tem glifo nenhum a desenhar,
//! tem retângulos definidos pela SPEC (avanço 1em, ascent 0,8em, descent
//! 0,2em — `style::ahem::ahem_fill_band` diz qual banda cada carácter
//! preenche), então pintá-la é aritmética exata contra a referência, não uma
//! aproximação — `fn pintar_texto_ahem` abaixo. Só o caso SEM `transform`
//! pinta (`mat.is_none()`, mesma condição que já guarda `Pixels`): um glifo
//! por-carácter sob uma matriz pediria a mesma composição que `Quad` já faz
//! para retângulos comuns, e nenhuma fixture de `linebox` combina as duas
//! coisas — alargar quando uma aparecer, não antes. `Image` — aponta para um handle do
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

use rts_dom::layout::{self, DisplayItem, DisplayList, Mat2d, Rect, TextMeasurer};
use rts_dom::Dom;
use std::io::Write;
use std::path::Path;

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
}

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

    /// Um bitmap RGBA8 esticado a `r` por vizinho mais próximo — o suficiente
    /// para uma régua de 1280×800 sobre ícones e fixtures; um filtro bilinear
    /// mudaria pixels de borda que a tolerância por canal já absorve.
    fn fill_pixels(&mut self, r: Rect, data: &[u8], w: u32, h: u32, clip: Option<Rect>) {
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.w).ceil() as i32;
        let y1 = (r.y + r.h).ceil() as i32;
        for y in y0..y1 {
            let sy = (((y as f32 + 0.5 - r.y) / r.h) * h as f32).floor().clamp(0.0, (h - 1) as f32) as usize;
            for x in x0..x1 {
                let sx = (((x as f32 + 0.5 - r.x) / r.w) * w as f32).floor().clamp(0.0, (w - 1) as f32) as usize;
                let i = (sy * w as usize + sx) * 4;
                if i + 3 >= data.len() {
                    continue;
                }
                let c = (u32::from(data[i]) << 24) | (u32::from(data[i + 1]) << 16) | (u32::from(data[i + 2]) << 8) | u32::from(data[i + 3]);
                self.blend(x, y, c, clip);
            }
        }
    }

    /// Um quadrilátero CONVEXO já em coordenadas de tela, por varrimento: em
    /// cada linha de pixels, o vão entre a menor e a maior intersecção das
    /// quatro arestas com o centro da linha. É o que pinta um lado de borda
    /// com junção diagonal (`DisplayItem::Quad`) — o `fill_rect_mat` não
    /// serve porque um trapézio não é a imagem de um retângulo por uma matriz.
    fn fill_quad(&mut self, pts: [(f32, f32); 4], color: u32, clip: Option<Rect>) {
        let y0 = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor() as i32;
        let y1 = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
        for y in y0..y1 {
            let fy = y as f32 + 0.5;
            let (mut xa, mut xb) = (f32::INFINITY, f32::NEG_INFINITY);
            for i in 0..4 {
                let (p, q) = (pts[i], pts[(i + 1) % 4]);
                if (p.1 <= fy) != (q.1 <= fy) {
                    let x = p.0 + (fy - p.1) * (q.0 - p.0) / (q.1 - p.1);
                    xa = xa.min(x);
                    xb = xb.max(x);
                }
            }
            if xa > xb {
                continue;
            }
            for x in (xa - 0.5).ceil() as i32..(xb - 0.5).ceil() as i32 {
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

    /// `r` sob `mat` (rotação/skew/matrix, não a translação/escala pura que
    /// `fill_rect` já cobre exatamente): preenche o QUADRILÁTERO real, não a
    /// bounding box axis-aligned. Percorre a bbox dos 4 cantos transformados
    /// e, por pixel, volta ao referencial ORIGINAL pela INVERSA — a mesma
    /// técnica de um rasterizador de textura (ponto-no-polígono por
    /// coordenadas locais em vez de um teste geométrico de aresta, porque o
    /// polígono é sempre um paralelogramo e a inversa já responde "dentro?"
    /// em uma multiplicação).
    fn fill_rect_mat(&mut self, r: Rect, mat: &Mat2d, color: u32, clip: Option<Rect>) {
        let Some(inv) = mat_invert(mat) else { return };
        let (bx0, by0, bx1, by1) = transformed_bbox(r, mat);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let (ox, oy) = inv.apply(fx, fy);
                if ox >= r.x && ox <= r.x + r.w && oy >= r.y && oy <= r.y + r.h {
                    self.blend(x, y, color, clip);
                }
            }
        }
    }

    /// A mesma ideia de `fill_rect_mat`, mas com o gradiente calculado no
    /// referencial LOCAL (`ox`,`oy`, já sem a matriz) — o CSS pinta o
    /// gradiente na caixa e SÓ DEPOIS aplica o `transform` (Transforms 1 §3:
    /// o alvo é a imagem já composta), então o `t` de interpolação usa a
    /// mesma fórmula de `fill_gradient`, só que alimentada pelo ponto
    /// devolvido pela inversa em vez do ponto de tela cru.
    fn fill_gradient_mat(&mut self, r: Rect, mat: &Mat2d, c0: u32, c1: u32, angle_deg: f32, clip: Option<Rect>) {
        let Some(inv) = mat_invert(mat) else { return };
        let rad = angle_deg.to_radians();
        let (dx, dy) = (rad.sin(), -rad.cos());
        let corners = [(r.x, r.y), (r.x + r.w, r.y), (r.x, r.y + r.h), (r.x + r.w, r.y + r.h)];
        let ts: Vec<f32> = corners.iter().map(|(cx, cy)| cx * dx + cy * dy).collect();
        let tmin = ts.iter().cloned().fold(f32::INFINITY, f32::min);
        let tmax = ts.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let span = (tmax - tmin).max(0.0001);
        let (bx0, by0, bx1, by1) = transformed_bbox(r, mat);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let (ox, oy) = inv.apply(fx, fy);
                if ox < r.x || ox > r.x + r.w || oy < r.y || oy > r.y + r.h {
                    continue;
                }
                let t = ((ox * dx + oy * dy) - tmin) / span;
                let t = t.clamp(0.0, 1.0);
                self.blend(x, y, lerp_color(c0, c1, t), clip);
            }
        }
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

    /// `stroke_rect` sob uma matriz: as 4 tiras, cada uma pelo seu próprio
    /// `fill_rect_mat` — um paralelogramo por lado é exato para uma
    /// transformação afim (a aresta reta de um retângulo continua reta sob
    /// `matrix()`), o mesmo argumento que já vale para `fill_rect_mat`.
    fn stroke_rect_mat(&mut self, r: Rect, mat: &Mat2d, width: f32, color: u32, clip: Option<Rect>) {
        let w = width.max(1.0);
        self.fill_rect_mat(Rect::new(r.x, r.y, r.w, w), mat, color, clip);
        self.fill_rect_mat(Rect::new(r.x, r.y + r.h - w, r.w, w), mat, color, clip);
        self.fill_rect_mat(Rect::new(r.x, r.y, w, r.h), mat, color, clip);
        self.fill_rect_mat(Rect::new(r.x + r.w - w, r.y, w, r.h), mat, color, clip);
    }
}

/// A inversa de uma matriz afim 2D (`[[a,c,e],[b,d,f],[0,0,1]]`), ou `None`
/// se o determinante for ~0 (`scale(0)`, degenerada — nada pintável de todo
/// modo). Fórmula fechada de uma 2×2 mais a translação recomposta.
fn mat_invert(m: &Mat2d) -> Option<Mat2d> {
    let det = m.a * m.d - m.b * m.c;
    if det.abs() < 1.0e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    let a = m.d * inv_det;
    let b = -m.b * inv_det;
    let c = -m.c * inv_det;
    let d = m.a * inv_det;
    Some(Mat2d {
        a,
        b,
        c,
        d,
        e: -(a * m.e + c * m.f),
        f: -(b * m.e + d * m.f),
    })
}

/// A caixa de pixels inteiros que cobre os 4 cantos de `r` sob `mat` —
/// mesma conta que `Mat2d::transform_rect_bbox`, mas já em `i32` de canvas
/// (floor/ceil, como `fill_rect`) para os dois rasterizadores por matriz
/// iterarem sobre ela.
fn transformed_bbox(r: Rect, mat: &Mat2d) -> (i32, i32, i32, i32) {
    let pts = [
        mat.apply(r.x, r.y),
        mat.apply(r.x + r.w, r.y),
        mat.apply(r.x, r.y + r.h),
        mat.apply(r.x + r.w, r.y + r.h),
    ];
    let min_x = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.0));
    let max_x = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.0));
    let min_y = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.1));
    let max_y = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.1));
    (
        min_x.floor().max(0.0) as i32,
        min_y.floor().max(0.0) as i32,
        (max_x.ceil() as i32).min(W as i32),
        (max_y.ceil() as i32).min(H as i32),
    )
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

/// Pinta UM item de texto Ahem como a spec define — sem medidor nenhum: cada
/// carácter avança exatamente `size` (`AHEM_ADVANCE`, `style::ahem`) a partir
/// de `x`, e a banda vertical que preenche (cheia, metade de cima, metade de
/// baixo, ou nenhuma) vem de `rts_dom::style::ahem_fill_band`. `x`/`y` são o
/// canto superior-esquerdo do texto (`text_top`, ver `DisplayItem::Text`),
/// então a banda de cada carácter é `y + top_frac*size .. y + bottom_frac*size`.
fn pintar_texto_ahem(canvas: &mut Canvas, x: f32, y: f32, text: &str, size: f32, letter_spacing: f32, color: u32, clip: Option<Rect>) {
    let mut cx = x;
    for c in text.chars() {
        if let Some((topo, fundo)) = rts_dom::style::ahem_fill_band(c) {
            let r = Rect::new(cx, y + topo * size, size, (fundo - topo) * size);
            canvas.fill_rect(r, color, clip);
        }
        cx += size + letter_spacing;
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
    let ctx = layout::LayoutCtx {
        viewport_w: W as f32,
        viewport_h: H as f32,
        measurer: &layout::ApproxMeasurer,
    };
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
    let mut saltados_texto = 0usize;
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
            DisplayItem::Text { x, y, text, size, mono, ahem, color, letter_spacing, .. }
                if *ahem && mat.is_none() =>
            {
                pintar_texto_ahem(&mut canvas, *x + dx, *y + dy, text, *size, *letter_spacing, *color, clip);
                pintados += 1;
            }
            DisplayItem::Text { x, y, text, size, mono, .. } => {
                let (mx, my) = match mat {
                    Some(m) => m.apply(*x, *y),
                    None => (*x + dx, *y + dy),
                };
                let r = text_mask_rect(mx, my, text, *size, *mono);
                mask.push([r.x, r.y, r.w, r.h]);
                saltados_texto += 1;
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

    eprintln!(
        "rts-raster: {pintados} itens pintados, {saltados_texto} texto (mascarado), \
         {saltados_imagem} imagem (mascaradas, sem handle table aqui)"
    );
}

fn shift(r: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(r.x + dx, r.y + dy, r.w, r.h)
}
