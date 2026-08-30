// `window` — o objeto global do browser, escrito em `.ts` sobre o que o motor
// já tem (document/timers/eventos/fetch). Um `<script>` de página recebe um
// `window` de verdade injetado pelo `runScripts` (como recebe `document`).
//
// ## Regras de design (limites do motor, iguais às do dom.ts)
//   1. Propriedades públicas são GETTER/SETTER ou métodos — nunca campo lido de
//      fora após uma chamada (falha "shape not proven"). Campos internos só via
//      `this.`.
//   2. `window` é uma INSTÂNCIA de `WindowImpl`; `runScripts` faz
//      `const window = __makeWindow(__h, url)` e injeta no corpo do script,
//      junto com `const document = window.document`.
//
// O que NÃO tem (documentado, honesto): renderização/reflow real de
// dimensões (innerWidth/Height são o viewport passado), sem history real de
// navegação (pushState só guarda), localStorage é em-memória por processo (não
// persiste em disco). Cobre o que a maioria dos scripts LÊ no boot.

// `Location` — window.location. Parseia a URL da página uma vez; os campos são
// getters. `assign`/`replace`/`reload` são no-op logados (não há navegação real
// dentro de um script; o host controla a navegação).
class WindowLocation {
  _href: string;
  _protocol: string;
  _host: string;
  _hostname: string;
  _port: string;
  _pathname: string;
  _search: string;
  _hash: string;

  constructor(url: string) {
    this._href = url;
    // protocolo
    let rest = url;
    let proto = "https:";
    const ps = url.indexOf("://");
    if (ps >= 0) {
      proto = url.substring(0, ps + 1);
      rest = url.substring(ps + 3);
    }
    this._protocol = proto;
    // hash
    let hash = "";
    const hp = rest.indexOf("#");
    if (hp >= 0) { hash = rest.substring(hp); rest = rest.substring(0, hp); }
    this._hash = hash;
    // search
    let search = "";
    const qp = rest.indexOf("?");
    if (qp >= 0) { search = rest.substring(qp); rest = rest.substring(0, qp); }
    this._search = search;
    // authority + path
    let authority = rest;
    let pathname = "/";
    const sp = rest.indexOf("/");
    if (sp >= 0) { authority = rest.substring(0, sp); pathname = rest.substring(sp); }
    this._pathname = pathname;
    this._host = authority;
    // hostname + port
    let hostname = authority;
    let port = "";
    const cp = authority.lastIndexOf(":");
    if (cp >= 0) { hostname = authority.substring(0, cp); port = authority.substring(cp + 1); }
    this._hostname = hostname;
    this._port = port;
  }

  get href(): string { return this._href; }
  get protocol(): string { return this._protocol; }
  get host(): string { return this._host; }
  get hostname(): string { return this._hostname; }
  get port(): string { return this._port; }
  get pathname(): string { return this._pathname; }
  get search(): string { return this._search; }
  get hash(): string { return this._hash; }
  get origin(): string { return this._protocol + "//" + this._host; }

  // navegação: o host decide; aqui é no-op (o script não navega sozinho).
  assign(url: string): void { }
  replace(url: string): void { }
  reload(): void { }
  toString(): string { return this._href; }
}

// `Navigator` — window.navigator. userAgent reflete o UA de Chrome que o nosso
// fetch usa (somos um browser). Campos estáticos suficientes pro feature-detect.
class WindowNavigator {
  get userAgent(): string {
    return "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
  }
  get platform(): string { return "Win32"; }
  get language(): string { return "pt-BR"; }
  get languages(): string[] { return ["pt-BR", "pt", "en"]; }
  get vendor(): string { return "Google Inc."; }
  get appName(): string { return "Netscape"; }
  get appVersion(): string {
    return "5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
  }
  get onLine(): boolean { return true; }
  get cookieEnabled(): boolean { return true; }
  get hardwareConcurrency(): number { return 8; }
  get maxTouchPoints(): number { return 0; }
}

// `History` — window.history. Sem navegação real: length começa em 1, pushState/
// replaceState guardam o último estado, back/forward/go são no-op.
class WindowHistory {
  _length: number;
  constructor() { this._length = 1; }
  get length(): number { return this._length; }
  get scrollRestoration(): string { return "auto"; }
  pushState(state: any, title: string, url: string): void { this._length = this._length + 1; }
  replaceState(state: any, title: string, url: string): void { }
  back(): void { }
  forward(): void { }
  go(delta: number): void { }
}

// `Storage` — window.localStorage / sessionStorage. Em-memória por processo (NÃO
// persiste em disco; um increment poderia gravar em fs). API do browser: get/set/
// remove/clear/key/length. Guarda pares chave→valor num array paralelo (o subset
// não tem Map-por-string ergonômico dentro de classe; array é seguro).
class WebStorage {
  _keys: string[];
  _vals: string[];
  constructor() { this._keys = []; this._vals = []; }
  get length(): number { return this._keys.length; }
  __idx(key: string): number {
    let i = 0;
    while (i < this._keys.length) {
      if (this._keys[i] === key) return i;
      i = i + 1;
    }
    return -1;
  }
  getItem(key: string): any {
    const i = this.__idx(key);
    if (i < 0) return null;
    return this._vals[i];
  }
  setItem(key: string, value: string): void {
    const i = this.__idx(key);
    if (i >= 0) { this._vals[i] = value; return; }
    this._keys.push(key);
    this._vals.push(value);
  }
  removeItem(key: string): void {
    const i = this.__idx(key);
    if (i < 0) return;
    this._keys.splice(i, 1);
    this._vals.splice(i, 1);
  }
  clear(): void { this._keys = []; this._vals = []; }
  key(n: number): any {
    if (n < 0 || n >= this._keys.length) return null;
    return this._keys[n];
  }
}

// `WindowImpl` — o objeto window. Carrega document, location, navigator,
// history, storages e o viewport. Métodos globais (setTimeout/atob/fetch...) já
// são funções globais no motor; o window as ESPELHA como métodos para o padrão
// `window.setTimeout(...)` que muitos scripts usam.
class WindowImpl {
  _doc: Document;
  _loc: WindowLocation;
  _nav: WindowNavigator;
  _hist: WindowHistory;
  _ls: WebStorage;
  _ss: WebStorage;
  _iw: number;
  _ih: number;

  constructor(domHandle: i64, url: string, vw: number, vh: number) {
    this._doc = new Document(domHandle);
    this._loc = new WindowLocation(url);
    this._nav = new WindowNavigator();
    this._hist = new WindowHistory();
    this._ls = new WebStorage();
    this._ss = new WebStorage();
    this._iw = vw;
    this._ih = vh;
    // Os timers como PROPRIEDADES PRÓPRIAS, com o `this` já preso.
    //
    // Um método do protótipo chamado pelo NOME LIVRE — `setTimeout(fn, 0)`,
    // que é como todo o código de página o escreve — chega cá sem receiver:
    // a cadeia de escopo lê a propriedade e invoca, e `this` é `undefined`.
    // Medido: `TypeError: Cannot read properties of undefined (reading
    // '_doc')` na primeira linha de qualquer script que agende alguma coisa.
    //
    // Num browser `setTimeout(…)` nu tem `this === window`, e a resposta certa
    // a longo prazo é o receiver vir do objeto onde o nome foi encontrado —
    // uma decisão do emissor, não desta classe. Enquanto ela não existe, uma
    // arrow presa aqui dá a mesma resposta para estes cinco, que são os que
    // uma página chama nus. Os outros continuam a precisar de `window.`.
    const doc = this._doc;
    (this as any).setTimeout = (fn: any, ms: number) => DomTimers.add(doc._dom, fn, ms, 0);
    (this as any).clearTimeout = (id: number) => { DomTimers.cancel(doc._dom, id); };
    (this as any).setInterval = (fn: any, ms: number) => DomTimers.add(doc._dom, fn, ms, 1);
    (this as any).clearInterval = (id: number) => { DomTimers.cancel(doc._dom, id); };
    (this as any).requestAnimationFrame = (fn: any) => DomTimers.add(doc._dom, fn, 16, 0);
  }

  get document(): Document { return this._doc; }
  get location(): WindowLocation { return this._loc; }
  get navigator(): WindowNavigator { return this._nav; }
  get history(): WindowHistory { return this._hist; }
  get localStorage(): WebStorage { return this._ls; }
  get sessionStorage(): WebStorage { return this._ss; }
  get innerWidth(): number { return this._iw; }
  get innerHeight(): number { return this._ih; }
  get outerWidth(): number { return this._iw; }
  get outerHeight(): number { return this._ih; }
  get devicePixelRatio(): number { return 1; }
  get name(): string { return ""; }
  get closed(): boolean { return false; }
  // window.self / window.window / window.top / window.parent apontam pra ele
  // mesmo (single-frame). Getters retornam o próprio window.
  get self(): WindowImpl { return this; }
  get window(): WindowImpl { return this; }
  get top(): WindowImpl { return this; }
  get parent(): WindowImpl { return this; }
  // `globalThis === window` num browser, e aqui isso deixou de ser uma
  // curiosidade: o escopo de um `<script>` É este objeto, então sem este getter
  // o nome livre `globalThis` caía no global do PROCESSO — outro objeto, que
  // nenhuma página devia alcançar.
  get globalThis(): WindowImpl { return this; }

  // Timers: vão para a FILA POR DOCUMENTO em Rust (`DomTimers`), dirigida pelo
  // frame do host via `pumpTimerCallbacks(doc)` — NÃO para os timers do motor
  // (que agendam noutro caminho e nunca disparam no loop da janela). Rust
  // porque cada `new Function` é um programa novo: fila `.ts` seria
  // por-programa e o pump do host bombearia uma fila vazia para sempre.
  setTimeout(fn: any, ms: number): number { return DomTimers.add(this._doc._dom, fn, ms, 0); }
  clearTimeout(id: number): void { DomTimers.cancel(this._doc._dom, id); }
  setInterval(fn: any, ms: number): number { return DomTimers.add(this._doc._dom, fn, ms, 1); }
  clearInterval(id: number): void { DomTimers.cancel(this._doc._dom, id); }
  atob(s: string): string { return atob(s); }
  btoa(s: string): string { return btoa(s); }
  encodeURIComponent(s: string): string { return encodeURIComponent(s); }
  decodeURIComponent(s: string): string { return decodeURIComponent(s); }

  // Eventos no nível de window: delega ao <html>/<body> do document (o modelo
  // de eventos do DOM). addEventListener('load'/'DOMContentLoaded', fn) dispara
  // via o mesmo pumpEventCallbacks. v1: guarda o listener no elemento raiz.
  addEventListener(type: string, cb: any): void {
    const root = this._doc.querySelector("body");
    if (root !== null) { root.addEventListener(type, cb); }
  }
  removeEventListener(type: string): void {
    const root = this._doc.querySelector("body");
    if (root !== null) { root.removeEventListener(type); }
  }
  dispatchEvent(type: string): number {
    const root = this._doc.querySelector("body");
    if (root !== null) { return root.dispatchEvent(type); }
    return 0;
  }

  // scroll/alert/etc: no-op (não há navegação/dialog nativo no script).
  scrollTo(x: number, y: number): void { }
  scrollBy(x: number, y: number): void { }
  focus(): void { }
  blur(): void { }
  getComputedStyle(el: Element): any {
    // devolve um objeto com getPropertyValue delegando ao computed do motor.
    return { __el: el };
  }

  // `matchMedia(query)` — feature-detect comum no boot. v1: devolve um objeto
  // MediaQueryList estático (matches=false; sem listener vivo de resize). Cobre
  // o `window.matchMedia('(...)').matches` que muitos scripts leem.
  matchMedia(query: string): any {
    return { media: query, matches: false };
  }

  // `requestAnimationFrame` — sem loop de tempo no script (o host dirige o
  // frame). v1: agenda como um setTimeout de ~16ms (não é o rAF real, mas o
  // callback dispara). `cancelAnimationFrame` = clearTimeout.
  requestAnimationFrame(cb: any): number { return DomTimers.add(this._doc._dom, cb, 16, 0); }
  cancelAnimationFrame(id: number): void { DomTimers.cancel(this._doc._dom, id); }

  // `queueMicrotask` — espelha o global (já existe no motor).
  queueMicrotask(cb: any): void { queueMicrotask(cb); }
}

// `Node` (as constantes de nodeType) vive em RUST: `rts-shared/src/globals/
// node_constants`, declarado com `#[rtse::constant(global = "Node")]`. Constante
// numérica atravessa a borda sem problema, então não há razão para ficar aqui.
// (O `MutationObserver` abaixo guarda um CALLBACK — objeto JS vivo, que não
// atravessa — e por isso continua no `.ts`.)

// `MutationObserver` — observa mutações da árvore. Implementação HONESTA e
// PARCIAL: registra o callback e `observe`/`disconnect`/`takeRecords` existem
// com a assinatura certa, mas NÃO há entrega automática de mutações (o DOM não
// emite notificação de mudança ainda).
//
// Por que existe assim: o padrão dominante no boot de uma página é
// `new MutationObserver(cb); o.observe(html, {...})` para reagir a nós FUTUROS.
// Sem a classe, o `new` derruba o script inteiro — e com ele todo o resto do
// bootstrap, que não tem nada a ver com mutação. Com o stub, o script roda e só
// a reação a mutação futura fica inerte. É a mesma escolha do browser para uma
// página sem mutações: o callback simplesmente não dispara.
//
// Quando o DOM ganhar notificação de mutação, `__deliver` é o ponto de entrada:
// o resto da API já está no lugar.
class MutationObserver {
  _cb: any;
  _alvos: number[];
  constructor(cb: any) {
    this._cb = cb;
    this._alvos = [];
  }
  // `observe(target, options)` — registra o alvo. As opções (childList/subtree/
  // attributes) são aceitas e guardadas implicitamente pelo registro.
  observe(target: any, options: any): void {
    this._alvos.push(1);
  }
  disconnect(): void { this._alvos = []; }
  // Sem fila de mutação ainda: nunca há registro pendente.
  takeRecords(): any[] { return []; }
}

// Fábrica usada pelo `runScripts` para injetar o window num <script>. `vw/vh` são
// o viewport (o host passa; default 1000x800 quando não sabe).
function __makeWindow(domHandle: i64, url: string, vw: number, vh: number): WindowImpl {
  return new WindowImpl(domHandle, url, vw, vh);
}

// ── ESCOPO GLOBAL COMPARTILHADO ENTRE OS <script> DO MESMO DOCUMENTO ──────────
//
// Num browser, TODOS os <script> de uma página compartilham UM único objeto
// global: o script A define `requireLazy = ...` e o script B, compilado depois,
// enxerga. Sem isso cada <script> é uma ilha — foi exatamente o que reprovou o
// boot do WhatsApp/Meta (1 de 33 scripts rodava: o loader `requireLazy` nascia
// no script 2 e morria com ele, e os 28 seguintes caíam em "unknown function").
//
// O modelo aqui: um `window` VIVO por documento (`__winFor`) + um SACO DE
// GLOBAIS (`__G`) que é um objeto simples — o motor aceita propriedade dinâmica
// num objeto literal (`g.foo = fn` e `g.foo()` despacham), então o saco carrega
// o que os scripts criam em tempo de execução, que é o que uma classe `WindowImpl`
// (shape fixo) não consegue carregar.
//
// Os dois vivem enquanto o documento viver, chaveados pelo handle do DOM.

// `handle do DOM → window vivo`. Arrays paralelos (o motor lida melhor com
// arrays de primitivo/handle do que com Map de objeto neste caminho dinâmico).
const __winKeys: i64[] = [];
const __winVals: any[] = [];
const __gVals: any[] = [];
// Nomes já PUBLICADOS no saco por scripts anteriores do mesmo documento (um
// array de nomes por documento). É o que permite ao script N+1 CHAMAR o que o
// script N criou: `__bindGlobals` qualifica essas leituras para `__G.<nome>`.
const __gNames: any[] = [];

// Índice do documento `h` nas tabelas acima; -1 se ainda não tem.
function __winIndex(h: i64): number {
  let i = 0;
  while (i < __winKeys.length) {
    if (__winKeys[i] === h) return i;
    i = i + 1;
  }
  return -1;
}

// `window` PERSISTENTE do documento `h` — criado na primeira chamada e REUSADO
// por todos os <script> seguintes (é o que faz `window.x = 1` num script ser
// visível no próximo, como no browser).
function __winFor(h: i64, url: string, vw: number, vh: number): WindowImpl {
  // A busca compara o handle COMPLETO (`_doc._dom` do window guardado), não o
  // `h` que chegou por parâmetro: um handle repassado como param `i64` chega
  // TRUNCADO (#1870), e dois documentos distintos colapsavam no mesmo índice —
  // os globais de uma página vazavam para a outra.
  let i = 0;
  while (i < __winVals.length) {
    const w = __winVals[i];
    if (w.document._dom === h) return w;
    i = i + 1;
  }
  const w = new WindowImpl(h, url, vw, vh);
  __winKeys.push(h);
  __winVals.push(w);
  __gVals.push({});
  __gNames.push([]);
  return w;
}

// Os nomes globais já publicados no documento `h` (lista viva; `runScripts`
// acrescenta os que cada script cria).
function __globalNames(h: i64, url: string, vw: number, vh: number): string[] {
  const idx = __winIndex(h);
  if (idx >= 0) return __gNames[idx];
  __winFor(h, url, vw, vh);
  return __gNames[__winIndex(h)];
}

// Descarta o escopo global do documento `h` (chamado no `free` do documento).
function __dropWindow(h: i64): void {
  DomTimers.drop(h);
  const idx = __winIndex(h);
  if (idx < 0) return;
  __winKeys.splice(idx, 1);
  __winVals.splice(idx, 1);
  __gVals.splice(idx, 1);
  __gNames.splice(idx, 1);
}

// ── O prelude alcançável de dentro de um `<script>` ──
//
// Aqui, no FIM de `window.ts`, porque este é o último ficheiro do prelude
// (`DOM_TS` concatena dom.ts + scriptscope.ts + window.ts) e uma `class` não
// é içada: publicar `MutationObserver` do `dom.ts` lia-a antes de existir e
// derrubava o prelude inteiro num TDZ.────────────────────────
//
// `__runScriptAt` compila o corpo do script com `new Function`, e um `new
// Function` é um PROGRAMA NOVO: não vê o topo do programa que o criou. O
// prelude é concatenado ao fonte do utilizador (`rts-host/src/run.rs`), então
// `__winFor` é uma função de topo DESSE programa e ficam
// invisíveis exatamente onde o prologue as chama — `typeof __winFor` responde
// `undefined` lá dentro, e o `try/catch` que isola um script quebrado engolia o
// `ReferenceError` como se a página é que estivesse errada.
//
// A cadeia de escopo de um programa novo termina no objeto global, e é isso que
// as duas linhas abaixo usam. Não é decoração: sem elas nenhum `<script>` de
// página corre, e a falha aparece como um script silenciosamente sem efeito.
//
// Só este, e não o prelude inteiro: é o que o executor de `<script>` nomeia. Publicar o resto poria nomes internos ao alcance do
// JavaScript da página, que é superfície que ninguém pediu.
(globalThis as any).__winFor = __winFor;

// `Node` — as constantes de `nodeType` da spec do DOM. Um script usa-as como
// guarda antes de tocar num nó (`el.nodeType === Node.ELEMENT_NODE`), e sem
// elas a guarda lança em vez de responder `false` — o que derruba o script
// inteiro por causa de uma verificação defensiva.
(globalThis as any).Node = {
  ELEMENT_NODE: 1,
  ATTRIBUTE_NODE: 2,
  TEXT_NODE: 3,
  CDATA_SECTION_NODE: 4,
  PROCESSING_INSTRUCTION_NODE: 7,
  COMMENT_NODE: 8,
  DOCUMENT_NODE: 9,
  DOCUMENT_TYPE_NODE: 10,
  DOCUMENT_FRAGMENT_NODE: 11,
};

// `MutationObserver` é uma classe do prelude, e uma classe do prelude é tão
// invisível a um `new Function` quanto uma função dele. O que este alcance
// trava não é a entrega de mutações — o stub regista e segue, e diz isso de si
// próprio — é o `new` não lançar: um script que observa o DOM à cabeça morria
// na primeira linha e nenhuma das seguintes corria.
(globalThis as any).MutationObserver = MutationObserver;
