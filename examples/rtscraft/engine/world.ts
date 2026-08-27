// RTS-MINE engine — mundo voxel em TypedArrays do runtime actual.
// ids de bloco: 0 ar, 1 grama, 2 terra, 3 pedra, 4 água, 5 areia, 6 tronco,
// 7 folhas. Entidades não vivem no grid — têm renderer próprio.

export class World {
  buf: Uint8Array;
  hbuf: Uint8Array;
  cbuf: Uint8Array;
  heightCounts: Int32Array;
  maxH: number;
  sx: number;
  sy: number;
  sz: number;
  waterLevel: number;

  constructor(sx: number, sy: number, sz: number, waterLevel: number) {
    this.sx = sx;
    this.sy = sy;
    this.sz = sz;
    this.waterLevel = waterLevel;
    this.buf = new Uint8Array(sx * sy * sz);
    this.hbuf = new Uint8Array(sx * sz);
    this.cbuf = new Uint8Array((sx / 4) * (sy / 4) * (sz / 4));
    this.heightCounts = new Int32Array(sy + 1);
    this.maxH = 0;
  }

  bumpChunk(x: number, y: number, z: number, delta: number): void {
    const cxx = this.sx / 4;
    const czz = this.sz / 4;
    const idx = ((y >> 2) * czz + (z >> 2)) * cxx + (x >> 2);
    this.cbuf[idx] = this.cbuf[idx] + delta;
  }

  rebuildChunks(): void {
    const cxx = this.sx / 4;
    const cyy = this.sy / 4;
    const czz = this.sz / 4;
    let ci = 0;
    const total = cxx * cyy * czz;
    while (ci < total) {
      this.cbuf[ci] = 0;
      ci = ci + 1;
    }
    let y = 0;
    while (y < this.sy) {
      let z = 0;
      while (z < this.sz) {
        let x = 0;
        while (x < this.sx) {
          const b = this.buf[(y * this.sz + z) * this.sx + x];
          if (b !== 0) this.bumpChunk(x, y, z, 1);
          x = x + 1;
        }
        z = z + 1;
      }
      y = y + 1;
    }
  }

  updateColumn(x: number, z: number): void {
    const hidx = z * this.sx + x;
    const oldTop = this.hbuf[hidx];
    let ty = this.sy - 1;
    let top = 0;
    while (ty >= 0) {
      const b = this.buf[(ty * this.sz + z) * this.sx + x];
      if (b !== 0) { top = ty + 1; break; }
      ty = ty - 1;
    }
    if (oldTop === top) return;
    this.hbuf[hidx] = top;
    this.heightCounts[oldTop] = this.heightCounts[oldTop] - 1;
    this.heightCounts[top] = this.heightCounts[top] + 1;
    if (top > this.maxH) {
      this.maxH = top;
    } else if (oldTop === this.maxH && top < oldTop) {
      let candidate = this.maxH;
      while (candidate > 0 && this.heightCounts[candidate] === 0) candidate = candidate - 1;
      this.maxH = candidate;
    }
  }

  rebuildHeights(): void {
    let i = 0;
    while (i <= this.sy) {
      this.heightCounts[i] = 0;
      i = i + 1;
    }
    this.maxH = 0;
    let z = 0;
    while (z < this.sz) {
      let x = 0;
      while (x < this.sx) {
        const hidx = z * this.sx + x;
        let ty = this.sy - 1;
        let top = 0;
        while (ty >= 0) {
          const b = this.buf[(ty * this.sz + z) * this.sx + x];
          if (b !== 0) { top = ty + 1; break; }
          ty = ty - 1;
        }
        this.hbuf[hidx] = top;
        this.heightCounts[top] = this.heightCounts[top] + 1;
        if (top > this.maxH) this.maxH = top;
        x = x + 1;
      }
      z = z + 1;
    }
  }

  get(x: number, y: number, z: number): number {
    if (x < 0 || x >= this.sx || y < 0 || y >= this.sy || z < 0 || z >= this.sz) return 0;
    return this.buf[(y * this.sz + z) * this.sx + x];
  }

  set(x: number, y: number, z: number, id: number): void {
    if (x < 0 || x >= this.sx || y < 0 || y >= this.sy || z < 0 || z >= this.sz) return;
    const idx = (y * this.sz + z) * this.sx + x;
    const old = this.buf[idx];
    if (old === id) return;
    this.buf[idx] = id;
    if (old === 0 && id !== 0) this.bumpChunk(x, y, z, 1);
    if (old !== 0 && id === 0) this.bumpChunk(x, y, z, -1);
    this.updateColumn(x, z);
  }

  solid(x: number, y: number, z: number): number {
    if (x < 0 || x >= this.sx || y < 0 || y >= this.sy || z < 0 || z >= this.sz) return 0;
    const b = this.buf[(y * this.sz + z) * this.sx + x];
    if (b !== 0 && b !== 4) return 1;
    return 0;
  }

  topY(x: number, z: number): number {
    if (x < 0 || x >= this.sx || z < 0 || z >= this.sz) return -1;
    let y = this.hbuf[z * this.sx + x] - 1;
    while (y >= 0) {
      const b = this.buf[(y * this.sz + z) * this.sx + x];
      if (b !== 0 && b !== 4) return y;
      y = y - 1;
    }
    return -1;
  }

  generate(): void {
    const wx = this.sx;
    const wy = this.sy;
    const wz = this.sz;
    const wl = this.waterLevel;
    let gx = 0;
    while (gx < wx) {
      let gz = 0;
      while (gz < wz) {
        const s1 = Math.sin(gx * 0.23) * Math.cos(gz * 0.19);
        const s2 = Math.sin((gx * 0.5 + gz) * 0.11);
        const hn = Math.sin(gx * 12.9898 + gz * 78.233) * 43758.5453;
        const rnd = hn - Math.floor(hn);
        let hgt = Math.floor(9.0 + 5.0 * s1 + 3.0 * s2 + rnd * 2.0);
        if (hgt < 2) hgt = 2;
        if (hgt > 24) hgt = 24;
        let gy = 0;
        while (gy < wy) {
          let b = 0;
          if (gy < hgt - 3) b = 3;
          else if (gy < hgt) b = 2;
          else if (gy === hgt) {
            if (hgt <= wl) b = 5;
            else b = 1;
          } else if (gy <= wl) b = 4;
          this.buf[(gy * wz + gz) * wx + gx] = b;
          gy = gy + 1;
        }
        gz = gz + 1;
      }
      gx = gx + 1;
    }

    let tx = 3;
    while (tx < wx - 3) {
      let tz = 3;
      while (tz < wz - 3) {
        const hn2 = Math.sin(tx * 91.17 + tz * 41.921) * 33421.77;
        const r2 = hn2 - Math.floor(hn2);
        if (r2 > 0.975) {
          let ty = wy - 1;
          let top = -1;
          while (ty >= 0) {
            const bb = this.buf[(ty * wz + tz) * wx + tx];
            if (bb !== 0) { top = ty; break; }
            ty = ty - 1;
          }
          if (top > wl && top + 7 < wy) {
            const tb = this.buf[(top * wz + tz) * wx + tx];
            if (tb === 1) {
              let k = 1;
              while (k <= 4) {
                this.buf[((top + k) * wz + tz) * wx + tx] = 6;
                k = k + 1;
              }
              let ly = top + 4;
              while (ly <= top + 5) {
                let lx = tx - 1;
                while (lx <= tx + 1) {
                  let lz = tz - 1;
                  while (lz <= tz + 1) {
                    const idx = (ly * wz + lz) * wx + lx;
                    if (this.buf[idx] === 0) this.buf[idx] = 7;
                    lz = lz + 1;
                  }
                  lx = lx + 1;
                }
                ly = ly + 1;
              }
              this.buf[((top + 6) * wz + tz) * wx + tx] = 7;
            }
          }
        }
        tz = tz + 1;
      }
      tx = tx + 1;
    }

    this.rebuildHeights();
    this.rebuildChunks();
  }
}
