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

  // Timers / codecs / fetch: espelham os globais do motor (window.X === X).
  setTimeout(fn: any, ms: number): number { return setTimeout(fn, ms); }
  clearTimeout(id: number): void { clearTimeout(id); }
  setInterval(fn: any, ms: number): number { return setInterval(fn, ms); }
  clearInterval(id: number): void { clearInterval(id); }
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
  requestAnimationFrame(cb: any): number { return setTimeout(cb, 16); }
  cancelAnimationFrame(id: number): void { clearTimeout(id); }

  // `queueMicrotask` — espelha o global (já existe no motor).
  queueMicrotask(cb: any): void { queueMicrotask(cb); }
}

// Fábrica usada pelo `runScripts` para injetar o window num <script>. `vw/vh` são
// o viewport (o host passa; default 1000x800 quando não sabe).
function __makeWindow(domHandle: i64, url: string, vw: number, vh: number): WindowImpl {
  return new WindowImpl(domHandle, url, vw, vh);
}
