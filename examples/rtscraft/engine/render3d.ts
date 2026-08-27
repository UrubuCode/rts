// RTS-MINE — renderer de entidades por billboards sobre o framebuffer.

export function drawBillboard(fbuf: Uint32Array, wbuf: Uint8Array, stbuf: Float64Array, ex: f64, ey: f64, ez: f64, size: f64, kind: number): void {
  const ox = stbuf[0];
  const oy = stbuf[1];
  const oz = stbuf[2];
  const yaw = stbuf[4];
  const pitch = stbuf[5];
  const wx = stbuf[14];
  const wy = stbuf[15];
  const wz = stbuf[16];
  const rw = stbuf[18];
  const rh = stbuf[19];

  const cyw = Math.cos(yaw);
  const syw = Math.sin(yaw);
  const cp = Math.cos(pitch);
  const sp = Math.sin(pitch);
  const fx = syw * cp;
  const fy = sp;
  const fz = cyw * cp;
  const rxx = cyw;
  const rxz = 0 - syw;
  const ux = 0 - syw * sp;
  const uy = cp;
  const uz = 0 - cyw * sp;

  const dxw = ex - ox;
  const dyw = ey - oy;
  const dzw = ez - oz;
  const cz = dxw * fx + dyw * fy + dzw * fz;
  if (cz < 0.35) return;
  const cx = dxw * rxx + dzw * rxz;
  const cyv = dxw * ux + dyw * uy + dzw * uz;

  const thf = 0.66;
  const asp = rw / rh;
  const scx = ((cx / cz) / (thf * asp) * 0.5 + 0.5) * rw;
  const scy = (0.5 - (cyv / cz) / thf * 0.5) * rh;
  const ts2 = stbuf[20];
  const squash = 1.0 + Math.sin(ts2 * 5.0 + ex * 2.7 + ez * 1.9) * 0.12;
  const hpx = (size / cz) * (rh / (2 * thf)) / squash;
  const wpx = (size / cz) * (rh / (2 * thf)) * squash;
  if (hpx < 1.5) return;
  let x0 = (scx - wpx * 0.5) | 0;
  let x1 = (scx + wpx * 0.5) | 0;
  let y0 = (scy - hpx * 0.5) | 0;
  let y1 = (scy + hpx * 0.5) | 0;
  if (x1 < 0 || y1 < 0 || x0 > rw - 1 || y0 > rh - 1) return;
  if (x0 < 0) x0 = 0;
  if (y0 < 0) y0 = 0;
  if (x1 > rw - 1) x1 = rw - 1;
  if (y1 > rh - 1) y1 = rh - 1;

  const dist = Math.sqrt(dxw * dxw + dyw * dyw + dzw * dzw);
  const idist = 1 / dist;
  const ndx = dxw * idist;
  const ndy = dyw * idist;
  const ndz = dzw * idist;
  const ddx = Math.abs(1 / ndx);
  const ddy = Math.abs(1 / ndy);
  const ddz = Math.abs(1 / ndz);
  let mX = Math.floor(ox);
  let mY = Math.floor(oy);
  let mZ = Math.floor(oz);
  let stepX = 1;
  let tMaxX: f64 = (mX + 1 - ox) * ddx;
  if (ndx < 0) { stepX = -1; tMaxX = (ox - mX) * ddx; }
  let stepY = 1;
  let tMaxY: f64 = (mY + 1 - oy) * ddy;
  if (ndy < 0) { stepY = -1; tMaxY = (oy - mY) * ddy; }
  let stepZ = 1;
  let tMaxZ: f64 = (mZ + 1 - oz) * ddz;
  if (ndz < 0) { stepZ = -1; tMaxZ = (oz - mZ) * ddz; }
  const limit = dist - 0.6;
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
      const block = wbuf[(mY * wz + mZ) * wx + mX];
      if (block !== 0 && block !== 4) { occluded = 1; break; }
    }
    stp = stp + 1;
  }
  if (occluded !== 0) return;

  let xStep = 1;
  if (wpx > 44) xStep = 2;
  const invW = 1 / wpx;
  const invH = 1 / hpx;
  let py = y0;
  while (py <= y1) {
    const v = (py + 0.5 - scy) * 2 * invH;
    let px = x0;
    if (xStep === 2) {
      const odd = py - Math.floor(py * 0.5) * 2;
      px = x0 + odd;
    }
    while (px <= x1) {
      const u = (px + 0.5 - scx) * 2 * invW;
      const rr = u * u + v * v;
      if (rr <= 1.0) {
        let g = 1.0 - rr * 0.35 - v * 0.18;
        let cr = 92 * g;
        let cg = 205 * g;
        let cb = 96 * g;
        const eu1 = u + 0.34;
        const eu2 = u - 0.34;
        const ev = v + 0.18;
        if (eu1 * eu1 + ev * ev < 0.028) { cr = 20; cg = 34; cb = 22; }
        if (eu2 * eu2 + ev * ev < 0.028) { cr = 20; cg = 34; cb = 22; }
        const bu = u + 0.42;
        const bv = v + 0.5;
        if (bu * bu + bv * bv < 0.04) { cr = cr + 90; cg = cg + 90; cb = cb + 80; }
        if (cr > 255) cr = 255;
        if (cg > 255) cg = 255;
        if (cb > 255) cb = 255;
        fbuf[py * rw + px] = (cr | 0) | ((cg | 0) << 8) | ((cb | 0) << 16) | (0xFF << 24);
      }
      px = px + xStep;
    }
    py = py + 1;
  }
}
