import { WX, WY, WZ, WL, ST_SLOTS, RMAXW, RMAXH } from "./config";
import { World } from "./engine/world";
import { renderFrame } from "./raycast";

// Benchmark headless do renderer real do jogo, com TypedArrays e sem janela.

const world = new World(WX, WY, WZ, WL);
world.generate();

const fbytes = new Uint8Array(RMAXW * RMAXH * 4);
const fbuf = new Uint32Array(fbytes.buffer);
const stbuf = new Float64Array(ST_SLOTS);
stbuf[14] = WX;
stbuf[15] = WY;
stbuf[16] = WZ;
stbuf[17] = WL;
stbuf[0] = 32.5;
stbuf[1] = 16.0;
stbuf[2] = 32.5;
stbuf[5] = 0 - 0.15;
stbuf[21] = world.maxH;

const WBUF = world.buf;
const CBUF = world.cbuf;
const FRAMES = 40;

let pi = 0;
while (pi < 5) {
  let rw = 192;
  let rh = 128;
  if (pi === 1) { rw = 240; rh = 160; }
  if (pi === 2) { rw = 288; rh = 192; }
  if (pi === 3) { rw = 336; rh = 224; }
  if (pi === 4) { rw = 384; rh = 256; }
  stbuf[18] = rw;
  stbuf[19] = rh;

  const t0 = performance.now();
  let fr = 0;
  let yaw: f64 = 0.7;
  while (fr < FRAMES) {
    yaw = yaw + 0.03;
    stbuf[4] = yaw;
    const par = fr % 2;
    renderFrame(fbuf, WBUF, CBUF, stbuf, fr * 0.016, rw, rh, par);
    fr = fr + 1;
  }
  const t1 = performance.now();
  const ms = (t1 - t0) / FRAMES;
  const fps = Math.floor(1000 / ms);
  print(rw + " x " + rh + " (checkerboard): " + ms + " ms/frame  -> teto ~" + fps + " FPS");
  pi = pi + 1;
}
