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
const page =
  "<h1>Meu DOOM HTML</h1>" +
  "<p>Texto com <b>negrito</b> e <i>italico</i> na mesma linha fluindo inline ate quebrar quando a largura da janela acaba e o renderizador precisa passar para a proxima linha.</p>" +
  "<h2>Estilos aninhados</h2>" +
  "<p>Da pra combinar <b>negrito com <i>italico dentro</i> dele</b>, e tambem <i>italico com <b>negrito dentro</b></i>, mostrando a pilha de estilos.</p>" +
  "<h2>Entidades</h2>" +
  "<p>Comparadores escapados: <b>a &lt; b</b> e <b>b &gt; a</b>, alem do E comercial: Tom &amp; Jerry.</p>" +
  "<h3>Subtitulo nivel 3</h3>" +
  "<div>Este bloco usa &lt;div&gt; em vez de &lt;p&gt;, mas a drenagem trata igual: abre escopo, flui o <i>texto inline</i> e fecha.</div>" +
  "<h2>Tags desconhecidas</h2>" +
  "<p>Uma <span>span</span> e um <code>code</code> sao ignorados pelo parser, mas o <b>texto interno continua</b> aparecendo normalmente.</p>" +
  "<h3>Fim</h3>" +
  "<p>Ultimo <i>paragrafo</i> da pagina renderizada pelo motor <b>RTS</b> com <b><i>egui</i></b>.</p>";

const win = new Window("HTML render", 460, 320);
while (win.isOpen()) {
  if (!win.pump()) break;
  win.beginFrame();
  win.html(page);
  win.endFrame();
}
win.close();