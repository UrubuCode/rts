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

// -- Identidade de no: um no, UM objeto ------------------------------------
//
// `getElementById("b")` chamado duas vezes tem de responder o MESMO objeto, e
// o que se escreveu nele tem de continuar la. Nao e purismo: toda a biblioteca
// que ANOTA nos assume-o — o React guarda o fiber em `no.__reactFiber$xyz` e
// vai busca-lo por `event.target.__reactFiber$xyz`, o jQuery guarda ali o cache
// de dados, o D3 o `__data__`. Com um wrapper novo a cada acesso escreve-se num
// objeto e le-se noutro, e o sintoma nao se parece nada com a causa: uma app que
// MONTA, PINTA e nao responde a um unico clique — foi o que o React 18 fez aqui.
//
// A chave e o par (handle do DOM, `NodeId`), e nao so o `NodeId`, porque dois
// documentos abertos ao mesmo tempo tem cada um a sua arena e os `idx` colidem.
//
// NAO precisa de invalidacao, e a razao esta do lado Rust em vez de aqui — e
// mudou de forma no lote M (ciclo de vida do no): ate la, `arvore.rs` so
// fazia `nodes.push` e um `idx` nunca era reciclado, entao a chave (h, abi)
// era estavel para sempre. Agora `dom.releaseSubtree` PODE reciclar um `idx`
// desanexado sem wrapper vivo — mas a GERACAO passou a ser POR NO
// (`dom/freelist.rs`), entao reciclar incrementa so a geracao DESSE `idx`, o
// `NodeId` empacotado `(generation << 32) | idx` muda, e a chave abi deste
// mapa muda com ele. Uma entrada VELHA fica presa a um abi que nenhum `idx`
// reciclado volta a produzir — nao e lida por acidente, so deixa de ser
// escrita: e por isso que `removeChild`/`remove()` so chamam
// `dom.releaseSubtree` quando NENHUM no da subarvore tem wrapper (ver
// `__maybeReleaseSubtree` abaixo) — reciclar um `idx` com wrapper vivo
// deixaria esse wrapper a apontar, pela chave antiga, para um no que ja nao
// existe (a proxima leitura por esse wrapper resolve a `None` no Rust, mas o
// objeto TS continuaria "vivo" e mudo). O que a cache ainda custa e nao
// reciclar quando ha um wrapper vivo na subarvore — o mesmo no fica "lixo"
// na arena, como antes do lote M, ate esse wrapper deixar de existir.
const __wrappers: Map<i64, Map<i64, any>> = new Map();

// O wrapper DESTE no, sempre o mesmo. Todo o `dom.ts` passa por aqui em vez de
// `new Element`: uma unica chamada em falta reintroduz o defeito exactamente no
// caminho que a esqueceu, e um caminho desses e invisivel a quem le o resto.
function __elem(h: i64, node: number): Element {
  let daArvore = __wrappers.get(h);
  if (daArvore === undefined) {
    daArvore = new Map();
    __wrappers.set(h, daArvore);
  }
  const visto = daArvore.get(node);
  if (visto !== undefined) return visto;
  const novo = new Element(h, node);
  daArvore.set(node, novo);
  return novo;
}

// `true` se `node` (abi, NÃO desempacotado) tem wrapper vivo em `__wrappers`
// — a pergunta que só o TS sabe responder (§4.M: o Rust não vê este mapa).
function __wrapperExists(h: i64, node: number): boolean {
  const daArvore = __wrappers.get(h);
  if (daArvore === undefined) return false;
  return daArvore.get(node) !== undefined;
}

// `true` se `node` OU algum descendente dele tem wrapper vivo. Só chamado no
// caminho de remoção (abaixo) — percorrer a subárvore tem custo, mas é o que
// torna seguro reciclar: reciclar com um wrapper vivo lá dentro deixaria esse
// wrapper apontando para um `idx` de geração errada (o comentário de
// `__wrappers` acima tem o porquê).
function __subtreeHasWrapper(h: i64, node: number): boolean {
  if (__wrapperExists(h, node)) return true;
  const n = dom.childNodesCount(h, node);
  let i = 0;
  while (i < n) {
    if (__subtreeHasWrapper(h, dom.childNodeAt(h, node, i))) return true;
    i = i + 1;
  }
  return false;
}

// Chamado DEPOIS de `node` já estar desanexado (`remove()`/`removeChild` já
// correram). Esquece o CACHE deste nó — não o objeto JS: quem já tinha o
// wrapper (`el`) continua com o MESMO objeto e os mesmos campos `_dom`/
// `_node`, só deixa de ser o que uma consulta futura por este `NodeId`
// devolveria (que agora, sendo o nó removido, teria de criar um novo mesmo
// assim). É isto que torna seguro reciclar mesmo quando um wrapper foi
// "guardado" pelo chamador: reciclar não invalida o OBJETO, só o `NodeId`
// que ele carrega — uma leitura por ele passa a `resolve` a `None` do lado
// Rust e responde vazio/`false` em vez de lançar (ver `get isConnected`
// acima e `dom-bridge`, que já devolvem default em vez de entrar em pânico
// para um `NodeId` que não resolve).
//
// Recicla a subárvore inteira via `dom.releaseSubtree` só se NENHUM
// DESCENDENTE ainda tem wrapper em cache — reciclar com um filho em cache
// deixaria ESSE wrapper (que continua um objeto JS válido para quem o
// segura) a apontar para um `idx` que já não é dele.
//
// O que isto NÃO fecha: um wrapper cujo único dono é o `__wrappers` (nunca
// lido de novo, nunca solto por ninguém) — hoje esse caso já não acontece,
// porque esta função sempre esquece a ENTRADA do nó que ela própria recebe.
// O caso que ficaria por fechar é justamente o oposto — reciclar cedo demais
// um nó com wrapper vivo — que a checagem de subárvore acima já cobre.
// `WeakRef`/`FinalizationRegistry` não substituem nada disto: verificado
// nesta sessão (2026-09-04) que `WeakRef.deref()` neste motor NUNCA
// devolve `undefined` — o alvo é guardado como propriedade própria comum
// (`crates/rts-core/.../weakref.rs`), então pôr um wrapper atrás de um
// `WeakRef` no cache não o tornaria coletável, só adicionaria indireção.
function __maybeReleaseSubtree(h: i64, node: number): void {
  const daArvore = __wrappers.get(h);
  if (daArvore !== undefined) daArvore.delete(node);
  if (!__subtreeHasWrapper(h, node)) {
    dom.releaseSubtree(h, node);
  }
}

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
  const target = __elem(h, node);
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
    // Quando o evento aconteceu. Um browser dá milissegundos desde o início do
    // documento; aqui é o relógio do sistema, que serve para o que um programa
    // faz com isto — comparar dois eventos.
    timeStamp: Date.now(),
  };
  // `nativeEvent` — o evento "de baixo", e aqui é ESTE.
  //
  // Num browser há dois objetos: o nativo, que o motor cria, e o SINTÉTICO, que
  // uma biblioteca embrulha à volta dele para normalizar diferenças entre
  // browsers. O React lê `event.nativeEvent` para chegar ao primeiro, e é assim
  // que o seu sistema de eventos delegados encontra o alvo real.
  //
  // Aqui não há dois: este objeto é o nativo. Apontá-lo a si próprio é dizer
  // isso — e não é um atalho, é a resposta certa quando a camada que ele
  // procura não existe. Sem isto, o React lia `undefined` e nenhum `onClick`
  // disparava, sem um erro.
  //
  // Depois do literal e não dentro dele, porque uma propriedade não se pode
  // referir ao objeto que ainda está a ser construído.
  event.nativeEvent = event;
  let j = 0;
  while (j < n) {
    event.currentTarget = __elem(h, nodes[j]);
    state.passive = passives[j] !== 0 ? 1 : 0;
    event.eventPhase = nodes[j] === node ? 2 : (captures[j] !== 0 ? 1 : 3);
    // `engine.invoke_cb` reconstitui o Function word no runtime e chama o
    // listener com o mesmo objecto de evento mutável.
    engine.invoke_cb(cbs[j], event, event.currentTarget);
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
  const target = __elem(h, node);
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
    event.currentTarget = __elem(h, nodes[j]);
    state.passive = passives[j] !== 0 ? 1 : 0;
    event.eventPhase = nodes[j] === node ? 2 : (captures[j] !== 0 ? 1 : 3);
    engine.invoke_cb(cbs[j], event, event.currentTarget);
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
  const target = __elem(h, node);
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
    event.currentTarget = __elem(h, nodes[j]);
    state.passive = passives[j] !== 0 ? 1 : 0;
    event.eventPhase = nodes[j] === node ? 2 : (captures[j] !== 0 ? 1 : 3);
    engine.invoke_cb(cbs[j], event, event.currentTarget);
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
    return __elem(this._dom, n);
  }

  // `el.querySelectorAll(sel)` — todos os DESCENDENTES que casam (subárvore).
  querySelectorAll(sel: string): Element[] {
    const out: Element[] = [];
    const n = dom.queryAllWithinCount(this._dom, this._node, sel);
    let i = 0;
    while (i < n) {
      out.push(__elem(this._dom, dom.queryAllWithinAt(this._dom, this._node, sel, i)));
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
      out.push(__elem(this._dom, node));
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
      out.push(__elem(this._dom, node));
      i = i + 1;
    }
    return out;
  }

  // `el.isConnected` — sobe por `parentNode` até achar a raiz `#document`
  // (`dom.rootId`) ou ficar sem pai. Não precisa de um primitivo novo: é a
  // mesma travessia que `is_attached` faz do lado Rust (`consulta.rs`), só
  // que essa é privada ao crate — refazê-la aqui em 2 chamadas existentes é
  // mais barato do que expor mais um membro do bridge para isto (§4.M).
  // `false` depois de `remove()`/`releaseSubtree`: um `NodeId` que já não
  // resolve (geração reciclada) faz `dom.parentNode` responder `-1` na
  // primeira volta, tal como um nó solto sem pai — sem `TypeError`.
  get isConnected(): boolean {
    const root = dom.rootId(this._dom);
    let cur: number = this._node;
    while (cur !== __DOM_NONE) {
      if (cur === root) return true;
      cur = dom.parentNode(this._dom, cur);
    }
    return false;
  }

  // ── Navegação (parentNode / first|lastChild / next|previousSibling) ──────────
  // Getters que devolvem `Element | null` (null no fim/sem pai). Extrair o NodeId
  // para uma const antes de comparar com -1 (limite do motor i64-cmp inline).
  get parentNode(): Element | null {
    const n = dom.parentNode(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get firstChild(): Element | null {
    const n = dom.firstChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get lastChild(): Element | null {
    const n = dom.lastChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get nextSibling(): Element | null {
    const n = dom.nextSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get previousSibling(): Element | null {
    const n = dom.previousSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }

  // ── Traversal POR ELEMENTO (#1757) — pula nós de texto/comentário ────────────
  get firstElementChild(): Element | null {
    const n = dom.firstElementChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get lastElementChild(): Element | null {
    const n = dom.lastElementChild(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get nextElementSibling(): Element | null {
    const n = dom.nextElementSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }
  get previousElementSibling(): Element | null {
    const n = dom.previousElementSibling(this._dom, this._node);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
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
    return __elem(this._dom, n);
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
    return __elem(this._dom, n);
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
    return __elem(this._dom, n);
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
    __maybeReleaseSubtree(this._dom, child._node);
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
  // -- O que um controlo de formulario expoe -------------------------------
  //
  // `el.value` LE o valor editado e ESCREVE por cima dele. Sao os dois lados de
  // uma so propriedade e nenhum se escrevia com o outro: a leitura respondia
  // `undefined` — o `getAttribute("value")` da o valor INICIAL do HTML, nao o
  // que o utilizador digitou — e a escrita nao tinha caminho nenhum, porque o
  // vocabulario do DOM era alimentar tecla a tecla.
  //
  // Sem isto nao ha formulario: nem ler o que se digitou, nem limpar o campo
  // depois de submeter (`el.value = ""`), nem um controlled input, que e como
  // o React e o Preact fazem TODOS os campos.
  get value(): string {
    return dom.inputValue(this._dom, this._node);
  }
  set value(v: string) {
    dom.setInputValue(this._dom, this._node, v);
  }
  // `el.focus()` / `el.blur()` — para onde vai a proxima tecla. O foco e do
  // DOCUMENTO e nao do no, entao o `blur` so o larga se for este que o tem:
  // um `blur` num elemento qualquer nao pode desfocar outro.
  focus(): void {
    dom.focusInput(this._dom, this._node);
  }
  blur(): void {
    if (dom.focusedInput(this._dom) === this._node) {
      dom.focusInput(this._dom, __DOM_NONE);
    }
  }
  // `el.click()` — dispara um clique como se o rato o tivesse dado, COM
  // bubbling, que e o que a spec diz e o que um `<button>` submetido por
  // programa precisa.
  click(): void {
    this.dispatchEvent("click");
  }

  // -- Os `on<evento>` como PROPRIEDADE ------------------------------------
  //
  // `el.onclick = fn` regista; `el.onclick` responde o que foi registado. Sao
  // acessores e nao dados porque a spec diz que a atribuicao REGISTA — e porque
  // um `in` sobre eles tem de responder `true` mesmo antes de alguem escrever.
  //
  // Essa ultima e a razao de existirem, e nao a ergonomia. O Preact escolhe o
  // nome do evento assim:
  //
  //     l = l.toLowerCase() in n ? l.toLowerCase().slice(2) : l.slice(2)
  //
  // Sem `onclick` no elemento o `in` responde `false`, e ele regista **"Click"**
  // com maiuscula — um tipo que nada despacha. A aplicacao monta, pinta, e
  // nenhum `onClick` dispara; nao ha erro nenhum, porque do ponto de vista do
  // Preact o registo correu bem.
  //
  // Sao os seis que um `in` desta forma consulta na pratica. Um `on*` que aqui
  // nao esteja volta a cair no ramo errado, e a lista cresce quando alguem
  // medir que falta — em vez de setenta acessores escritos por precaucao.
  get onclick(): any { return this.__on("click"); }
  set onclick(fn: any) { this.__setOn("click", fn); }
  get oninput(): any { return this.__on("input"); }
  set oninput(fn: any) { this.__setOn("input", fn); }
  get onchange(): any { return this.__on("change"); }
  set onchange(fn: any) { this.__setOn("change", fn); }
  get onkeydown(): any { return this.__on("keydown"); }
  set onkeydown(fn: any) { this.__setOn("keydown", fn); }
  get onkeyup(): any { return this.__on("keyup"); }
  set onkeyup(fn: any) { this.__setOn("keyup", fn); }
  get onsubmit(): any { return this.__on("submit"); }
  set onsubmit(fn: any) { this.__setOn("submit", fn); }

  // O par por tras dos acessores. Guarda o ultimo `on<tipo>` num campo proprio
  // para que a LEITURA responda a funcao — o DOM guarda o callback como palavra
  // opaca e nao ha caminho de volta dela para o valor.
  __on(tipo: string): any {
    const tabela: any = (this as any).__onHandlers;
    return tabela === undefined ? null : tabela[tipo];
  }
  __setOn(tipo: string, fn: any): void {
    let tabela: any = (this as any).__onHandlers;
    if (tabela === undefined) { tabela = {}; (this as any).__onHandlers = tabela; }
    // Uma segunda atribuicao SUBSTITUI, e nao acumula: `el.onclick = a` seguido
    // de `el.onclick = b` deixa so o `b`, ao contrario de dois
    // `addEventListener`. Sem este `remove` os dois disparavam.
    if (tabela[tipo] !== undefined && tabela[tipo] !== null) {
      dom.removeListener(this._dom, this._node, tipo);
    }
    tabela[tipo] = fn;
    if (fn !== null && fn !== undefined) {
      dom.addListenerCbOptions(this._dom, this._node, tipo, fn);
    }
  }

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
  // `no.data` — o MESMO texto que o `nodeValue` abaixo, com o outro nome que a
  // spec lhe da em `CharacterData` (Text e Comment). Nao e um alias de
  // conveniencia: e o nome que o Preact usa, e so ele.
  //
  //     if (null === x) m === k || (c && n.data === k) || (n.data = k)
  //
  // e o diff de texto dele. O React escreve `nodeValue`; o Preact escreve
  // `data`, e sem esta propriedade a atribuicao ia para o vazio em silencio —
  // nao lancava, porque escrever num campo que nao existe cria-o no wrapper.
  // O sintoma era uma lista que encolhia ao clicar e um contador que nunca
  // mudava, na mesma pagina: o que muda de ESTRUTURA reconciliava e o que muda
  // so de TEXTO ficava parado.
  //
  // `localName` vem junto porque o Preact tambem o consulta para decidir se
  // reaproveita um no (`y.localName === x`), e sem ele reaproveita nada.
  get data(): string {
    return dom.nodeValue(this._dom, this._node);
  }
  set data(value: string) {
    dom.setNodeValue(this._dom, this._node, value);
  }
  // `el.localName` — o nome da tag em minusculas, que e o que este DOM guarda.
  // Difere do `tagName` so em HTML maiusculo e em XML, e nenhum dos dois se
  // representa aqui, entao os dois respondem o mesmo por construcao.
  get localName(): string {
    return dom.tagName(this._dom, this._node);
  }

  // `node.nodeValue` — texto cru de Text/Comment. ⚠️ CORTE: a spec dá `null` para
  // Element/Document, mas a fronteira ABI (string) não carrega null → devolve `''`
  // nesses casos (um Text vazio tambem e '', indistinguivel).
  //
  // O SET era SO o metodo `setNodeValue`, e a razao que estava escrita aqui —
  // "o motor nao dispara setters de propriedade" — deixou de ser verdade: o
  // `textContent` trezentas linhas acima ja tem um. O que o corte custava nao
  // era ergonomia: um reconciliador de React escreve `no.nodeValue = t` para
  // trocar o texto de um no sem lhe mexer na identidade, e uma propriedade
  // so-com-getter LANCA nessa atribuicao. A app montava, o clique chegava, e o
  // commit morria ai.
  get nodeValue(): string {
    return dom.nodeValue(this._dom, this._node);
  }
  set nodeValue(value: string) {
    dom.setNodeValue(this._dom, this._node, value);
  }
  // Mantido: e o que os chamadores deste prelude escrevem, e os dois caminhos
  // sao a mesma mutacao.
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

  // `el.getBoundingClientRect()` — o retângulo (border-box), sem argumento (fiel ao
  // MDN: nem o browser nem `boundingRect(doc,node,which)` recebem viewport por
  // chamada — o layout usa o viewport ATUAL do `Dom`, default 1280×800 headless).
  // Já em pontos (o Rust devolve `f32` direto, não `i64`×1000); nó sem caixa vem 0.
  // ANTES chamava a chave errada com um 4º argumento que a função Rust não tem
  // (`dom.boundingComponent(doc,node,vw,which)`) e lançava TypeError sempre — a
  // certa é `boundingRect`, 3 argumentos.
  getBoundingClientRect(): DOMRectLike {
    const x = dom.boundingRect(this._dom, this._node, 0);
    const y = dom.boundingRect(this._dom, this._node, 1);
    const w = dom.boundingRect(this._dom, this._node, 2);
    const h = dom.boundingRect(this._dom, this._node, 3);
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
    __maybeReleaseSubtree(this._dom, this._node);
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

  // ── scroll (#2621 lote G) ─────────────────────────────────────────────────
  //
  // O offset vive no `Dom` (Rust, `dom/scroll.rs`) — não neste objeto e não
  // no backend: `get`/`set` só chamam o primitivo, exactamente como o resto
  // desta fachada. Sem overflow (elemento não é uma região rolável), o motor
  // responde scrollTop/scrollLeft=0 e scrollWidth/Height===clientWidth/Height
  // — a mesma resposta de um browser para um elemento que não rola.
  get scrollTop(): number { return dom.scrollTop(this._dom, this._node); }
  set scrollTop(v: number) { dom.setScrollTop(this._dom, this._node, v); }
  get scrollLeft(): number { return dom.scrollLeft(this._dom, this._node); }
  set scrollLeft(v: number) { dom.setScrollLeft(this._dom, this._node, v); }
  get scrollWidth(): number { return dom.scrollWidth(this._dom, this._node); }
  get scrollHeight(): number { return dom.scrollHeight(this._dom, this._node); }
  get clientWidth(): number { return dom.clientWidth(this._dom, this._node); }
  get clientHeight(): number { return dom.clientHeight(this._dom, this._node); }

  // `el.scrollTo({top,left})` ou `el.scrollTo(x,y)` — as duas formas que o
  // browser aceita. Na forma-objeto, um eixo ausente ({top:5}, sem `left`)
  // mantém o offset actual desse eixo; a forma posicional exige os dois.
  scrollTo(arg1: any, arg2: any): void {
    if (typeof arg1 === "object" && arg1 !== null) {
      const left = arg1.left !== undefined ? arg1.left : this.scrollLeft;
      const top = arg1.top !== undefined ? arg1.top : this.scrollTop;
      dom.elementScrollTo(this._dom, this._node, left, top);
      return;
    }
    dom.elementScrollTo(this._dom, this._node, arg1, arg2);
  }
  // `el.scrollBy(dx, dy)` — relativo ao offset actual.
  scrollBy(dx: number, dy: number): void {
    dom.elementScrollTo(this._dom, this._node, this.scrollLeft + dx, this.scrollTop + dy);
  }
  // `el.scrollIntoView()` — mínimo: alinha o topo deste elemento com o topo
  // da região (ou da página) que rola. Sem opções (`block`/`inline`,
  // `behavior: smooth`) — é o que o browser faz sem argumento nenhum.
  scrollIntoView(): void {
    dom.scrollIntoView(this._dom, this._node);
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
    if (root !== __DOM_NONE) return __elem(this._dom, root);
    const body = dom.querySelector(this._dom, "body");
    if (body !== __DOM_NONE) return __elem(this._dom, body);
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
    return n === __DOM_NONE ? null : __elem(this._dom, n);
  }

  get head(): Element | null {
    const n = dom.querySelector(this._dom, "head");
    return n === __DOM_NONE ? null : __elem(this._dom, n);
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
    return __elem(this._dom, n);
  }

  querySelectorAll(sel: string): Element[] {
    const out: Element[] = [];
    const n = dom.querySelectorAllCount(this._dom, sel);
    let i = 0;
    while (i < n) {
      const node = dom.querySelectorAllAt(this._dom, sel, i);
      out.push(__elem(this._dom, node));
      i = i + 1;
    }
    return out;
  }

  // `document.getElementById(id)` usa igualdade textual no índice do DOM;
  // não é um seletor CSS e, portanto, funciona para IDs como `a.b`.
  getElementById(id: string): Element | null {
    const n = dom.getById(this._dom, id);
    if (n === __DOM_NONE) return null;
    return __elem(this._dom, n);
  }

  // ── getElementsBy* (#1758) — coleções por classe/tag/name ────────────────────
  getElementsByClassName(name: string): Element[] {
    const out: Element[] = [];
    const n = dom.getByClassCount(this._dom, name);
    let i = 0;
    while (i < n) {
      out.push(__elem(this._dom, dom.getByClassAt(this._dom, name, i)));
      i = i + 1;
    }
    return out;
  }
  getElementsByTagName(tag: string): Element[] {
    const out: Element[] = [];
    const n = dom.getByTagCount(this._dom, tag);
    let i = 0;
    while (i < n) {
      out.push(__elem(this._dom, dom.getByTagAt(this._dom, tag, i)));
      i = i + 1;
    }
    return out;
  }
  getElementsByName(name: string): Element[] {
    const out: Element[] = [];
    const n = dom.getByNameCount(this._dom, name);
    let i = 0;
    while (i < n) {
      out.push(__elem(this._dom, dom.getByNameAt(this._dom, name, i)));
      i = i + 1;
    }
    return out;
  }

  // `document.createElement(tag)` — elemento solto (anexe com appendChild).
  createElement(tag: string): Element {
    const n = dom.createElement(this._dom, tag);
    return __elem(this._dom, n);
  }

  // `document.createTextNode(text)` — nó de texto solto (anexe com appendChild).
  createTextNode(text: string): Element {
    const n = dom.createTextNode(this._dom, text);
    return __elem(this._dom, n);
  }

  // `document.createComment(text)` — nó de comentário solto (nodeType 8).
  createComment(text: string): Element {
    const n = dom.createComment(this._dom, text);
    return __elem(this._dom, n);
  }

  // `document.documentElement` — o elemento `<html>`, não a raiz `#document`.
  get documentElement(): Element | null {
    const root = dom.documentElement(this._dom);
    if (root === __DOM_NONE) return null;
    return __elem(this._dom, root);
  }

  // Tamanho da arena — inclui nós desanexados sem wrapper que ainda não
  // passaram por `releaseSubtree` (ver `dom.nodeCount`). Não é o `.d.ts` do
  // DOM real; existe para a régua do lote M ("inserir e remover N vezes não
  // faz a arena crescer sem limite") ter algo a medir do lado TS.
  get nodeCount(): number {
    return dom.nodeCount(this._dom);
  }

  // `document.close()` — liberta o documento no lado Rust (`dom.free`,
  // §4.M) e o escopo global que `runScripts` lhe tenha aberto
  // (`__dropWindow`, `window.ts`). Nenhum dos dois tinha chamador antes
  // deste lote — a arena inteira e o escopo de página de um documento
  // descartado ficavam para sempre. Não é chamado automaticamente: um
  // `Document` vive enquanto o programa quiser (é ele quem decide que
  // acabou), então só quem criou o documento pode chamar `close()`.
  // Chamar duas vezes é seguro: o segundo `dom.free` apaga um handle já
  // ausente do store (sem efeito) e o segundo `__dropWindow` sai cedo
  // (`idx < 0`, `window.ts:452`).
  close(): void {
    dom.free(this._dom);
    __dropWindow(this._dom);
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

  // 0) <style>@import ...</style> inline — o MESMO `__inlineImports` do <link>
  //    (lote P, §5.P item 3), numa função à parte (`window.ts`, este ficheiro
  //    está no teto): "lógica nova = módulo novo pequeno; nos grandes entram
  //    chamadas" (CLAUDE.md). Tem de correr ANTES do parse de CSS pelo Rust
  //    (`collect_embedded_css` lê o texto do nó directo, sem ver `@import`).
  __expandInlineStyleImports(h, baseUrl);

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

  // 3) <img src="data:image/png;base64,…"> — descodificado pela ponte e guardado
  //    no documento (lote V-img). `http(s)` e ficheiros locais ficam para a fase
  //    seguinte deste loader; sem pixels a caixa vem dos atributos/CSS como antes.
  const imgCount = dom.getByTagCount(h, "img");
  let k = 0;
  while (k < imgCount) {
    loaded = loaded + __loadImageAt(h, k, baseUrl);
    k = k + 1;
  }

  return loaded;
}

// Entrega à ponte o k-ésimo `<img>`: `data:` descodificada na hora, um caminho
// local (relativo à base do documento) lido do disco pela ponte. `http(s)`
// ainda não — não há `fetchBytes` no motor novo e um PNG não atravessa como
// texto (dito no PLAN, lote V-img-2). Devolve 1/0.
function __loadImageAt(h: i64, k: number, baseUrl: string): number {
  const node = dom.getByTagAt(h, "img", k);
  if (node === __DOM_NONE) return 0;
  const src = dom.getAttribute(h, node, "src");
  if (src.length === 0) return 0;
  if (src.length > 11 && src.substring(0, 11) === "data:image/") return dom.setImageDataUrl(h, node, src) as number;
  if (src.length > 5 && src.substring(0, 5) === "data:") return 0;
  const abs = __resolveUrl(baseUrl, src);
  if (abs.length > 7 && abs.substring(0, 7) === "http://") return 0;
  if (abs.length > 8 && abs.substring(0, 8) === "https://") return 0;
  return dom.setImageFile(h, node, abs) as number;
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
