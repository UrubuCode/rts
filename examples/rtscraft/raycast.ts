import buffer from "rts:buffer";
import math from "rts:math";

// RTS-MINE — renderer: raycasting DDA 3D por software + raio de mira (edição).
// Cada pixel lança um raio no grid de voxels; escreve RGBA no framebuffer.
// Estado da câmera/mira no stbuf (ver config.ts). Floats em `let` são anotados
// `: f64` (limite do motor: `let x = 0.0` sem anotação é classificado int).

/// Raio curto do centro da tela: marca o bloco mirado (slots 64..80) e a célula
/// adjacente pra colocar (slots 88..104). -1 = nada ao alcance.
// NOTA: params de HANDLE são `i64` (não `number`) — ver world.ts.
export function castEditRay(wbuf: i64, stbuf: i64): void {
  const ox = buffer.read_f64(stbuf, 0);
  const oy = buffer.read_f64(stbuf, 8);
  const oz = buffer.read_f64(stbuf, 16);
  const yaw = buffer.read_f64(stbuf, 32);
  const pitch = buffer.read_f64(stbuf, 40);
  const wx = buffer.read_f64(stbuf, 112);
  const wy = buffer.read_f64(stbuf, 120);
  const wz = buffer.read_f64(stbuf, 128);

  const cp = math.cos(pitch);
  const fx = math.sin(yaw) * cp;
  const fy = math.sin(pitch);
  const fz = math.cos(yaw) * cp;

  const ddx = math.abs(1 / fx);
  const ddy = math.abs(1 / fy);
  const ddz = math.abs(1 / fz);
  const mapX0 = math.floor(ox);
  const mapY0 = math.floor(oy);
  const mapZ0 = math.floor(oz);
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
    const eb = buffer.read_u8(wbuf, (mY * wz + mZ) * wx + mX);
    if (eb !== 0 && eb !== 4) {
      selX = mX; selY = mY; selZ = mZ;
      plX = ppX; plY = ppY; plZ = ppZ;
      break;
    }
    est = est + 1;
  }

  buffer.write_f64(stbuf, 64, selX);
  buffer.write_f64(stbuf, 72, selY);
  buffer.write_f64(stbuf, 80, selZ);
  buffer.write_f64(stbuf, 88, plX);
  buffer.write_f64(stbuf, 96, plY);
  buffer.write_f64(stbuf, 104, plZ);
}

/// Renderiza um frame no framebuffer `fbuf` (RGBA8, rw×rh): raycasting DDA por
/// pixel, céu em gradiente, sombreamento por face, borda de bloco, névoa, água
/// animada por `tsec` e destaque do bloco mirado.
/// `parity`: 0/1 = CHECKERBOARD (só os pixels de (x+y+parity) par; os demais
/// mantêm o frame anterior — o fbuf persiste); 2 = frame completo (warm-up).
/// Otimizações (o "chunk culling" do raycasting):
///   - `cbuf` (contagem de blocos por chunk 4×4×4): célula dentro de chunk
///     VAZIO avança sem NENHUMA chamada extern (1 read de chunk por chunk
///     atravessado, zero reads do grid);
///   - maxH: raio subindo acima do topo global sai direto pro céu;
///   - céu empacotado 1× por LINHA (pixels de céu = 1 write, zero floor);
///   - truncamento por `|0` (sem call extern de math.floor no hot path).
export function renderFrame(fbuf: i64, wbuf: i64, cbuf: i64, stbuf: i64, tsec: f64, rw: number, rh: number, parity: number): void {
  const ox = buffer.read_f64(stbuf, 0);
  const oy = buffer.read_f64(stbuf, 8);
  const oz = buffer.read_f64(stbuf, 16);
  const yaw = buffer.read_f64(stbuf, 32);
  const pitch = buffer.read_f64(stbuf, 40);
  const selX = buffer.read_f64(stbuf, 64);
  const selY = buffer.read_f64(stbuf, 72);
  const selZ = buffer.read_f64(stbuf, 80);
  const wx = buffer.read_f64(stbuf, 112);
  const wy = buffer.read_f64(stbuf, 120);
  const wz = buffer.read_f64(stbuf, 128);
  const maxH = buffer.read_f64(stbuf, 168);

  // base da câmera
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
  const mapX0 = math.floor(ox);
  const mapY0 = math.floor(oy);
  const mapZ0 = math.floor(oz);

  const thf = 0.66; // tan(fov/2)
  const asp = rw / rh;
  const MAXT = 30.0; // alcance dos raios; a névoa quadrática esconde o corte
  const cxx = wx / 4; // dims do grid de chunks 4×4×4
  const czz = wz / 4;
  // célula/chunk iniciais do raio (a ORIGEM é a mesma pra todos os pixels —
  // calculado 1× por frame; o DDA mantém as coords de chunk INCREMENTALMENTE,
  // sem bitwise, que no motor cai no caminho genérico e custa como call)
  const chX0 = math.floor(mapX0 / 4);
  const chY0 = math.floor(mapY0 / 4);
  const chZ0 = math.floor(mapZ0 / 4);
  const inX0 = mapX0 - chX0 * 4;
  const inY0 = mapY0 - chY0 * 4;
  const inZ0 = mapZ0 - chZ0 * 4;

  let pyi = 0;
  while (pyi < rh) {
    const v = (1 - 2 * (pyi + 0.5) / rh) * thf;
    const rdxRow = fx + ux * v;
    const rdyRow = fy + uy * v;
    const rdzRow = fz + uz * v;
    // gradiente do céu desta linha — EMPACOTADO uma vez (pixels de céu só escrevem)
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

      // DDA (Amanatides & Woo) no grid de voxels
      const ddx = math.abs(1 / dx);
      const ddy = math.abs(1 / dy);
      const ddz = math.abs(1 / dz);
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
      // coords de chunk incrementais (sem bitwise): ch* = chunk, in* = 0..3
      let chX = chX0;
      let chY = chY0;
      let chZ = chZ0;
      let inX = inX0;
      let inY = inY0;
      let inZ = inZ0;
      let chunkDirty = 1;  // 1 = reler a contagem do chunk corrente
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
          // céu garantido: subindo acima do topo global do mundo
          if (stepY > 0 && mY >= maxH) break;
          // chunk-skip: 1 read de cbuf por CHUNK atravessado; célula em chunk
          // vazio avança sem ler o grid (zero extern)
          if (chunkDirty !== 0) {
            chunkDirty = 0;
            chunkCnt = buffer.read_u8(cbuf, (chY * czz + chZ) * cxx + chX);
          }
          if (chunkCnt !== 0) {
            const bb = buffer.read_u8(wbuf, (mY * wz + mZ) * wx + mX);
            if (bb !== 0) { hitB = bb; break; }
          }
        }
        stp = stp + 1;
      }

      if (hitB === 0) {
        // céu: cor pré-empacotada da linha — 1 write e nada mais
        buffer.write_i32(fbuf, (pyi * rw + pxi) * 4, skyPacked);
      } else {
        let cr: f64 = skR;
        let cg: f64 = skG;
        let cb: f64 = skB;
        // fração na face (sem floor: o bloco atingido é (mX,mY,mZ))
        const hx = ox + dx * tHit;
        const hy = oy + dy * tHit;
        const hz = oz + dz * tHit;
        let fu: f64 = 0.5;
        let fv: f64 = 0.5;
        if (side === 0) { fu = hz - mZ; fv = hy - mY; }
        else if (side === 1) { fu = hx - mX; fv = hz - mZ; }
        else { fu = hx - mX; fv = hy - mY; }

        // cor base por bloco (1 grama, 2 terra, 3 pedra, 4 água, 5 areia, 6 tronco, 7 folhas)
        let br = 125;
        let bg = 125;
        let bb2 = 125;
        if (hitB === 1) {
          if (side === 1 && stepY < 0) { br = 106; bg = 170; bb2 = 64; }       // topo
          else if (side === 1) { br = 134; bg = 96; bb2 = 67; }                // fundo
          else if (fv > 0.72) { br = 106; bg = 170; bb2 = 64; }                // faixa
          else { br = 121; bg = 85; bb2 = 58; }
        } else if (hitB === 2) { br = 134; bg = 96; bb2 = 67; }
        else if (hitB === 4) {
          const wave = math.sin(hx * 2.2 + hz * 2.2 + tsec * 2.5) * 14;
          br = 46 + wave; bg = 92 + wave; bb2 = 210;
        } else if (hitB === 5) { br = 219; bg = 207; bb2 = 163; }
        else if (hitB === 6) { br = 102; bg = 81; bb2 = 50; }
        else if (hitB === 7) { br = 58; bg = 142; bb2 = 48; }

        // sombreamento por face
        let sh: f64 = 0.8;                  // faces x
        if (side === 2) sh = 0.68;          // faces z
        if (side === 1) {
          sh = 1.0;                          // topo
          if (stepY > 0) sh = 0.52;          // fundo
        }
        // borda do bloco (o "grid" do Minecraft)
        if (fu < 0.04 || fu > 0.96 || fv < 0.04 || fv > 0.96) sh = sh * 0.78;
        // variação por bloco (hash barato, truncamento sem call)
        const kq = (mX * 3 + mY * 5 + mZ * 7) * 0.25;
        const kf = kq - (kq | 0);
        sh = sh * (0.92 + kf * 0.08);
        // bloco mirado levemente destacado
        if (mX === selX && mY === selY && mZ === selZ) sh = sh * 1.25;

        // névoa em direção ao céu
        let fog: f64 = tHit / MAXT;
        fog = fog * fog;
        if (fog > 1) fog = 1;
        cr = br * sh * (1 - fog) + skR * fog;
        cg = bg * sh * (1 - fog) + skG * fog;
        cb = bb2 * sh * (1 - fog) + skB * fog;

        if (cr > 255) cr = 255;
        if (cg > 255) cg = 255;
        if (cb > 255) cb = 255;
        if (cr < 0) cr = 0;
        if (cg < 0) cg = 0;
        if (cb < 0) cb = 0;
        const rgba = (cr | 0) | ((cg | 0) << 8) | ((cb | 0) << 16) | (0xFF << 24);
        buffer.write_i32(fbuf, (pyi * rw + pxi) * 4, rgba);
      }
      pxi = pxi + step;
    }
    pyi = pyi + 1;
  }
}
