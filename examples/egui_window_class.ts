// Demonstra a classe `Window` de alto nível (TS) da GUI egui — a API que o dev
// desenhou, agora funcional com o dispatch de método sobre parâmetro de classe.
//
// Rodar:  target/debug/rts.exe run examples/egui_window_class.ts
//
// NOTA: o callback de `run()` precisa anotar o parâmetro com a classe concreta
// (`(w: Window) => ...`) — o engine novo não infere o tipo do callback pelo
// contexto. Com a anotação, `w.label()`/`w.button()` despacham normalmente.

import egui from "rts:egui";

class Window {
  __h: number;
  constructor(title: string, width: number, height: number) {
    this.__h = egui.openWindow(title, width, height, 0);
  }
  isOpen(): boolean { return egui.isOpen(this.__h) !== 0; }
  pump(): boolean { return egui.pump(this.__h) === 0; }
  beginFrame(): void { egui.beginFrame(this.__h); }
  endFrame(): void { egui.endFrame(this.__h); }
  label(text: string): void { egui.label(this.__h, text); }
  button(text: string): boolean { return egui.button(this.__h, text) !== 0; }
  slider(value: number, min: number, max: number): number {
    return egui.slider(this.__h, value, min, max);
  }
  close(): void { egui.close(this.__h); }
  // Açúcar de loop: roda enquanto a janela estiver aberta, chamando `frame` por
  // quadro com a própria Window como argumento.
  run(frame: (w: Window) => void): void {
    while (this.isOpen()) {
      if (!this.pump()) break;
      this.beginFrame();
      frame(this);
      this.endFrame();
    }
    this.close();
  }
}

const win = new Window("RTS — classe Window", 380, 260);
let cliques = 0;
let volume = 0.5;

win.run((w: Window) => {
  w.label("GUI egui via classe Window (TS)");
  if (w.button("clique")) cliques = cliques + 1;
  w.label("cliques: " + cliques);
  volume = w.slider(volume, 0.0, 1.0);
  w.label("volume: " + volume);
});
