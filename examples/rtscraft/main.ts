import { existsSync, readFileSync, writeFileSync } from "node:fs";

import { W, H, RW, RH, RMAXW, RMAXH, WX, WY, WZ, WL, ST_SLOTS,
         KESC, K1, K2, K3, K4, K5 } from "./config";
import { App, createAppAt } from "./engine/app";
import { Scene } from "./engine/core";
import { World } from "./engine/world";
import { Input } from "./engine/input";
import { drawBillboard } from "./engine/render3d";
import { PlayerController } from "./game/player";
import { Slime } from "./game/slime";
import { castEditRay, renderFrame } from "./raycast";

// RTS-MINE — voxel em primeira pessoa sobre a superfície actual do RTS.
// A renderização de mundo e entidades continua em software; a janela e a UI
// usam a API moderna rts:egui/rts:input.

const world = new World(WX, WY, WZ, WL);
world.generate();

// Uint32 para escrever pixels e Uint8 para entregar os mesmos bytes ao backend.
const fbytes = new Uint8Array(RMAXW * RMAXH * 4);
const fbuf = new Uint32Array(fbytes.buffer);
const stbuf = new Float64Array(ST_SLOTS);
stbuf[14] = WX;
stbuf[15] = WY;
stbuf[16] = WZ;
stbuf[17] = WL;
const WBUF = world.buf;
const CBUF = world.cbuf;
let curRW = RW;
let curRH = RH;

const settingsPath = "E:/RTS-MINE/settings.txt";
if (existsSync(settingsPath)) {
  const cfgS = readFileSync(settingsPath, "utf8");
  if (cfgS === "0") { curRW = 192; curRH = 128; }
  if (cfgS === "1") { curRW = 240; curRH = 160; }
  if (cfgS === "2") { curRW = 288; curRH = 192; }
  if (cfgS === "3") { curRW = 336; curRH = 224; }
  if (cfgS === "4") { curRW = 384; curRH = 256; }
}
let lowW = Math.floor(curRW * 0.15) * 4;
let lowH = Math.floor(curRH * 0.15) * 4;

const app: App = createAppAt("RTS-MINE — Minecraft voxel no RTS", W, H, 420, 180);
const WINH = app._win;
const inputSys = new Input(WINH);
app.setMouseLock(1);

const scene = new Scene();
const player = new PlayerController(world, inputSys);
player.spawnAt(32, 32);
scene.add(player);
let sN = 0;
while (sN < 6) {
  scene.add(new Slime(world, 14 + sN * 7.3, 12 + sN * 9.1, sN));
  sN = sN + 1;
}

let curSlot = 0;
let placed = 0;
let broken = 0;
let tsec: f64 = 0.0;
let exitWhy = 0;
let parity = 0;
let forceFull = 2;
let menuOpen = 0;
let movT: f64 = 0.0;
let motionPrev = 0;

let pfUpd: f64 = 0.0;
let pfRay: f64 = 0.0;
let pfBil: f64 = 0.0;
let pfImg: f64 = 0.0;
let pfEnd: f64 = 0.0;
let pfN = 0;

while (app.running()) {
  const goOn = app.beginFrame();
  if (goOn === 0) { exitWhy = 1; break; }
  inputSys.capture();
  let dt = app.delta();
  if (dt > 120) dt = 120;
  const dts = dt / 1000.0;
  tsec = tsec + dts;

  const kEsc = inputSys.quitPressed();
  if (kEsc !== 0) {
    if (menuOpen === 0) {
      menuOpen = 1;
      app.setMouseLock(0);
    } else {
      menuOpen = 0;
      app.setMouseLock(1);
    }
  }

  let quitNow = 0;
  let movedNow = 0;
  const tp0 = performance.now();

  if (menuOpen === 0) {
    inputSys.crosshairCursor();
    scene.update(dts);

    const mdxm = inputSys.mouseDX();
    const mdym = inputSys.mouseDY();
    const axHm = inputSys.axisH();
    const axVm = inputSys.axisV();
    const mAbs = Math.abs(mdxm) + Math.abs(mdym);
    if (mAbs > 0.5 || axHm !== 0 || axVm !== 0) movedNow = 1;

    stbuf[0] = player.transform.x;
    stbuf[1] = player.transform.y;
    stbuf[2] = player.transform.z;
    stbuf[4] = player.transform.yaw;
    stbuf[5] = player.transform.pitch;
    castEditRay(WBUF, stbuf);

    if (inputSys.pressed(K1) !== 0) curSlot = 0;
    if (inputSys.pressed(K2) !== 0) curSlot = 1;
    if (inputSys.pressed(K3) !== 0) curSlot = 2;
    if (inputSys.pressed(K4) !== 0) curSlot = 3;
    if (inputSys.pressed(K5) !== 0) curSlot = 4;
    const whl = inputSys.wheel();
    if (whl > 0.5) curSlot = curSlot - 1;
    if (whl < 0 - 0.5) curSlot = curSlot + 1;
    if (curSlot < 0) curSlot = 4;
    if (curSlot > 4) curSlot = 0;

    let curBlock = 1;
    if (curSlot === 1) curBlock = 3;
    if (curSlot === 2) curBlock = 5;
    if (curSlot === 3) curBlock = 6;
    if (curSlot === 4) curBlock = 7;

    if (inputSys.clickLeft() !== 0) {
      const selX = stbuf[8];
      if (selX >= 0) {
        world.set(selX, stbuf[9], stbuf[10], 0);
        broken = broken + 1;
      }
    }
    if (inputSys.clickRight() !== 0) {
      const plX = stbuf[11];
      if (plX >= 0) {
        const plY = stbuf[12];
        const plZ = stbuf[13];
        const bx = Math.floor(player.transform.x);
        const bfy = Math.floor(player.transform.y - 1.5);
        const bey = Math.floor(player.transform.y);
        const bz = Math.floor(player.transform.z);
        let ov = 0;
        if (plX === bx && plZ === bz && (plY === bfy || plY === bey)) ov = 1;
        if (ov === 0) {
          world.set(plX, plY, plZ, curBlock);
          placed = placed + 1;
        }
      }
    }
  }

  const tp1 = performance.now();

  if (movedNow !== 0) movT = 0.15;
  else movT = movT - dts;
  let frW = curRW;
  let frH = curRH;
  let frPar = 0;
  if (movT > 0) {
    frW = lowW;
    frH = lowH;
    frPar = 2;
    motionPrev = 1;
  } else {
    if (motionPrev !== 0) { forceFull = 1; motionPrev = 0; }
    if (forceFull > 0) { frPar = 2; forceFull = forceFull - 1; }
    else {
      frPar = parity;
      if (parity === 0) parity = 1;
      else parity = 0;
    }
  }

  stbuf[18] = frW;
  stbuf[19] = frH;
  stbuf[20] = tsec;
  stbuf[21] = world.maxH;

  renderFrame(fbuf, WBUF, CBUF, stbuf, tsec, frW, frH, frPar);
  const tp2 = performance.now();

  let ei = 0;
  while (ei < scene.objects.length) {
    const o = scene.objects[ei];
    if (o.alive !== 0 && o.spriteKind !== 0) {
      drawBillboard(fbuf, WBUF, stbuf, o.transform.x, o.transform.y, o.transform.z, o.spriteSize, o.spriteKind);
    }
    ei = ei + 1;
  }
  const tp3 = performance.now();

  app.image(0, 0, W, H, fbytes, frW, frH);

  if (menuOpen === 0) {
    app.line(W / 2 - 10, H / 2, W / 2 + 10, H / 2, 2, 0xFFFFFFC8);
    app.line(W / 2, H / 2 - 10, W / 2, H / 2 + 10, 2, 0xFFFFFFC8);
    let selCol = 0x6AAA40FF;
    if (curSlot === 1) selCol = 0x7D7D7DFF;
    if (curSlot === 2) selCol = 0xDBCFA3FF;
    if (curSlot === 3) selCol = 0x665132FF;
    if (curSlot === 4) selCol = 0x3A8F30FF;
    const hbW = 5 * 52 + 8;
    const hbX = W / 2 - hbW / 2;
    const hbY = H - 66;
    app.box(hbX - 4, hbY - 4, hbW + 8, 60, 0x00000078, 0, 0, 8);
    let si = 0;
    while (si < 5) {
      let scol = 0x6AAA40FF;
      if (si === 1) scol = 0x7D7D7DFF;
      if (si === 2) scol = 0xDBCFA3FF;
      if (si === 3) scol = 0x665132FF;
      if (si === 4) scol = 0x3A8F30FF;
      let bord = 0x333333FF;
      if (si === curSlot) bord = 0xFFFFFFFF;
      app.box(hbX + si * 52 + 4, hbY, 48, 48, scol, 3, bord, 6);
      app.text(hbX + si * 52 + 8, hbY + 2, "" + (si + 1), 0xFFFFFFDC, 13);
      si = si + 1;
    }
    const hx = W - 118;
    const hy = H - 148;
    app.box(hx + 8, hy + 14, 64, 64, selCol, 2, 0x00000060, 4);
    app.box(hx + 8, hy, 64, 18, selCol, 2, 0x00000060, 4);
    app.box(hx + 8, hy, 64, 18, 0xFFFFFF30, 0, 0, 4);
    app.box(hx + 72, hy + 14, 16, 64, selCol, 2, 0x00000060, 4);
    app.box(hx + 72, hy + 14, 16, 64, 0x00000048, 0, 0, 4);
    const fpsNow = Math.floor(app.fps());
    const ipx = Math.floor(player.transform.x);
    const ipy = Math.floor(player.transform.y);
    const ipz = Math.floor(player.transform.z);
    app.text(10, 8, "RTS-MINE  |  FPS " + fpsNow + "  |  " + curRW + "x" + curRH, 0xFFFFFFE6, 15);
    app.text(10, 28, "pos " + ipx + "," + ipy + "," + ipz + "  quebrados " + broken + "  colocados " + placed, 0xE0E8F0C8, 13);
    app.text(10, H - 22, "WASD move | MOUSE olha | Espaco pula | esq quebra | dir coloca | scroll/1-5 bloco | Esc menu", 0xFFFFFFA0, 12);
  } else {
    inputSys.defaultCursor();
    app.box(0, 0, W, H, 0x00000088, 0, 0, 0);
    const mw = 420;
    const mh = 470;
    const mx0 = W / 2 - mw / 2;
    const my0 = H / 2 - mh / 2;
    app.box(mx0, my0, mw, mh, 0x0A1120F0, 2, 0x3E6FB0FF, 12);
    app.text(mx0 + 24, my0 + 18, "CONFIG", 0x66CCFFFF, 26);
    app.text(mx0 + 24, my0 + 58, "Resolucao: " + curRW + " x " + curRH + " (checkerboard)", 0xAAB8C8FF, 15);

    let bi = 0;
    let newRW = 0;
    let newRH = 0;
    let newIdx = 0;
    while (bi < 5) {
      let pw = 192;
      let ph2 = 128;
      if (bi === 1) { pw = 240; ph2 = 160; }
      if (bi === 2) { pw = 288; ph2 = 192; }
      if (bi === 3) { pw = 336; ph2 = 224; }
      if (bi === 4) { pw = 384; ph2 = 256; }
      const byy = my0 + 86 + bi * 50;
      const st = app.clickable(40 + bi, mx0 + 24, byy, mw - 48, 42);
      let fill = 0x1B2A40FF;
      if (pw === curRW) fill = 0x24405EFF;
      if (st === 1) fill = 0x2A3F5EFF;
      if (st === 2) fill = 0x152238FF;
      let bord2 = 0x3E6FB0FF;
      if (pw === curRW) bord2 = 0x77BBFFFF;
      app.box(mx0 + 24, byy, mw - 48, 42, fill, 2, bord2, 8);
      let lbl = pw + " x " + ph2;
      if (bi === 0) lbl = lbl + "  (retro, ~96 fps)";
      if (bi === 1) lbl = lbl + "  (padrao, ~60 fps)";
      if (bi === 2) lbl = lbl + "  (bonito, ~42 fps)";
      if (bi === 3) lbl = lbl + "  (~31 fps)";
      if (bi === 4) lbl = lbl + "  (nitido, ~24 fps)";
      app.text(mx0 + 40, byy + 11, lbl, 0xE8F0FFFF, 17);
      if (st === 3) { newRW = pw; newRH = ph2; newIdx = bi; }
      bi = bi + 1;
    }
    if (newRW !== 0) {
      curRW = newRW;
      curRH = newRH;
      lowW = Math.floor(newRW * 0.15) * 4;
      lowH = Math.floor(newRH * 0.15) * 4;
      forceFull = 2;
      writeFileSync(settingsPath, "" + newIdx);
    }

    const byBack = my0 + 86 + 5 * 50 + 12;
    const stB = app.clickable(46, mx0 + 24, byBack, mw - 48, 42);
    let fillB = 0x223A2EFF;
    if (stB === 1) fillB = 0x2E5040FF;
    if (stB === 2) fillB = 0x1A2A22FF;
    app.box(mx0 + 24, byBack, mw - 48, 42, fillB, 2, 0x55CC88FF, 8);
    app.text(mx0 + 40, byBack + 11, "Voltar ao jogo (Esc)", 0xFFFFFFFF, 17);
    if (stB === 3) { menuOpen = 0; app.setMouseLock(1); }

    const byQuit = byBack + 52;
    const stQ = app.clickable(47, mx0 + 24, byQuit, mw - 48, 42);
    let fillQ = 0x3A2228FF;
    if (stQ === 1) fillQ = 0x502E38FF;
    if (stQ === 2) fillQ = 0x2A1A1EFF;
    app.box(mx0 + 24, byQuit, mw - 48, 42, fillQ, 2, 0xCC6677FF, 8);
    app.text(mx0 + 40, byQuit + 11, "Sair do jogo", 0xFFFFFFFF, 17);
    if (stQ === 3) quitNow = 1;
  }

  const tp4 = performance.now();
  app.endFrame();
  const tp5 = performance.now();

  pfUpd = pfUpd + (tp1 - tp0);
  pfRay = pfRay + (tp2 - tp1);
  pfBil = pfBil + (tp3 - tp2);
  pfImg = pfImg + (tp4 - tp3);
  pfEnd = pfEnd + (tp5 - tp4);
  pfN = pfN + 1;
  if (pfN >= 60) {
    const inv = 1.0 / pfN;
    print("[prof] upd=" + (pfUpd * inv) + " ray=" + (pfRay * inv) + " bil=" + (pfBil * inv) + " img+hud=" + (pfImg * inv) + " end=" + (pfEnd * inv) + " ms | dt=" + dt + " fps=" + Math.floor(app.fps()) + " res=" + frW + "x" + frH);
    pfUpd = 0.0;
    pfRay = 0.0;
    pfBil = 0.0;
    pfImg = 0.0;
    pfEnd = 0.0;
    pfN = 0;
  }

  if (quitNow !== 0) { exitWhy = 2; app.close(); break; }
}

print("[exit] motivo=" + exitWhy + " (0=running falso, 1=beginFrame falso, 2=Sair) tsec=" + tsec);
app.close();
