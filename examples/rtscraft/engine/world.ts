import buffer from "rts:buffer";
import math from "rts:math";

// RTS-MINE engine — o mundo voxel como CLASSE (estilo Unity: um "singleton" de
// cena injetado nos GameObjects). O grid vive num buffer u8; o raycaster lê o
// handle cru (`world.buf`) por performance; a lógica de jogo usa os métodos.
//
// ids de bloco: 0 ar, 1 grama, 2 terra, 3 pedra, 4 água, 5 areia, 6 tronco,
// 7 folhas. (Entidades NÃO vivem no grid — têm renderer próprio, render3d.ts.)

export class World {
  buf: i64;
  /// heightmap de CULLING do renderer: topo+1 de qualquer bloco não-ar por
  /// coluna (u8, índice z*sx+x). Células acima disso são céu garantido — o
  /// raycaster pula a leitura do grid (o "chunk culling" do raycasting).
  hbuf: i64;
  /// topo global (nenhum bloco acima disso em lugar nenhum): raio subindo
  /// acima de maxH sai direto pro céu.
  maxH: number;
  /// CHUNKS 4×4×4: contagem de blocos não-ar por chunk (u8). O raycaster pula
  /// células em chunk vazio SEM ler o grid — o "chunk culling" do raycasting.
  cbuf: i64;
  sx: number;
  sy: number;
  sz: number;
  waterLevel: number;

  constructor(sx: number, sy: number, sz: number, waterLevel: number) {
    this.sx = sx;
    this.sy = sy;
    this.sz = sz;
    this.waterLevel = waterLevel;
    this.buf = buffer.alloc(sx * sy * sz);
    this.hbuf = buffer.alloc(sx * sz);
    this.cbuf = buffer.alloc((sx / 4) * (sy / 4) * (sz / 4));
    this.maxH = 0;
  }

  /// Ajusta a contagem do chunk 4×4×4 que contém (x,y,z) em `delta` (+1/-1).
  bumpChunk(x: number, y: number, z: number, delta: number): void {
    const cxx = this.sx / 4;
    const czz = this.sz / 4;
    const idx = ((y >> 2) * czz + (z >> 2)) * cxx + (x >> 2);
    const cur = buffer.read_u8(this.cbuf, idx);
    buffer.write_u8(this.cbuf, idx, cur + delta);
  }

  /// Reconstrói as contagens de chunk do zero (chamar após generate()).
  rebuildChunks(): void {
    const cxx = this.sx / 4;
    const cyy = this.sy / 4;
    const czz = this.sz / 4;
    // zera
    let ci = 0;
    const total = cxx * cyy * czz;
    while (ci < total) {
      buffer.write_u8(this.cbuf, ci, 0);
      ci = ci + 1;
    }
    // conta
    let y = 0;
    while (y < this.sy) {
      let z = 0;
      while (z < this.sz) {
        let x = 0;
        while (x < this.sx) {
          const b = buffer.read_u8(this.buf, (y * this.sz + z) * this.sx + x);
          if (b !== 0) this.bumpChunk(x, y, z, 1);
          x = x + 1;
        }
        z = z + 1;
      }
      y = y + 1;
    }
  }

  /// Recalcula o topo (não-ar) de UMA coluna após um edit. maxH só cresce
  /// (conservador — remoção não abaixa o teto global; segue correto).
  updateColumn(x: number, z: number): void {
    let ty = this.sy - 1;
    let top = 0;
    while (ty >= 0) {
      const b = buffer.read_u8(this.buf, (ty * this.sz + z) * this.sx + x);
      if (b !== 0) { top = ty + 1; break; }
      ty = ty - 1;
    }
    buffer.write_u8(this.hbuf, z * this.sx + x, top);
    if (top > this.maxH) this.maxH = top;
  }

  /// Reconstrói o heightmap inteiro + maxH (chamar após generate()).
  rebuildHeights(): void {
    let z = 0;
    while (z < this.sz) {
      let x = 0;
      while (x < this.sx) {
        this.updateColumn(x, z);
        x = x + 1;
      }
      z = z + 1;
    }
  }

  /// id do bloco em (x,y,z); fora dos limites = 0 (ar).
  get(x: number, y: number, z: number): number {
    if (x < 0 || x >= this.sx || y < 0 || y >= this.sy || z < 0 || z >= this.sz) return 0;
    return buffer.read_u8(this.buf, (y * this.sz + z) * this.sx + x);
  }

  /// escreve o bloco (ignora fora dos limites) e atualiza o culling
  /// (coluna do heightmap + contagem do chunk).
  set(x: number, y: number, z: number, id: number): void {
    if (x < 0 || x >= this.sx || y < 0 || y >= this.sy || z < 0 || z >= this.sz) return;
    const old = buffer.read_u8(this.buf, (y * this.sz + z) * this.sx + x);
    buffer.write_u8(this.buf, (y * this.sz + z) * this.sx + x, id);
    if (old === 0 && id !== 0) this.bumpChunk(x, y, z, 1);
    if (old !== 0 && id === 0) this.bumpChunk(x, y, z, 0 - 1);
    this.updateColumn(x, z);
  }

  /// 1 se o bloco é sólido pra física (água não é). Lê o grid direto
  /// (sem passar por get() — dispatch aninhado custa no hot path da física).
  solid(x: number, y: number, z: number): number {
    if (x < 0 || x >= this.sx || y < 0 || y >= this.sy || z < 0 || z >= this.sz) return 0;
    const b = buffer.read_u8(this.buf, (y * this.sz + z) * this.sx + x);
    if (b !== 0 && b !== 4) return 1;
    return 0;
  }

  /// y do bloco sólido mais alto em (x,z); -1 se nenhum. Leitura direta e
  /// começa do teto REAL do mundo (maxH), não do topo do grid.
  topY(x: number, z: number): number {
    if (x < 0 || x >= this.sx || z < 0 || z >= this.sz) return -1;
    let y = this.maxH;
    if (y > this.sy - 1) y = this.sy - 1;
    while (y >= 0) {
      const b = buffer.read_u8(this.buf, (y * this.sz + z) * this.sx + x);
      if (b !== 0 && b !== 4) return y;
      y = y - 1;
    }
    return -1;
  }

  /// Gera o terreno: heightmap (senos + hash), camadas, água, areia e árvores.
  generate(): void {
    const wb = this.buf;
    const wx = this.sx;
    const wy = this.sy;
    const wz = this.sz;
    const wl = this.waterLevel;

    let gx = 0;
    while (gx < wx) {
      let gz = 0;
      while (gz < wz) {
        const s1 = math.sin(gx * 0.23) * math.cos(gz * 0.19);
        const s2 = math.sin((gx * 0.5 + gz) * 0.11);
        const hn = math.sin(gx * 12.9898 + gz * 78.233) * 43758.5453;
        const rnd = hn - math.floor(hn);
        let hgt = math.floor(9.0 + 5.0 * s1 + 3.0 * s2 + rnd * 2.0);
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
          buffer.write_u8(wb, (gy * wz + gz) * wx + gx, b);
          gy = gy + 1;
        }
        gz = gz + 1;
      }
      gx = gx + 1;
    }

    // árvores determinísticas
    let tx = 3;
    while (tx < wx - 3) {
      let tz = 3;
      while (tz < wz - 3) {
        const hn2 = math.sin(tx * 91.17 + tz * 41.921) * 33421.77;
        const r2 = hn2 - math.floor(hn2);
        if (r2 > 0.975) {
          let ty = wy - 1;
          let top = -1;
          while (ty >= 0) {
            const bb = buffer.read_u8(wb, (ty * wz + tz) * wx + tx);
            if (bb !== 0) { top = ty; break; }
            ty = ty - 1;
          }
          if (top > wl && top + 7 < wy) {
            const tb = buffer.read_u8(wb, (top * wz + tz) * wx + tx);
            if (tb === 1) {
              let k = 1;
              while (k <= 4) {
                buffer.write_u8(wb, ((top + k) * wz + tz) * wx + tx, 6);
                k = k + 1;
              }
              let ly = top + 4;
              while (ly <= top + 5) {
                let lx = tx - 1;
                while (lx <= tx + 1) {
                  let lz = tz - 1;
                  while (lz <= tz + 1) {
                    const cur = buffer.read_u8(wb, (ly * wz + lz) * wx + lx);
                    if (cur === 0) buffer.write_u8(wb, (ly * wz + lz) * wx + lx, 7);
                    lz = lz + 1;
                  }
                  lx = lx + 1;
                }
                ly = ly + 1;
              }
              buffer.write_u8(wb, ((top + 6) * wz + tz) * wx + tx, 7);
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
