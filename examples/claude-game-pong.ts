import egui from "rts:egui";
import input from "rts:input";
import audio from "rts:audio";
import buffer from "rts:buffer";
import math from "rts:math";

// PONG — jogo COMPLETO com telas: MENU inicial → CONFIG → JOGO. Testa a stack de
// UI num jogo real: máquina de estados (telas), navegação por mouse (clickable),
// sliders de config, render, input (teclado keyDown), App loop (delta time),
// física + colisão + IA + placar. Estado em `let` (limite do motor: sem closures).
//   target/release/rts.exe run examples/claude-game-pong.ts

const W = 700;
const H = 460;
const app = createAppAt("Pong com IA — menu/config/jogo", W, H, 2100, 360);

// ── ÁUDIO: device + bipes pré-gerados (gerar no loop seria custoso) ──────────
// rts:audio (cpal): o TS gera samples f32 num buffer e dá audio.write. NOTA: os
// membros são snake_case (open_output, não openOutput).
const SR = 44100;
const dev = audio.open_output(SR, 1, SR);
// gera um bipe de `freq` Hz por `secs` num buffer (com fade pra não estalar).
const beepN = 3000; // ~0.068s — curto pro jogo
const beepPaddle = buffer.alloc(beepN * 4); // agudo (raquete)
const beepWall = buffer.alloc(beepN * 4);   // grave (parede)
const beepScore = buffer.alloc(beepN * 4);  // ponto
let bi = 0;
while (bi < beepN) {
  const tt = bi / SR;
  let env = 0.25;
  if (bi < 200) env = env * (bi / 200);
  if (bi > beepN - 400) env = env * ((beepN - bi) / 400);
  buffer.write_f32(beepPaddle, bi * 4, math.sin(6.283185307 * 660 * tt) * env);
  buffer.write_f32(beepWall, bi * 4, math.sin(6.283185307 * 330 * tt) * env);
  buffer.write_f32(beepScore, bi * 4, math.sin(6.283185307 * 220 * tt) * env);
  bi = bi + 1;
}

// ── MÚSICA DE FUNDO: HARMONIA (acordes) em loop ──────────────────────────────
// Cada passo toca um ACORDE INTEIRO (3 notas somadas = harmonia real), com baixo +
// timbre rico (fundamental + 2 harmônicos) + envelope suave. Progressão Am–F–C–G
// (vi–IV–I–V, sequência clássica e bonita). Cada acorde = 1 buffer enviado quando
// a fila esvazia (loop).
const chordN = 24000;         // ~0.54s por acorde (respira)
const chordBuf = buffer.alloc(chordN * 4);
// 4 acordes × 3 notas (Hz) + baixo (oitava abaixo da 1ª). raiz/3ª/5ª:
// Am = A3 C4 E4 | F = F3 A4 C4 | C = C4 E4 G4 | G = G3 B3 D4
const ch0a = 220.0; const ch0b = 261.6; const ch0c = 329.6; // Am
const ch1a = 174.6; const ch1b = 220.0; const ch1c = 261.6; // F
const ch2a = 261.6; const ch2b = 329.6; const ch2c = 392.0; // C
const ch3a = 196.0; const ch3b = 246.9; const ch3c = 293.7; // G
let chordStep = 0;
let musicOn = 1;

// ── ESTADO DE TELA (máquina de estados) ──────────────────────────────────────
// 0 = MENU, 1 = CONFIG, 2 = JOGO
let screen = 0;

// ── CONFIG (ajustável na tela de config) ─────────────────────────────────────
let aiDifficulty = 0.5;   // 0..1 → vira aispeed
let ballSpeed = 0.5;      // 0..1 → vira multiplicador da bola

// ── ESTADO DO JOGO ───────────────────────────────────────────────────────────
const pw = 14;
const ph = 90;
const lx = 30;
const rx = W - 30 - pw;
let ly = H / 2 - ph / 2;
let ry = H / 2 - ph / 2;
const pspeed = 0.5;
const br = 10;
let bx = W / 2;
let by = H / 2;
let bvx = 0.32;
let bvy = 0.20;
let scoreL = 0;
let scoreR = 0;

const KEY_W = 122;
const KEY_S = 118;

// NOTA: reset feito INLINE no loop (não em função) — const top-level (W/H/ph/
// ballSpeed) não captura dentro de função no motor novo (limite #1). Por isso a
// lógica de reiniciar usa flags `doReset*` lidas no topo do loop.
let doResetGame = 0;
let doResetBall = 0; // 0 = não; 1 = dir+1; 2 = dir-1

while (app.running()) {
  if (!app.beginFrame()) break;
  const dt = app.delta();
  app.fillRect(0, 0, W, H, 0x0A0E14FF);

  // ── RESETS inline (flags setadas por outras telas; ler aqui evita função) ────
  const spd = 0.22 + ballSpeed * 0.35; // 0.22..0.57
  if (doResetGame !== 0) {
    scoreL = 0; scoreR = 0;
    ly = H / 2 - ph / 2; ry = H / 2 - ph / 2;
    bx = W / 2; by = H / 2; bvx = spd; bvy = spd * 0.6;
    doResetGame = 0;
  }
  if (doResetBall !== 0) {
    bx = W / 2; by = H / 2;
    let dir = 1; if (doResetBall === 2) dir = 0 - 1;
    bvx = spd * dir; bvy = spd * 0.6;
    doResetBall = 0;
  }

  // ── MÚSICA DE FUNDO inline: quando a fila esvazia, gera+envia o próximo ACORDE ─
  // (inline, não função — const top-level não captura em função, limite #1)
  if (musicOn !== 0 && audio.queued_frames(dev) < chordN) {
    // escolhe as 3 notas do acorde atual
    let na = ch0a; let nb = ch0b; let nc = ch0c;
    if (chordStep === 1) { na = ch1a; nb = ch1b; nc = ch1c; }
    if (chordStep === 2) { na = ch2a; nb = ch2b; nc = ch2c; }
    if (chordStep === 3) { na = ch3a; nb = ch3b; nc = ch3c; }
    const bass = na / 2; // baixo uma oitava abaixo da raiz
    // gera o acorde: soma das 3 notas (cada uma com fundamental + harmônico leve)
    // + baixo. envelope suave (fade in/out longo = pad, não pluck agressivo).
    const tp = 6.283185307;
    let k = 0;
    while (k < chordN) {
      const tt2 = k / 44100;
      // envelope: ataque + sustain + release (pad)
      let env2 = 1.0;
      if (k < 2000) env2 = k / 2000;
      if (k > chordN - 4000) env2 = (chordN - k) / 4000;
      // cada nota: fundamental + 2º harmônico (metade) + 3º (um terço) = timbre rico
      let v = 0.0;
      v = v + math.sin(tp * na * tt2) + math.sin(tp * na * 2 * tt2) * 0.4;
      v = v + math.sin(tp * nb * tt2) * 0.8;
      v = v + math.sin(tp * nc * tt2) * 0.7;
      v = v + math.sin(tp * bass * tt2) * 0.9; // baixo dá corpo
      // normaliza (soma de ~4 fontes) e aplica volume de fundo
      const sample = v * 0.035 * env2;
      buffer.write_f32(chordBuf, k * 4, sample);
      k = k + 1;
    }
    audio.write(dev, chordBuf, chordN);
    chordStep = chordStep + 1;
    if (chordStep > 3) chordStep = 0;
  }

  if (screen === 0) {
    // ══ MENU INICIAL ═══════════════════════════════════════════════════════════
    app.text(W / 2 - 90, 70, "PONG", 0x66CCFFFF, 64);
    app.text(W / 2 - 130, 150, "stack de UI do RTS em jogo", 0x808890FF, 16);

    const bw = 240;
    const bxm = W / 2 - bw / 2;
    // Jogar
    let s1 = app.clickable(10, bxm, 210, bw, 50);
    let c1 = 0x223A2EFF; if (s1 === 1) c1 = 0x2E5040FF; if (s1 === 2) c1 = 0x1A2A22FF;
    app.box(bxm, 210, bw, 50, c1, 2, 0x55CC88FF, 10);
    app.text(W / 2 - 32, 224, "Jogar", 0xFFFFFFFF, 20);
    if (s1 === 3) { doResetGame = 1; screen = 2; }
    // Config
    let s2 = app.clickable(11, bxm, 274, bw, 50);
    let c2 = 0x223040FF; if (s2 === 1) c2 = 0x2E4258FF; if (s2 === 2) c2 = 0x1A2430FF;
    app.box(bxm, 274, bw, 50, c2, 2, 0x6699CCFF, 10);
    app.text(W / 2 - 42, 288, "Config", 0xFFFFFFFF, 20);
    if (s2 === 3) { screen = 1; }
    // Sair
    let s3 = app.clickable(12, bxm, 338, bw, 50);
    let c3 = 0x3A2228FF; if (s3 === 1) c3 = 0x502E38FF; if (s3 === 2) c3 = 0x2A1A1EFF;
    app.box(bxm, 338, bw, 50, c3, 2, 0xCC6677FF, 10);
    app.text(W / 2 - 28, 352, "Sair", 0xFFFFFFFF, 20);
    if (s3 === 3) { app.close(); break; }

  } else if (screen === 1) {
    // ══ CONFIG ═════════════════════════════════════════════════════════════════
    app.text(W / 2 - 70, 50, "CONFIG", 0x66CCFFFF, 40);

    app.text(80, 140, "Dificuldade da IA:", 0xC0C8D0FF, 18);
    aiDifficulty = app.slider(80, 172, 540, aiDifficulty, 0, 1);
    let dlabel = "Media";
    if (aiDifficulty < 0.33) dlabel = "Facil";
    if (aiDifficulty > 0.66) dlabel = "Dificil";
    app.text(80, 200, dlabel, 0xAAFFCCFF, 16);

    app.text(80, 240, "Velocidade da bola:", 0xC0C8D0FF, 18);
    ballSpeed = app.slider(80, 272, 540, ballSpeed, 0, 1);

    // toggle de música
    let sm = app.clickable(21, 80, 310, 240, 40);
    let cm = 0x223040FF;
    if (musicOn !== 0) cm = 0x224030FF;
    if (sm === 1) cm = 0x2E4258FF;
    app.box(80, 310, 240, 40, cm, 2, 0x6699CCFF, 8);
    let mlabel = "Musica: OFF";
    if (musicOn !== 0) mlabel = "Musica: ON";
    app.text(100, 320, mlabel, 0xFFFFFFFF, 16);
    if (sm === 3) { if (musicOn !== 0) musicOn = 0; else musicOn = 1; }

    // Voltar
    let sb = app.clickable(20, W / 2 - 90, 372, 180, 44);
    let cb = 0x223040FF; if (sb === 1) cb = 0x2E4258FF; if (sb === 2) cb = 0x1A2430FF;
    app.box(W / 2 - 90, 360, 180, 48, cb, 2, 0x6699CCFF, 10);
    app.text(W / 2 - 38, 373, "Voltar", 0xFFFFFFFF, 18);
    if (sb === 3) { screen = 0; }

  } else {
    // ══ JOGO ═══════════════════════════════════════════════════════════════════
    // jogador
    if (input.keyDown(app._win, KEY_W) !== 0) ly = ly - pspeed * dt;
    if (input.keyDown(app._win, KEY_S) !== 0) ly = ly + pspeed * dt;
    // IA (velocidade pela config: 0.20..0.46)
    const aispeed = 0.20 + aiDifficulty * 0.26;
    const rcenter = ry + ph / 2;
    let aitarget = H / 2;
    if (bvx > 0) aitarget = by;
    const diff = aitarget - rcenter;
    if (diff > 10) ry = ry + aispeed * dt;
    if (diff < 0 - 10) ry = ry - aispeed * dt;
    // clamp
    if (ly < 0) ly = 0;
    if (ly > H - ph) ly = H - ph;
    if (ry < 0) ry = 0;
    if (ry > H - ph) ry = H - ph;

    // física
    bx = bx + bvx * dt;
    by = by + bvy * dt;
    if (by < br) { by = br; bvy = 0 - bvy; audio.write(dev, beepWall, beepN); }
    if (by > H - br) { by = H - br; bvy = 0 - bvy; audio.write(dev, beepWall, beepN); }
    // colisão esquerda (bipe agudo)
    if (bx - br < lx + pw && bx > lx && by > ly && by < ly + ph && bvx < 0) {
      bx = lx + pw + br; bvx = 0 - bvx;
      bvy = bvy + ((by - (ly + ph / 2)) / (ph / 2)) * 0.15;
      audio.write(dev, beepPaddle, beepN);
    }
    // colisão direita (bipe agudo)
    if (bx + br > rx && bx < rx + pw && by > ry && by < ry + ph && bvx > 0) {
      bx = rx - br; bvx = 0 - bvx;
      bvy = bvy + ((by - (ry + ph / 2)) / (ph / 2)) * 0.15;
      audio.write(dev, beepPaddle, beepN);
    }
    // pontos (bipe grave)
    if (bx < 0) { scoreR = scoreR + 1; doResetBall = 1; audio.write(dev, beepScore, beepN); }
    if (bx > W) { scoreL = scoreL + 1; doResetBall = 2; audio.write(dev, beepScore, beepN); }

    // render do jogo
    let dy = 0;
    while (dy < H) { app.box(W / 2 - 2, dy, 4, 16, 0x223040FF, 0, 0, 0); dy = dy + 28; }
    app.text(W / 2 - 90, 24, "" + scoreL, 0x66CCFFFF, 48);
    app.text(W / 2 + 60, 24, "" + scoreR, 0xFF99AAFF, 48);
    app.box(lx, ly, pw, ph, 0x66CCFFFF, 0, 0, 4);
    app.box(rx, ry, pw, ph, 0xFF99AAFF, 0, 0, 4);
    app.box(bx - br, by - br, br * 2, br * 2, 0xFFFFFFFF, 0, 0, br);
    app.text(20, H - 24, "VOCE (W/S)", 0x66CCFFFF, 14);
    app.text(W - 60, H - 24, "CPU", 0xFF99AAFF, 14);

    // botao ESC->menu (canto): pressionar Esc volta
    if (input.keyPressed(app._win, 2) !== 0) { screen = 0; }
    app.text(W / 2 - 60, H - 24, "Esc = menu", 0x556066FF, 13);
  }

  app.endFrame();
}

app.close();
