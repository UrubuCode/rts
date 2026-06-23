// Camada de alto nível da GUI egui — classe `Window` em TypeScript puro, por
// cima dos primitivos `egui.*` (Rust). Segue a doutrina do projeto: o Rust expõe
// só primitivos (openWindow/pump/beginFrame/endFrame/label/button/slider); a
// ergonomia (a classe, as options, o loop) vive aqui em TS.
//
// FORMA SUPORTADA HOJE — loop explícito + métodos diretos na instância:
//
//   import egui, { Window } from "rts:egui";
//   const win = new Window({ render: "vulkan", width: 340, height: 600 });
//   while (win.isOpen()) {
//     win.beginFrame();
//     win.label("Olá");
//     if (win.button("Salvar")) salvar();
//     win.endFrame();
//   }
//   win.close();
//
// O receiver dos métodos é a instância concreta `win` (provada via `new Window`),
// que o engine novo despacha estaticamente. A forma com callback
// `win.run((ui: Ui) => ...)` depende do engine ler a anotação de parâmetro de
// classe — quando isso landar, o método `run()` abaixo passa a compilar.
//
// `render` aceita os nomes de API gráfica que o dev escolheu; mapeamos para o
// backend real: wgpu=0 cobre vulkan/metal/dx12; glow=1 cobre opengl.

import egui from "rts:egui";

// Opções de criação da janela.
interface WindowOptions {
  title?: string;
  width?: number;
  height?: number;
  render?: "vulkan" | "metal" | "dx12" | "opengl" | "wgpu" | "glow";
}

class Window {
  __h: number;

  constructor(opts: WindowOptions = {}) {
    const title = opts.title !== undefined ? opts.title : "RTS";
    const width = opts.width !== undefined ? opts.width : 800;
    const height = opts.height !== undefined ? opts.height : 600;
    const backend = Window.__backendCode(opts.render);
    this.__h = egui.openWindow(title, width, height, backend);
  }

  // Traduz o nome da API gráfica para o código de backend do primitivo:
  // 0 = wgpu (vulkan/metal/dx12/default), 1 = glow (opengl).
  static __backendCode(render?: string): number {
    if (render === "opengl" || render === "glow") return 1;
    return 0;
  }

  // ── Loop (forma suportada hoje) ──────────────────────────────────────────
  // true enquanto a janela não foi fechada.
  isOpen(): boolean {
    return egui.isOpen(this.__h) !== 0;
  }
  // Bombeia eventos do SO (não bloqueante). Retorna false se a janela pediu
  // para fechar (o `while` deve então sair).
  pump(): boolean {
    return egui.pump(this.__h) === 0;
  }
  // Abre o frame egui.
  beginFrame(): void {
    egui.beginFrame(this.__h);
  }
  // Fecha o frame egui e apresenta.
  endFrame(): void {
    egui.endFrame(this.__h);
  }

  // ── Widgets (métodos diretos na instância) ───────────────────────────────
  label(text: string): void {
    egui.label(this.__h, text);
  }
  // true se o botão foi clicado neste frame.
  button(text: string): boolean {
    return egui.button(this.__h, text) !== 0;
  }
  // Retorna o valor (possivelmente arrastado) do slider.
  slider(value: number, min: number, max: number): number {
    return egui.slider(this.__h, value, min, max);
  }
  // Renderiza HTML básico (h1/h2/h3, p/div, b/strong, i/em) no frame ativo.
  html(text: string): void {
    egui.html(this.__h, text);
  }

  // Imprime no stderr a árvore de DOM retida (último html parseado), indentada
  // estilo devtools. Ferramenta de inspeção/teste do DOM gerado.
  dumpDom(): void {
    egui.domDump(this.__h);
  }

  // Fecha a janela.
  close(): void {
    egui.close(this.__h);
  }

  // ── Açúcar de callback (depende do fix de dispatch de param de classe) ────
  // `win.run((w) => { w.label(...) })` — roda o loop por baixo e chama `frame`
  // a cada quadro com a própria Window como argumento. Habilitado quando o
  // engine novo passar a despachar métodos sobre parâmetros tipados com classe.
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
