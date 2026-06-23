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

// ── Alocador dinâmico de blocos ──────────────────────────────────────────────
// O mapa "tag → como renderizar" vive AQUI no TS (não no Rust). O Rust é um
// motor de layout genérico que só aplica primitivos; estas constantes e os
// `defineBlock`/`defineInline` abaixo dizem o que cada tag significa. Edite à
// vontade — sem recompilar o Rust.

// Eixo de layout (DISPLAY). block ⇒ VERTICAL, inline-flow ⇒ WRAP.
const DISPLAY_VERTICAL = 0;
const DISPLAY_WRAP = 1;
const DISPLAY_HORIZONTAL = 2;
const DISPLAY_GRID = 3;
// Marcador de item de lista.
const PREFIX_NONE = 0;
const PREFIX_BULLET = 1;
const PREFIX_NUMBER = 2;
// Flags (bitmask). Compartilhadas por bloco e inline.
const FLAG_MONO = 1;
const FLAG_PRESERVE_WS = 2;
const FLAG_HEADING = 4;
const FLAG_BOLD = 8;
const FLAG_ITALIC = 16;

// Registra os defaults do HTML. Idempotente — chamado no 1º `new Window`.
let __blocksRegistered = false;
function registerDefaultBlocks(): void {
  if (__blocksRegistered) return;
  __blocksRegistered = true;

  // Cabeçalhos: HEADING + `indent` reusado como tamanho de fonte.
  egui.defineBlock("h1", DISPLAY_VERTICAL, 28, PREFIX_NONE, FLAG_HEADING);
  egui.defineBlock("h2", DISPLAY_VERTICAL, 22, PREFIX_NONE, FLAG_HEADING);
  egui.defineBlock("h3", DISPLAY_VERTICAL, 18, PREFIX_NONE, FLAG_HEADING);

  // Blocos genéricos (CSS block): empilham; parágrafo flui inline.
  egui.defineBlock("p", DISPLAY_WRAP, 0, PREFIX_NONE, 0);
  egui.defineBlock("div", DISPLAY_VERTICAL, 0, PREFIX_NONE, 0);
  egui.defineBlock("section", DISPLAY_VERTICAL, 0, PREFIX_NONE, 0);
  egui.defineBlock("article", DISPLAY_VERTICAL, 0, PREFIX_NONE, 0);
  egui.defineBlock("header", DISPLAY_VERTICAL, 0, PREFIX_NONE, 0);
  egui.defineBlock("footer", DISPLAY_VERTICAL, 0, PREFIX_NONE, 0);
  egui.defineBlock("blockquote", DISPLAY_VERTICAL, 24, PREFIX_NONE, 0);

  // Listas: bloco com recuo; o item carrega o marcador.
  egui.defineBlock("ul", DISPLAY_VERTICAL, 16, PREFIX_NONE, 0);
  egui.defineBlock("ol", DISPLAY_VERTICAL, 16, PREFIX_NONE, 0);
  egui.defineBlock("li", DISPLAY_WRAP, 0, PREFIX_BULLET, 0);

  // Tabela: grade 2-D.
  egui.defineBlock("table", DISPLAY_GRID, 0, PREFIX_NONE, 0);
  egui.defineBlock("tr", DISPLAY_HORIZONTAL, 0, PREFIX_NONE, 0);
  egui.defineBlock("td", DISPLAY_WRAP, 0, PREFIX_NONE, 0);
  egui.defineBlock("th", DISPLAY_WRAP, 0, PREFIX_NONE, FLAG_BOLD);

  // Pré-formatado: bloco monoespaçado que preserva espaços.
  egui.defineBlock("pre", DISPLAY_VERTICAL, 0, PREFIX_NONE, FLAG_MONO | FLAG_PRESERVE_WS);

  // Inlines: só ligam bits de estilo.
  egui.defineInline("b", FLAG_BOLD);
  egui.defineInline("strong", FLAG_BOLD);
  egui.defineInline("i", FLAG_ITALIC);
  egui.defineInline("em", FLAG_ITALIC);
  egui.defineInline("code", FLAG_MONO);
}

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
    registerDefaultBlocks();
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
