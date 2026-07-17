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

// extrai <style> inline + CSS dos <link> baixados + <script> inline + body.
const site: i64 = dom.parseHtml(raw);
let styles = "";
const sc = dom.querySelectorAllCount(site, "style");
let i = 0;
while (i < sc) {
  const s = dom.querySelectorAllAt(site, "style", i);
  styles = styles + "<style>" + dom.getText(site, s) + "</style>";
  i = i + 1;
}
const lc = dom.querySelectorAllCount(site, "link");
let cssExt = 0;
let li = 0;
while (li < lc) {
  const l = dom.querySelectorAllAt(site, "link", li);
  if (dom.getAttribute(site, l, "rel") === "stylesheet") {
    const css = fetchNs.fetchText(resolveUrl(URL, dom.getAttribute(site, l, "href")));
    if (css.length > 0) { styles = styles + "<style>" + css + "</style>"; cssExt = cssExt + 1; }
  }
  li = li + 1;
}
let scripts = "";
const scc = dom.querySelectorAllCount(site, "script");
let k = 0;
let inl = 0;
while (k < scc) {
  const s = dom.querySelectorAllAt(site, "script", k);
  if (dom.getAttribute(site, s, "src").length === 0) {
    const code = dom.getText(site, s);
    if (code.length > 0) { scripts = scripts + "<script>" + code + "</script>"; inl = inl + 1; }
  }
  k = k + 1;
}
const body = dom.querySelector(site, "body");
const inner = dom.innerHtml(site, body);
io.print("styles=" + sc + " css_ext=" + cssExt + " scripts_inline=" + inl + " body=" + inner.length + "B");
dom.free(site);

const html = "<html><body>" + styles + "<div class='doc'>" + inner + "</div>" + scripts + "</body></html>";
const d: i64 = dom.parseHtml(html);

// Executa os <script> da página com o window/document injetados.
const docF = new Document(d);
const njs = runScriptsAt(docF, URL);
io.print("scripts executados: " + njs);

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
