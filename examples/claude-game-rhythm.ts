import egui from "rts:egui";
import input from "rts:input";
import audio from "rts:audio";
import buffer from "rts:buffer";
import math from "rts:math";

// JOGO DE RITMO — notas caem em 4 colunas; aperte D/F/J/K quando a nota cruzar a
// linha de acerto. Pontuação por precisão (Perfect/Good/Miss) + combo + som ao
// acertar. Música em loop sincronizada. Testa: timing preciso, input responsivo,
// áudio de baixa latência, render. Tudo via a stack do RTS.
//   target/release/rts.exe run examples/claude-game-rhythm.ts
// Lições de áudio aplicadas: abrir audio ANTES da janela; fila curta (~80ms) p/
// sincronia; fase = pos×delta (NÃO acumular float, o motor não persiste); device
// dita SR/CH (estéreo → escrever CH samples por frame).

// ── ÁUDIO (antes da janela) ──────────────────────────────────────────────────
const dev = audio.open_output(48000, 2, 48000);
const SR = audio.sample_rate(dev);
const CH = audio.channels(dev);
const chunkBuf = buffer.alloc(SR * CH * 4);
let aClock = 0;
const tp = 6.283185307;
// efeitos: hit (acerto) e miss
let sfxHit = 0;
let sfxMiss = 0;
const sfxLen = 4000;
let musicVol = 0.5;
let sfxVol = 0.9;

// ── DIMENSÕES e PISTA ────────────────────────────────────────────────────────
const W = 480;
const H = 640;
const cols = 4;
const colW = 100;        // largura de cada coluna
const laneX = 40;        // x inicial das pistas
const hitY = 540;        // linha de acerto (y)
const noteH = 24;        // altura da nota
const hitWindow = 60;    // janela de acerto (px) — Good
const perfWindow = 25;   // janela Perfect (px)

// teclas por coluna: D=103, F=105, J=109, K=110 (KEY_A=100 + offset)
const KEY_D = 103;
const KEY_F = 105;
const KEY_J = 109;
const KEY_K = 110;

// ── NOTAS — pré-geradas (coluna 0..3, tempo em ms de queda) ──────────────────
// padrão simples cíclico. cada nota: coluna + "y atual" (desce). Geramos um padrão
// e reaproveitamos com offset de tempo. Pra simplicidade, arrays paralelos.
const NOTE_COUNT = 64;
const noteCol = buffer.alloc(NOTE_COUNT * 4);   // coluna de cada nota
const noteTime = buffer.alloc(NOTE_COUNT * 4);  // tempo (ms) em que cruza a hitY
const noteHit = buffer.alloc(NOTE_COUNT * 4);   // 0=ativa, 1=acertada, 2=perdida
// gera um padrão ritmado (intervalo ~500ms, coluna pseudo-variada)
let ni = 0;
while (ni < NOTE_COUNT) {
  // coluna varia num padrão: 0,1,2,3,2,1,0,1,...
  let col = ni % 4;
  if ((ni / 4) % 2 === 1) col = 3 - (ni % 4); // espelha em blocos alternados
  buffer.write_i32(noteCol, ni * 4, col);
  buffer.write_i32(noteTime, ni * 4, 2000 + ni * 480); // começa em 2s, ~480ms entre notas
  buffer.write_i32(noteHit, ni * 4, 0);
  ni = ni + 1;
}

const fallSpeed = 0.45;  // px por ms (velocidade de queda)

// ── ESTADO ───────────────────────────────────────────────────────────────────
let screen = 0;          // 0=menu, 1=jogo, 2=fim
let gameTime = 0;        // tempo do jogo (ms)
let score = 0;
let combo = 0;
let maxCombo = 0;
let perfects = 0;
let goods = 0;
let misses = 0;
let flash0 = 0; let flash1 = 0; let flash2 = 0; let flash3 = 0; // brilho ao apertar
let judgeText = 0;       // 0=nada 1=Perfect 2=Good 3=Miss
let judgeTimer = 0;

const app = createAppAt("Ritmo (D F J K)", W, H, 2200, 120);

while (app.running()) {
  if (!app.beginFrame()) break;
  const dt = app.delta();
  app.fillRect(0, 0, W, H, 0x0A0C12FF);

  // ── ÁUDIO (fila curta p/ sincronia) ────────────────────────────────────────
  const targetQueue = 3800;
  let queued = audio.queued_frames(dev);
  let free = targetQueue - queued;
  if (free < 0) free = 0;
  if (free > 0) {
    // música: baixo pulsante + acorde simples (fase = pos×delta)
    const dBass = tp * 110.0 / SR;
    const dM1 = tp * 220.0 / SR; const dM2 = tp * 330.0 / SR;
    const beatLen = 24000; // ~0.5s por batida
    let j = 0;
    while (j < free) {
      const gj = aClock + j;
      const pos = gj % beatLen;
      // baixo com decay no início da batida (envelope simples por pos)
      let bassEnv = 0.0;
      if (pos < 8000) bassEnv = (8000 - pos) / 8000;
      let mus = math.sin(pos * dBass) * bassEnv * 0.5;
      mus = mus + (math.sin(pos * dM1) + math.sin(pos * dM2) * 0.6) * 0.06;
      let s = mus * musicVol;
      // efeitos
      if (sfxHit > 0)  { s = s + math.sin(gj * tp * 880.0 / SR) * 0.25 * sfxVol; sfxHit = sfxHit - 1; }
      if (sfxMiss > 0) { s = s + math.sin(gj * tp * 140.0 / SR) * 0.22 * sfxVol; sfxMiss = sfxMiss - 1; }
      let c = 0;
      while (c < CH) { buffer.write_f32(chunkBuf, (j * CH + c) * 4, s); c = c + 1; }
      j = j + 1;
    }
    audio.write(dev, chunkBuf, free * CH);
    aClock = aClock + free;
  }

  if (screen === 0) {
    // ══ MENU ════════════════════════════════════════════════════════════════════
    app.text(W / 2 - 70, 140, "RITMO", 0x66CCFFFF, 56);
    app.text(W / 2 - 150, 220, "Aperte D F J K quando a nota chegar na linha", 0x99A0AAFF, 14);
    let s1 = app.clickable(1, W / 2 - 100, 300, 200, 56);
    let c1 = 0x224030FF; if (s1 === 1) c1 = 0x2E5040FF; if (s1 === 2) c1 = 0x1A2A22FF;
    app.box(W / 2 - 100, 300, 200, 56, c1, 2, 0x55CC88FF, 12);
    app.text(W / 2 - 40, 318, "Jogar", 0xFFFFFFFF, 22);
    if (s1 === 3) {
      // reset
      gameTime = 0; score = 0; combo = 0; maxCombo = 0;
      perfects = 0; goods = 0; misses = 0;
      let r = 0;
      while (r < NOTE_COUNT) { buffer.write_i32(noteHit, r * 4, 0); r = r + 1; }
      screen = 1;
    }
    app.text(W / 2 - 70, 420, "Volume musica:", 0xC0C8D0FF, 13);
    musicVol = app.slider(W / 2 - 70, 442, 200, musicVol, 0, 1);

  } else if (screen === 1) {
    // ══ JOGO ════════════════════════════════════════════════════════════════════
    gameTime = gameTime + dt;

    // pistas (4 colunas) + brilho ao apertar
    let cc = 0;
    while (cc < cols) {
      const cx = laneX + cc * colW;
      let lc = 0x12161EFF;
      if (cc % 2 === 1) lc = 0x161A24FF;
      app.box(cx, 0, colW - 4, H, lc, 0, 0, 0);
      cc = cc + 1;
    }
    // brilho das colunas (decai)
    if (flash0 > 0) { app.box(laneX + 0 * colW, 0, colW - 4, H, 0x3366CC33 & 0xFFFFFFFF, 0, 0, 0); flash0 = flash0 - dt; }
    if (flash1 > 0) { app.box(laneX + 1 * colW, 0, colW - 4, H, 0x33CC6633 & 0xFFFFFFFF, 0, 0, 0); flash1 = flash1 - dt; }
    if (flash2 > 0) { app.box(laneX + 2 * colW, 0, colW - 4, H, 0xCC663333 & 0xFFFFFFFF, 0, 0, 0); flash2 = flash2 - dt; }
    if (flash3 > 0) { app.box(laneX + 3 * colW, 0, colW - 4, H, 0xCC33CC33 & 0xFFFFFFFF, 0, 0, 0); flash3 = flash3 - dt; }

    // linha de acerto
    app.box(laneX, hitY, cols * colW - 4, 4, 0xFFFFFFCC & 0xFFFFFFFF, 0, 0, 0);
    // teclas embaixo
    app.text(laneX + 0 * colW + 38, hitY + 30, "D", 0x6699CCFF, 24);
    app.text(laneX + 1 * colW + 38, hitY + 30, "F", 0x66CC99FF, 24);
    app.text(laneX + 2 * colW + 38, hitY + 30, "J", 0xCC9966FF, 24);
    app.text(laneX + 3 * colW + 38, hitY + 30, "K", 0xCC66CCFF, 24);

    // detecta apertos (transição via keyPressed)
    let pressed0 = input.keyPressed(app._win, KEY_D);
    let pressed1 = input.keyPressed(app._win, KEY_F);
    let pressed2 = input.keyPressed(app._win, KEY_J);
    let pressed3 = input.keyPressed(app._win, KEY_K);
    if (pressed0 !== 0) flash0 = 120;
    if (pressed1 !== 0) flash1 = 120;
    if (pressed2 !== 0) flash2 = 120;
    if (pressed3 !== 0) flash3 = 120;

    // desenha + julga notas
    let k = 0;
    while (k < NOTE_COUNT) {
      const hit = buffer.read_i32(noteHit, k * 4);
      if (hit === 0) {
        const col = buffer.read_i32(noteCol, k * 4);
        const t = buffer.read_i32(noteTime, k * 4);
        // y da nota: na hitY quando gameTime == t; antes está acima
        const ny = hitY - (t - gameTime) * fallSpeed;
        // só desenha se na tela
        if (ny > -noteH && ny < H + noteH) {
          const cx = laneX + col * colW;
          let ncolor = 0x6699CCFF;
          if (col === 1) ncolor = 0x66CC99FF;
          if (col === 2) ncolor = 0xCC9966FF;
          if (col === 3) ncolor = 0xCC66CCFF;
          app.box(cx + 6, ny - noteH / 2, colW - 16, noteH, ncolor, 0, 0, 6);
        }
        // verifica acerto: a tecla da coluna foi apertada E a nota está na janela?
        let keyHit = 0;
        if (col === 0 && pressed0 !== 0) keyHit = 1;
        if (col === 1 && pressed1 !== 0) keyHit = 1;
        if (col === 2 && pressed2 !== 0) keyHit = 1;
        if (col === 3 && pressed3 !== 0) keyHit = 1;
        if (keyHit !== 0) {
          let dist = ny - hitY;
          if (dist < 0) dist = 0 - dist; // abs
          if (dist < hitWindow) {
            // ACERTO
            buffer.write_i32(noteHit, k * 4, 1);
            sfxHit = sfxLen;
            combo = combo + 1;
            if (combo > maxCombo) maxCombo = combo;
            if (dist < perfWindow) { score = score + 100; perfects = perfects + 1; judgeText = 1; }
            else { score = score + 50; goods = goods + 1; judgeText = 2; }
            judgeTimer = 400;
          }
        }
        // MISS: passou da janela sem acertar
        if (ny > hitY + hitWindow) {
          buffer.write_i32(noteHit, k * 4, 2);
          misses = misses + 1;
          combo = 0;
          sfxMiss = sfxLen;
          judgeText = 3;
          judgeTimer = 400;
        }
      }
      k = k + 1;
    }

    // HUD: score, combo, julgamento
    app.text(20, 20, "Score: " + score, 0xFFFFFFFF, 22);
    app.text(20, 50, "Combo: " + combo, 0xAAFFCCFF, 18);
    if (judgeTimer > 0) {
      let jt = "Perfect"; let jc = 0x66FF99FF;
      if (judgeText === 2) { jt = "Good"; jc = 0xFFCC66FF; }
      if (judgeText === 3) { jt = "Miss"; jc = 0xFF6666FF; }
      app.text(W / 2 - 50, 460, jt, jc, 28);
      judgeTimer = judgeTimer - dt;
    }

    // fim: passou do tempo da última nota + 2s
    if (gameTime > 2000 + NOTE_COUNT * 480 + 2000) { screen = 2; }
    // Esc volta ao menu
    if (input.keyPressed(app._win, 2) !== 0) { screen = 0; }

  } else {
    // ══ FIM ═════════════════════════════════════════════════════════════════════
    app.text(W / 2 - 80, 120, "RESULTADO", 0x66CCFFFF, 36);
    app.text(W / 2 - 90, 200, "Score: " + score, 0xFFFFFFFF, 28);
    app.text(W / 2 - 90, 250, "Max combo: " + maxCombo, 0xAAFFCCFF, 20);
    app.text(W / 2 - 90, 290, "Perfect: " + perfects, 0x66FF99FF, 18);
    app.text(W / 2 - 90, 320, "Good: " + goods, 0xFFCC66FF, 18);
    app.text(W / 2 - 90, 350, "Miss: " + misses, 0xFF6666FF, 18);
    let sr = app.clickable(2, W / 2 - 90, 420, 180, 50);
    let rc = 0x224030FF; if (sr === 1) rc = 0x2E5040FF;
    app.box(W / 2 - 90, 420, 180, 50, rc, 2, 0x55CC88FF, 10);
    app.text(W / 2 - 40, 434, "Menu", 0xFFFFFFFF, 20);
    if (sr === 3) { screen = 0; }
  }

  app.endFrame();
}

audio.close(dev);
app.close();
