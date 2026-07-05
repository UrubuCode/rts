import buffer from "rts:buffer";
import math from "rts:math";

// RTS-MINE engine — renderer de ENTIDADES (billboards por software).
//
// Como no Minecraft, cada coisa tem seu render: o MUNDO é o raycaster
// (raycast.ts); ENTIDADES são sprites projetados por este módulo, pintados
// direto no framebuffer depois do mundo. Oclusão: um raio DDA da câmera até a
// entidade (binária — atrás de parede some).
//
// Lê a câmera + resolução ativa + tsec do stbuf (slots em config.ts).

/// Desenha um billboard em (ex,ey,ez) com altura `size` (em blocos).
/// `kind`: 1 = slime (corpo verde quicante com olhos).
export function drawBillboard(fbuf: i64, wbuf: i64, stbuf: i64, ex: f64, ey: f64, ez: f64, size: f64, kind: number): void {
  const ox = buffer.read_f64(stbuf, 0);
  const oy = buffer.read_f64(stbuf, 8);
  const oz = buffer.read_f64(stbuf, 16);
  const yaw = buffer.read_f64(stbuf, 32);
  const pitch = buffer.read_f64(stbuf, 40);
  const wx = buffer.read_f64(stbuf, 112);
  const wy = buffer.read_f64(stbuf, 120);
  const wz = buffer.read_f64(stbuf, 128);
  const rw = buffer.read_f64(stbuf, 144);
  const rh = buffer.read_f64(stbuf, 152);

  // base da câmera (mesma convenção do raycast.ts)
  const cyw = math.cos(yaw);
  const syw = math.sin(yaw);
  const cp = math.cos(pitch);
  const sp = math.sin(pitch);
  const fx = syw * cp;
  const fy = sp;
  const fz = cyw * cp;
  const rxx = cyw;
  const rxz = 0 - syw;
  const ux = 0 - syw * sp;
  const uy = cp;
  const uz = 0 - cyw * sp;

  // vetor câmera→entidade em espaço de câmera
  const dxw = ex - ox;
  const dyw = ey - oy;
  const dzw = ez - oz;
  const cz = dxw * fx + dyw * fy + dzw * fz;
  if (cz < 0.35) return; // atrás/em cima da câmera
  const cx = dxw * rxx + dzw * rxz;
  const cyv = dxw * ux + dyw * uy + dzw * uz;

  // ── projeção pra tela (ANTES da oclusão: fora do FOV sai de graça) ─────────
  const thf = 0.66;
  const asp = rw / rh;
  const scx = ((cx / cz) / (thf * asp) * 0.5 + 0.5) * rw;
  const scy = (0.5 - (cyv / cz) / thf * 0.5) * rh;
  const ts2 = buffer.read_f64(stbuf, 160);
  const squash = 1.0 + math.sin(ts2 * 5.0 + ex * 2.7 + ez * 1.9) * 0.12;
  const hpx = (size / cz) * (rh / (2 * thf)) / squash;
  const wpx = (size / cz) * (rh / (2 * thf)) * squash;
  if (hpx < 1.5) return; // longe demais pra 1 pixel
  let x0 = (scx - wpx * 0.5) | 0;
  let x1 = (scx + wpx * 0.5) | 0;
  let y0 = (scy - hpx * 0.5) | 0;
  let y1 = (scy + hpx * 0.5) | 0;
  if (x1 < 0 || y1 < 0 || x0 > rw - 1 || y0 > rh - 1) return; // fora da tela
  if (x0 < 0) x0 = 0;
  if (y0 < 0) y0 = 0;
  if (x1 > rw - 1) x1 = rw - 1;
  if (y1 > rh - 1) y1 = rh - 1;

  // ── oclusão binária: DDA da câmera até a entidade ──────────────────────────
  const dist = math.sqrt(dxw * dxw + dyw * dyw + dzw * dzw);
  const idist = 1 / dist;
  const ndx = dxw * idist;
  const ndy = dyw * idist;
  const ndz = dzw * idist;
  const ddx = math.abs(1 / ndx);
  const ddy = math.abs(1 / ndy);
  const ddz = math.abs(1 / ndz);
  let mX = math.floor(ox);
  let mY = math.floor(oy);
  let mZ = math.floor(oz);
  let stepX = 1;
  let tMaxX: f64 = (mX + 1 - ox) * ddx;
  if (ndx < 0) { stepX = -1; tMaxX = (ox - mX) * ddx; }
  let stepY = 1;
  let tMaxY: f64 = (mY + 1 - oy) * ddy;
  if (ndy < 0) { stepY = -1; tMaxY = (oy - mY) * ddy; }
  let stepZ = 1;
  let tMaxZ: f64 = (mZ + 1 - oz) * ddz;
  if (ndz < 0) { stepZ = -1; tMaxZ = (oz - mZ) * ddz; }
  const limit = dist - 0.6; // para antes do corpo da entidade
  let occluded = 0;
  let stp = 0;
  while (stp < 96) {
    let t: f64 = 0.0;
    if (tMaxX < tMaxY && tMaxX < tMaxZ) { t = tMaxX; tMaxX = tMaxX + ddx; mX = mX + stepX; }
    else if (tMaxY < tMaxZ) { t = tMaxY; tMaxY = tMaxY + ddy; mY = mY + stepY; }
    else { t = tMaxZ; tMaxZ = tMaxZ + ddz; mZ = mZ + stepZ; }
    if (t > limit) break;
    if (mX < 0 || mX >= wx || mZ < 0 || mZ >= wz || mY < 0) break;
    if (mY < wy) {
      const bb = buffer.read_u8(wbuf, (mY * wz + mZ) * wx + mX);
      if (bb !== 0 && bb !== 4) { occluded = 1; break; }
    }
    stp = stp + 1;
  }
  if (occluded !== 0) return;

  // ── pinta o sprite procedural (kind 1 = slime) ─────────────────────────────
  // sprite GRANDE (perto da câmera) pinta colunas alternadas (metade dos
  // writes) — o dither casa com o checkerboard do mundo
  let xStep = 1;
  if (wpx > 44) xStep = 2;
  const invW = 1 / wpx;
  const invH = 1 / hpx;
  let py = y0;
  while (py <= y1) {
    // u,v normalizados no corpo [-1, 1]
    const v = (py + 0.5 - scy) * 2 * invH;
    let px = x0;
    if (xStep === 2) {
      // xadrez sem bitwise (bitwise sobre float cai no caminho genérico)
      const odd = py - math.floor(py * 0.5) * 2;
      px = x0 + odd;
    }
    while (px <= x1) {
      const u = (px + 0.5 - scx) * 2 * invW;
      // corpo arredondado: descarta fora da "cápsula"
      const rr = u * u + v * v;
      if (rr <= 1.0) {
        // gradiente: topo mais claro; borda mais escura
        let g = 1.0 - rr * 0.35 - v * 0.18;
        let cr = 92 * g;
        let cg = 205 * g;
        let cb = 96 * g;
        // olhos (dois pontos escuros) e boca
        const eu1 = u + 0.34;
        const eu2 = u - 0.34;
        const ev = v + 0.18;
        if (eu1 * eu1 + ev * ev < 0.028) { cr = 20; cg = 34; cb = 22; }
        if (eu2 * eu2 + ev * ev < 0.028) { cr = 20; cg = 34; cb = 22; }
        // brilho no topo-esquerda
        const bu = u + 0.42;
        const bv = v + 0.5;
        if (bu * bu + bv * bv < 0.04) { cr = cr + 90; cg = cg + 90; cb = cb + 80; }
        if (cr > 255) cr = 255;
        if (cg > 255) cg = 255;
        if (cb > 255) cb = 255;
        const rgba = (cr | 0) | ((cg | 0) << 8) | ((cb | 0) << 16) | (0xFF << 24);
        buffer.write_i32(fbuf, (py * rw + px) * 4, rgba);
      }
      px = px + xStep;
    }
    py = py + 1;
  }
}
