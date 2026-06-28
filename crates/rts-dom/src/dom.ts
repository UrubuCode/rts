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
  // (regra 2). NOTE: o primitivo busca na árvore inteira; refino por subárvore
  // chega quando os seletores compostos chegarem.
  querySelector(sel: string): Element | null {
    const n = dom.querySelector(this._dom, sel);
    if (n === __DOM_NONE) return null;
    return new Element(this._dom, n);
  }

  // `el.querySelectorAll(sel)` — todos os que casam, como array. Montado via
  // count+at (evita retorno de array do Rust).
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
