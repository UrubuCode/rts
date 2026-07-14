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

// fetch.fetchText. Senão trata como página local site/<name>.html.
function load(name: string): string {
  // URL remota? (contém '.') → download real.
  if (name.indexOf(".") >= 0) {
    const url = normalize(name);
    io.print("GET " + url);
    const raw = fetchNs.fetchText(url);
    if (raw.length === 0) {
      return page(name,
        "<h1 style='color:#f97316'>Falhou</h1>"
        + "<p style='color:#a4afc4'>Nao consegui baixar <b>" + url + "</b>.</p>");
    }
    return page(name, extractSite(raw, url));
  }
  // Página local.
  const path = "site/" + name + ".html";
  if (fs.exists(path)) return page(name, fs.read_text(path));
  if (fs.exists("dist/" + path)) return page(name, fs.read_text("dist/" + path));
  return page(name,
    "<h1 style='color:#f97316'>404</h1>"
    + "<p style='color:#a4afc4'>Nao achei <b>" + path + "</b>. "
    + "Tente <b>home</b>, <b>sobre</b>, ou uma URL como <b>example.com</b>.</p>");
}

// Página inicial embutida (não depende de arquivo).
const HOME =
  "<h1 style='color:#22d3ee;font-size:40px'>Mini-browser RTS</h1>"
  + "<p style='color:#a4afc4;font-size:18px'>A barra de cima e um &lt;input&gt; DE VERDADE, "
  + "renderizado e editado pelo motor do RTS. Clique nela, digite e aperte Enter.</p>"
  + "<p style='color:#8592a8'>Paginas locais: escreva <b>home</b> ou <b>sobre</b>.</p>";

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

// NAVEGA: mostra "carregando", pinta 1 frame, baixa, troca a página. Retorna o
// novo handle do DOM. `w` (a janela) vem por parâmetro (o engine não captura
// const module-level dentro de função).
function navigate(w: number, cur: number, val: string): number {
  io.print("[nav] indo para: " + val);
  // 1) tela de "carregando" — pinta ANTES do fetch (que bloqueia).
  const loading = page(val,
    "<h1 style='color:#22d3ee'>Carregando...</h1>"
    + "<p style='color:#a4afc4'>Baixando <b>" + val + "</b></p>");
  dom.free(cur);
  let doc = dom.parseHtml(loading);
  egui.beginFrame(w);
  egui.render(w, doc);
  egui.endFrame(w);
  egui.pump(w); // empurra o frame de loading pra tela
  // 2) baixa e troca.
  const next = val === "home" ? page("home", HOME) : load(val);
  dom.free(doc);
  doc = dom.parseHtml(next);
  dom.focusInput(doc, urlInput(doc));
  io.print("[nav] pronto: " + val);
  return doc;
}

// Abre já no site pedido.
io.print("[boot] abrindo akyronhost.com...");
d = navigate(win, d, "akyronhost.com");

let frame = 0;
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  frame = frame + 1;

  const mx = input.mouseX(win);
  const my = input.mouseY(win);
  const clicked = input.mouseClicked(win, 0);

  let doNav = false;

  // Clique: no botão "Ir" → navega; senão foca/desfoca o input sob o cursor.
  if (clicked !== 0) {
    const inp = urlInput(d);
    const irx = rectComp(d, inp, VW, 0);
    const iry = rectComp(d, inp, VW, 1);
    const irw = rectComp(d, inp, VW, 2);
    const irh = rectComp(d, inp, VW, 3);
    io.print("[click] mouse=(" + mx + "," + my + ")  input_rect=(" + irx + "," + iry + " " + irw + "x" + irh + ")");
    if (overGo(d, VW, mx, my)) {
      io.print("[click] botao IR");
      doNav = true;
    } else {
      const hit = dom.inputAt(d, VW, mx, my);
      io.print("[click] inputAt=" + hit);
      dom.focusInput(d, hit);
    }
  }

  // Digitação no input focado.
  const typed = input.textInput(win);
  if (typed.length > 0) {
    const changed = dom.inputFeedText(d, typed);
    io.print("[type] '" + typed + "' changed=" + changed
      + " focused=" + dom.focusedInput(d)
      + " val='" + dom.inputValue(d, urlInput(d)) + "'");
  }
  if (input.key(win, KEY_BACKSPACE, PHASE_PRESSED) !== 0) {
    io.print("[key] backspace");
    dom.inputBackspace(d);
  }
  if (input.key(win, KEY_ENTER, PHASE_PRESSED) !== 0) {
    io.print("[key] ENTER");
    doNav = true;
  }

  // Navega (Enter ou botão Ir).
  if (doNav) {
    const val = dom.inputValue(d, urlInput(d));
    io.print("[nav] valor da barra = '" + val + "'");
    if (val.length > 0) {
      d = navigate(win, d, val);
    } else {
      io.print("[nav] barra vazia, ignorado");
    }
  }

  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

dom.free(d);
egui.close(win);
