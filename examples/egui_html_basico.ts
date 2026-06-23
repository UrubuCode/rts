import egui from "rts:egui";
class Window {
  __h: number;
  constructor(t: string, w: number, h: number) { this.__h = egui.openWindow(t, w, h, 0); }
  isOpen(): boolean { return egui.isOpen(this.__h) !== 0; }
  pump(): boolean { return egui.pump(this.__h) === 0; }
  beginFrame(): void { egui.beginFrame(this.__h); }
  endFrame(): void { egui.endFrame(this.__h); }
  html(s: string): void { egui.html(this.__h, s); }
  close(): void { egui.close(this.__h); }
}
const win = new Window("HTML render", 460, 320);
while (win.isOpen()) {
  if (!win.pump()) break;
  win.beginFrame();
  win.html("<h1>Meu DOOM HTML</h1><p>Texto com <b>negrito</b> e <i>italico</i> na mesma linha fluindo inline ate quebrar.</p><h2>Subtitulo</h2><p>Outro <b>paragrafo</b> de bloco.</p>");
  win.endFrame();
}
win.close();
