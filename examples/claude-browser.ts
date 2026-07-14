// Mini-browser no RTS — barra de URL EDITÁVEL + área de página.
//
// Prova a feature nova do rts-dom: `<input>` editável (foco por clique, digitação,
// backspace, cursor) — TODA a lógica no DOM; o egui só pinta a DisplayList e reporta
// clique/tecla crus. Ao apertar Enter, "navega": carrega site/<valor>.html (páginas
// locais). O fetch remoto entra quando o `fetch()` global do engine estiver ligado.
//
//   target/release/rts.exe run examples/claude-browser.ts
import egui from "rts:egui";
import dom from "rts:dom";
import input from "rts:input";
import fetchNs from "rts:fetch";
import { fs, io } from "rts";

const KEY_ENTER = 1;
const KEY_BACKSPACE = 4;
const PHASE_PRESSED = 1;
const VW = 1000;

// Monta o documento: barra fixa no topo (com o <input>) + o conteúdo da página.
// ENVOLVE tudo num <html><body> — sem esse wrapper, o `:root {}` do CSS do site
// (onde o Tailwind/frameworks definem TODAS as variáveis de tema) não casa com
// nenhum elemento (o motor exige um <html> único), e aí `var(--...)` fica sem
// valor → o site vem sem estilo. O <style> da barra vem por ÚLTIMO para vencer
// resets do site (`body{}`/`*{}`) na área do chrome.
function page(urlValue: string, bodyHtml: string): string {
  return "<html><body>"
    + "<style>"
    + ".rtsbar{display:flex;align-items:center;gap:12px;background:#1b2130;padding:18px 20px}"
    + ".rtsbar .u{width:740px;background:#0b0d11;color:#e6e9ef;border-width:2px;border-color:#3a4558;border-radius:10px;padding:14px 16px;font-size:17px}"
    + ".rtsbar .go{background:#f97316;color:#1a0e03;font-weight:bold;padding:14px 26px;border-radius:10px;font-size:17px}"
    + "</style>"
    + "<div class='rtsbar'>"
    +   "<input id='urlbar' class='u' value='" + urlValue + "' placeholder='digite uma pagina e Enter'>"
    +   "<span id='go' class='go'>Ir</span>"
    + "</div>"
    + "<div class='doc'>" + bodyHtml + "</div></body></html>";
}

// Normaliza a URL: sem esquema, assume https://.
function normalize(u: string): string {
  if (u.length >= 8 && u.substring(0, 8) === "https://") return u;
  if (u.length >= 7 && u.substring(0, 7) === "http://") return u;
  return "https://" + u;
}

// "Navega": se o valor parece uma URL (tem um ponto), BAIXA da internet via
// Resolve o href de um <link> contra a URL da página (base). Aceita: absoluto
// (http...), protocolo-relativo (//host/x), raiz (/x) e relativo (x/y).
function resolveUrl(base: string, href: string): string {
  if (href.length >= 4 && href.substring(0, 4) === "http") return href;
  if (href.length >= 2 && href.substring(0, 2) === "//") return "https:" + href;
  // origem de base = esquema + host (até a 3ª barra).
  let origin = base;
  const p = base.indexOf("://");
  if (p >= 0) {
    const slash = base.indexOf("/", p + 3);
    origin = slash >= 0 ? base.substring(0, slash) : base;
  }
  if (href.length >= 1 && href.substring(0, 1) === "/") return origin + href;
  return origin + "/" + href;
}

// Extrai de um HTML completo baixado: <style> inline + CSS dos <link
// rel=stylesheet> (BAIXADOS) + o conteúdo do <body>. Assim a página entra na
// área .doc SEM o <!DOCTYPE><html><head> (que quebrava a barra) e COM o CSS real
// do site (inclusive Tailwind/frameworks nos <link>).
function extractSite(rawHtml: string, pageUrl: string): string {
  const site = dom.parseHtml(rawHtml);
  let styles = "";
  // 1) <style> inline.
  const sc = dom.querySelectorAllCount(site, "style");
  let i = 0;
  while (i < sc) {
    const s = dom.querySelectorAllAt(site, "style", i);
    styles = styles + "<style>" + dom.getText(site, s) + "</style>";
    i = i + 1;
  }
  // 2) <link rel=stylesheet> → baixa o CSS e injeta como <style>.
  const lc = dom.querySelectorAllCount(site, "link");
  let cssCount = 0;
  let j = 0;
  while (j < lc) {
    const l = dom.querySelectorAllAt(site, "link", j);
    const rel = dom.getAttribute(site, l, "rel");
    if (rel === "stylesheet") {
      const href = dom.getAttribute(site, l, "href");
      if (href.length > 0) {
        const cssUrl = resolveUrl(pageUrl, href);
        const css = fetchNs.fetchText(cssUrl);
        io.print("[css] " + cssUrl + " -> " + css.length + "B");
        if (css.length > 0) {
          styles = styles + "<style>" + css + "</style>";
          cssCount = cssCount + 1;
        }
      }
    }
    j = j + 1;
  }
  // 3) innerHTML do body.
  const body = dom.querySelector(site, "body");
  const inner = body >= 0 ? dom.innerHtml(site, body) : rawHtml;
  io.print("[site] styles_inline=" + sc + " css_baixados=" + cssCount + " body=" + inner.length + "B");
  dom.free(site);
  return styles + inner;
}

// Página LOCAL (home/site/<name>.html). URLs remotas vão pelo caminho assíncrono.
function load(name: string): string {
  const path = "site/" + name + ".html";
  if (fs.exists(path)) return page(name, fs.read_text(path));
  if (fs.exists("dist/" + path)) return page(name, fs.read_text("dist/" + path));
  return page(name,
    "<h1 style='color:#f97316'>404</h1>"
    + "<p style='color:#a4afc4'>Nao achei <b>" + path + "</b>. "
    + "Tente <b>home</b>, <b>sobre</b>, ou uma URL como <b>example.com</b>.</p>");
}

// Página inicial embutida (não depende de arquivo — abre instantânea).
const HOME =
  "<div style='padding:40px 0'>"
  + "<h1 style='color:#22d3ee;font-size:44px'>Mini-browser RTS</h1>"
  + "<p style='color:#a4afc4;font-size:19px'>A barra de cima e um &lt;input&gt; DE VERDADE, "
  + "renderizado e editado pelo motor do RTS. Clique nela, digite uma URL e aperte Enter.</p>"
  + "<p style='color:#8592a8;font-size:16px'>Ex.: <b>example.com</b>, <b>wikipedia.org</b>. "
  + "Ao navegar, aparece 'Carregando...' enquanto baixa (o download bloqueia ~1-2s — normal).</p>"
  + "<p style='color:#64748b;font-size:14px'>Sites que montam o tema por JavaScript "
  + "(DaisyUI/React) vem sem cor — o RTS ainda nao executa JS da pagina.</p>"
  + "</div>";

let d = dom.parseHtml(page("home", HOME));

// Descobre o NodeId do <input> da barra (o primeiro input do doc).
function urlInput(doc: number): number {
  return dom.querySelector(doc, "#urlbar"); // id dedicado: nunca confunde com <input> do site baixado
}

// Retângulo (x,y,w,h) de um nó, em pontos (via getBoundingClientRect do motor).
// which: 0=x 1=y 2=w 3=h. -1 se o nó não tem rect. `vw` inline (o engine não
// captura const module-level dentro de função).
function rectComp(doc: number, node: number, vw: number, which: number): number {
  return dom.boundingComponent(doc, node, vw, which) / 1000;
}

// O mouse está sobre o botão "Ir" (id=go)?
function overGo(doc: number, vw: number, mx: number, my: number): boolean {
  const go = dom.querySelector(doc, "#go");
  if (go < 0) return false;
  const x = rectComp(doc, go, vw, 0);
  const y = rectComp(doc, go, vw, 1);
  const w = rectComp(doc, go, vw, 2);
  const h = rectComp(doc, go, vw, 3);
  if (x < 0 || w <= 0) return false;
  return mx >= x && mx <= x + w && my >= y && my <= y + h;
}

const win = egui.openWindow("RTS Browser", VW, 720, 0);

// Estado de DOWNLOAD ASSÍNCRONO (o fetch roda numa thread, a UI NÃO congela).
// `pendingTicket` = 0 quando não há download; senão o ticket do fetchTextAsync.
let pendingTicket = 0;
let pendingUrl = "";

// Troca o DOM `cur` por uma página LOCAL/instantânea (home/site local). Retorna o
// novo handle. Para URL remota, use `startNav` (assíncrono).
function localPage(cur: number, val: string): number {
  dom.free(cur);
  const html = val === "home" ? page("home", HOME) : load(val);
  const doc = dom.parseHtml(html);
  dom.focusInput(doc, urlInput(doc));
  return doc;
}

// Mostra "Carregando" e retorna o DOM dela (o loop pollа o download).
function loadingPage(cur: number, val: string): number {
  dom.free(cur);
  const doc = dom.parseHtml(page(val,
    "<h1 style='color:#22d3ee'>Carregando...</h1>"
    + "<p style='color:#a4afc4'>Baixando <b>" + val + "</b> (sem travar a janela)</p>"));
  return doc;
}

// Abre na HOME (instantânea).
io.print("[boot] home instantânea (digite uma URL e Enter)");
d = localPage(d, "home");

let frame = 0;
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  frame = frame + 1;

  // ── POLL do download assíncrono (não bloqueia): quando pronto, monta a página ──
  if (pendingTicket !== 0) {
    const st = fetchNs.fetchPoll(pendingTicket);
    if (st === 1) {
      const raw = fetchNs.fetchTake(pendingTicket);
      pendingTicket = 0;
      io.print("[nav] baixou " + raw.length + "B de " + pendingUrl);
      const html = raw.length === 0
        ? page(pendingUrl, "<h1 style='color:#f97316'>Falhou</h1><p style='color:#a4afc4'>Nao consegui baixar.</p>")
        : page(pendingUrl, extractSite(raw, normalize(pendingUrl)));
      dom.free(d);
      d = dom.parseHtml(html);
      dom.focusInput(d, urlInput(d));
    } else if (st < 0) {
      pendingTicket = 0; // ticket inválido: aborta
    }
  }

  const mx = input.mouseX(win);
  const my = input.mouseY(win);
  const clicked = input.mouseClicked(win, 0);
  let doNav = false;

  // Clique: no botão "Ir" → navega; senão foca/desfoca o input sob o cursor.
  if (clicked !== 0) {
    if (overGo(d, VW, mx, my)) {
      doNav = true;
    } else {
      const hit = dom.inputAt(d, VW, mx, my);
      dom.focusInput(d, hit);
    }
  }

  // Digitação no input focado.
  const typed = input.textInput(win);
  if (typed.length > 0) dom.inputFeedText(d, typed);
  if (input.key(win, KEY_BACKSPACE, PHASE_PRESSED) !== 0) dom.inputBackspace(d);
  if (input.key(win, KEY_ENTER, PHASE_PRESSED) !== 0) doNav = true;

  // Navega (Enter ou botão Ir) — só se não há download em andamento.
  if (doNav && pendingTicket === 0) {
    const val = dom.inputValue(d, urlInput(d));
    io.print("[nav] '" + val + "'");
    if (val.length > 0) {
      if (val.indexOf(".") >= 0) {
        // URL remota → dispara download ASSÍNCRONO (a UI segue fluida com "Carregando").
        pendingUrl = val;
        const url = normalize(val);
        io.print("GET (async) " + url);
        pendingTicket = fetchNs.fetchTextAsync(url);
        d = loadingPage(d, val);
      } else {
        // página local (instantânea).
        d = localPage(d, val);
      }
    }
  }

  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

dom.free(d);
egui.close(win);
