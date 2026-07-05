import buffer from "rts:buffer";
import math from "rts:math";
import time from "rts:time";
import io from "rts:io";

import { WX, WY, WZ, WL, ST_SLOTS, RMAXW, RMAXH } from "./config";
import { World } from "./engine/world";
import { renderFrame } from "./raycast";

// bench headless do renderer REAL (importa o raycast.ts do jogo): mede
// ms/frame em cada preset de resolução, com checkerboard (como no jogo).
//   E:\rts\target\release\rts.exe run E:\RTS-MINE\bench.ts

const world = new World(WX, WY, WZ, WL);
world.generate();

const fbuf = buffer.alloc(RMAXW * RMAXH * 4);
const stbuf = buffer.alloc(ST_SLOTS * 8);
buffer.write_f64(stbuf, 112, WX);
buffer.write_f64(stbuf, 120, WY);
buffer.write_f64(stbuf, 128, WZ);
buffer.write_f64(stbuf, 136, WL);
// câmera fixa num ponto alto olhando o vale (cenário típico)
buffer.write_f64(stbuf, 0, 32.5);
buffer.write_f64(stbuf, 8, 16.0);
buffer.write_f64(stbuf, 16, 32.5);
buffer.write_f64(stbuf, 40, 0 - 0.15);
buffer.write_f64(stbuf, 168, world.maxH);

const WBUF = world.buf;
const CBUF = world.cbuf;
const FRAMES = 40;

let pi = 0;
while (pi < 5) {
  let rw = 192; let rh = 128;
  if (pi === 1) { rw = 240; rh = 160; }
  if (pi === 2) { rw = 288; rh = 192; }
  if (pi === 3) { rw = 336; rh = 224; }
  if (pi === 4) { rw = 384; rh = 256; }
  buffer.write_f64(stbuf, 144, rw);
  buffer.write_f64(stbuf, 152, rh);

  const t0 = time.now_ms();
  let fr = 0;
  let yaw: f64 = 0.7;
  while (fr < FRAMES) {
    yaw = yaw + 0.03;
    buffer.write_f64(stbuf, 32, yaw);
    const par = fr % 2; // checkerboard alternando, como no jogo
    renderFrame(fbuf, WBUF, CBUF, stbuf, fr * 0.016, rw, rh, par);
    fr = fr + 1;
  }
  const t1 = time.now_ms();
  const ms = (t1 - t0) / FRAMES;
  const fps = math.floor(1000 / ms);
  io.print(rw + " x " + rh + " (checkerboard): " + ms + " ms/frame  → teto ~" + fps + " FPS");
  pi = pi + 1;
}

buffer.free(fbuf);
buffer.free(stbuf);
