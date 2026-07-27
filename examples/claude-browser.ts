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
import imgdec from "rts:imgdec";
import { fs, io, buffer } from "rts";

const KEY_ENTER = 1;
const KEY_BACKSPACE = 4;
const KEY_A = 100; // KEY_A..Z = 100..125
const KEY_C = 102;
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
  // 3) <script> INLINE (head+body): preserva o fonte para o runScripts executar
  //    depois do parse da página montada. <script src> é logado mas não baixado
  //    (o JS externo de sites grandes é minificado além do subset — baixar
  //    megabytes que vão bailar não vale o tempo de load; o inline roda).
  let scripts = "";
  const scc = dom.querySelectorAllCount(site, "script");
  let k = 0;
  let inlineCount = 0;
  while (k < scc) {
    const s = dom.querySelectorAllAt(site, "script", k);
    const src = dom.getAttribute(site, s, "src");
    const stype = dom.getAttribute(site, s, "type");
    const isJs = stype.length === 0 || stype === "text/javascript" || stype === "module";
    if (src.length > 0) {
      io.print("[js] externo (nao baixado): " + src);
    } else if (isJs) {
      const code = dom.getText(site, s);
      if (code.length > 0) {
        scripts = scripts + "<script>" + code + "</script>";
        inlineCount = inlineCount + 1;
      }
    }
    k = k + 1;
  }
  // 4) innerHTML do body.
  const body = dom.querySelector(site, "body");
  const inner = body >= 0 ? dom.innerHtml(site, body) : rawHtml;
  io.print("[site] styles_inline=" + sc + " css_baixados=" + cssCount + " scripts_inline=" + inlineCount + " body=" + inner.length + "B");
  dom.free(site);
  return styles + inner + scripts;
}

// Percorre os <img> do doc já montado, baixa cada src (binário) + decodifica +
// setImage — as imagens aparecem no render. Síncrono com poll (imagens de UI são
// pequenas); limita a 12 imagens p/ não demorar. `doc`/handles como i64 (bug do
// handle-via-param-number). `pageUrl` resolve src relativos.
function loadImages(doc: i64, pageUrl: string): void {
  const count = dom.querySelectorAllCount(doc, "img");
  const max = count < 12 ? count : 12;
  let i = 0;
  while (i < max) {
    const node = dom.querySelectorAllAt(doc, "img", i);
    const src = dom.getAttribute(doc, node, "src");
    if (src.length > 0 && src.indexOf("data:") !== 0) {
      const url = resolveUrl(pageUrl, src);
      const t = fetchNs.fetchBytesAsync(url);
      let st = fetchNs.fetchBytesPoll(t);
      let guard = 0;
      while (st === 0 && guard < 30000000) { st = fetchNs.fetchBytesPoll(t); guard = guard + 1; }
      if (st === 1) {
        const buf = fetchNs.fetchBytesTake(t);
        const img = imgdec.decode(buffer.ptr(buf), buffer.len(buf));
        if (img !== 0) {
          dom.setImage(doc, node, img, 8, imgdec.width(img), imgdec.height(img));
        }
      }
    }
    i = i + 1;
  }
  io.print("[img] " + max + "/" + count + " imagens processadas");
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

// Página inicial embutida (não depende de arquivo — abre instantânea). É uma
// APRESENTAÇÃO do RTS que exercita o próprio motor: gradiente, grid, box-shadow,
// oklch, border-radius, transform — o motor se prova a si mesmo.
const HOME =
  "<style>"
  + ".hero{background:linear-gradient(120deg,#f97316,#d946ef);border-radius:20px;padding:44px 40px;margin:28px 0}"
  + ".hero h1{color:#ffffff;font-size:46px;margin:0 0 10px 0}"
  + ".hero p{color:#ffffff;font-size:19px;margin:0}"
  + ".pill{display:inline-block;background:oklch(0.28 0.03 260);color:#7dd3fc;font-size:13px;letter-spacing:2px;text-transform:uppercase;padding:7px 16px;border-radius:999px;margin-bottom:22px}"
  + ".stats{display:grid;grid-template-columns:repeat(4,1fr);gap:16px;margin:8px 0 32px 0}"
  + ".stat{background:#10141c;border-radius:16px;padding:22px 18px;box-shadow:0 8px 20px rgba(0,0,0,0.4)}"
  + ".stat .n{font-size:30px;font-weight:bold;color:#f97316}"
  + ".stat .l{font-size:12px;color:#8592a8;text-transform:uppercase;letter-spacing:1px}"
  + ".cards{display:grid;grid-template-columns:repeat(3,1fr);gap:18px}"
  + ".card{background:#0c1220;border-radius:16px;padding:24px 22px;box-shadow:0 6px 16px rgba(0,0,0,0.35)}"
  + ".card h3{color:#ffffff;font-size:19px;margin:0 0 8px 0}"
  + ".card p{color:#93a0b6;font-size:14px;margin:0}"
  + ".dot{width:38px;height:38px;border-radius:11px;margin-bottom:16px;background:oklch(0.7 0.19 40)}"
  + ".card:nth-child(2) .dot{background:oklch(0.72 0.15 200)}"
  + ".card:nth-child(3) .dot{background:oklch(0.65 0.2 300)}"
  + "</style>"
  + "<span class='pill'>TypeScript compilado para nativo</span>"
  + "<div class='hero'><h1>Mini-browser feito no RTS</h1>"
  + "<p>Esta pagina foi renderizada pelo motor CSS nativo do RTS. A barra de cima e um &lt;input&gt; de verdade. Digite uma URL e aperte Enter.</p></div>"
  + "<div class='stats'>"
  + "<div class='stat'><div class='n'>16.9ms</div><div class='l'>Monte Carlo AOT</div></div>"
  + "<div class='stat'><div class='n'>5.14x</div><div class='l'>Mais rapido que Bun</div></div>"
  + "<div class='stat'><div class='n'>10MB</div><div class='l'>.exe standalone</div></div>"
  + "<div class='stat'><div class='n'>0</div><div class='l'>Dependencias JS</div></div>"
  + "</div>"
  + "<div class='cards'>"
  + "<div class='card'><div class='dot'></div><h3>Motor CSS proprio</h3><p>Grid, flexbox, gradiente, box-shadow, oklch, transform, calc() e cascade @layer — tudo em Rust, sem navegador.</p></div>"
  + "<div class='card'><div class='dot'></div><h3>Download real</h3><p>fetch assincrono (HTTPS+TLS) numa thread: baixa a pagina sem congelar a janela.</p></div>"
  + "<div class='card'><div class='dot'></div><h3>Roda sozinho</h3><p>Compila para um .exe nativo de ~10MB. Sem Node, sem Bun, sem runtime empacotado.</p></div>"
  + "</div>";

let d = dom.parseHtml(page("home", HOME));

// Descobre o NodeId do <input> da barra (o primeiro input do doc).
function urlInput(doc: i64): number {
  return dom.querySelector(doc, "#urlbar"); // id dedicado: nunca confunde com <input> do site baixado
}

// Retângulo (x,y,w,h) de um nó, em pontos (via getBoundingClientRect do motor).
// which: 0=x 1=y 2=w 3=h. -1 se o nó não tem rect. `vw` inline (o engine não
// captura const module-level dentro de função).
function rectComp(doc: i64, node: i64, vw: number, which: number): number {
  return dom.boundingComponent(doc, node, vw, which) / 1000;
}

// O mouse está sobre o botão "Ir" (id=go)?
function overGo(doc: i64, vw: number, mx: number, my: number): boolean {
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
function localPage(cur: i64, val: string): number {
  dom.free(cur);
  const html = val === "home" ? page("home", HOME) : load(val);
  const doc = dom.parseHtml(html);
  dom.focusInput(doc, urlInput(doc));
  return doc;
}

// Mostra "Carregando" e retorna o DOM dela (o loop pollа o download).
function loadingPage(cur: i64, val: string): number {
  dom.free(cur);
  const doc = dom.parseHtml(page(val,
    "<h1 style='color:#22d3ee'>Carregando...</h1>"
    + "<p style='color:#a4afc4'>Baixando <b>" + val + "</b> (sem travar a janela)</p>"));
  return doc;
}

// Abre na HOME (instantânea).
io.print("[boot] home instantânea (digite uma URL e Enter)");
d = localPage(d, "home");
// Fachada Document sobre o handle corrente — o runScripts/pumpEventCallbacks
// do prelude falam a fachada. Recriada a cada navegação (d muda).
let docF = new Document(d);
io.print("[boot] urlbar node=" + urlInput(d) + " inputs=" + dom.querySelectorAllCount(d, "input") + " val='" + dom.inputValue(d, urlInput(d)) + "'");

let frame = 0;
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  frame = frame + 1;

  // ── POLL do download assíncrono (não bloqueia): quando pronto, monta a página ──
  if (pendingTicket !== 0) {
    const st = fetchNs.fetchPoll(pendingTicket);
    if (frame % 30 === 0) io.print("[poll] ticket=" + pendingTicket + " st=" + st);
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
      // baixa + decodifica as imagens do site (aparecem no render).
      if (raw.length > 0) loadImages(d, normalize(pendingUrl));
      // EXECUTA os <script> inline da página (in-process, com `document`
      // apontando pra este DOM). Script que não compila no subset é isolado
      // (loga o erro e segue — como o console do browser).
      docF = new Document(d);
      const njs = runScriptsAt(docF, normalize(pendingUrl));
      io.print("[js] scripts executados: " + njs);
    } else if (st < 0) {
      pendingTicket = 0; // ticket inválido: aborta
    }
  }

  const mx = input.mouseX(win);
  const my = input.mouseY(win);
  const clicked = input.mouseClicked(win, 0);
  let doNav = false;
  // href da AÇÃO DEFAULT de um clique em `<a>` (vazio = nada a navegar). Lido
  // depois do `pumpEventCallbacks`, que é quem despacha o evento.
  let linkHref = "";

  // Clique: no botão "Ir" → navega; senão foca/desfoca o input sob o cursor.
  if (clicked !== 0) {
    if (overGo(d, VW, mx, my)) {
      io.print("[click] IR (mouse " + mx + "," + my + ")");
      doNav = true;
    } else {
      const hit = dom.inputAt(d, VW, mx, my);
      io.print("[click] mouse=(" + mx + "," + my + ") inputAt=" + hit);
      dom.focusInput(d, hit);
      // AÇÃO DEFAULT do clique num link. NÃO despachamos o evento aqui: o
      // `render.rs` já empurrou o clique na fila crua e o `pumpEventCallbacks`
      // (fim do loop) o despacha — despachar de novo faria o listener da página
      // rodar DUAS vezes. Aqui só resolvemos QUAL link foi clicado; o
      // `preventDefault` é consultado depois, junto do pump.
      const alvo = dom.nodeAt(d, VW, mx, my);
      if (alvo !== -1) {
        const ancora = dom.closest(d, alvo, "a");
        if (ancora !== -1) {
          linkHref = dom.getAttribute(d, ancora, "href");
        }
      }
    }
  }

  // Digitação no input focado (o textInput JÁ inclui o texto colado com Ctrl+V).
  const typed = input.textInput(win);
  if (typed.length > 0) dom.inputFeedText(d, typed);
  if (input.key(win, KEY_BACKSPACE, PHASE_PRESSED) !== 0) dom.inputBackspace(d);
  if (input.key(win, KEY_ENTER, PHASE_PRESSED) !== 0) doNav = true;

  // ── Atalhos com Ctrl (copiar / apagar tudo) ──────────────────────────────────
  const ctrl = input.modCtrl(win) !== 0;
  if (ctrl && input.key(win, KEY_C, PHASE_PRESSED) !== 0) {
    // Ctrl+C: copia o valor do input focado para o clipboard do SO.
    input.copyText(win, dom.inputValue(d, urlInput(d)));
    io.print("[copy] '" + dom.inputValue(d, urlInput(d)) + "'");
  }
  if (ctrl && input.key(win, KEY_A, PHASE_PRESSED) !== 0) {
    // Ctrl+A: "seleciona tudo" → como não há seleção visual, apaga a barra (limpa p/
    // digitar/colar uma URL nova de uma vez).
    let n = 0;
    while (dom.inputValue(d, urlInput(d)).length > 0 && n < 300) {
      dom.inputBackspace(d);
      n = n + 1;
    }
  }

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
        docF = new Document(d);
      } else {
        // página local (instantânea) — roda os <script> dela também.
        d = localPage(d, val);
        docF = new Document(d);
        runScriptsAt(docF, "https://localhost/" + val);
      }
    }
  }

  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
  // Entrega os cliques do frame aos addEventListener registrados pelos <script>
  // da página (hit-test do render → fila crua → callbacks, PRs #1884/#1885).
  // Devolve 1 se algum listener chamou `preventDefault()` neste frame.
  const cancelou = pumpEventCallbacksCancelable(docF);

  // AÇÃO DEFAULT do link, DEPOIS dos listeners (a ordem do browser: o handler
  // roda primeiro e pode cancelar). `#`/`javascript:` não navegam — são o idioma
  // de "link que só existe pro onclick".
  if (linkHref.length > 0 && cancelou === 0) {
    const ehAncora = linkHref.substring(0, 1) === "#";
    const ehJs = linkHref.length >= 11 && linkHref.substring(0, 11) === "javascript:";
    if (!ehAncora && !ehJs) {
      // Base = a URL da página corrente (a última navegada), para resolver href
      // relativo. Vazia na página inicial local → o resolveUrl trata como raiz.
      const base = normalize(pendingUrl);
      const destino = resolveUrl(base, linkHref);
      io.print("[link] " + linkHref + " -> " + destino);
      pendingUrl = destino;
      pendingTicket = fetchNs.fetchTextAsync(normalize(destino));
      d = loadingPage(d, destino);
      docF = new Document(d);
    }
  }
}

dom.free(d);
egui.close(win);
