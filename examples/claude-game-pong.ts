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

// ── ÁUDIO como STREAM SÍNCRONO (o game loop É o gerador de áudio) ─────────────
// Resposta a "o loop de áudio não pode ser o game loop?": SIM. A cada frame o game
// loop preenche SÓ o espaço livre do ring (available_frames) com os samples daquele
// intervalo — áudio anda JUNTO com o vídeo. MIXA (soma) música + efeitos no mesmo
// sample. O callback cpal (thread RT) consome o ring; o jogo o alimenta.
// IMPORTANTE: abrir o áudio ANTES da janela (createApp) — o wgpu/egui pode
// interferir na inicialização do device se a janela vier primeiro. Lê SR/CH REAIS
// (o device dita: ex. 48000Hz/2 canais); gera por FRAME escrevendo CH samples
// intercalados (L,R) — gerar mono num device estéreo silencia/desalinha.
const dev = audio.open_output(48000, 2, 48000);
const SR = audio.sample_rate(dev);
const CH = audio.channels(dev);
const chunkBuf = buffer.alloc(SR * CH * 4); // 1s de frames (todos os canais)

// RELÓGIO de áudio: quantos samples já geramos (conta acordes; NÃO usado pra fase).
let aClock = 0;

// MÚSICA: progressão Am–F–C–G. FASE ACUMULADA por oscilador (incrementa
// 2π·freq/SR por sample, mantida em [0,2π)) — onda contínua que NUNCA degrada
// (fase = gt/SR vira número gigante e math.sin perde precisão → silêncio: era o bug).
const chordLen = 26000;       // ~0.59s por acorde
let musicOn = 1;
let phA = 0.0;  // fase nota 1 (raiz)
let phB = 0.0;  // fase nota 2 (3ª)
let phC = 0.0;  // fase nota 3 (5ª)
let phBass = 0.0; // fase baixo
let phSfx = 0.0;  // fase do efeito ativo

// EFEITOS: cada um tem um "tempo restante" (em samples). >0 = tocando. O stream
// soma a senoide do efeito enquanto durar. (disparados nas colisões.)
let sfxPaddle = 0;  // samples restantes do bipe da raquete
let sfxWall = 0;
let sfxScore = 0;
let sfxHover = 0;   // tique de hover nos botões
const sfxLen = 3000; // duração de um efeito (~0.07s)
const hoverLen = 1500; // tique de hover mais curto
let lastHover = -1;  // qual botão estava sob o mouse no frame anterior (transição)

// ── ESTADO DE TELA (máquina de estados) ──────────────────────────────────────
// 0 = MENU, 1 = CONFIG, 2 = JOGO
let screen = 0;

// ── CONFIG (ajustável na tela de config) ─────────────────────────────────────
let aiDifficulty = 0.5;   // 0..1 → vira aispeed
let ballSpeed = 0.5;      // 0..1 → vira multiplicador da bola
let musicVol = 0.6;       // 0..1 → volume da música
let sfxVol = 0.8;         // 0..1 → volume dos efeitos

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

// janela criada por ÚLTIMO (após TODAS as declarações) — teste mostrou que criar
// a janela no MEIO das declarações de áudio silenciava o som.
const app = createAppAt("Pong com IA — menu/config/jogo", W, H, 2100, 360);
// TECLADO via MÉTODO do app (`app.keyDown(k)`): dentro do método `this._win`
// resolve o handle certo. Passar `app._win` (campo) DIRETO como argumento de
// `input.keyDown(app._win, k)` no top-level lê 0 no AOT (diverge do JIT) — limite
// do motor: acesso a campo de instância como arg inline fora de um método.

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

  // ── STREAM DE ÁUDIO SÍNCRONO: o game loop gera os samples DESTE frame ─────────
  // Preenche o espaço livre do ring (available_frames) com samples MIXADOS: música
  // (acorde da progressão, pela posição de aClock) + efeitos ativos (somados). Como
  // só geramos o que cabe, áudio e vídeo andam juntos.
  // ÁUDIO: música de acordes (Am–F–C–G) + efeitos. Mantém o corpo SIMPLES que
  // funciona (fase = pos×delta, sem acumular float). Os sfx são somados de forma
  // enxuta (sem divisão no hot loop — o decay usa o próprio contador).
  // LATÊNCIA BAIXA: manter a fila CURTA pra os efeitos tocarem quase na hora.
  // Encher 1s (48000) deixava ~1s de atraso (som fora de sincronia). Mantemos só
  // ~3 frames de vídeo de áudio à frente (~2400 samples = ~50ms): se a fila já tem
  // o bastante, não gera nada; senão, completa só o necessário.
  const targetQueue = 3800; // ~80ms de buffer (baixa latência sem engasgar)
  let queued = audio.queued_frames(dev);
  let free = targetQueue - queued;
  if (free < 0) free = 0;
  if (free > 0) {
    const tp = 6.283185307;
    // acorde atual pela progressão (step pela posição global / chordLen)
    const step = math.floor(aClock / chordLen) % 4;
    let na = 220.0; let nb = 261.6; let nc = 329.6;       // Am
    if (step === 1) { na = 174.6; nb = 220.0; nc = 261.6; } // F
    if (step === 2) { na = 261.6; nb = 329.6; nc = 392.0; } // C
    if (step === 3) { na = 196.0; nb = 246.9; nc = 293.7; } // G
    const dA = tp * na / SR; const dB = tp * nb / SR; const dC = tp * nc / SR;
    // deltas dos efeitos (fase por sample) — pré-calculados fora do loop
    const dHover = tp * 880.0 / SR; const dWall = tp * 330.0 / SR;
    const dScore = tp * 220.0 / SR; const dPaddle = tp * 660.0 / SR;
    let j = 0;
    while (j < free) {
      const gj = aClock + j;
      const pos = gj % chordLen;
      // MÚSICA: 3 notas do acorde (fase = pos×delta) — harmonia × volume da config
      let mus = math.sin(pos * dA) + math.sin(pos * dB) * 0.7 + math.sin(pos * dC) * 0.6;
      let s = mus * 0.09 * musicVol;
      // EFEITOS × volume da config. contador decai 1 por sample = envelope natural.
      if (sfxHover > 0)  { s = s + math.sin(gj * dHover) * 0.20 * sfxVol;  sfxHover = sfxHover - 1; }
      if (sfxWall > 0)   { s = s + math.sin(gj * dWall) * 0.28 * sfxVol;   sfxWall = sfxWall - 1; }
      if (sfxScore > 0)  { s = s + math.sin(gj * dScore) * 0.30 * sfxVol;  sfxScore = sfxScore - 1; }
      if (sfxPaddle > 0) { s = s + math.sin(gj * dPaddle) * 0.28 * sfxVol; sfxPaddle = sfxPaddle - 1; }
      let c = 0;
      while (c < CH) { buffer.write_f32(chunkBuf, (j * CH + c) * 4, s); c = c + 1; }
      j = j + 1;
    }
    audio.write(dev, chunkBuf, free * CH);
    aClock = aClock + free;
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

    // SOM DE HOVER: tique quando o mouse ENTRA num botão (transição, não todo frame)
    let curHover = -1;
    if (s1 === 1 || s1 === 2) curHover = 10;
    if (s2 === 1 || s2 === 2) curHover = 11;
    if (s3 === 1 || s3 === 2) curHover = 12;
    if (curHover !== lastHover && curHover !== -1) { sfxHover = hoverLen; }
    lastHover = curHover;

  } else if (screen === 1) {
    // ══ CONFIG ═════════════════════════════════════════════════════════════════
    app.text(W / 2 - 70, 24, "CONFIG", 0x66CCFFFF, 36);

    // ── seção JOGO ──
    app.text(60, 76, "JOGO", 0x6699CCFF, 16);
    app.text(60, 100, "Dificuldade da IA:", 0xC0C8D0FF, 15);
    aiDifficulty = app.slider(280, 100, 360, aiDifficulty, 0, 1);
    let dlabel = "Media";
    if (aiDifficulty < 0.33) dlabel = "Facil";
    if (aiDifficulty > 0.66) dlabel = "Dificil";
    app.text(60, 122, dlabel, 0xAAFFCCFF, 13);

    app.text(60, 150, "Velocidade da bola:", 0xC0C8D0FF, 15);
    ballSpeed = app.slider(280, 150, 360, ballSpeed, 0, 1);

    // ── seção ÁUDIO ──
    app.text(60, 200, "AUDIO", 0x66CC99FF, 16);
    // toggle de música
    let sm = app.clickable(21, 60, 224, 200, 36);
    let cm = 0x223040FF;
    if (musicOn !== 0) cm = 0x224030FF;
    if (sm === 1) cm = 0x2E4258FF;
    app.box(60, 224, 200, 36, cm, 2, 0x6699CCFF, 8);
    let mlabel = "Musica: OFF";
    if (musicOn !== 0) mlabel = "Musica: ON";
    app.text(78, 232, mlabel, 0xFFFFFFFF, 15);
    if (sm === 3) { if (musicOn !== 0) musicOn = 0; else musicOn = 1; }

    app.text(60, 278, "Volume musica:", 0xC0C8D0FF, 15);
    musicVol = app.slider(280, 278, 360, musicVol, 0, 1);

    app.text(60, 312, "Volume efeitos:", 0xC0C8D0FF, 15);
    sfxVol = app.slider(280, 312, 360, sfxVol, 0, 1);

    // Voltar
    let sb = app.clickable(20, W / 2 - 90, 376, 180, 44);
    let cb = 0x223040FF; if (sb === 1) cb = 0x2E4258FF; if (sb === 2) cb = 0x1A2430FF;
    app.box(W / 2 - 90, 376, 180, 44, cb, 2, 0x6699CCFF, 10);
    app.text(W / 2 - 38, 388, "Voltar", 0xFFFFFFFF, 18);
    if (sb === 3) { screen = 0; }

  } else {
    // ══ JOGO ═══════════════════════════════════════════════════════════════════
    // jogador
    if (app.keyDown(KEY_W) !== 0) ly = ly - pspeed * dt;
    if (app.keyDown(KEY_S) !== 0) ly = ly + pspeed * dt;
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
    if (by < br) { by = br; bvy = 0 - bvy; sfxWall = sfxLen; }
    if (by > H - br) { by = H - br; bvy = 0 - bvy; sfxWall = sfxLen; }
    // colisão esquerda (dispara efeito agudo — o stream mixa)
    if (bx - br < lx + pw && bx > lx && by > ly && by < ly + ph && bvx < 0) {
      bx = lx + pw + br; bvx = 0 - bvx;
      bvy = bvy + ((by - (ly + ph / 2)) / (ph / 2)) * 0.15;
      sfxPaddle = sfxLen;
    }
    // colisão direita
    if (bx + br > rx && bx < rx + pw && by > ry && by < ry + ph && bvx > 0) {
      bx = rx - br; bvx = 0 - bvx;
      bvy = bvy + ((by - (ry + ph / 2)) / (ph / 2)) * 0.15;
      sfxPaddle = sfxLen;
    }
    // pontos (efeito grave)
    if (bx < 0) { scoreR = scoreR + 1; doResetBall = 1; sfxScore = sfxLen; }
    if (bx > W) { scoreL = scoreL + 1; doResetBall = 2; sfxScore = sfxLen; }

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
    if (app.keyPressed(2) !== 0) { screen = 0; }
    app.text(W / 2 - 60, H - 24, "Esc = menu", 0x556066FF, 13);
  }

  app.endFrame();
}

app.close();
