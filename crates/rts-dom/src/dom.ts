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

// Despacho de evento com CALLBACKS: coleta os pares (nó, fn-word) do Rust
// (`dispatchCollect` — alvo primeiro, depois bubbling), COPIA tudo para arrays
// locais (um callback pode re-despachar e sobrescrever o scratch do Dom) e invoca
// cada fn com um objeto de evento `{type, target, currentTarget}` (subset do Event
// do browser). Devolve o total notificado (callbacks coletados; a fila de polling
// legada também é alimentada pelo mesmo dispatchCollect).
function __dispatchWithCallbacks(h: i64, node: number, type: string, bubbles: number): number {
  const n = dom.dispatchCollect(h, node, type, bubbles);
  if (n === 0) return 0;
  const cbs: number[] = [];
  const nodes: number[] = [];
  let i = 0;
  while (i < n) {
    cbs.push(dom.dispatchCbAt(h, i));
    nodes.push(dom.dispatchCbNode(h, i));
    i = i + 1;
  }
  const target = new Element(h, node);
  let j = 0;
  while (j < n) {
    const cur = new Element(h, nodes[j]);
    // engine.invoke_cb: o cb atravessou a borda I64 da ABI (vira número); a
    // bridge re-taggeia para função e invoca com 1 argumento (o objeto Event).
    engine.invoke_cb(cbs[j], { type: type, target: target, currentTarget: cur });
    j = j + 1;
  }
  return n;
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

  // `el.innerHTML` — GET serializa os filhos. O jeito #1 de mexer no DOM em apps.
  get innerHTML(): string {
    return dom.innerHtml(this._dom, this._node);
  }
  // ⚠️ O SET é via MÉTODO `setInnerHTML(html)`, não o setter `el.innerHTML = ...`:
  // o motor RTS atual não dispara setters de propriedade de classe (o `app.x = v`
  // não chama `set x()` — cria um campo no objeto). Métodos despacham certo. Quando
  // o motor suportar setters, o `set innerHTML` volta. (mesmo motivo de classList*
  // serem métodos, não um objeto `.classList` vivo.)
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
  addEventListener(type: string, cb?: any): void {
    if (cb === undefined) {
      dom.addListener(this._dom, this._node, type);
      return;
    }
    dom.addListenerCb(this._dom, this._node, type, cb);
  }
  removeEventListener(type: string): void {
    dom.removeListener(this._dom, this._node, type);
  }
  // `el.dispatchEvent(type)` — dispara COM BUBBLING (como `new Event(t, {bubbles:
  // true})`): invoca os callbacks registrados (alvo → ancestrais) e alimenta a fila
  // de polling legada. Devolve quantos listeners (callbacks + polling) notificou.
  dispatchEvent(type: string): number {
    return __dispatchWithCallbacks(this._dom, this._node, type, 1);
  }
  // `el.dispatchEventNoBubble(type)` — dispara SÓ no alvo (como `new Event(t)`, que
  // é bubbles:false por padrão; focus/blur/mouseenter não borbulham).
  dispatchEventNoBubble(type: string): number {
    return __dispatchWithCallbacks(this._dom, this._node, type, 0);
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

  // `document.getElementById(id)` — atalho para `#id`.
  getElementById(id: string): Element | null {
    return this.querySelector("#" + id);
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

  // `document.documentElement` — a raiz `#document`.
  get documentElement(): Element {
    const root = dom.rootId(this._dom);
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

// Expande os `@import url(...)` / `@import "..."` de um CSS, INLINE e recursivamente.
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
    __dispatchWithCallbacks(h, node, t, 1);
    despachados = despachados + 1;
    guard = guard + 1;
  }
  return despachados;
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
  // `: i64` no handle e no param da helper: handle via param `number` corrompe
  // (fcvt vs bitcast — issue #1870 do motor).
  const h: i64 = doc._dom;
  let ran = 0;
  const scriptCount = dom.getByTagCount(h, "script");
  let j = 0;
  while (j < scriptCount) {
    ran = ran + __runScriptAt(h, j, url);
    j = j + 1;
  }
  return ran;
}

// Roda o j-ésimo `<script>`: inline usa o texto do nó; externo usa o fonte que o
// `loadResources` materializou no nó (mesmo caminho). Devolve 1 (rodou) ou 0.
function __runScriptAt(h: i64, j: number, url: string): number {
  const node = dom.getByTagAt(h, "script", j);
  if (node === __DOM_NONE) return 0;
  // Só executa JAVASCRIPT: `type` vazio/`text|application/javascript`/`module`.
  // `type="application/json"` (dados de config — o WhatsApp/Meta usa MUITO) e
  // outros tipos NÃO são código; executá-los gerava `syntax error` em massa.
  const st = dom.getAttribute(h, node, "type").toLowerCase();
  const isJs = st.length === 0 || st === "text/javascript"
    || st === "application/javascript" || st === "module"
    || st === "application/x-javascript" || st === "text/ecmascript";
  if (!isJs) return 0;
  // O CÓDIGO vem do texto inline OU de um `src=data:...;base64,<b64>` (o
  // WhatsApp/Meta embutem quase todo o JS assim). data-URI base64 é decodificado
  // via `atob`; data-URI de texto puro (`data:...,<code>`) usa o payload direto.
  // `src=http(s)` externo NÃO é baixado aqui (o extractSite do browser decide).
  let code = dom.getText(h, node);
  const src = dom.getAttribute(h, node, "src");
  if (src.length > 5 && src.substring(0, 5) === "data:") {
    const comma = src.indexOf(",");
    if (comma < 0) return 0;
    const meta = src.substring(0, comma);
    const payload = src.substring(comma + 1);
    code = meta.indexOf("base64") >= 0 ? atob(payload) : decodeURIComponent(payload);
  }
  if (code.length === 0) return 0;
  // Injeta o AMBIENTE de browser no corpo: `window` (location/navigator/history/
  // storage/timers) + `document` (do window) + os aliases globais que os scripts
  // esperam (self/globalThis/top/parent === window). `return 0` final garante o
  // dynfn tipado i64 (o corpo de um <script> normalmente não tem return).
  const prologue = "const window = __makeWindow(__h, __url, 1000, 800);\n"
    + "const document = window.document;\n"
    + "const self = window; const globalThis = window; const top = window; const parent = window;\n"
    + "const location = window.location; const navigator = window.navigator;\n"
    + "const history = window.history; const localStorage = window.localStorage;\n";
  const body = prologue + code + "\nreturn 0;";
  let ok = 1;
  try {
    const f = new Function("__h", "__url", body);
    f(h, url);
  } catch (e) {
    // Script quebrado não derruba a página (comportamento do browser).
    ok = 0;
  }
  return ok;
}
