// O ciclo de vida de uma PÁGINA — o que `parseDocument` (dom.ts) nunca faz e
// `loadDocument` (aqui) faz: correr os `<script>` do parse, disparar
// `DOMContentLoaded` no documento e `load` na window, e mover
// `document.readyState` pelos três estados que a navegação atravessa.
//
// Módulo À PARTE de dom.ts/window.ts (que já passam o teto de 500 linhas
// deste crate fora dos dois crates do motor — CLAUDE.md): lógica NOVA entra
// aqui, nunca acrescentada a um ficheiro que já excede. Concatenado DEPOIS de
// window.ts em `DOM_TS` (`lib.rs`) — usa `Document`/`Element`/`__elem`
// (dom.ts) e `WindowImpl`/`__winFor` (window.ts), todos já em escopo, e as
// suas próprias funções são chamadas de dom.ts (o gancho em
// `appendChild`/`insertBefore`) por declaração de função ser IÇADA: o motor
// já depende disto — `__runScriptAt`, em dom.ts, chama `__winFor`, que só é
// declarada mais tarde em window.ts, e funciona pela mesma razão.
//
// PRINCÍPIO (o pedido do lote): o DOM não conhece o compilador nem nada do
// RTS. Ele só faz o que a spec HTML manda — correr um `<script>` quando ele
// liga ao documento, disparar os dois eventos, avançar o `readyState`. Para
// CORRER o texto, este ficheiro chama a MESMA costura que `__runScriptAt`
// já usa (`DomScope.run` → `rts_core::entry::evaluate_in_scope_with_receiver`,
// ver `docs/ui/page-script-bridge.md`) — nada de novo entra nessa costura.

// ── O gancho de ligação — appendChild/insertBefore chamam isto (dom.ts) ──────
//
// Só faz alguma coisa quando o DOCUMENTO tem scripting ligado (`loadDocument`
// liga-o; `parseDocument` nunca o faz — um `<script>` anexado a uma árvore de
// `parseDocument` fica mudo, exatamente como um `DOMParser` real).
function __afterConnect(h: i64, node: number): void {
  if (dom.scriptingEnabled(h) === 0) return;
  if (dom.tagName(h, node) !== "script") return;
  if (!__isConnectedTo(h, node)) return;
  __runConnectedScript(h, node);
}

// Um nó está "ligado ao documento" se É a raiz `<html>` ou está debaixo dela —
// a única raiz que este parser produz (sem `<template>`/shadow root aqui).
function __isConnectedTo(h: i64, node: number): boolean {
  const root = dom.documentElement(h);
  if (root === __DOM_NONE) return false;
  if (root === node) return true;
  return dom.contains(h, root, node) === 1;
}

// Corre (ou agenda) o `<script>` que acabou de ligar. Mesmo filtro de `type`
// e mesma descodificação de `data:` que `__runScriptAt` (dom.ts) usa para um
// `<script>` do parse — a diferença é só o que faz sentido para um ligado por
// MUTAÇÃO: inline corre já, SEM `load` (a spec nunca dispara `load` para texto
// inline); com `src` é uma TAREFA, não algo que esta chamada corre ela mesma.
function __runConnectedScript(h: i64, node: number): void {
  const st = dom.getAttribute(h, node, "type").toLowerCase();
  const isJs = st.length === 0 || st === "text/javascript"
    || st === "application/javascript" || st === "module"
    || st === "application/x-javascript" || st === "text/ecmascript";
  if (!isJs) return;
  const src = dom.getAttribute(h, node, "src");
  if (src.length === 0) {
    __execConnectedScript(h, node, dom.getText(h, node));
    return;
  }
  // A MESMA fila de um `setTimeout(fn, 0)` (`DomTimers`) — é a fila de
  // tarefas do documento em todo o sentido que interessa aqui: FIFO, drenada
  // pela mesma bomba. `http(s)` fica fora deste lote (um fetch síncrono aqui
  // bloquearia quem anexou o nó); `__sourceOfSrc` devolve "" para ele e a
  // tarefa vira no-op, como um recurso que falhou a carregar.
  DomTimers.add(h, () => { __runScheduledScript(h, node, src); }, 0, 0);
}

function __runScheduledScript(h: i64, node: number, src: string): void {
  const code = __sourceOfSrc(src);
  if (code.length === 0) return;
  __execConnectedScript(h, node, code);
  // Só um `src` dispara `load` — nunca um inline (ramo acima). SEM bubbling:
  // a spec HTML nunca borbulha `load`/`error` de um recurso (`<script>`,
  // `<img>`…) — e AQUI isso não é só fidelidade: com bubbling, um `<script>`
  // filho de `<body>` dispararia também o `load` da WINDOW (registado no
  // MESMO elemento `body`, ver `WindowImpl.addEventListener` em `window.ts`)
  // — medido: "window-load" aparecia duas vezes.
  __elem(h, node).dispatchEventNoBubble("load");
}

// Só `data:` por agora, espelhando `__loadScriptAt` (dom.ts) — `http(s)` fica
// fora deste lote.
function __sourceOfSrc(src: string): string {
  if (src.length > 5 && src.substring(0, 5) === "data:") {
    const comma = src.indexOf(",");
    if (comma < 0) return "";
    const meta = src.substring(0, comma);
    const payload = src.substring(comma + 1);
    return meta.indexOf("base64") >= 0 ? atob(payload) : decodeURIComponent(payload);
  }
  return "";
}

// A execução em si, partilhada por inline e agendado: a mesma preparação de
// escopo (`__prepararEscopo`, dom.ts) e a mesma porta `DomScope.run` que
// `__runScriptAt` usa para um `<script>` do parse.
function __execConnectedScript(h: i64, node: number, code: string): void {
  if (code.length === 0) return;
  const doc = new Document(h);
  const url = __urlOf(h);
  __prepararEscopo(doc, url);
  const janela = __winFor(h, url, 1000, 800);
  const ok = DomScope.run(h, code, janela);
  if (ok === 0) {
    const porque = DomScope.lastError(h);
    console.error("[page] <script> ligado por appendChild/insertBefore falhou: " + porque);
  }
}

// ── URL por documento — para um <script> ligado muito depois de loadDocument ──
//
// `__winFor` ignora url/vw/vh quando já existe window para `h` (window.ts) —
// sempre verdade aqui, pois só se chega a este caminho via `loadDocument`, que
// já correu ao menos os scripts do parse (mesmo que zero) e por isso já criou
// a window. Guardado ainda assim, uma string por documento: um placeholder
// estaria errado no dia em que um script ligar antes de qualquer `<script>`
// do parse correr.
const __urlKeys: i64[] = [];
const __urlVals: string[] = [];

function __urlOf(h: i64): string {
  let i = 0;
  while (i < __urlKeys.length) {
    if (__urlKeys[i] === h) return __urlVals[i];
    i = i + 1;
  }
  return "https://localhost/";
}

function __setUrlOf(h: i64, url: string): void {
  let i = 0;
  while (i < __urlKeys.length) {
    if (__urlKeys[i] === h) { __urlVals[i] = url; return; }
    i = i + 1;
  }
  __urlKeys.push(h);
  __urlVals.push(url);
}

// Descarta o estado deste documento — chamado por `document.close()` (dom.ts),
// mesmo padrão de `__dropWindow`/`DomTimers.drop`.
function __dropLifecycle(h: i64): void {
  let i = 0;
  while (i < __urlKeys.length) {
    if (__urlKeys[i] === h) {
      __urlKeys.splice(i, 1);
      __urlVals.splice(i, 1);
      return;
    }
    i = i + 1;
  }
}

// ── loadDocument: navegação ──────────────────────────────────────────────────
//
// `parseDocument` (dom.ts) continua exatamente o que sempre foi: parseia,
// nada corre. Isto é a OUTRA coisa que um browser faz com uma string HTML —
// abri-la como página. Os passos de uma navegação real, pela mesma ordem:
//   1. parse + loadResources (folhas/scripts/imagens que a página referencia)
//   2. corre cada `<script>` do parse, em ordem de documento — o MESMO laço
//      que `runScriptsAt` corre, sem a bomba final dele (ver o porquê abaixo)
//   3. readyState = "interactive"; dispara DOMContentLoaded no documento
//   4. drena a fila de tarefas do documento (um `<script src>` ligado dentro
//      de um script do passo 2 — via `__afterConnect` acima — pode ter
//      agendado uma), DEPOIS readyState = "complete"; dispara load na window
//
// Drenar a fila ANTES do passo 3 e não depois dele é o que a régua medida
// (Edge, 05/09) pede: um `<script src>` ligado durante o passo 2 é uma
// TAREFA na mesma fila que `setTimeout` usa, e teria corrido dentro da bomba
// final que `runScriptsAt` já faz — antes deste código ter a chance de
// disparar DOMContentLoaded. Por isso o laço aqui é o de `__runScriptAt`
// direto (a mesma costura, ver o cabeçalho do ficheiro) e não uma chamada a
// `runScriptsAt`: a wrapper continua a existir para quem já a chama (browser,
// apps) — só `loadDocument` precisa da bomba movida para depois do evento.
function loadDocument(html: string, url: string): Document {
  const doc = parseDocument(html);
  __setUrlOf(doc._dom, url);
  dom.setScriptingEnabled(doc._dom, 1);
  dom.setReadyState(doc._dom, "loading");
  loadResources(doc, url);

  const scriptCount = dom.getByTagCount(doc._dom, "script");
  let j = 0;
  while (j < scriptCount) {
    __runScriptAt(doc, j, url);
    j = j + 1;
  }

  dom.setReadyState(doc._dom, "interactive");
  doc.dispatchEvent("DOMContentLoaded");

  // Fecha o TASK da página — mesmas duas linhas que o fim de `runScriptsAt`
  // corre, só que depois do DOMContentLoaded: drena microtasks/timers que os
  // scripts acima enfileiraram (§4), incluindo um `<script src>` ligado por
  // `appendChild` durante eles.
  engine.run_event_loop();
  pumpTimerCallbacks(doc);
  const erroMicro = engine.take_error();
  if (erroMicro !== undefined) {
    console.error("[page] erro em microtask: " + erroMicro);
  }

  dom.setReadyState(doc._dom, "complete");
  __winFor(doc._dom, url, 1000, 800).dispatchEvent("load");
  return doc;
}
