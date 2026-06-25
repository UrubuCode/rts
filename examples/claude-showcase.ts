import egui from "rts:egui";
import render from "rts:render";
import buffer from "rts:buffer";

// SHOWCASE — uma janela com TABS que mostram VÁRIAS coisas do stack de UI sobre
// render.*: Widgets, Video (render.image), Animacao (delta time) e Sobre. Tudo via
// a biblioteca de componentes (app.*), egui é só o backend plugável.
//   target/release/rts.exe run examples/claude-showcase.ts

// nasce na TELA 2 (monitor secundario: left>=1920, top>=313)
const app = createAppAt("RTS UI — Showcase (tabs de multi coisas)", 560, 460, 2150, 420);

// buffer pro "video" procedural (aba Video)
const IW = 96;
const IH = 72;
const buf = buffer.alloc(IW * IH * 4);
const ptr = buffer.ptr(buf);

// estado
let activeTab = 0;
let counter = 0;
let volume = 0.5;
let brightness = 60;
let darkMode = 1;
let t = 0;          // tempo do video
let bx = 80;        // bolinha (aba Animacao)
let by = 200;
let vx = 0.15;
let vy = 0.11;
let fpsTarget = 0;  // controlador de FPS (0 = ilimitado)

while (app.running()) {
  if (!app.beginFrame()) break;

  const dt = app.delta();

  let bg = 0x0E1218FF;
  if (darkMode === 0) bg = 0x1A2230FF;
  app.fillRect(0, 0, 560, 460, bg);

  // ── barra de TABS (literais — array-indexado de string baila) ────────────────
  const tw = 140;
  let a0 = 0; if (activeTab === 0) a0 = 1;
  let a1 = 0; if (activeTab === 1) a1 = 1;
  let a2 = 0; if (activeTab === 2) a2 = 1;
  let a3 = 0; if (activeTab === 3) a3 = 1;
  if (app.tab(0, 0, tw, 40, "Widgets", a0) !== 0) activeTab = 0;
  if (app.tab(tw, 0, tw, 40, "Video", a1) !== 0) activeTab = 1;
  if (app.tab(tw * 2, 0, tw, 40, "Animacao", a2) !== 0) activeTab = 2;
  if (app.tab(tw * 3, 0, tw, 40, "Sobre", a3) !== 0) activeTab = 3;

  // ── conteudo por aba ─────────────────────────────────────────────────────────
  if (activeTab === 0) {
    // WIDGETS — componentes interativos
    app.column(28, 64, 504);
    app.autoLabel("Componentes de UI imediata", 0xFFFFFFFF);
    app.autoLabel("Contador: " + counter, 0x99CCFFFF);
    if (app.autoButton("Incrementar")) counter = counter + 1;
    if (app.autoButton("Resetar")) counter = 0;
    app.autoLabel("Volume:", 0xC0C8D0FF);
    volume = app.autoSlider(volume, 0, 1);
    app.autoLabel("Brilho:", 0xC0C8D0FF);
    brightness = app.autoSlider(brightness, 0, 100);
    darkMode = app.autoCheckbox(darkMode, "Modo escuro");
    app.progressBar(28, 420, 504, 16, brightness / 100);

  } else if (activeTab === 1) {
    // VIDEO — bitmap procedural animado via render.image
    app.text(28, 60, "render.image: pixels gerados em TS, pintados pelo backend", 0xC0C8D0FF, 14);
    let py = 0;
    while (py < IH) {
      let px = 0;
      while (px < IW) {
        const r = (px * 4 + t) & 0xFF;
        const g = (py * 4 + t * 2) & 0xFF;
        const b = ((px + py) * 3 + t * 2) & 0xFF;
        const rgba = r | (g << 8) | (b << 16) | (0xFF << 24);
        buffer.write_i32(buf, (py * IW + px) * 4, rgba);
        px = px + 1;
      }
      py = py + 1;
    }
    render.image(app._win, 60, 90, 440, 320, ptr, IW, IH);
    app.box(60, 90, 440, 320, 0x00000000, 2, 0x33CC88FF, 0);
    t = t + 2;
    if (t > 100000) t = 0;

  } else if (activeTab === 2) {
    // ANIMACAO — bolinha quicando com delta time. Area: x[60..500] y[90..430],
    // raio 20 => centro fica em [80..480] x [110..410].
    app.text(28, 60, "FPS medido: " + app.fps() + "  (dt=" + dt + "ms)  alvo=" + fpsTarget, 0x99CCFFFF, 14);
    // CONTROLADOR DE FPS — botoes pra escolher o cap (testar 30/60/120/ilimitado)
    if (app.button(28, 84, 90, 30, "30 fps")) { fpsTarget = 30; app.setFps(30); }
    if (app.button(126, 84, 90, 30, "60 fps")) { fpsTarget = 60; app.setFps(60); }
    if (app.button(224, 84, 90, 30, "120 fps")) { fpsTarget = 120; app.setFps(120); }
    if (app.button(322, 84, 110, 30, "ilimitado")) { fpsTarget = 0; app.setFps(0); }

    bx = bx + vx * dt;
    by = by + vy * dt;
    // ao bater, CLAMPA a posicao na borda E inverte a velocidade (sem clamp a
    // bolinha penetra com dt grande, inverte, e fica presa/escapa). Caixa
    // deslocada pra baixo (y 130) p/ caber os botoes de fps.
    if (bx < 80) { bx = 80; vx = 0 - vx; }
    if (bx > 480) { bx = 480; vx = 0 - vx; }
    if (by < 150) { by = 150; vy = 0 - vy; }
    if (by > 410) { by = 410; vy = 0 - vy; }
    app.box(60, 130, 440, 300, 0x10161EFF, 1, 0x33445566 & 0xFFFFFFFF, 8);
    app.box(bx - 20, by - 20, 40, 40, 0xFF8800FF, 0, 0, 20);

  } else {
    // SOBRE
    app.column(28, 70, 504);
    app.autoLabel("RTS UI Stack", 0xFFFFFFFF);
    app.autoLabel("Uma fundacao de UI sobre render.* (backend plugavel).", 0xC0C8D0FF);
    app.autoLabel("- DOM real + layout em TS (paralelizavel)", 0xB0B8C0FF);
    app.autoLabel("- canvas ergonomico + componentes", 0xB0B8C0FF);
    app.autoLabel("- render.image (video/imagem)", 0xB0B8C0FF);
    app.autoLabel("- multi-window, multi-monitor", 0xB0B8C0FF);
    app.autoLabel("O egui e so um backend; trocavel.", 0x808890FF);
  }

  app.endFrame();
}

buffer.free(buf);
app.close();
