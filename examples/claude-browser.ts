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
import cryptoNs from "rts:crypto";
import { fs, io, buffer, env, time, hash } from "rts";

const KEY_ENTER = 1;
const KEY_BACKSPACE = 4;
const KEY_A = 100; // KEY_A..Z = 100..125
const KEY_C = 102;
const PHASE_PRESSED = 1;
const VW = 1000;

// ── CACHE DE RECURSOS EM DISCO ──────────────────────────────────────────────
//
// Um recurso externo (CSS, bundle JS, imagem) é imutável na prática: as URLs da
// Meta/Facebook já carregam o hash do conteúdo no caminho, que é o padrão de
// qualquer CDN. Baixar 14,8 MB de bundle a cada execução custa ~1,4 s de rede e,
// pior, torna a repro NÃO-DETERMINÍSTICA — a Meta serve conteúdo diferente a
// cada carga, então o mesmo bug muda de forma entre execuções.
//
// O cache é do BROWSER, não do DOM: `rts-dom` não tem uma linha de rede (ele
// parseia HTML/CSS e faz layout), então rede e cache pertencem ao host.
//
// Chave = hash da URL, para caber em nome de arquivo qualquer que seja a URL
// (uma URL de CDN passa de 200 caracteres). É SipHash, não critografia: aqui só
// precisa ser determinístico e bem distribuído, não resistente a colisão
// adversarial. `RTS_NO_CACHE=1` desliga; `RTS_CACHE_DIR` escolhe o diretório.
const cacheOn = env.get_var("RTS_NO_CACHE").length < 1;
const cacheDir = cacheDirPath();

function cacheDirPath(): string {
  const custom = env.get_var("RTS_CACHE_DIR");
  if (custom.length > 0) return custom;
  return ".rts-webcache";
}

function cachePath(url: string): string {
  return cacheDir + "/" + hash.hash_str(url) + ".bin";
}

// Lê do cache; devolve "" quando não há entrada (ou o cache está desligado).
// `fs.read_all` escreve num buffer do chamador (não devolve string), então o
// tamanho vem do `fs.size` — o que também evita realocar para um bundle de 6 MB.
function cacheGet(url: string): string {
  if (!cacheOn) return "";
  const p = cachePath(url);
  if (!fs.exists(p)) return "";
  const sz = fs.size(p);
  if (sz < 1) return "";
  const buf = buffer.alloc(sz);
  const n = fs.read_all(p, buffer.ptr(buf), sz);
  if (n < 1) { buffer.free(buf); return ""; }
  const out = buffer.to_string(buf);
  buffer.free(buf);
  return out;
}

function cachePut(url: string, body: string): void {
  if (!cacheOn) return;
  if (body.length < 1) return;
  if (!fs.exists(cacheDir)) fs.create_dir_all(cacheDir);
  fs.write(cachePath(url), body);
}

// `fetchText` com cache: um acerto evita a rede inteira.
function fetchTextCached(url: string): string {
  const hit = cacheGet(url);
  if (hit.length > 0) return hit;
  const body = fetchNs.fetchText(url);
  cachePut(url, body);
  return body;
}

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
  // 1) <style> inline: contados só para o log — as tags ficam onde estão, dentro
  //    do head/body preservados abaixo (mover mudaria a cascade; duplicar, pior).
  const sc = dom.querySelectorAllCount(site, "style");
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
        const css = fetchTextCached(cssUrl);
        io.print("[css] " + cssUrl + " -> " + css.length + "B");
        if (css.length > 0) {
          styles = styles + "<style>" + css + "</style>";
          cssCount = cssCount + 1;
        }
      }
    }
    j = j + 1;
  }
  // 3) <script>: preserva o FONTE para o runScripts executar depois do parse da
  //    página montada. Três origens:
  //      • inline — o texto do próprio nó;
  //      • `src="data:...;base64,…"` — a tag passa INTACTA (o motor decodifica
  //        ao executar). Não é caso de nicho: WhatsApp/Meta embutem quase todo o
  //        bootstrap assim (numa carga real, 33 dos 42 `<script src>` são
  //        data-URI), então ignorá-los era descartar o loader da página;
  //      • `src="http(s)://…"` — ainda NÃO baixado. São os bundles de aplicação
  //        (no WhatsApp, 9 arquivos somando ~15 MB); já compilam sem estourar
  //        memória, mas cada um leva segundos, o que travaria o load da janela.
  const scc = dom.querySelectorAllCount(site, "script");
  let k = 0;
  let inlineCount = 0;
  let dataCount = 0;
  let extCount = 0;
  let extBytes = 0;
  // `RTS_BUNDLES=1` baixa os `<script src=http>` — os bundles de aplicação, que
  // no WhatsApp somam ~14,8 MB e são onde mora a UI inteira (o HTML entrega só
  // um splash: medido, o DOM é IDÊNTICO antes e depois de rodar os 33 scripts
  // do bootstrap).
  //
  // DESLIGADO POR PADRÃO porque EXECUTAR os bundles derruba o processo com
  // SEGFAULT (o download em si leva 1,3 s e funciona). Isolado: cada bundle
  // compila sozinho, e os 9 compilam em sequência no mesmo processo sem crash —
  // o crash só aparece quando eles RODAM sobre o DOM da página. Enquanto isso
  // não for resolvido, ligar por padrão trocaria uma janela que abre por um
  // processo que morre.
  const baixarBundles = env.get_var("RTS_BUNDLES").length > 0;
  const t0ext = time.now_ms();
  while (k < scc) {
    const s = dom.querySelectorAllAt(site, "script", k);
    const src = dom.getAttribute(site, s, "src");
    if (src.length > 5 && src.substring(0, 5) === "data:") dataCount = dataCount + 1;
    else if (src.length > 0) {
      extCount = extCount + 1;
      if (baixarBundles) {
        // Baixa o bundle e o reapresenta como `data:` URI base64 — a MESMA forma
        // que os 33 scripts do bootstrap já usam e que o motor decodifica ao
        // executar. Reescrever o fonte como `<script>` INLINE estilhaçaria a
        // página: esta função devolve HTML (`styles + headInner + inner`), que é
        // re-parseado, e JS minificado carrega `<`, `&` e `</script>` dentro de
        // strings. Convertendo para data-URI o conteúdo atravessa a
        // serialização intacto, sem caminho novo no motor.
        const js = fetchTextCached(src);
        if (js.length > 0) {
          extBytes = extBytes + js.length;
          dom.setAttr(site, s, "src", "data:text/javascript;base64," + cryptoNs.base64_encode_str(js));
          io.print("[bundle] " + src.substring(src.lastIndexOf("/") + 1, src.length) + " " + js.length + "B");
        } else {
          io.print("[bundle] FALHOU (0 bytes): " + src);
        }
      }
    }
    else if (dom.getText(site, s).length > 0) inlineCount = inlineCount + 1;
    k = k + 1;
  }
  if (extCount > 0) {
    if (baixarBundles) {
      io.print("[js] " + extCount + " bundles baixados, " + extBytes + "B em "
        + (time.now_ms() - t0ext) + "ms — compilacao leva minutos");
    } else {
      io.print("[js] " + extCount + " <script src=http> nao baixados — a UI mora neles"
        + " (RTS_BUNDLES=1 baixa; hoje SEGFAULTA ao executar)");
    }
  }
  // 4) head + body na ORDEM ORIGINAL, com os <script> onde o autor os pôs. Duas
  //    lições de uma carga real do WhatsApp aqui:
  //      • re-embutir código decodificado como <script> inline estilhaça a
  //        página (JS minificado tem `<`, `&`, `</script>` em strings — o parser
  //        HTML corta no primeiro `</script>`); as tags `src=data:` ficam
  //        intactas e o `__runScriptAt` do motor decodifica ao executar;
  //      • mover os scripts para o fim REORDENA: o loader (`requireLazy`) mora
  //        no <head> e os 25 chamadores no <body> — com o head atrás, todos
  //        falhavam com "call to unknown function `requireLazy`".
  const headEl = dom.querySelector(site, "head");
  const headInner = headEl >= 0 ? dom.innerHtml(site, headEl) : "";
  const body = dom.querySelector(site, "body");
  const inner = body >= 0 ? dom.innerHtml(site, body) : rawHtml;
  io.print("[site] styles_inline=" + sc + " css_baixados=" + cssCount + " scripts_inline=" + inlineCount + " scripts_data=" + dataCount + " body=" + inner.length + "B");
  dom.free(site);
  return styles + headInner + inner;
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
// URL inicial: `RTS_URL=https://exemplo.com rts run examples/claude-browser.ts`
// abre direto no site (dispara o download assíncrono já no boot, mostrando a
// tela de "Carregando"); sem a variável, abre na home embutida.
const urlBoot = env.get_var("RTS_URL");
// Sem "." nem http, é o NOME de uma página local (site/<nome>.html) — mesmo
// atalho que a barra de URL já aceita digitado.
const bootLocal = urlBoot.length > 0 && urlBoot.indexOf(".") < 0
  && urlBoot.indexOf("http") !== 0 ? 1 : 0;
if (urlBoot.length > 0 && bootLocal === 0) {
  io.print("[boot] abrindo direto: " + urlBoot);
  pendingUrl = urlBoot;
  pendingTicket = fetchNs.fetchTextAsync(normalize(urlBoot));
  d = loadingPage(d, urlBoot);
} else if (bootLocal === 1) {
  io.print("[boot] pagina local: " + urlBoot);
  d = localPage(d, urlBoot);
} else {
  d = localPage(d, "home");
}
// Fachada Document sobre o handle corrente — o runScripts/pumpEventCallbacks
// do prelude falam a fachada. Recriada a cada navegação (d muda).
let docF = new Document(d);
// Página local também executa os <script> dela (a home embutida não tem nenhum;
// o caminho remoto roda os seus quando o download completa).
if (bootLocal === 1) {
  io.print("[js] scripts locais executados: " + runScriptsAt(docF, "https://localhost/" + urlBoot));
}
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
      const baixado = fetchNs.fetchTake(pendingTicket);
      pendingTicket = 0;
      // A PÁGINA também entra no cache, e o ACERTO TEM PRECEDÊNCIA — igual aos
      // demais recursos. Sem isto a medição não fecha: a Meta serve HTML
      // diferente a cada requisição, e duas execuções seguidas davam contagens
      // de erro diferentes (7 e 5) sem nada ter mudado no motor.
      // `RTS_NO_CACHE=1` busca sempre da rede.
      const emCache = cacheGet(normalize(pendingUrl));
      let raw = emCache;
      if (raw.length < 1) {
        raw = baixado;
        cachePut(normalize(pendingUrl), raw);
      }
      io.print("[nav] " + raw.length + "B de " + pendingUrl
        + (emCache.length > 0 ? " (cache)" : ""));
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
      // Baixa os recursos EXTERNOS que a página pede (`<link rel=stylesheet>` e
      // `<script src=http>`) e materializa o fonte no nó — é o que o browser faz
      // antes de executar. Ligado por padrão desde que o pré-passo saiu do `.ts`
      // para Rust: os bundles de aplicação da Meta somam ~15 MB e o pré-passo
      // sozinho levava 49 s (varredura quadrática nossa, não bundle patológico);
      // hoje é ~1 s e o custo real vira compilação. `RTS_NO_EXT=1` desliga.
      // `< 1` e não `=== 0`: variável AUSENTE devolve length -1 (não 0).
      if (env.get_var("RTS_NO_EXT").length < 1) {
        const tRes = time.now_ms();
        const nres = loadResources(docF, normalize(pendingUrl));
        io.print("[res] " + nres + " recursos externos em " + (time.now_ms() - tRes) + "ms");
      }
      const tJs = time.now_ms();
      const njs = runScriptsAt(docF, normalize(pendingUrl));
      // Globais que o bootstrap da página publicou (o `requireLazy` da Meta e
      // companhia). Lido via `docF._dom`, NUNCA pelo `d` solto: handle passado
      // como valor entre chamadas corrompe (#1870) e o contador vem 0.
      io.print("[js] globais da pagina: " + DomScope.count(docF._dom));
      io.print("[js] scripts executados: " + njs + " em " + (time.now_ms() - tJs) + "ms");
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
      io.print("[click] IR (mouse " + mx + "," + my + ")");
      doNav = true;
    } else {
      const hit = dom.inputAt(d, VW, mx, my);
      io.print("[click] mouse=(" + mx + "," + my + ") inputAt=" + hit);
      dom.focusInput(d, hit);
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
  pumpEventCallbacks(docF);
  pumpTimerCallbacks(docF);
}

dom.free(d);
egui.close(win);
