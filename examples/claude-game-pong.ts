import egui from "rts:egui";
import input from "rts:input";

// PONG com IA — você (W/S, esquerda) contra o COMPUTADOR (direita, IA). Testa a
// stack de UI: render, input (teclado keyDown), App loop (delta time), física +
// colisão + placar + IA. A IA persegue a bola com velocidade limitada + zona morta
// (vencível, não imbatível). Estado em `let` (limite do motor: sem closures).
//   target/release/rts.exe run examples/claude-game-pong.ts

const W = 700;
const H = 460;
const app = createAppAt("Pong com IA (você W/S vs CPU)", W, H, 2100, 360);

// raquetes (x fixo, y move). pw/ph = tamanho.
const pw = 14;
const ph = 90;
const lx = 30;          // raquete esquerda x
const rx = W - 30 - pw; // raquete direita x
let ly = H / 2 - ph / 2;
let ry = H / 2 - ph / 2;
const pspeed = 0.5;     // px por ms (jogador)
const aispeed = 0.34;   // px por ms (IA — mais lenta que o jogador = vencível)

// bola
let bx = W / 2;
let by = H / 2;
let bvx = 0.32;         // px por ms
let bvy = 0.20;
const br = 10;

// placar
let scoreL = 0;
let scoreR = 0;

// códigos de tecla (neutros): W=100+22, S=100+18, setas 5(cima)/6(baixo)
const KEY_W = 122;
const KEY_S = 118;
const KEY_UP = 5;
const KEY_DOWN = 6;

while (app.running()) {
  if (!app.beginFrame()) break;
  const dt = app.delta();

  // ── JOGADOR: raquete esquerda (W/S, keyDown = segurando) ─────────────────────
  if (input.keyDown(app._win, KEY_W) !== 0) ly = ly - pspeed * dt;
  if (input.keyDown(app._win, KEY_S) !== 0) ly = ly + pspeed * dt;

  // ── IA: raquete direita persegue a bola ──────────────────────────────────────
  // mira no centro da raquete vs o y da bola, com ZONA MORTA (não treme) e
  // velocidade limitada (vencível). Só "acorda" quando a bola vem na direção dela
  // (bvx > 0), senão recua devagar pro centro — parece estratégia, não onisciência.
  const rcenter = ry + ph / 2;
  let target = H / 2;            // descansa no centro
  if (bvx > 0) target = by;      // bola vindo: persegue
  const diff = target - rcenter;
  const deadzone = 10;
  if (diff > deadzone) ry = ry + aispeed * dt;
  if (diff < 0 - deadzone) ry = ry - aispeed * dt;

  // clamp das raquetes na tela
  if (ly < 0) ly = 0;
  if (ly > H - ph) ly = H - ph;
  if (ry < 0) ry = 0;
  if (ry > H - ph) ry = H - ph;

  // ── FÍSICA: move a bola ──────────────────────────────────────────────────────
  bx = bx + bvx * dt;
  by = by + bvy * dt;

  // quica no teto/chão
  if (by < br) { by = br; bvy = 0 - bvy; }
  if (by > H - br) { by = H - br; bvy = 0 - bvy; }

  // colisão com a raquete esquerda
  if (bx - br < lx + pw && bx > lx && by > ly && by < ly + ph && bvx < 0) {
    bx = lx + pw + br;
    bvx = 0 - bvx;
    // adiciona efeito conforme onde bateu na raquete
    const hit = (by - (ly + ph / 2)) / (ph / 2);
    bvy = bvy + hit * 0.15;
  }
  // colisão com a raquete direita
  if (bx + br > rx && bx < rx + pw && by > ry && by < ry + ph && bvx > 0) {
    bx = rx - br;
    bvx = 0 - bvx;
    const hit = (by - (ry + ph / 2)) / (ph / 2);
    bvy = bvy + hit * 0.15;
  }

  // ponto: bola saiu pela esquerda (R marca) ou direita (L marca)
  if (bx < 0) {
    scoreR = scoreR + 1;
    bx = W / 2; by = H / 2; bvx = 0.32; bvy = 0.20;
  }
  if (bx > W) {
    scoreL = scoreL + 1;
    bx = W / 2; by = H / 2; bvx = 0 - 0.32; bvy = 0.20;
  }

  // ── RENDER ───────────────────────────────────────────────────────────────────
  app.fillRect(0, 0, W, H, 0x0A0E14FF);
  // linha central tracejada
  let dy = 0;
  while (dy < H) {
    app.box(W / 2 - 2, dy, 4, 16, 0x223040FF, 0, 0, 0);
    dy = dy + 28;
  }
  // placar
  app.text(W / 2 - 90, 24, "" + scoreL, 0x66CCFFFF, 48);
  app.text(W / 2 + 60, 24, "" + scoreR, 0xFF99AAFF, 48);
  // raquetes
  app.box(lx, ly, pw, ph, 0x66CCFFFF, 0, 0, 4);
  app.box(rx, ry, pw, ph, 0xFF99AAFF, 0, 0, 4);
  // bola
  app.box(bx - br, by - br, br * 2, br * 2, 0xFFFFFFFF, 0, 0, br);
  // ajuda
  app.text(20, H - 24, "VOCE (W/S)", 0x66CCFFFF, 14);
  app.text(W - 60, H - 24, "CPU", 0xFF99AAFF, 14);

  app.endFrame();
}

app.close();
