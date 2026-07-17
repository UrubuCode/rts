// Breakout — jogo jogável no motor do RTS.
//   Frontend: site/breakout.html (HTML+CSS, o "gabinete" do jogo)
//   Backend:  este .ts (física, colisões, tijolos, vidas, teclado)
//
// A lógica NÃO fica num <script> da página (o subset não tem loop de tempo no
// script) — fica AQUI, no programa host, que dirige o jogo por frame mutando o
// DOM carregado do HTML. O egui só renderiza a DisplayList e reporta o teclado.
//
//   cargo build -p rts-runtime && target/release/rts.exe run examples/claude-breakout.ts
import egui from "rts:egui";
import dom from "rts:dom";
import input from "rts:input";
import { io, fs } from "rts";

// key codes (render_backend.rs): Space=3, ArrowLeft=7, ArrowRight=8, A=100
const KEY_SPACE = 3;
const KEY_LEFT = 7;
const KEY_RIGHT = 8;
const KEY_A = 100;   // WASD alternativo: A=100, D=103
const KEY_D = 103;
const PHASE_DOWN = 0; // segurar

const VW = 700;
const VH = 620;
const STAGE_W = 640.0;
const STAGE_H = 480.0;
const PADDLE_W = 96.0;
const PADDLE_H = 16.0;
const BALL_SZ = 16.0;
const BRICK_W = 58.0;
const BRICK_H = 22.0;
const BRICK_COLS = 10;
const BRICK_ROWS = 5;
const GAP = 4.0;

// Lê o HTML do gabinete (arquivo local).
let baseHtml = "";
if (fs.exists("site/breakout.html")) baseHtml = fs.read_text("site/breakout.html");
else if (fs.exists("dist/site/breakout.html")) baseHtml = fs.read_text("dist/site/breakout.html");
else {
  io.print("ERRO: site/breakout.html nao encontrado");
}

const d: i64 = dom.parseHtml("<html><body>" + baseHtml + "</body></html>");
const doc = new Document(d);

// Cria os tijolos dinamicamente via DOM (backend gera o nível).
const stage = doc.getElementById("stage");
const cores = ["#f38ba8", "#fab387", "#f9e2af", "#a6e3a1", "#89b4fa"];
// contagem de tijolos vivos (lida por posição paralela — arrays de primitivos).
const brickX: number[] = [];
const brickY: number[] = [];
const brickAlive: number[] = [];
const brickNode: number[] = [];
let nbricks = 0;
if (stage !== null) {
  let r = 0;
  while (r < BRICK_ROWS) {
    let c = 0;
    while (c < BRICK_COLS) {
      const x = 8.0 + c * (BRICK_W + GAP);
      const y = 40.0 + r * (BRICK_H + GAP);
      const el = doc.createElement("div");
      el.setAttribute("class", "brick");
      el.setStyleProp("left", "" + x + "px");
      el.setStyleProp("top", "" + y + "px");
      el.setStyleProp("width", "" + BRICK_W + "px");
      el.setStyleProp("background", cores[r]);
      stage.appendChild(el);
      brickX.push(x);
      brickY.push(y);
      brickAlive.push(1);
      brickNode.push(el.nodeId);
      nbricks = nbricks + 1;
      c = c + 1;
    }
    r = r + 1;
  }
}
io.print("tijolos criados: " + nbricks);

// Referências dos elementos móveis.
const paddle = doc.getElementById("paddle");
const ball = doc.getElementById("ball");
const scoreEl = doc.getElementById("score");
const livesEl = doc.getElementById("lives");
const msgEl = doc.getElementById("msg");

// Aplica largura/altura fixas da raquete uma vez.
if (paddle !== null) {
  paddle.setStyleProp("width", "" + PADDLE_W + "px");
  paddle.setStyleProp("height", "" + PADDLE_H + "px");
}

// ── ESTADO DO JOGO (o backend) ────────────────────────────────────────────────
let paddleX = STAGE_W / 2.0 - PADDLE_W / 2.0;
const paddleY = STAGE_H - 34.0;
let ballX = STAGE_W / 2.0 - BALL_SZ / 2.0;
let ballY = paddleY - BALL_SZ - 2.0;
let velX = 0.0;
let velY = 0.0;
let launched = 0;
let score = 0;
let lives = 3;
let vivos = nbricks;
let gameOver = 0;
let won = 0;

function setLeft(el: Element | null, v: number): void {
  if (el !== null) el.setStyleProp("left", "" + v + "px");
}
function setTop(el: Element | null, v: number): void {
  if (el !== null) el.setStyleProp("top", "" + v + "px");
}

// posiciona a raquete no fundo.
if (paddle !== null) setTop(paddle, paddleY);

const win = egui.openWindow("RTS Breakout", VW, VH, 0);
io.print("jogo aberto — setas movem, ESPACO lanca");

let frame = 0;
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  frame = frame + 1;

  // ── INPUT: raquete ────────────────────────────────────────────────────────
  const left = input.key(win, KEY_LEFT, PHASE_DOWN) !== 0 || input.key(win, KEY_A, PHASE_DOWN) !== 0;
  const right = input.key(win, KEY_RIGHT, PHASE_DOWN) !== 0 || input.key(win, KEY_D, PHASE_DOWN) !== 0;
  if (left) paddleX = paddleX - 7.0;
  if (right) paddleX = paddleX + 7.0;
  if (paddleX < 0.0) paddleX = 0.0;
  if (paddleX > STAGE_W - PADDLE_W) paddleX = STAGE_W - PADDLE_W;
  setLeft(paddle, paddleX);

  // ── LANÇAMENTO ──────────────────────────────────────────────────────────────
  if (launched === 0 && gameOver === 0) {
    // bola gruda na raquete até o espaço.
    ballX = paddleX + PADDLE_W / 2.0 - BALL_SZ / 2.0;
    setLeft(ball, ballX);
    setTop(ball, ballY);
    if (input.key(win, KEY_SPACE, PHASE_DOWN) !== 0) {
      launched = 1;
      velX = 4.0;
      velY = -5.0;
    }
  }

  // ── FÍSICA DA BOLA ──────────────────────────────────────────────────────────
  if (launched === 1 && gameOver === 0) {
    ballX = ballX + velX;
    ballY = ballY + velY;

    // paredes laterais
    if (ballX <= 0.0) { ballX = 0.0; velX = -velX; }
    if (ballX >= STAGE_W - BALL_SZ) { ballX = STAGE_W - BALL_SZ; velX = -velX; }
    // teto
    if (ballY <= 0.0) { ballY = 0.0; velY = -velY; }

    // raquete
    if (velY > 0.0 && ballY + BALL_SZ >= paddleY && ballY + BALL_SZ <= paddleY + PADDLE_H + 6.0) {
      if (ballX + BALL_SZ >= paddleX && ballX <= paddleX + PADDLE_W) {
        velY = -velY;
        ballY = paddleY - BALL_SZ;
        // ângulo depende de onde bateu na raquete (efeito).
        const hit = (ballX + BALL_SZ / 2.0) - (paddleX + PADDLE_W / 2.0);
        velX = hit * 0.12;
        if (velX > 6.0) velX = 6.0;
        if (velX < -6.0) velX = -6.0;
      }
    }

    // caiu embaixo → perde vida
    if (ballY > STAGE_H) {
      lives = lives - 1;
      if (livesEl !== null) livesEl.setInnerHTML("" + lives);
      launched = 0;
      velX = 0.0; velY = 0.0;
      ballY = paddleY - BALL_SZ - 2.0;
      if (lives <= 0) {
        gameOver = 1;
        if (msgEl !== null) msgEl.setInnerHTML("FIM DE JOGO<br><span class='sub'>Pontos: " + score + "</span>");
      }
    }

    // ── COLISÃO COM TIJOLOS ───────────────────────────────────────────────────
    let bi = 0;
    while (bi < nbricks) {
      if (brickAlive[bi] === 1) {
        const bx = brickX[bi];
        const by = brickY[bi];
        if (ballX + BALL_SZ >= bx && ballX <= bx + BRICK_W
          && ballY + BALL_SZ >= by && ballY <= by + BRICK_H) {
          // acertou: mata o tijolo, quica, pontua.
          brickAlive[bi] = 0;
          velY = -velY;
          score = score + 10;
          vivos = vivos - 1;
          if (scoreEl !== null) scoreEl.setInnerHTML("" + score);
          // esconde o tijolo: display:none tira do render (via a fachada Element).
          const bnode = brickNode[bi];
          const bel = new Element(d, bnode);
          bel.setStyleProp("display", "none");
          bi = nbricks; // sai do loop (um tijolo por frame)
        }
      }
      bi = bi + 1;
    }

    if (vivos <= 0 && won === 0) {
      won = 1;
      gameOver = 1;
      if (msgEl !== null) msgEl.setInnerHTML("VOCE VENCEU!<br><span class='sub'>Pontos: " + score + "</span>");
    }

    setLeft(ball, ballX);
    setTop(ball, ballY);
  }

  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
dom.free(d);
egui.close(win);
io.print("fim. pontos: " + score);
