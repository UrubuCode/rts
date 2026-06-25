// App helper — o LOOP BASE pronto. O dev mantém o while, mas o boilerplate
// (pump/begin/timing/end) some no app.*. deltaTime move a animação independente
// de FPS. `createApp`/`App`/`Canvas` vêm do prelude rts:canvas (sobre render.*).
// Quem quiser outro loop usa egui.pump/render.* crus.
//   target/release/rts.exe run examples/claude-app-loop.ts

const app = createApp("App loop base (delta time)", 480, 320);
app.moveTo(2000, 580); // tela 2

// estado da bolinha (module-level — captura segura)
let x = 60;
let y = 60;
let vx = 0.12;  // pixels por ms
let vy = 0.09;
const r = 24;

while (app.running()) {
  if (!app.beginFrame()) break;

  const dt = app.delta(); // ms desde o frame anterior

  // física por delta time (independe do FPS)
  x = x + vx * dt;
  y = y + vy * dt;
  if (x < r || x > 480 - r) vx = 0 - vx;
  if (y < r || y > 320 - r) vy = 0 - vy;

  app.fillRect(0, 0, 480, 320, 0x0E1218FF);
  // a "bolinha" (quadrado arredondado)
  app.box(x - r, y - r, r * 2, r * 2, 0x33CC88FF, 0, 0, r);
  app.text(16, 12, "Loop base: app.beginFrame/delta/endFrame", 0xB0B8C0FF, 14);
  app.text(16, 32, "dt (ms): " + dt + "  frame: " + app.frameCount(), 0x99CCFFFF, 13);
  app.text(16, 290, "O dev mantem o while; o app.* tira o boilerplate.", 0x808890FF, 12);

  app.endFrame();
}

app.close();
