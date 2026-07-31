// Aplicativo standalone: abre o WhatsApp Web (ou qualquer site) numa janela.
// Compilado para um .exe nativo com `rts compile`. Sem Node, sem Bun.
//
//   cargo build -p rts-runtime && target/release/rts.exe compile examples/claude-wa-app.ts wa-app
//   ./wa-app.exe
import egui from "rts:egui";
import dom from "rts:dom";
import fetchNs from "rts:fetch";
import imgdec from "rts:imgdec";
import { io, buffer } from "rts";

const URL = "https://web.whatsapp.com/";
const VW = 1100;
const VH = 760;

// Resolve href/src relativos contra a URL da página.
function resolveUrl(base: string, href: string): string {
  if (href.length >= 4 && href.substring(0, 4) === "http") return href;
  if (href.length >= 2 && href.substring(0, 2) === "//") return "https:" + href;
  let origin = base;
  const p = base.indexOf("://");
  if (p >= 0) {
    const slash = base.indexOf("/", p + 3);
    origin = slash >= 0 ? base.substring(0, slash) : base;
  }
  if (href.length >= 1 && href.substring(0, 1) === "/") return origin + href;
  return origin + "/" + href;
}

io.print("=== RTS WhatsApp App ===");
io.print("baixando " + URL + " ...");
const raw = fetchNs.fetchText(URL);
io.print("html: " + raw.length + " bytes");

// UM ÚNICO DOM, a página inteira. A versão anterior remontava um
// `<html><body>…` com só o innerHTML do body + os scripts inline — o que
// DESCARTAVA o `<head>`, onde a Meta põe os `<script src=http>` que definem
// `requireLazy`. Sem o loader, os ~33 scripts `src=data:` da página morriam
// todos em `call to unknown function 'requireLazy'`. Mantendo o documento como
// veio, a ordem head→body fica correta e os bundles carregam antes de quem os
// consome.
const d: i64 = dom.parseHtml(raw);
const docF = new Document(d);

// Baixa os recursos EXTERNOS que a página pede (`<link rel=stylesheet>` e
// `<script src=http>`) e materializa o fonte no nó — é o que o browser faz
// antes de executar.
const nres = loadResources(docF, URL);
io.print("recursos externos: " + nres);

const njs = runScriptsAt(docF, URL);
io.print("scripts executados: " + njs + "  globais publicados: " + DomScope.count(docF._dom));

// Baixa + decodifica as imagens (aparecem no render).
const imgCount = dom.querySelectorAllCount(d, "img");
const maxImg = imgCount < 10 ? imgCount : 10;
let im = 0;
while (im < maxImg) {
  const node = dom.querySelectorAllAt(d, "img", im);
  const src = dom.getAttribute(d, node, "src");
  if (src.length > 0 && src.indexOf("data:") !== 0) {
    const t = fetchNs.fetchBytesAsync(resolveUrl(URL, src));
    let st = fetchNs.fetchBytesPoll(t);
    let guard = 0;
    while (st === 0 && guard < 20000000) { st = fetchNs.fetchBytesPoll(t); guard = guard + 1; }
    if (st === 1) {
      const buf = fetchNs.fetchBytesTake(t);
      const img = imgdec.decode(buffer.ptr(buf), buffer.len(buf));
      if (img !== 0) { dom.setImage(d, node, img, 8, imgdec.width(img), imgdec.height(img)); }
    }
  }
  im = im + 1;
}

io.print("abrindo janela...");
const win = egui.openWindow("WhatsApp Web — RTS", VW, VH, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
  pumpEventCallbacks(docF);
}
dom.free(d);
egui.close(win);
