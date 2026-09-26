//! Glyph coverage: the outline from `skrifa`, flattened to lines, filled by a
//! scanline of our own — four sub-rows per pixel row, exact horizontal
//! coverage at each span's ends.
//!
//! The fill rule is NONZERO. Neither `glyf` nor CFF carries a rule per glyph:
//! both formats define their outlines under nonzero winding (TrueType's
//! overlapping contours rely on it), so "as the outline says" is nonzero
//! everywhere this crate reads. `swash` is the named alternative if this
//! ever needs hinting or subpixel positioning; it does not today, so it is
//! not a dependency.

use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId};

use crate::face::Face;

/// A coverage bitmap. `left` is the pen-relative x of column 0; `top` is how
/// far row 0 lies ABOVE the baseline (y up), as FreeType's bitmap_top.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
    pub alpha: Vec<u8>,
}

const SUB: usize = 4;
const FLATTEN_STEPS: usize = 8;

/// Rasterises glyph `id` of `face` at `size` px. An empty glyph (a space)
/// answers an empty bitmap; `None` only when the glyph does not exist.
pub fn rasterise(face: &Face, id: u16, size: f32) -> Option<Bitmap> {
    let (data, index) = face.data();
    let font = FontRef::from_index(data, index).ok()?;
    let glyph = font.outline_glyphs().get(GlyphId::new(u32::from(id)))?;
    let mut pen = Flatten::default();
    glyph.draw(DrawSettings::unhinted(Size::new(size), LocationRef::default()), &mut pen).ok()?;
    pen.close();
    Some(fill(&pen.edges))
}

/// Line segments in pixel space, y DOWN (so `y = -outline_y`).
#[derive(Default)]
struct Flatten {
    edges: Vec<[f32; 4]>,
    start: (f32, f32),
    at: (f32, f32),
}

impl Flatten {
    fn push(&mut self, x: f32, y: f32) {
        let p = (x, -y);
        if p != self.at {
            self.edges.push([self.at.0, self.at.1, p.0, p.1]);
        }
        self.at = p;
    }
}

impl OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = (x, -y);
        self.at = self.start;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(x, y);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = (self.at.0, -self.at.1);
        for i in 1..=FLATTEN_STEPS {
            let t = i as f32 / FLATTEN_STEPS as f32;
            let u = 1.0 - t;
            self.push(u * u * x0 + 2.0 * u * t * cx + t * t * x, u * u * y0 + 2.0 * u * t * cy + t * t * y);
        }
    }
    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (x0, y0) = (self.at.0, -self.at.1);
        for i in 1..=FLATTEN_STEPS {
            let t = i as f32 / FLATTEN_STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push(a * x0 + b * c0x + c * c1x + d * x, a * y0 + b * c0y + c * c1y + d * y);
        }
    }
    fn close(&mut self) {
        if self.at != self.start {
            self.edges.push([self.at.0, self.at.1, self.start.0, self.start.1]);
            self.at = self.start;
        }
    }
}

fn fill(edges: &[[f32; 4]]) -> Bitmap {
    if edges.is_empty() {
        return Bitmap::default();
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for e in edges {
        x0 = x0.min(e[0]).min(e[2]);
        x1 = x1.max(e[0]).max(e[2]);
        y0 = y0.min(e[1]).min(e[3]);
        y1 = y1.max(e[1]).max(e[3]);
    }
    let (left, top) = (x0.floor() as i32, y0.floor() as i32);
    let w = (x1.ceil() as i32 - left).max(0) as usize;
    let h = (y1.ceil() as i32 - top).max(0) as usize;
    let mut acc = vec![0f32; w * h];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..h {
        for s in 0..SUB {
            let y = top as f32 + row as f32 + (s as f32 + 0.5) / SUB as f32;
            crossings.clear();
            for e in edges {
                let (ya, yb) = (e[1], e[3]);
                if ya == yb || y < ya.min(yb) || y >= ya.max(yb) {
                    continue;
                }
                let x = e[0] + (y - ya) * (e[2] - e[0]) / (yb - ya);
                crossings.push((x - left as f32, if yb > ya { 1 } else { -1 }));
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    span(&mut acc[row * w..(row + 1) * w], pair[0].0, pair[1].0);
                }
            }
        }
    }
    let alpha = acc.iter().map(|&c| (c / SUB as f32 * 255.0).round().clamp(0.0, 255.0) as u8).collect();
    Bitmap { w: w as u32, h: h as u32, left, top: -top, alpha }
}

/// Adds the covered fraction of each pixel of `row` between `a` and `b`.
fn span(row: &mut [f32], a: f32, b: f32) {
    let (a, b) = (a.max(0.0), b.min(row.len() as f32));
    if b <= a {
        return;
    }
    let (ia, ib) = (a.floor() as usize, (b.ceil() as usize).min(row.len()));
    for (i, px) in row.iter_mut().enumerate().take(ib).skip(ia) {
        let lo = a.max(i as f32);
        let hi = b.min(i as f32 + 1.0);
        *px += (hi - lo).max(0.0);
    }
}
