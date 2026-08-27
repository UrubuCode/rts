// RTS-MINE — renderer de voxels por software + raio de edição.
// O framebuffer é uma view Uint32 para escrita de pixels e a view Uint8 partilhada
// é entregue ao backend egui. O mundo/stbuf são TypedArrays do runtime actual.

export function castEditRay(wbuf: Uint8Array, stbuf: Float64Array): void {
  const ox = stbuf[0];
  const oy = stbuf[1];
  const oz = stbuf[2];
  const yaw = stbuf[4];
  const pitch = stbuf[5];
  const wx = stbuf[14];
  const wy = stbuf[15];
  const wz = stbuf[16];

  const cp = Math.cos(pitch);
  const fx = Math.sin(yaw) * cp;
  const fy = Math.sin(pitch);
  const fz = Math.cos(yaw) * cp;
  const ddx = Math.abs(1 / fx);
  const ddy = Math.abs(1 / fy);
  const ddz = Math.abs(1 / fz);
  const mapX0 = Math.floor(ox);
  const mapY0 = Math.floor(oy);
  const mapZ0 = Math.floor(oz);
  let stepX = 1;
  let tMaxX: f64 = (mapX0 + 1 - ox) * ddx;
  if (fx < 0) { stepX = -1; tMaxX = (ox - mapX0) * ddx; }
  let stepY = 1;
  let tMaxY: f64 = (mapY0 + 1 - oy) * ddy;
  if (fy < 0) { stepY = -1; tMaxY = (oy - mapY0) * ddy; }
  let stepZ = 1;
  let tMaxZ: f64 = (mapZ0 + 1 - oz) * ddz;
  if (fz < 0) { stepZ = -1; tMaxZ = (oz - mapZ0) * ddz; }

  let mX = mapX0;
  let mY = mapY0;
  let mZ = mapZ0;
  let selX = -1;
  let selY = -1;
  let selZ = -1;
  let plX = -1;
  let plY = -1;
  let plZ = -1;
  let est = 0;
  while (est < 16) {
    const ppX = mX;
    const ppY = mY;
    const ppZ = mZ;
    let et: f64 = 0.5;
    if (tMaxX < tMaxY && tMaxX < tMaxZ) { et = tMaxX; tMaxX = tMaxX + ddx; mX = mX + stepX; }
    else if (tMaxY < tMaxZ) { et = tMaxY; tMaxY = tMaxY + ddy; mY = mY + stepY; }
    else { et = tMaxZ; tMaxZ = tMaxZ + ddz; mZ = mZ + stepZ; }
    if (et > 6.0) break;
    if (mX < 0 || mX >= wx || mZ < 0 || mZ >= wz || mY < 0 || mY >= wy) break;
    const eb = wbuf[(mY * wz + mZ) * wx + mX];
    if (eb !== 0 && eb !== 4) {
      selX = mX; selY = mY; selZ = mZ;
      plX = ppX; plY = ppY; plZ = ppZ;
      break;
    }
    est = est + 1;
  }

  stbuf[8] = selX;
  stbuf[9] = selY;
  stbuf[10] = selZ;
  stbuf[11] = plX;
  stbuf[12] = plY;
  stbuf[13] = plZ;
}

export function renderFrame(fbuf: Uint32Array, wbuf: Uint8Array, cbuf: Uint8Array, stbuf: Float64Array, tsec: f64, rw: number, rh: number, parity: number): void {
  const ox = stbuf[0];
  const oy = stbuf[1];
  const oz = stbuf[2];
  const yaw = stbuf[4];
  const pitch = stbuf[5];
  const selX = stbuf[8];
  const selY = stbuf[9];
  const selZ = stbuf[10];
  const wx = stbuf[14];
  const wy = stbuf[15];
  const wz = stbuf[16];
  const maxH = stbuf[21];

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
  const mapX0 = Math.floor(ox);
  const mapY0 = Math.floor(oy);
  const mapZ0 = Math.floor(oz);

  const thf = 0.66;
  const asp = rw / rh;
  const MAXT = 30.0;
  const cxx = wx / 4;
  const czz = wz / 4;
  const chX0 = Math.floor(mapX0 / 4);
  const chY0 = Math.floor(mapY0 / 4);
  const chZ0 = Math.floor(mapZ0 / 4);
  const inX0 = mapX0 - chX0 * 4;
  const inY0 = mapY0 - chY0 * 4;
  const inZ0 = mapZ0 - chZ0 * 4;

  let pyi = 0;
  while (pyi < rh) {
    const v = (1 - 2 * (pyi + 0.5) / rh) * thf;
    const rdxRow = fx + ux * v;
    const rdyRow = fy + uy * v;
    const rdzRow = fz + uz * v;
    let vv: f64 = (v + 0.66) / 1.32;
    if (vv < 0) vv = 0;
    if (vv > 1) vv = 1;
    const skR = 168 - 96 * vv;
    const skG = 208 - 76 * vv;
    const skB = 238 - 16 * vv;
    const skyPacked = (skR | 0) | ((skG | 0) << 8) | ((skB | 0) << 16) | (0xFF << 24);
    let step = 2;
    let pxi = (pyi + parity) & 1;
    if (parity === 2) { step = 1; pxi = 0; }
    while (pxi < rw) {
      const u = (2 * (pxi + 0.5) / rw - 1) * thf * asp;
      const dx = rdxRow + rxx * u;
      const dy = rdyRow;
      const dz = rdzRow + rxz * u;
      const ddx = Math.abs(1 / dx);
      const ddy = Math.abs(1 / dy);
      const ddz = Math.abs(1 / dz);
      let stepX = 1;
      let tMaxX: f64 = (mapX0 + 1 - ox) * ddx;
      if (dx < 0) { stepX = -1; tMaxX = (ox - mapX0) * ddx; }
      let stepY = 1;
      let tMaxY: f64 = (mapY0 + 1 - oy) * ddy;
      if (dy < 0) { stepY = -1; tMaxY = (oy - mapY0) * ddy; }
      let stepZ = 1;
      let tMaxZ: f64 = (mapZ0 + 1 - oz) * ddz;
      if (dz < 0) { stepZ = -1; tMaxZ = (oz - mapZ0) * ddz; }
      let mX = mapX0;
      let mY = mapY0;
      let mZ = mapZ0;
      let chX = chX0;
      let chY = chY0;
      let chZ = chZ0;
      let inX = inX0;
      let inY = inY0;
      let inZ = inZ0;
      let chunkDirty = 1;
      let chunkCnt = 1;
      let hitB = 0;
      let side = 0;
      let tHit: f64 = 0.5;
      let stp = 0;
      while (stp < 96) {
        if (tMaxX < tMaxY && tMaxX < tMaxZ) {
          tHit = tMaxX; tMaxX = tMaxX + ddx; mX = mX + stepX; side = 0;
          inX = inX + stepX;
          if (inX > 3) { inX = 0; chX = chX + 1; chunkDirty = 1; }
          else if (inX < 0) { inX = 3; chX = chX - 1; chunkDirty = 1; }
        } else if (tMaxY < tMaxZ) {
          tHit = tMaxY; tMaxY = tMaxY + ddy; mY = mY + stepY; side = 1;
          inY = inY + stepY;
          if (inY > 3) { inY = 0; chY = chY + 1; chunkDirty = 1; }
          else if (inY < 0) { inY = 3; chY = chY - 1; chunkDirty = 1; }
        } else {
          tHit = tMaxZ; tMaxZ = tMaxZ + ddz; mZ = mZ + stepZ; side = 2;
          inZ = inZ + stepZ;
          if (inZ > 3) { inZ = 0; chZ = chZ + 1; chunkDirty = 1; }
          else if (inZ < 0) { inZ = 3; chZ = chZ - 1; chunkDirty = 1; }
        }
        if (tHit > MAXT) break;
        if (mX < 0 || mX >= wx || mZ < 0 || mZ >= wz) break;
        if (mY < 0) break;
        if (mY >= wy) {
          if (stepY > 0) break;
        } else {
          if (stepY > 0 && mY >= maxH) break;
          if (chunkDirty !== 0) {
            chunkDirty = 0;
            if (chX < 0 || chX >= cxx || chY < 0 || chY >= wy / 4 || chZ < 0 || chZ >= czz) chunkCnt = 0;
            else chunkCnt = cbuf[(chY * czz + chZ) * cxx + chX];
          }
          if (chunkCnt !== 0) {
            const block = wbuf[(mY * wz + mZ) * wx + mX];
            if (block !== 0) { hitB = block; break; }
          }
        }
        stp = stp + 1;
      }

      const pix = pyi * rw + pxi;
      if (hitB === 0) {
        fbuf[pix] = skyPacked;
      } else {
        let cr: f64 = skR;
        let cg: f64 = skG;
        let cb: f64 = skB;
        const hx = ox + dx * tHit;
        const hy = oy + dy * tHit;
        const hz = oz + dz * tHit;
        let fu: f64 = 0.5;
        let fv: f64 = 0.5;
        if (side === 0) { fu = hz - mZ; fv = hy - mY; }
        else if (side === 1) { fu = hx - mX; fv = hz - mZ; }
        else { fu = hx - mX; fv = hy - mY; }
        let br = 125;
        let bg = 125;
        let bb = 125;
        if (hitB === 1) {
          if (side === 1 && stepY < 0) { br = 106; bg = 170; bb = 64; }
          else if (side === 1) { br = 134; bg = 96; bb = 67; }
          else if (fv > 0.72) { br = 106; bg = 170; bb = 64; }
          else { br = 121; bg = 85; bb = 58; }
        } else if (hitB === 2) { br = 134; bg = 96; bb = 67; }
        else if (hitB === 4) {
          const wave = Math.sin(hx * 2.2 + hz * 2.2 + tsec * 2.5) * 14;
          br = 46 + wave; bg = 92 + wave; bb = 210;
        } else if (hitB === 5) { br = 219; bg = 207; bb = 163; }
        else if (hitB === 6) { br = 102; bg = 81; bb = 50; }
        else if (hitB === 7) { br = 58; bg = 142; bb = 48; }
        let sh: f64 = 0.8;
        if (side === 2) sh = 0.68;
        if (side === 1) {
          sh = 1.0;
          if (stepY > 0) sh = 0.52;
        }
        if (fu < 0.04 || fu > 0.96 || fv < 0.04 || fv > 0.96) sh = sh * 0.78;
        const kq = (mX * 3 + mY * 5 + mZ * 7) * 0.25;
        const kf = kq - (kq | 0);
        sh = sh * (0.92 + kf * 0.08);
        if (mX === selX && mY === selY && mZ === selZ) sh = sh * 1.25;
        let fog: f64 = tHit / MAXT;
        fog = fog * fog;
        if (fog > 1) fog = 1;
        cr = br * sh * (1 - fog) + skR * fog;
        cg = bg * sh * (1 - fog) + skG * fog;
        cb = bb * sh * (1 - fog) + skB * fog;
        if (cr > 255) cr = 255;
        if (cg > 255) cg = 255;
        if (cb > 255) cb = 255;
        if (cr < 0) cr = 0;
        if (cg < 0) cg = 0;
        if (cb < 0) cb = 0;
        fbuf[pix] = (cr | 0) | ((cg | 0) << 8) | ((cb | 0) << 16) | (0xFF << 24);
      }
      pxi = pxi + step;
    }
    pyi = pyi + 1;
  }
}
