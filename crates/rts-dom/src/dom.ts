// Fachada DOM ergonômica em TypeScript — `document` global + `Element`, com a API
// e os NOMES do DOM real do browser, escrita em `.ts` sobre os primitivos do
// namespace `rts:dom` (parseHtml/querySelector/getText/setText/...).
//
// ## Regras de design (impostas pelas capacidades do motor — ver levantamento)
// O motor prova a "shape" de uma instância por FLUXO DE VALOR. Disso saem 2 regras
// que esta fachada segue à risca (e que casam com o formato natural do DOM):
//   1. Toda propriedade pública do DOM é um GETTER/SETTER, nunca um campo público
//      lido de fora após uma chamada (campo-pós-chamada falha "shape not proven").
//      Os campos internos `_dom`/`_node` só são lidos via `this.` (sempre provado).
//   2. APIs que retornam `T | null` são MÉTODOS de classe (`document.querySelector`),
//      nunca funções livres (função livre degrada `null` → `NaN`).
//
// `dom.*` são os primitivos do namespace `rts:dom`. `-1` é a sentinela "nó nenhum".

const __DOM_NONE = -1;
const __LISTENER_OPTIONS_SEPARATOR = "\u001f";
const __compositionStates: Map<i64, number> = new Map();

function __listenerFlags(options: any): number {
  if (options === true) return 1;
  if (options === null || options === undefined) return 0;
  let flags = 0;
  if (options.capture === true) flags = flags + 1;
  if (options.once === true) flags = flags + 2;
  if (options.passive === true) flags = flags + 4;
  return flags;
}

function __listenerEventName(type: string, options: any): string {
  return type + __LISTENER_OPTIONS_SEPARATOR + __listenerFlags(options);
}

// Despacho de evento com CALLBACKS: coleta os pares (nó, fn-word) do Rust
// (`dispatchCollect` — alvo primeiro, depois bubbling), COPIA tudo para arrays
// locais (um callback pode re-despachar e sobrescrever o scratch do Dom) e invoca
// cada fn com um objeto de evento `{type, target, currentTarget}` (subset do Event
// do browser). Devolve o total notificado (callbacks coletados; a fila de polling
// legada também é alimentada pelo mesmo dispatchCollect).
function __dispatchWithCallbacks(
  h: i64,
  node: number,
  type: string,
  bubbles: number,
  trusted: number,
): number {
  const n = dom.dispatchCollect(h, node, type, bubbles);
  if (n === 0) return 0;
  const cbs: number[] = [];
  const nodes: number[] = [];
  const captures: number[] = [];
  const passives: number[] = [];
  let i = 0;
  while (i < n) {
    cbs.push(dom.dispatchCbAt(h, i));
    nodes.push(dom.dispatchCbNode(h, i));
    captures.push(dom.dispatchCbCapture(h, i));
    passives.push(dom.dispatchCbPassive(h, i));
    i = i + 1;
  }
  const target = new Element(h, node);
  const state = { stopped: 0, immediate: 0, passive: 0 };
  const event: any = {
    type: type,
    target: target,
    currentTarget: target,
    data: "",
    inputType: "",
    isComposing: false,
    bubbles: bubbles !== 0,
    cancelable: true,
    defaultPrevented: false,
    eventPhase: 0,
    isTrusted: trusted !== 0,
    cancelBubble: false,
    stopPropagation: function () {
      state.stopped = 1;
      event.cancelBubble = true;
    },
    stopImmediatePropagation: function () {
      state.stopped = 1;
      state.immediate = 1;
      event.cancelBubble = true;
    },
    preventDefault: function () {
      if (event.cancelable && state.passive === 0) event.defaultPrevented = true;
    },
  };
  let j = 0;
  while (j < n) {
    event.currentTarget = new Element(h, nodes[j]);
    state.passive = passives[j] !== 0 ? 1 : 0;
    event.eventPhase = nodes[j] === node ? 2 : (captures[j] !== 0 ? 1 : 3);
    // `engine.invoke_cb` reconstitui o Function word no runtime e chama o
    // listener com o mesmo objecto de evento mutável.
    engine.invoke_cb(cbs[j], event);
    if (state.immediate !== 0) break;
    if (state.stopped !== 0 && (j + 1 >= n || nodes[j + 1] !== nodes[j])) break;
    j = j + 1;
  }
  return n;
}

function __keyboardKey(code: number, shift: number): string {
  if (code >= 100 && code <= 125) {
    const letters = "abcdefghijklmnopqrstuvwxyz";
    const letter = letters.charAt(code - 100);
    return shift !== 0 ? letter.toUpperCase() : letter;
  }
  if (code >= 130 && code <= 139) return String(code - 130);
  if (code >= 140 && code <= 151) return "F" + (code - 139);
  if (code === 1) return "Enter";
  if (code === 2) return "Escape";
  if (code === 3) return " ";
  if (code === 4) return "Backspace";
  if (code === 5) return "ArrowUp";
  if (code === 6) return "ArrowDown";
  if (code === 7) return "ArrowLeft";
  if (code === 8) return "ArrowRight";
  if (code === 9) return "Tab";
  if (code === 10) return "Delete";
  if (code === 11) return "Insert";
  if (code === 12) return "Home";
  if (code === 13) return "End";
  if (code === 14) return "PageUp";
  if (code === 15) return "PageDown";
  return "Unidentified";
}

function __keyboardCode(code: number): string {
  if (code >= 100 && code <= 125) {
    const letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    return "Key" + letters.charAt(code - 100);
  }
  if (code >= 130 && code <= 139) return "Digit" + (code - 130);
  if (code >= 140 && code <= 151) return "F" + (code - 139);
  if (code === 1) return "Enter";
  if (code === 2) return "Escape";
  if (code === 3) return "Space";
  if (code === 4) return "Backspace";
  if (code === 5) return "ArrowUp";
  if (code === 6) return "ArrowDown";
  if (code === 7) return "ArrowLeft";
  if (code === 8) return "ArrowRight";
  if (code === 9) return "Tab";
  if (code === 10) return "Delete";
  if (code === 11) return "Insert";
  if (code === 12) return "Home";
  if (code === 13) return "End";
  if (code === 14) return "PageUp";
  if (code === 15) return "PageDown";
  return "Unidentified";
}

function __dispatchKeyboardWithCallbacks(
  h: i64,
  node: number,
  pressed: number,
  repeat: number,
  keyCode: number,
  ctrl: number,
  shift: number,
  alt: number,
  meta: number,
): number {
  const type = pressed !== 0 ? "keydown" : "keyup";
  const n = dom.dispatchCollect(h, node, type, 1);
  if (n === 0) return 0;
  const cbs: number[] = [];
  const nodes: number[] = [];
  const captures: number[] = [];
  const passives: number[] = [];
  let i = 0;
  while (i < n) {
    cbs.push(dom.dispatchCbAt(h, i));
    nodes.push(dom.dispatchCbNode(h, i));
    captures.push(dom.dispatchCbCapture(h, i));
    passives.push(dom.dispatchCbPassive(h, i));
    i = i + 1;
  }
  const target = new Element(h, node);
  const key = __keyboardKey(keyCode, shift);
  const code = __keyboardCode(keyCode);
  const state = { stopped: 0, immediate: 0, passive: 0 };
  const event: any = {
    type: type,
    target: target,
    currentTarget: target,
    key: key,
    code: code,
    keyCode: keyCode,
    which: keyCode,
    repeat: repeat !== 0,
    ctrlKey: ctrl !== 0,
    shiftKey: shift !== 0,
    altKey: alt !== 0,
    metaKey: meta !== 0,
    bubbles: true,
    cancelable: true,
    defaultPrevented: false,
    eventPhase: 0,
    isTrusted: true,
    cancelBubble: false,
    stopPropagation: function () {
      state.stopped = 1;
      event.cancelBubble = true;
    },
    stopImmediatePropagation: function () {
      state.stopped = 1;
      state.immediate = 1;
      event.cancelBubble = true;
    },
    preventDefault: function () {
      if (event.cancelable && state.passive === 0) event.defaultPrevented = true;
    },
  };
  let j = 0;
  while (j < n) {
    event.currentTarget = new Element(h, nodes[j]);
    state.passive = passives[j] !== 0 ? 1 : 0;
    event.eventPhase = nodes[j] === node ? 2 : (captures[j] !== 0 ? 1 : 3);
    engine.invoke_cb(cbs[j], event);
    if (state.immediate !== 0) break;
    if (state.stopped !== 0 && (j + 1 >= n || nodes[j + 1] !== nodes[j])) break;
    j = j + 1;
  }
  return event.defaultPrevented ? 1 : 0;
}

function __dispatchInputCallbacks(
  h: i64,
  node: number,
  type: string,
  data: string,
  inputType: string,
  isComposing: number,
  trusted: number,
): number {
  const n = dom.dispatchCollect(h, node, type, 1);
  if (n === 0) return 0;
  const cbs: number[] = [];
  const nodes: number[] = [];
  const captures: number[] = [];
  const passives: number[] = [];
  let i = 0;
  while (i < n) {
    cbs.push(dom.dispatchCbAt(h, i));
    nodes.push(dom.dispatchCbNode(h, i));
    captures.push(dom.dispatchCbCapture(h, i));
    passives.push(dom.dispatchCbPassive(h, i));
    i = i + 1;
  }
  const target = new Element(h, node);
  const state = { stopped: 0, immediate: 0, passive: 0 };
  const event: any = {
    type: type,
    target: target,
    currentTarget: target,
    data: data,
    inputType: inputType,
    isComposing: isComposing !== 0,
    bubbles: true,
    cancelable: type === "beforeinput",
    defaultPrevented: false,
    eventPhase: 0,
    isTrusted: trusted !== 0,
    cancelBubble: false,
    stopPropagation: function () {
      state.stopped = 1;
      event.cancelBubble = true;
    },
    stopImmediatePropagation: function () {
      state.stopped = 1;
      state.immediate = 1;
      event.cancelBubble = true;
    },
    preventDefault: function () {
      if (event.cancelable && state.passive === 0) event.defaultPrevented = true;
    },
  };
  let j = 0;
  while (j < n) {
    event.currentTarget = new Element(h, nodes[j]);
    state.passive = passives[j] !== 0 ? 1 : 0;
    event.eventPhase = nodes[j] === node ? 2 : (captures[j] !== 0 ? 1 : 3);
    engine.invoke_cb(cbs[j], event);
    if (state.immediate !== 0) break;
    if (state.stopped !== 0 && (j + 1 >= n || nodes[j + 1] !== nodes[j])) break;
    j = j + 1;
  }
  return event.defaultPrevented ? 1 : 0;
}

// camelCase → kebab-case para o açúcar de `dataset` (`userId` → `user-id`).
function __camelToKebab(s: string): string {
  let out = "";
  let i = 0;
  while (i < s.length) {
    const ch = s.charAt(i);
    const lo = ch.toLowerCase();
    if (ch !== lo) {
      out = out + "-" + lo;
    } else {
      out = out + ch;
    }
    i = i + 1;
  }
  return out;
}

// DOMRect-like — o retorno de `getBoundingClientRect()`. Campos numéricos simples
// (o browser tem um objeto DOMRect; aqui um literal com os mesmos campos).
interface DOMRectLike {
  x: number;
  y: number;
  width: number;
  height: number;
  top: number;
  left: number;
  right: number;
  bottom: number;
}

// Um nó do DOM. Embrulha (handle-do-dom, NodeId-versionado). Todas as propriedades
// do browser (textContent, id, className, tagName) são accessors.
class Element {
  _dom: number; // handle do DOM dono
  _node: number; // NodeId versionado deste nó

  constructor(dom: number, node: number) {
    this._dom = dom;
    this._node = node;
  }

  // `el.textContent` — getter concatena o texto dos descendentes; setter substitui
  // o conteúdo por um único nó de texto (igual ao browser).
  get textContent(): string {
    return dom.getText(this._dom, this._node);
  }
  set textContent(t: string) {
    dom.setText(this._dom, this._node, t);
  }

  // `el.innerHTML` — GET/SET serializa ou substitui os filhos.
  get innerHTML(): string {
    return dom.innerHtml(this._dom, this._node);
  }
  set innerHTML(html: string) {
    dom.setInnerHtml(this._dom, this._node, html);
  }
  // Método explícito mantido para hosts que ainda não despacham setters de
  // propriedades de classe; ambos os caminhos usam a mesma mutação Rust.
  setInnerHTML(html: string): void {
    dom.setInnerHtml(this._dom, this._node, html);
  }

  // `el.outerHTML` — GET inclui o próprio elemento.
  get outerHTML(): string {
    return dom.outerHtml(this._dom, this._node);
  }

  // `el.tagName` — o browser devolve em CAIXA ALTA para HTML.
  get tagName(): string {
    return dom.tagName(this._dom, this._node).toUpperCase();
  }

  // `el.id` (get/set via atributo).
  get id(): string {
    return dom.getAttribute(this._dom, this._node, "id");
  }
  set id(v: string) {
    dom.setAttr(this._dom, this._node, "id", v);
  }

  // `el.className` (get/set via atributo class).
  get className(): string {
    return dom.getAttribute(this._dom, this._node, "class");
  }
  set className(v: string) {
    dom.setAttr(this._dom, this._node, "class", v);
  }

  // `el.getAttribute(name)` — string vazia se ausente (o primitivo já normaliza).
  getAttribute(name: string): string {
    return dom.getAttribute(this._dom, this._node, name);
  }
  setAttribute(name: string, value: string): void {
    dom.setAttr(this._dom, this._node, name, value);
  }
  hasAttribute(name: string): boolean {
    // checa PRESENÇA (não valor) — atributos booleanos têm valor "" mas existem.
    return dom.hasAttr(this._dom, this._node, name) === 1;
  }

  // `el.querySelector(sel)` — primeiro descendente que casa, ou null. MÉTODO
  // `el.querySelector(sel)` — 1º DESCENDENTE que casa (restrito à subárvore deste
  // nó, fiel à MDN; #1758 corrigiu o antigo, que buscava a árvore toda).
  querySelector(sel: string): Element | null {
    const n = dom.queryWithin(this._dom, this._node, sel);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }

  // `el.querySelectorAll(sel)` — todos os DESCENDENTES que casam (subárvore).
  querySelectorAll(sel: string): Element[] {
    const out: Element[] = [];
    const n = dom.queryAllWithinCount(this._dom, this._node, sel);
    let i = 0;
    while (i < n) {
      out.push(new Element(this._dom, dom.queryAllWithinAt(this._dom, this._node, sel, i)));
      i = i + 1;
    }
    return out;
  }

  // `el.children` — filhos elemento (exclui texto), como array.
  get children(): Element[] {
    const out: Element[] = [];
    const n = dom.childCount(this._dom, this._node);
    let i = 0;
    while (i < n) {
      const node = dom.childAt(this._dom, this._node, i);
      out.push(new Element(this._dom, node));
      i = i + 1;
    }
    return out;
  }

  // `el.childNodes` — TODOS os filhos (inclui nós de texto), como array.
  get childNodes(): Element[] {
    const out: Element[] = [];
    const n = dom.childNodesCount(this._dom, this._node);
    let i = 0;
    while (i < n) {
      const node = dom.childNodeAt(this._dom, this._node, i);
      out.push(new Element(this._dom, node));
      i = i + 1;
    }
    return out;
  }

  // ── Navegação (parentNode / first|lastChild / next|previousSibling) ──────────
  // Getters que devolvem `Element | null` (null no fim/sem pai). Extrair o NodeId
  // para uma const antes de comparar com -1 (limite do motor i64-cmp inline).
  get parentNode(): Element | null {
    const n = dom.parentNode(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get firstChild(): Element | null {
    const n = dom.firstChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get lastChild(): Element | null {
    const n = dom.lastChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get nextSibling(): Element | null {
    const n = dom.nextSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get previousSibling(): Element | null {
    const n = dom.previousSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }

  // ── Traversal POR ELEMENTO (#1757) — pula nós de texto/comentário ────────────
  get firstElementChild(): Element | null {
    const n = dom.firstElementChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get lastElementChild(): Element | null {
    const n = dom.lastElementChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get nextElementSibling(): Element | null {
    const n = dom.nextElementSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  get previousElementSibling(): Element | null {
    const n = dom.previousElementSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  // `no.ownerDocument` — o documento a que este nó pertence. Todo o `Element`
  // já carrega o handle dele (`_dom`), por isso é o documento REAL e não um
  // objeto novo com o mesmo aspeto.
  //
  // Faltava, e o que a falta impedia: o React 18 liga os seus eventos
  // delegados com `container.ownerDocument.addEventListener(...)`, então
  // `createRoot(...).render(...)` morria em `Cannot read properties of
  // undefined (reading 'addEventListener')` — com o React inteiro já carregado
  // e a funcionar.
  get ownerDocument(): Document {
    return new Document(this._dom);
  }

  get parentElement(): Element | null {
    const n = dom.parentElement(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  // `el.childElementCount` — reusa o primitivo childCount (já existente).
  get childElementCount(): number {
    return dom.childCount(this._dom, this._node);
  }
  // `el.closest(sel)` — sobe até o 1º ancestral (ou o próprio) que casa o seletor.
  // ⚠️ CORTE: só seletor SIMPLES (tag/#id/.classe). Combinadores/compostos → #1752.
  // Seletor vazio/inválido devolve null (a spec lança SyntaxError — o motor não
  // propaga exceções da fronteira; tolerante por ora).
  closest(selector: string): Element | null {
    const n = dom.closest(this._dom, this._node, selector);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }
  // `el.matches(sel)` — testa o seletor simples NESTE nó. (mesmos cortes do closest:
  // só simples; vazio/inválido → false em vez de SyntaxError.)
  matches(selector: string): boolean {
    return dom.matches(this._dom, this._node, selector) === 1;
  }

  // ── Mutação rica (#1756) ─────────────────────────────────────────────────────
  // `el.cloneNode(deep)` — duplica o nó (deep=true com filhos); clone SOLTO.
  cloneNode(deep: boolean): Element {
    const n = dom.cloneNode(this._dom, this._node, deep ? 1 : 0);
    return new Element(this._dom, n);
  }
  // `parent.prepend(child)` — insere no INÍCIO. (variádico não no motor → 1 nó.)
  prepend(child: Element): void {
    dom.prepend(this._dom, this._node, child._node);
  }
  // `el.before(other)` / `after(other)` — insere como irmão (no pai).
  before(other: Element): void {
    dom.insertAdjacent(this._dom, this._node, other._node, 0);
  }
  after(other: Element): void {
    dom.insertAdjacent(this._dom, this._node, other._node, 1);
  }
  // `el.replaceWith(other)` — substitui este nó por outro.
  replaceWith(other: Element): void {
    dom.replaceWith(this._dom, this._node, other._node);
  }
  // `parent.replaceChild(new, old)` — substitui o filho old por new.
  replaceChild(newChild: Element, oldChild: Element): void {
    dom.replaceChild(this._dom, this._node, newChild._node, oldChild._node);
  }
  // `parent.removeChild(child)`.
  removeChild(child: Element): void {
    dom.removeChild(this._dom, this._node, child._node);
  }
  // `parent.replaceChildren()` sem args — remove todos os filhos. (a variante com
  // novos filhos: chame replaceChildrenClear() + appendChild manualmente.)
  replaceChildrenClear(): void {
    dom.clearChildren(this._dom, this._node);
  }

  // ── element.style + getComputedStyle (#1759) ─────────────────────────────────
  // ⚠️ `el.style.color = v` NÃO é viável (sem objeto-proxy/setter no motor). Em vez
  // disso: MÉTODOS por nome. Aceitam nome CSS (`background-color`) OU camelCase
  // (`backgroundColor`) — convertido com __camelToKebab.
  //
  // `el.style.setProperty(name, value)` — define UMA prop inline (preserva as outras).
  setStyleProp(name: string, value: string): void {
    dom.setStyleProperty(this._dom, this._node, __camelToKebab(name), value);
  }
  // `el.style.getPropertyValue(name)` — valor inline (só o style=""). ⚠️ CORTE: o
  // valor é NORMALIZADO (red→rgb(255, 0, 0)), enquanto o browser preserva o texto
  // cru no style.getPropertyValue (mas normaliza no getComputedStyle). Só props
  // conhecidas (as do ComputedStyle); props arbitrárias/custom (--var) → "".
  getStyleProp(name: string): string {
    return dom.inlineProperty(this._dom, this._node, __camelToKebab(name));
  }
  // `el.style.removeProperty(name)`.
  removeStyleProp(name: string): void {
    dom.removeStyleProperty(this._dom, this._node, __camelToKebab(name));
  }
  // `el.style.cssText` (get) — o style="" inteiro.
  get cssText(): string {
    return dom.cssText(this._dom, this._node);
  }
  // `el.style.cssText = v` via método (setter não dispara no motor).
  setCssText(text: string): void {
    dom.setCssText(this._dom, this._node, text);
  }
  // `getComputedStyle(el).<name>` — valor COMPUTADO (após cascade), formato browser.
  getComputedProp(name: string): string {
    return dom.computedProperty(this._dom, this._node, __camelToKebab(name));
  }

  // ── Eventos — callbacks REAIS + polling legado (#1760) ───────────────────────
  // `el.addEventListener(type, fn)` — como no browser: registra o callback; um
  // `dispatchEvent` invoca fn({type, target, currentTarget}) na ordem DOM (alvo →
  // bubbling). O Dom guarda o fn-word opaco (o antigo limite #195 caiu — Function
  // values são estáveis). A forma de 1 argumento continua valendo para o modelo de
  // POLLING legado (pumpEvents/getEventTargetId por frame).
  addEventListener(type: string, cb?: any, options?: any): void {
    if (cb === undefined) {
      dom.addListener(this._dom, this._node, type);
      return;
    }
    dom.addListenerCbOptions(this._dom, this._node, __listenerEventName(type, options), cb);
  }
  removeEventListener(type: string, cb?: any, options?: any): void {
    if (cb === undefined) {
      dom.removeListener(this._dom, this._node, type);
      return;
    }
    dom.removeListenerCb(this._dom, this._node, __listenerEventName(type, options), cb);
  }
  // `el.dispatchEvent(type)` — dispara COM BUBBLING (como `new Event(t, {bubbles:
  // true})`): invoca os callbacks registrados (alvo → ancestrais) e alimenta a fila
  // de polling legada. Devolve quantos listeners (callbacks + polling) notificou.
  dispatchEvent(type: string): number {
    return __dispatchWithCallbacks(this._dom, this._node, type, 1, 0);
  }
  // `el.dispatchEventNoBubble(type)` — dispara SÓ no alvo (como `new Event(t)`, que
  // é bubbles:false por padrão; focus/blur/mouseenter não borbulham).
  dispatchEventNoBubble(type: string): number {
    return __dispatchWithCallbacks(this._dom, this._node, type, 0, 0);
  }
  // `el.nodeId` — o NodeId cru deste elemento (p/ comparar no switch do polling).
  get nodeId(): number {
    return this._node;
  }

  // ── Node utils (#1762) ───────────────────────────────────────────────────────
  // `node.contains(other)` — other é este nó ou um descendente?
  contains(other: Element): boolean {
    return dom.contains(this._dom, this._node, other._node) === 1;
  }
  // `node.hasChildNodes()`.
  hasChildNodes(): boolean {
    return dom.hasChildNodes(this._dom, this._node) === 1;
  }
  // `node.nodeValue` — texto cru de Text/Comment. ⚠️ CORTE: a spec dá `null` para
  // Element/Document, mas a fronteira ABI (string) não carrega null → devolve `''`
  // nesses casos (um Text vazio também é '', indistinguível). SET é método
  // (setNodeValue) porque o motor não dispara setters de propriedade.
  get nodeValue(): string {
    return dom.nodeValue(this._dom, this._node);
  }
  setNodeValue(value: string): void {
    dom.setNodeValue(this._dom, this._node, value);
  }
  // `node.normalize()` — funde textos adjacentes + remove vazios.
  normalize(): void {
    dom.normalize(this._dom, this._node);
  }

  // ── Atributos extra (#1761) ──────────────────────────────────────────────────
  // `el.removeAttribute(name)`.
  removeAttribute(name: string): void {
    dom.removeAttr(this._dom, this._node, name);
  }
  // `el.toggleAttribute(name)` — alterna: adiciona se ausente, remove se presente.
  toggleAttribute(name: string): boolean {
    if (dom.hasAttr(this._dom, this._node, name) === 1) {
      dom.removeAttr(this._dom, this._node, name);
      return false;
    }
    dom.setAttr(this._dom, this._node, name, "");
    return true;
  }
  // `el.toggleAttribute(name, force)` — força o estado: force=true só ADICIONA
  // (nunca remove); force=false só REMOVE (nunca adiciona). Devolve o estado final.
  // (método separado porque o motor não tem parâmetro opcional/default real.)
  toggleAttributeForce(name: string, force: boolean): boolean {
    if (force) {
      if (dom.hasAttr(this._dom, this._node, name) !== 1) {
        dom.setAttr(this._dom, this._node, name, "");
      }
      return true;
    }
    dom.removeAttr(this._dom, this._node, name);
    return false;
  }
  // `el.getAttributeNames()` — nomes dos atributos, em ordem.
  getAttributeNames(): string[] {
    const out: string[] = [];
    const n = dom.attrCount(this._dom, this._node);
    let i = 0;
    while (i < n) {
      out.push(dom.attrNameAt(this._dom, this._node, i));
      i = i + 1;
    }
    return out;
  }
  // `el.dataset.foo` não é viável (sem objeto-proxy no motor) → métodos sobre
  // `data-*`. datasetGet("userId") lê `data-user-id`; datasetSet idem.
  // ⚠️ CORTES vs DOMStringMap: sem enumeração (for..in), sem delete, e o
  // camelCase→kebab só cobre letras ASCII A-Z (input não-ASCII não hifeniza).
  datasetGet(key: string): string {
    return dom.getAttribute(this._dom, this._node, "data-" + __camelToKebab(key));
  }
  datasetSet(key: string, value: string): void {
    dom.setAttr(this._dom, this._node, "data-" + __camelToKebab(key), value);
  }

  // `el.nodeType` — Element=1, Text=3, Comment=8, Document=9.
  get nodeType(): number {
    return dom.nodeType(this._dom, this._node);
  }
  // `el.nodeName` — tag p/ Element; `#text`/`#comment`/`#document` p/ os demais.
  get nodeName(): string {
    return dom.nodeName(this._dom, this._node);
  }

  // `el.setStyle(slot, val)` — aplica UM slot de estilo a ESTE nó (override por-nó,
  // vence tag e style="" inline). Slots: 0=color 1=bg 2=font_size 3=padding
  // 4=margin 5=border_width 6=border_color 7=corner_radius 8=width. É o caminho
  // imperativo de estilo (como `el.style.color = ...` do browser, por slot opaco).
  setStyle(slot: number, val: number): void {
    dom.setStyle(this._dom, this._node, slot, val);
  }

  // `el.getBoundingClientRect()` — o retângulo (border-box) deste elemento, lido do
  // LAYOUT que o motor calcula. Devolve um DOMRect-like {x,y,width,height,top,left,
  // right,bottom}. ⚠️ HEADLESS: o browser usa o viewport real; aqui o layout precisa
  // de uma largura — passe `viewportW` (default 1280). Os componentes vêm do Rust em
  // pontos×1000 (subpixel preservado); dividimos por 1000. Se o nó não tem caixa
  // (texto/inline/display:none), tudo é 0.
  getBoundingClientRect(viewportW: number): DOMRectLike {
    const vw = viewportW > 0 ? viewportW : 1280;
    // extrai cada componente para uma const antes de comparar (limite i64-cmp inline).
    const rawX = dom.boundingComponent(this._dom, this._node, vw, 0);
    const rawY = dom.boundingComponent(this._dom, this._node, vw, 1);
    const rawW = dom.boundingComponent(this._dom, this._node, vw, 2);
    const rawH = dom.boundingComponent(this._dom, this._node, vw, 3);
    // -1 (sem caixa) vira 0 — getBoundingClientRect de elemento sem layout é zeros.
    const x = rawX < 0 ? 0 : rawX / 1000;
    const y = rawY < 0 ? 0 : rawY / 1000;
    const w = rawW < 0 ? 0 : rawW / 1000;
    const h = rawH < 0 ? 0 : rawH / 1000;
    return {
      x: x, y: y, width: w, height: h,
      top: y, left: x, right: x + w, bottom: y + h,
    };
  }

  // `el.appendChild(child)` — anexa e devolve o filho (como o browser).
  appendChild(child: Element): Element {
    dom.appendChild(this._dom, this._node, child._node);
    return child;
  }

  // `el.insertBefore(child, reference)` — insere child antes de reference (ou no
  // fim se reference for null). Devolve child (como o browser).
  insertBefore(child: Element, reference: Element | null): Element {
    const ref = reference === null ? __DOM_NONE : reference._node;
    dom.insertBefore(this._dom, this._node, child._node, ref);
    return child;
  }

  // `el.remove()` — desliga do pai.
  remove(): void {
    dom.removeNode(this._dom, this._node);
  }

  // ── `el.style` — o objeto de estilo inline ──────────────────────
  //
  // Um PROXY, e nao um objeto com uma propriedade por nome CSS. Codigo real
  // escreve `node.style.color = "red"` e `node.style.backgroundColor = "#fff"`,
  // e o conjunto dos nomes possiveis e a especificacao CSS inteira: declarar
  // cada um seria uma lista a envelhecer, e responder so a alguns seria pior
  // que nao responder — uma escrita perdida em silencio.
  //
  // A traducao e feita no acesso: `backgroundColor` -> `background-color`, e o
  // valor vai para `dom.setStyleProperty`, que e o mesmo estilo inline que o
  // layout ja le. `setProperty`/`getPropertyValue`/`removeProperty` e `cssText`
  // ficam por nome, porque quem os usa nao passa pelo caminho camelCase.
  get style(): any {
    const dom_h = this._dom;
    const node_h = this._node;
    const alvo: any = {};
    return new Proxy(alvo, {
      get(_t: any, chave: any): any {
        const nome = "" + chave;
        if (nome === "setProperty") {
          return function (p: string, v: string) { dom.setStyleProperty(dom_h, node_h, p, v); };
        }
        if (nome === "getPropertyValue") {
          return function (p: string) { return dom.inlineProperty(dom_h, node_h, p); };
        }
        if (nome === "removeProperty") {
          return function (p: string) { dom.removeStyleProperty(dom_h, node_h, p); };
        }
        if (nome === "cssText") { return dom.cssText(dom_h, node_h); }
        return dom.inlineProperty(dom_h, node_h, __cssKebab(nome));
      },
      set(_t: any, chave: any, valor: any): boolean {
        const nome = "" + chave;
        if (nome === "cssText") { dom.setCssText(dom_h, node_h, "" + valor); return true; }
        dom.setStyleProperty(dom_h, node_h, __cssKebab(nome), "" + valor);
        return true;
      },
    });
  }

  // ── `el.classList` — o DOMTokenList ───────────────────────────
  //
  // Os metodos por baixo ja existiam (`classListAdd` e companhia); o que faltava
  // era o OBJETO, que e como todo o codigo escreve — `el.classList.add(...)` e
  // nao `el.classListAdd(...)`. O estado continua no atributo `class`, por isso
  // nao ha nada a manter em dia.
  get classList(): any {
    const eu = this;
    return {
      add(c: string): void { eu.classListAdd(c); },
      remove(c: string): void { eu.classListRemove(c); },
      toggle(c: string): boolean { return eu.classListToggle(c); },
      contains(c: string): boolean { return eu.classListContains(c); },
    };
  }

  // O namespace de um elemento. Este DOM nao os modela — o parser produz HTML e
  // nada mais — por isso a resposta e sempre a do HTML. Certa para tudo o que
  // este motor produz hoje, e ERRADA para um `<svg>`, o que fica dito aqui em
  // vez de descoberto por quem la chegar.
  get namespaceURI(): string {
    return "http://www.w3.org/1999/xhtml";
  }

  // ── classList (add/remove/contains/toggle) ───────────────────────────────────
  // Açúcar sobre o atributo `class`, com a semântica do DOMTokenList. Trabalha
  // sobre a string de classes separada por espaço (sem objeto vivo — o motor
  // despacha melhor métodos que retornam valor; estado mora no atributo).
  classListContains(cls: string): boolean {
    const list = this.getAttribute("class");
    return (" " + list + " ").indexOf(" " + cls + " ") !== __DOM_NONE;
  }
  classListAdd(cls: string): void {
    if (this.classListContains(cls)) return;
    const list = this.getAttribute("class");
    const next = list.length === 0 ? cls : list + " " + cls;
    this.setAttribute("class", next);
  }
  classListRemove(cls: string): void {
    const list = this.getAttribute("class");
    let out = "";
    let part = "";
    let i = 0;
    // reconstrói a lista pulando a classe alvo (split manual por espaço).
    while (i <= list.length) {
      const ch = i < list.length ? list.charAt(i) : " ";
      if (ch === " ") {
        if (part.length > 0 && part !== cls) {
          out = out.length === 0 ? part : out + " " + part;
        }
        part = "";
      } else {
        part = part + ch;
      }
      i = i + 1;
    }
    this.setAttribute("class", out);
  }
  classListToggle(cls: string): boolean {
    if (this.classListContains(cls)) {
      this.classListRemove(cls);
      return false;
    }
    this.classListAdd(cls);
    return true;
  }
}

// O `document` — fachada da árvore. No browser é a página; aqui embrulha um handle
// de DOM do `rts:dom`. `parseHtml`/`createElement`/`querySelector` são métodos
// (regra 2 para os que retornam `| null`).
class Document {
  _dom: number;

  constructor(dom_handle: number) {
    this._dom = dom_handle;
  }

  private eventTarget(): Element | null {
    const root = dom.documentElement(this._dom);
    if (root !== __DOM_NONE) return new Element(this._dom, root);
    const body = dom.querySelector(this._dom, "body");
    if (body !== __DOM_NONE) return new Element(this._dom, body);
    return null;
  }

  // `document.body` e `document.head` — os dois nós que um script de página
  // nomeia sem os procurar.
  //
  // Faltavam, e o mecanismo já cá estava: `eventTarget` acima resolve o `body`
  // desde sempre, só nunca o publicou. Medido num bundle real: uma comparação
  // `no.parentElement !== document.body` respondia sempre verdade, porque o
  // lado direito era `undefined` — a guarda passava sempre, em vez de decidir.
  //
  // `null` quando não há, que é o que um browser responde para um documento
  // sem `<body>`, e não um erro: um script que testa `if (document.body)`
  // escreve-se exatamente porque a resposta pode ser nenhuma.
  // `createElementNS(ns, tag)` — cria o elemento, e o namespace e IGNORADO.
  //
  // Nao e casca: o elemento e real e tem a tag pedida. O que nao acontece e o
  // namespace ser lembrado, porque este DOM nao os modela — e um `<svg>` criado
  // assim comporta-se como HTML. E a razao de o React montar uma arvore de HTML
  // e nao uma de SVG.
  createElementNS(_ns: string, tag: string): Element {
    return this.createElement(tag);
  }

  get body(): Element | null {
    const n = dom.querySelector(this._dom, "body");
    return n === __DOM_NONE ? null : new Element(this._dom, n);
  }

  get head(): Element | null {
    const n = dom.querySelector(this._dom, "head");
    return n === __DOM_NONE ? null : new Element(this._dom, n);
  }

  // Listeners do documento recebem eventos que borbulham até à raiz HTML.
  addEventListener(type: string, cb?: any): void {
    const target = this.eventTarget();
    if (target !== null) target.addEventListener(type, cb);
  }
  removeEventListener(type: string): void {
    const target = this.eventTarget();
    if (target !== null) target.removeEventListener(type);
  }
  dispatchEvent(type: string): number {
    const target = this.eventTarget();
    if (target === null) return 0;
    return target.dispatchEvent(type);
  }

  querySelector(sel: string): Element | null {
    const n = dom.querySelector(this._dom, sel);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }

  querySelectorAll(sel: string): Element[] {
    const out: Element[] = [];
    const n = dom.querySelectorAllCount(this._dom, sel);
    let i = 0;
    while (i < n) {
      const node = dom.querySelectorAllAt(this._dom, sel, i);
      out.push(new Element(this._dom, node));
      i = i + 1;
    }
    return out;
  }

  // `document.getElementById(id)` usa igualdade textual no índice do DOM;
  // não é um seletor CSS e, portanto, funciona para IDs como `a.b`.
  getElementById(id: string): Element | null {
    const n = dom.getById(this._dom, id);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }

  // ── getElementsBy* (#1758) — coleções por classe/tag/name ────────────────────
  getElementsByClassName(name: string): Element[] {
    const out: Element[] = [];
    const n = dom.getByClassCount(this._dom, name);
    let i = 0;
    while (i < n) {
      out.push(new Element(this._dom, dom.getByClassAt(this._dom, name, i)));
      i = i + 1;
    }
    return out;
  }
  getElementsByTagName(tag: string): Element[] {
    const out: Element[] = [];
    const n = dom.getByTagCount(this._dom, tag);
    let i = 0;
    while (i < n) {
      out.push(new Element(this._dom, dom.getByTagAt(this._dom, tag, i)));
      i = i + 1;
    }
    return out;
  }
  getElementsByName(name: string): Element[] {
    const out: Element[] = [];
    const n = dom.getByNameCount(this._dom, name);
    let i = 0;
    while (i < n) {
      out.push(new Element(this._dom, dom.getByNameAt(this._dom, name, i)));
      i = i + 1;
    }
    return out;
  }

  // `document.createElement(tag)` — elemento solto (anexe com appendChild).
  createElement(tag: string): Element {
    const n = dom.createElement(this._dom, tag);
    return new Element(this._dom, n);
  }

  // `document.createTextNode(text)` — nó de texto solto (anexe com appendChild).
  createTextNode(text: string): Element {
    const n = dom.createTextNode(this._dom, text);
    return new Element(this._dom, n);
  }

  // `document.createComment(text)` — nó de comentário solto (nodeType 8).
  createComment(text: string): Element {
    const n = dom.createComment(this._dom, text);
    return new Element(this._dom, n);
  }

  // `document.documentElement` — o elemento `<html>`, não a raiz `#document`.
  get documentElement(): Element | null {
    const root = dom.documentElement(this._dom);
    if (root === __DOM_NONE) return null;
    return new Element(this._dom, root);
  }
}

// `parseDocument(html)` — parseia HTML e devolve um `Document`. (Função livre SEM
// retorno `| null`, então é segura; o `| null` só seria problema em função livre.)
function parseDocument(html: string): Document {
  return new Document(dom.parseHtml(html));
}

// ── Polling de eventos (#1760) — consumir a fila gerada por dispatchEvent ─────────
// `pumpEvents(domHandle)` — o NodeId do próximo evento pendente (-1 = fila vazia).
// Chame em laço; após cada chamada não-(-1), use `getLastEventType(domHandle)` para
// o tipo. O domHandle vem de `document._dom` (ou guarde-o).
//
// ⚠️ USO ATÔMICO: o tipo é guardado num slot único — SEMPRE chame getLastEventType
// IMEDIATAMENTE após o pumpEvents correspondente, sem intercalar pumpEvents de OUTRO
// dom no meio (senão o tipo lido pode ser do outro evento). Padrão seguro:
//   let n = pumpEvents(d); while (n !== -1) { const t = getLastEventType(d); /* usa n,t */ n = pumpEvents(d); }
function pumpEvents(domHandle: number): number {
  return dom.pollEvent(domHandle);
}
// O tipo do evento entregue pelo último `pumpEvents` ('' se nenhum).
function getLastEventType(domHandle: number): string {
  return dom.pollEventType(domHandle);
}


// ── Carregamento de recursos externos (CSS/script) ───────────────────────────────
// O HTML referencia arquivos externos: `<link rel=stylesheet href>`, `@import` no
// CSS, `<script src>`. O `rts-dom` (Rust) é um motor PURO — não conhece a tag
// `<link>` nem lê arquivos. A POLÍTICA ("o que carregar, de onde, quando") mora aqui
// em TS, fiel à doutrina do projeto (o Rust expõe só primitivos: `dom.addStylesheet`
// para ligar CSS à cascade, `fs.read_text`/`fetch`/`runtime.eval` para o I/O).
//
// Origem: arquivo local (`fs.read_text`) ou `http(s)://` (`fetch().text()`, caminho
// SÍNCRONO — sem await, para não depender do loop async). Resolução de URL relativa
// é feita contra um `baseUrl` (o diretório/URL do documento).
//
// ⚠️ Sentinela: `fs.read_text` devolve "" em erro; `fetch` pode falhar — tratamos
// como "recurso ausente" (segue sem ele, como o browser tolera).

// Junta um caminho-base com uma referência relativa. Absolutos (`http://`, `/...`,
// `C:\`) passam direto; relativos (`./x`, `x.css`, `../y`) resolvem contra a base.
function __resolveUrl(base: string, ref: string): string {
  if (ref.length === 0) return ref;
  // URL absoluta (tem esquema "xxx://") ou protocol-relative "//host".
  if (__hasScheme(ref) || (ref.charAt(0) === "/" && ref.charAt(1) === "/")) return ref;
  // Raiz absoluta local "/abs".
  if (ref.charAt(0) === "/") return ref;
  // Caminho absoluto Windows "C:\..." ou "C:/...".
  if (ref.length >= 2 && ref.charAt(1) === ":") return ref;
  if (base.length === 0) return ref;
  // Relativo: corta o último segmento da base (o "arquivo") e anexa a ref.
  const dir = __dirOf(base);
  return __normalizeUrl(dir + "/" + ref);
}

// `true` se a string começa com um esquema de URL ("http://", "file://", "data:").
function __hasScheme(s: string): boolean {
  let i = 0;
  while (i < s.length) {
    const c = s.charAt(i);
    const isAlpha = (c >= "a" && c <= "z") || (c >= "A" && c <= "Z");
    const isSchemeChar = isAlpha || (c >= "0" && c <= "9") || c === "+" || c === "-" || c === ".";
    if (c === ":") return i > 0;
    if (!isSchemeChar) return false;
    i = i + 1;
  }
  return false;
}

// O diretório de uma URL/caminho (tudo antes do último "/", ou "\" no Windows).
function __dirOf(p: string): string {
  let cut = -1;
  let i = 0;
  while (i < p.length) {
    const c = p.charAt(i);
    if (c === "/" || c === "\\") cut = i;
    i = i + 1;
  }
  if (cut < 0) return "";
  return p.substring(0, cut);
}

// Colapsa "." e ".." num caminho separado por "/". Preserva um prefixo de esquema.
//
// NOTA (motor): passar como índice de `string.substring`/`charAt` um valor vindo do
// RETORNO de uma função de usuário, sobre uma string-PARÂMETRO, falha no subset
// numérico do motor ("method arg wants a number index but got Tagged"): o tipo do
// retorno não é provado Int32. `Math.trunc(...)` força a prova e destrava (testado:
// `| 0`/`+0`/ternário NÃO bastam; só `Math.trunc`/`Math.max`). Aplicado a `cut` aqui
// e a `at` em `__hostPrefixLen`.
function __normalizeUrl(p: string): string {
  const cut = Math.trunc(__hostPrefixLen(p)); // "scheme://host" a preservar (0 se nenhum)
  const prefix = p.substring(0, cut);
  const rest = p.substring(cut);
  const parts = rest.split("/");
  const out: string[] = [];
  let i = 0;
  while (i < parts.length) {
    const seg = parts[i];
    if (seg === "." || seg === "") {
      // ignora (mantém vazio inicial via prefixo)
    } else if (seg === "..") {
      if (out.length > 0) out.pop();
    } else {
      out.push(seg);
    }
    i = i + 1;
  }
  const joined = out.join("/");
  if (prefix.length > 0) return prefix + "/" + joined;
  if (rest.charAt(0) === "/") return "/" + joined;
  return joined;
}

// Tamanho do prefixo "scheme://host" (até a "/" do path, exclusiva), ou 0 se a
// string não começa por um esquema seguido de "//". Função PURA — ver a nota em
// `__normalizeUrl` sobre por que o host NÃO é fatiado por mutação condicional lá.
function __hostPrefixLen(p: string): number {
  const at = Math.trunc(__schemeSlashSlash(p)); // índice após "://", ou -1 (trunc: ver __normalizeUrl)
  if (at < 0) return 0;
  let h = at;
  while (h < p.length && p.charAt(h) !== "/") h = h + 1;
  return h;
}

// Índice logo após "://" de uma URL com esquema, ou -1.
function __schemeSlashSlash(p: string): number {
  let i = 0;
  while (i + 2 < p.length) {
    if (p.charAt(i) === ":" && p.charAt(i + 1) === "/" && p.charAt(i + 2) === "/") {
      return i + 3;
    }
    i = i + 1;
  }
  return -1;
}

// Lê o conteúdo textual de uma URL/caminho. `http(s)://` via fetch síncrono; o resto
// via filesystem. "" se não conseguir (recurso ausente — tolerado).
function __readResource(url: string): string {
  if (url.length > 7 && url.substring(0, 7) === "http://") return __fetchText(url);
  if (url.length > 8 && url.substring(0, 8) === "https://") return __fetchText(url);
  // local: tira "file://" se houver.
  let path = url;
  if (path.length > 7 && path.substring(0, 7) === "file://") path = path.substring(7);
  return fs.read_text(path);
}

// fetch síncrono de texto (`rts:fetch`.fetchText — HTTP GET via ureq+TLS, o mesmo
// caminho do mini-browser). "" em erro (a convenção tolerante do __readResource).
function __fetchText(url: string): string {
  return fetch.fetchText(url);
}

// Expande os imports CSS (`url(...)` ou string) INLINE e recursivamente.
// Cada import é resolvido contra a base do CSS que o contém. `seen` corta ciclos;
// `depth` limita a profundidade (defesa contra recursão patológica).
// NOTA (motor): sem `break` (ver `__trimEnd`); o fim do laço é controlado por `i`,
// que salta para `n` quando não há mais `@import`.
function __inlineImports(css: string, base: string, seen: string[], depth: number): string {
  if (depth <= 0) return css;
  let out = "";
  let i = 0;
  const n = css.length;
  while (i < n) {
    // procura o próximo "@import"
    const at = css.indexOf("@import", i);
    if (at < 0) {
      // não há mais imports: copia o resto e encerra o laço (i := n).
      out = out + css.substring(i);
      i = n;
    } else {
      out = out + css.substring(i, at);
      // acha o fim da regra (";")
      let end = css.indexOf(";", at);
      if (end < 0) end = n;
      const rule = css.substring(at + 7, end); // depois de "@import"
      const ref = __parseImportRef(rule);
      if (ref.length > 0) {
        const abs = __resolveUrl(base, ref);
        if (!__includes(seen, abs)) {
          seen.push(abs);
          const imported = __readResource(abs);
          if (imported.length > 0) {
            out = out + __inlineImports(imported, abs, seen, depth - 1) + "\n";
          }
        }
      }
      i = end + 1;
    }
  }
  return out;
}

// Extrai a URL de uma regra `@import` (sem o "@import"): `url("x")`, `url(x)`, `"x"`.
//
// NOTA (motor): cada transformação vai para uma `const` nova — NÃO mutamos uma
// variável-string num `if`/`while` para depois indexá-la, senão o tipo degrada a
// `Tagged` (ver a nota em `__normalizeUrl`).
function __parseImportRef(rule: string): string {
  // 1) tira espaços iniciais.
  let a = 0;
  while (a < rule.length && (rule.charAt(a) === " " || rule.charAt(a) === "\t" || rule.charAt(a) === "\n")) {
    a = a + 1;
  }
  const trimmed = rule.substring(a);
  // 2) desembrulha `url(...)` se presente; senão usa a string como veio.
  const inner = __unwrapUrl(trimmed);
  // 3) tira aspas e espaço final.
  return __stripQuotes(__trimEnd(inner));
}

// Se `s` começa por `url(`, devolve o conteúdo até o `)`; senão devolve `s` igual.
// Função PURA (cada ramo retorna uma `const`/argumento, sem mutação reaproveitada).
function __unwrapUrl(s: string): string {
  if (s.length >= 4 && s.substring(0, 4).toLowerCase() === "url(") {
    const close = s.indexOf(")");
    if (close < 0) return "";
    return s.substring(4, close);
  }
  return s;
}

// NOTA (motor): sem `break`/`continue` — o subset numérico do motor não os aceita
// em laço (vira "method arg wants a number index but got Tagged" na compilação). O
// laço usa uma flag de parada (`going`) no lugar.
function __trimEnd(s: string): string {
  let e = s.length;
  let going = true;
  while (going && e > 0) {
    const c = s.charAt(e - 1);
    if (c === " " || c === "\t" || c === "\n" || c === "\r") e = e - 1;
    else going = false;
  }
  return s.substring(0, e);
}

function __stripQuotes(s: string): string {
  if (s.length >= 2) {
    const f = s.charAt(0);
    const l = s.charAt(s.length - 1);
    if ((f === '"' && l === '"') || (f === "'" && l === "'")) return s.substring(1, s.length - 1);
  }
  return s;
}

function __includes(arr: string[], v: string): boolean {
  let i = 0;
  while (i < arr.length) {
    if (arr[i] === v) return true;
    i = i + 1;
  }
  return false;
}

// `loadResources(doc, baseUrl)` — percorre o documento e carrega os recursos externos
// para dentro do DOM. Roda UMA VEZ após o parse, antes do primeiro render (nunca no
// loop de frame — handle de string não persiste entre frames). Faz:
//   • `<link rel="stylesheet" href>`  → lê o CSS, expande @import, injeta na cascade.
//   • `@import` no CSS externo        → inline recursivo (via __inlineImports).
//   • `<script src>`                  → carrega o fonte e o materializa como texto do
//                                       nó (NÃO executa — ver __loadScriptAt).
// `baseUrl` é o diretório/URL do documento, para resolver href/src relativos ("" se
// não tiver — aí só absolutos carregam). Devolve quantos recursos foram carregados.
//
// ORIGEM: arquivo LOCAL (`fs.read_text`) e `http(s)://` REAL (`fetch.fetchText`,
// namespace rts:fetch — HTTP GET síncrono via ureq+TLS, o mesmo do mini-browser).
// Um <link> pra CDN baixa o CSS de verdade. A execução dos <script> é a fase
// seguinte, `runScripts(doc)`.
// NOTA (motor): sem `continue` (ver `__trimEnd`); o corpo de cada item é uma função
// auxiliar (`__loadLinkAt`/`__loadScriptAt`) que devolve 1 (carregou) ou 0, e o laço
// só soma — assim a guarda-cláusula vira `return 0` na helper, não `continue`.
function loadResources(doc: Document, baseUrl: string): number {
  // `: i64` — handle via param `number` corrompe (fcvt vs bitcast, #1870).
  const h: i64 = doc._dom;
  let loaded = 0;

  // 1) <link rel="stylesheet" href> — usa getByTag (independe do parser de seletor).
  const linkCount = dom.getByTagCount(h, "link");
  let i = 0;
  while (i < linkCount) {
    loaded = loaded + __loadLinkAt(h, i, baseUrl);
    i = i + 1;
  }

  // 2) <script src> — carrega e executa (runtime.eval). Scripts inline (sem src)
  //    são preservados como nós mas NÃO executados aqui (eval só do src externo).
  const scriptCount = dom.getByTagCount(h, "script");
  let j = 0;
  while (j < scriptCount) {
    loaded = loaded + __loadScriptAt(h, j, baseUrl);
    j = j + 1;
  }

  return loaded;
}

// Carrega o i-ésimo `<link>` se for uma folha de estilo com `href`. Devolve 1/0.
function __loadLinkAt(h: i64, i: number, baseUrl: string): number {
  const node = dom.getByTagAt(h, "link", i);
  if (node === __DOM_NONE) return 0;
  const rel = dom.getAttribute(h, node, "rel").toLowerCase();
  if (rel.indexOf("stylesheet") < 0) return 0;
  const href = dom.getAttribute(h, node, "href");
  if (href.length === 0) return 0;
  const abs = __resolveUrl(baseUrl, href);
  const css = __readResource(abs);
  if (css.length === 0) return 0;
  const seen: string[] = [abs];
  const expanded = __inlineImports(css, abs, seen, 16);
  dom.addStylesheet(h, expanded);
  return 1;
}

// Carrega o fonte do j-ésimo `<script src>` e o injeta no DOM como texto do nó
// `<script>` (fica acessível via `el.textContent`), via `dom.runScript`. Devolve 1/0.
// Carregar ≠ executar: a execução é a fase seguinte, `runScripts(doc)` (abaixo),
// que compila cada `<script>` in-process via `new Function` (o eval do motor novo).
function __loadScriptAt(h: i64, j: number, baseUrl: string): number {
  const node = dom.getByTagAt(h, "script", j);
  if (node === __DOM_NONE) return 0;
  const src = dom.getAttribute(h, node, "src");
  if (src.length === 0) return 0;
  const abs = __resolveUrl(baseUrl, src);
  const code = __readResource(abs);
  if (code.length === 0) return 0;
  dom.runScript(h, node, code);
  return 1;
}

// Bomba de eventos do BACKEND (hit-test do mouse no egui): drena a fila de
// eventos CRUS (`pollRawEvent` — o backend só empurra "clicou no nó X") e faz o
// dispatch COMPLETO de cada um (bubbling + callbacks registrados + fila de
// polling legada). Chamar UMA vez por frame no loop do app:
//   while (win.isOpen()) { ... egui.render(...); pumpEventCallbacks(doc); }
// Devolve quantos eventos foram despachados no frame.
function pumpEventCallbacks(doc: Document): number {
  const h: i64 = doc._dom;
  let despachados = 0;
  let guard = 0;
  // guard: um callback pode re-despachar; 256 eventos/frame é teto sano.
  while (guard < 256) {
    const node = dom.pollRawEvent(h);
    if (node === __DOM_NONE) return despachados;
    const t = dom.pollRawEventType(h);
    __dispatchWithCallbacks(h, node, t, 1, 1);
    despachados = despachados + 1;
    guard = guard + 1;
  }
  return despachados;
}

// Bomba de teclado do BACKEND: drena as transições emitidas pelo renderer egui
// e entrega `keydown`/`keyup` ao target focado. Os getters de metadados devem ser
// lidos imediatamente depois do poll, antes de qualquer outro poll do documento.
function __pumpKeyboardEvents(doc: Document, applyDefault: number): number {
  const h: i64 = doc._dom;
  let despachados = 0;
  let guard = 0;
  while (guard < 256) {
    const node = dom.pollRawKeyboardEvent(h);
    if (node === __DOM_NONE) return despachados;
    const pressed = dom.rawKeyboardPressed(h);
    const repeat = dom.rawKeyboardRepeat(h);
    const keyCode = dom.rawKeyboardKey(h);
    const ctrl = dom.rawKeyboardCtrl(h);
    const shift = dom.rawKeyboardShift(h);
    const alt = dom.rawKeyboardAlt(h);
    const meta = dom.rawKeyboardMeta(h);
    const prevented = __dispatchKeyboardWithCallbacks(h, node, pressed, repeat, keyCode, ctrl, shift, alt, meta);

    // Backspace é a primeira acção padrão de edição ligada ao cancelamento DOM.
    // O evento beforeinput ocorre depois de keydown e antes da mutação do value.
    if (applyDefault !== 0 && pressed !== 0 && keyCode === 4 && prevented === 0) {
      const focused = dom.focusedInput(h);
      if (focused !== __DOM_NONE) {
        const before = __dispatchInputCallbacks(h, focused, "beforeinput", "", "deleteContentBackward", 0, 1);
        if (before === 0 && dom.inputBackspaceAt(h, focused) !== 0) {
          __dispatchInputCallbacks(h, focused, "input", "", "deleteContentBackward", 0, 1);
        }
      }
    }
    despachados = despachados + 1;
    guard = guard + 1;
  }
  return despachados;
}

function pumpKeyboardEvents(doc: Document): number {
  return __pumpKeyboardEvents(doc, 0);
}

// Pump orientado a browser: além de keydown/keyup, entrega composição e texto
// editado. A fila raw mantém a ordem e o alvo capturado pelo backend; o parâmetro
// de janela não é necessário nesta fronteira.
function clearInputValue(doc: Document, node: number): number {
  const h: i64 = doc._dom;
  let deleted = 0;
  while (dom.inputValue(h, node).length > 0 && deleted < 300) {
    const before = __dispatchInputCallbacks(h, node, "beforeinput", "", "deleteContentBackward", 0, 1);
    if (before !== 0 || dom.inputBackspaceAt(h, node) === 0) break;
    __dispatchInputCallbacks(h, node, "input", "", "deleteContentBackward", 0, 1);
    deleted = deleted + 1;
  }
  return deleted;
}

function pumpInputEvents(doc: Document): number {
  const h: i64 = doc._dom;
  let dispatched = __pumpKeyboardEvents(doc, 1);
  let composing = __compositionStates.get(h) === 1 ? 1 : 0;
  let guard = 0;
  while (guard < 256) {
    const node = dom.pollRawInputEvent(h);
    if (node === __DOM_NONE) break;
    const kind = dom.rawInputKind(h);
    const data = dom.rawInputText(h);
    if (kind === 1) {
      const inputType = composing !== 0 ? "insertCompositionText" : "insertText";
      const before = __dispatchInputCallbacks(h, node, "beforeinput", data, inputType, composing, 1);
      if (before === 0 && dom.inputFeedTextAt(h, node, data) !== 0) {
        __dispatchInputCallbacks(h, node, "input", data, inputType, composing, 1);
      }
      dispatched = dispatched + 1;
    } else if (kind === 2) {
      __dispatchInputCallbacks(h, node, "compositionstart", data, "", 0, 1);
      composing = 1;
      dispatched = dispatched + 1;
    } else if (kind === 3) {
      composing = 1;
      __dispatchInputCallbacks(h, node, "compositionupdate", data, "", 1, 1);
      dispatched = dispatched + 1;
    } else if (kind === 4) {
      composing = 0;
      __dispatchInputCallbacks(h, node, "compositionend", data, "", 0, 1);
      dispatched = dispatched + 1;
    } else if (kind === 5) {
      if (composing !== 0) {
        composing = 0;
        __dispatchInputCallbacks(h, node, "compositionend", "", "", 0, 1);
        dispatched = dispatched + 1;
      }
    }
    guard = guard + 1;
  }
  __compositionStates.set(h, composing);
  return dispatched;
}

// Bomba de TIMERS da página: dispara os `setTimeout`/`setInterval` que os
// `<script>` agendaram (fila POR DOCUMENTO em Rust — `DomTimers`; o relógio é
// medido lá). Chamar uma vez por frame, ao lado de `pumpEventCallbacks`:
//   while (win.isOpen()) { ... pumpEventCallbacks(doc); pumpTimerCallbacks(doc); }
// Um interval re-arma ao disparar; o teto de 64/frame impede um interval de 1ms
// de travar o loop. Devolve quantos callbacks dispararam no frame.
function pumpTimerCallbacks(doc: Document): number {
  const h: i64 = doc._dom;
  // A volta do loop do MOTOR primeiro, e não só a fila de timers deste
  // documento. São duas filas e uma página precisa das duas: um `.then`, um
  // `queueMicrotask` e uma mensagem de `MessageChannel` viajam pela do motor.
  //
  // Sem isto, um framework concurrent nunca avança numa JANELA — e avança
  // headless, porque aí o programa de topo tem `await` e drena o motor por
  // acidente. Medido com o React 18: montava headless e deixava o `#root`
  // vazio na janela, com os mesmos scripts a correr sem um erro.
  //
  // O nome desta função fala de timers e ela passou a fazer mais do que isso.
  // Fica assim porque o que ela É continua a ser uma coisa só — *uma volta do
  // loop desta página* — e quem a chama, o frame do host, quer exatamente
  // isso; dividi-la em duas obrigaria todo o chamador a saber que há duas
  // filas, que é o conhecimento que esta função existe para não espalhar.
  engine.run_event_loop();
  let disparados = 0;
  let guard = 0;
  while (guard < 64) {
    const f = DomTimers.takeDue(h);
    if (f === undefined) return disparados;
    f();
    disparados = disparados + 1;
    guard = guard + 1;
  }
  return disparados;
}

// ── Execução de <script> — o "bloco JS" da página ──────────────────────────────
//
// `runScripts(doc)` compila e roda cada `<script>` do documento, em ordem de
// documento, IN-PROCESS: o corpo vai pelo `new Function` (pipeline swc→HIR→JIT do
// motor, hook COMPILE_FN_HOOK/dynfn.rs) e roda na MESMA thread — o store de DOMs é
// thread_local no Rust, então o script enxerga o MESMO documento. A ponte é 100%
// dados: prefixamos `const document = new Document(__h)` no corpo e passamos o
// handle como argumento. O programa aninhado inclui os mesmos preludes, então as
// classes Document/Element existem lá com a mesma API.
//
// Limites HONESTOS do subset dinâmico (documentados, não silenciosos):
//   • GETTER/SETTER de classe despacham também no caminho dinâmico (o prólogo do
//     motor registra `__get_/__set_<prop>` no proto — `el.textContent = x` chama o
//     setter REAL, como no browser).
//   • O retorno do dynfn é i64 — devolver string do script não sobrevive à borda.
//     Efeitos no DOM (o caso real de <script>) funcionam; "return de valor" não é
//     o contrato de um bloco de página mesmo.
//   • Sem `async`/`await` no corpo (limite do new Function herdado do motor).
// Erro de compilação/execução de um script NÃO derruba os demais (isolamento por
// try/catch, como no browser). Devolve quantos scripts rodaram com sucesso.
function runScripts(doc: Document): number {
  return runScriptsAt(doc, "https://localhost/");
}

// Como `runScripts`, mas com a URL da página (vira `window.location`). Use quando
// o browser sabe a URL de origem (o `window.location.href`/`origin` dos scripts).
function runScriptsAt(doc: Document, url: string): number {
  // O DOC (não o handle solto) é o que atravessa: `const h: i64 = doc._dom`
  // TRUNCA — medido, o campo carrega `281474976710656` e a variável fica com
  // `1`. Os dois valores endereçam o mesmo DOM nas fns de namespace, mas viram
  // CHAVES DIFERENTES no saco de globais: o script publicava `__d` sob uma e o
  // script seguinte procurava sob a outra, e os 9 bundles da Meta morriam em
  // "call to unknown function `__d`". Enquanto #1870 estiver aberto, passe o
  // Document.
  let ran = 0;
  const scriptCount = dom.getByTagCount(doc._dom, "script");
  let j = 0;
  while (j < scriptCount) {
    ran = ran + __runScriptAt(doc, j, url);
    j = j + 1;
  }
  // Fecha o TASK da página: drena microtasks/timers que os scripts enfileiraram.
  // Sem isto, um `.then`/`queueMicrotask` registrado por um `<script>` ficava na
  // fila para sempre — o callback nunca acontecia, sem erro nenhum.
  //
  // Fecha o TASK da página: uma volta do loop desta página, que drena as
  // microtasks do motor e a fila de timers do documento.
  //
  // UMA, e não em laço. Um laço esteve aqui duas vezes e saiu as duas, pela
  // mesma razão medida: com 64 voltas o React 18 continuava a não montar, e o
  // que o fez montar foi outra coisa — um `await` no programa de topo, que
  // SUSPENDE e devolve o controlo ao host. `run_event_loop()` chamado de dentro
  // do programa não é equivalente a isso, e o laço só repetia o que já não
  // chegava.
  //
  // Quem precisa de mais voltas tem duas formas honestas de as ter: um `await`
  // no programa, ou o frame do host, que bombeia a cada passagem. Nenhuma delas
  // é este laço.
  engine.run_event_loop();
  pumpTimerCallbacks(doc);
  // ISOLA como o console do browser: um callback de terceiro que LANÇA não pode
  // derrubar a página inteira. O erro vive no slot do MOTOR — um canal lateral
  // que um `try/catch` de `.ts` não observa —, então é preciso consumi-lo
  // explicitamente. Reporta e segue, que é o que o browser faz.
  const __erroMicro = engine.take_error();
  if (__erroMicro !== undefined) {
    console.error("[page] erro em microtask: " + __erroMicro);
  }
  return ran;
}

// Publica o ambiente de browser no escopo do documento, uma vez.
//
// Estes nomes eram `const` injetadas no topo de CADA script por um prólogo. Como
// propriedades do escopo são a mesma coisa dita onde um browser a diz — o global
// object — e param de ser recompiladas por script. `window` marca que já foi
// feito: é o primeiro a entrar e nenhum script legítimo o apaga.
function __prepararEscopo(doc: Document, url: string): void {
  const w: any = __winFor(doc._dom, url, 1000, 800);
  // Uma linha, e é a linha toda: o escopo dos `<script>` deste documento É o
  // `window`. Num browser são o mesmo objeto, e um bundle UMD depende disso —
  // o ramo de browser faz `factory(global.React = {})` e o script seguinte lê
  // `React` como nome livre.
  //
  // O que estava aqui antes publicava `window`, `document`, `self`,
  // `globalThis`, `top`, `parent`, `location`, `navigator`, `history`,
  // `localStorage` e cinco timers COMO PROPRIEDADES de um saco à parte. Eram
  // todos duplicados: a classe `WindowImpl` já os tem, e o saco à parte era o
  // que fazia `window.X = 42` num script e `typeof X` no seguinte responder
  // `"undefined"` — dois objetos onde a linguagem tem um.
  DomScope.adopt(doc._dom, w);
}

// Roda o j-ésimo `<script>`: inline usa o texto do nó; externo usa o fonte que o
// `backgroundColor` -> `background-color`. Um nome ja em kebab passa intacto, e
// um `--custom` tambem: a especificacao diz que uma propriedade customizada e
// usada tal e qual, e as maiusculas dentro dela sao significativas.
function __cssKebab(nome: string): string {
  if (nome.length > 1 && nome.charAt(0) === "-" && nome.charAt(1) === "-") return nome;
  let out = "";
  let i = 0;
  while (i < nome.length) {
    const c = nome.charAt(i);
    const baixo = c.toLowerCase();
    if (c !== baixo) { out = out + "-" + baixo; } else { out = out + c; }
    i = i + 1;
  }
  return out;
}

// `loadResources` materializou no nó (mesmo caminho). Devolve 1 (rodou) ou 0.
function __runScriptAt(doc: Document, j: number, url: string): number {
  // Recebe o DOCUMENT, não o handle: `doc._dom` numa variável `i64` trunca
  // (#1870) e o saco de globais passa a ser chaveado por dois valores.
  const node = dom.getByTagAt(doc._dom, "script", j);
  if (node === __DOM_NONE) return 0;
  // Só executa JAVASCRIPT: `type` vazio/`text|application/javascript`/`module`.
  // `type="application/json"` (dados de config — o WhatsApp/Meta usa MUITO) e
  // outros tipos NÃO são código; executá-los gerava `syntax error` em massa.
  const st = dom.getAttribute(doc._dom, node, "type").toLowerCase();
  const isJs = st.length === 0 || st === "text/javascript"
    || st === "application/javascript" || st === "module"
    || st === "application/x-javascript" || st === "text/ecmascript";
  if (!isJs) return 0;
  // O CÓDIGO vem do texto inline OU de um `src=data:...;base64,<b64>` (o
  // WhatsApp/Meta embutem quase todo o JS assim). data-URI base64 é decodificado
  // via `atob`; data-URI de texto puro (`data:...,<code>`) usa o payload direto.
  // `src=http(s)` externo NÃO é baixado aqui (o extractSite do browser decide).
  let code = dom.getText(doc._dom, node);
  const src = dom.getAttribute(doc._dom, node, "src");
  if (src.length > 5 && src.substring(0, 5) === "data:") {
    const comma = src.indexOf(",");
    if (comma < 0) return 0;
    const meta = src.substring(0, comma);
    const payload = src.substring(comma + 1);
    code = meta.indexOf("base64") >= 0 ? atob(payload) : decodeURIComponent(payload);
  }
  if (code.length === 0) return 0;
  __prepararEscopo(doc, url);
  // E o texto vai INTEIRO e COMO VEIO. Nada de `__normalizeScript`, nada de
  // `__bindGlobals`: o compilador resolve os nomes livres contra o escopo (a
  // porta `emit::page`), e as declarações de topo assentam nele para o script
  // seguinte as ler — que é o que a ECMA-262 §16.1.7 diz de script code.
  //
  // O que isto apaga é uma classe inteira de divergências: enquanto o texto era
  // reescrito, o motor compilava um programa que a página não serviu, e cada
  // caso que a varredura não via era um nome perdido em silêncio.
  const janela = __winFor(doc._dom, url, 1000, 800);
  const ok = DomScope.run(doc._dom, code, janela);
  // Reportado como um browser reporta: no console, com a origem, e a página
  // segue. O silêncio é o que faz um script morto parecer um script vazio.
  if (ok === 0) {
    const porque = DomScope.lastError(doc._dom);
    console.error("[page] <script> " + j + " de " + url + " falhou: " + porque);
  }
  return ok;
}
