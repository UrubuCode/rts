// Os construtores de interface do DOM — `window.HTMLIFrameElement` e a família.
//
// # O defeito que isto fecha
//
// Desde `dcad45737` o `instanceof` recusa um lado direito que não seja objecto,
// que é o que a especificação manda (§13.10.1, passo 1). Até aí, `x instanceof
// window.HTMLIFrameElement` sobre um `undefined` respondia `false` e a página
// seguia; hoje lança `TypeError: Right-hand side of 'instanceof' is not an
// object`, e o react-dom 18 morre a carregar por causa de UMA linha:
//
//     function ch(){for(var a=window,b=Qc();b instanceof a.HTMLIFrameElement;)…
//
// A recusa está certa. O que faltava era o nome. Um bundle real pergunta pela
// família — `Node`, `Element`, `HTMLElement`, e o `HTML*Element` do elemento
// que lhe interessa — e nenhum deles existia neste escopo.
//
// # Porquê `Symbol.hasInstance` e não uma hierarquia de classes
//
// O caminho óbvio seria `class HTMLIFrameElement extends Element {}`. Não
// serve: TODO elemento desta fachada é um `Element` construído por `__elem`, e
// nada sabe — nem pode saber, sem uma tabela de tag→classe que teria de estar
// em dia com o parser — que o nó 17 é um `<iframe>`. Com subclasses,
// `iframe instanceof HTMLIFrameElement` responderia `false`, que é a resposta
// errada em silêncio: exactamente o que a recusa do `instanceof` acabou de
// deixar de fazer.
//
// A pergunta é sobre o TAG, e o `Symbol.hasInstance` é o sítio onde a
// especificação deixa uma interface responder por si. Então cada nome é um
// objecto que carrega o seu tag e um gancho que o compara — e a resposta sai do
// documento, que é o único que sabe.
//
// # Porquê um objecto e não uma função
//
// `typeof window.HTMLIFrameElement` responde `"object"` aqui e `"function"` num
// browser. É uma divergência declarada, e o que se compra com ela é que o
// gancho não pode ser esquecido: uma função com o símbolo pendurado à mão
// precisaria de o pendurar em cada um dos nomes, e o nome a que faltasse
// responderia pela cadeia de protótipos — `false` para tudo, calado. Aqui o
// gancho está no protótipo de `DomInterface` e vale para todos por construção.
//
// Nenhum destes é chamável, e isso é o browser: `new HTMLIFrameElement()` é um
// `TypeError: Illegal constructor` lá também.

/// Uma interface do DOM, reduzida ao que o `instanceof` precisa de saber.
///
/// `_tag` vazio significa "qualquer elemento" — é o que `Node`, `Element` e
/// `HTMLElement` são desta fachada, que não distingue os três.
class DomInterface {
  _tag: string;
  /// O protótipo REAL dos elementos desta fachada. Um bundle não pergunta só
  /// `x instanceof Element`: lê `Element.prototype.matches` para o guardar ou
  /// substituir, e `HTMLElement.prototype.click` para o chamar por `.call`.
  /// Sem isto o WhatsApp Web morria em `Cannot read properties of undefined
  /// (reading 'matches')` no nono script. É o mesmo objecto para todos os
  /// nomes porque a fachada não distingue `Node`/`Element`/`HTML*Element`
  /// — e um bundle que remende `Element.prototype` remenda o que os nós usam.
  prototype: any;

  constructor(tag: string) {
    this._tag = tag;
    this.prototype = Element.prototype;
  }

  [Symbol.hasInstance](v: any): boolean {
    if (v === null || typeof v !== "object") { return false; }
    if (!(v instanceof Element)) { return false; }
    if (this._tag === "") { return true; }
    // `tagName` já responde em caixa alta, e a tabela abaixo escreve-se assim
    // pela mesma razão: uma comparação que precisasse de normalizar seria uma
    // segunda regra sobre a caixa dos nomes de tag.
    return v.tagName === this._tag;
  }
}

/// Instala em `alvo` — o `window` — os nomes que um `<script>` de página
/// espera encontrar, com o tag que cada um reconhece.
///
/// A lista é a que os bundles reais consultam — o react-dom pergunta pelo
/// `HTMLIFrameElement` e pelos três de formulário — mais os genéricos. Não é
/// toda a especificação de propósito: um nome aqui é uma promessa de que o
/// `instanceof` responde certo, e um nome a mais é uma promessa por cumprir.
function __instalaInterfaces(alvo: any): void {
  const nomes: string[] = [
    "Node", "",
    "Element", "",
    "HTMLElement", "",
    "HTMLIFrameElement", "IFRAME",
    "HTMLInputElement", "INPUT",
    "HTMLTextAreaElement", "TEXTAREA",
    "HTMLSelectElement", "SELECT",
    "HTMLOptionElement", "OPTION",
    "HTMLButtonElement", "BUTTON",
    "HTMLFormElement", "FORM",
    "HTMLAnchorElement", "A",
    "HTMLImageElement", "IMG",
    "HTMLCanvasElement", "CANVAS",
    "HTMLScriptElement", "SCRIPT",
    "HTMLStyleElement", "STYLE",
    "HTMLLinkElement", "LINK",
    "HTMLDivElement", "DIV",
    "HTMLSpanElement", "SPAN",
    "HTMLTableElement", "TABLE",
  ];
  let i = 0;
  while (i < nomes.length) {
    alvo[nomes[i]] = new DomInterface(nomes[i + 1]);
    i = i + 2;
  }
  // As constantes de `nodeType` vivem no `Node`, como na especificação: um
  // script guarda-se com `el.nodeType === Node.ELEMENT_NODE` antes de tocar
  // num nó, e sem elas a guarda responde `false` em silêncio. Viviam num
  // objecto próprio em `globalThis` — que um `<script>` de página NÃO vê: o
  // escopo de um script é o `window` (ver `scope.rs`), e foi assim que
  // `MutationObserver`, publicado da mesma forma, respondeu `is not defined`
  // no quinto script do WhatsApp Web.
  const node = alvo.Node;
  node.ELEMENT_NODE = 1;
  node.ATTRIBUTE_NODE = 2;
  node.TEXT_NODE = 3;
  node.CDATA_SECTION_NODE = 4;
  node.PROCESSING_INSTRUCTION_NODE = 7;
  node.COMMENT_NODE = 8;
  node.DOCUMENT_NODE = 9;
  node.DOCUMENT_TYPE_NODE = 10;
  node.DOCUMENT_FRAGMENT_NODE = 11;
  alvo.MutationObserver = MutationObserver;
  // Os construtores ECMAScript comuns, como PROPRIEDADES do `window` — não só
  // como nomes livres. Num browser `window` É o objecto global, então
  // `window.Object === Object` sempre; aqui `window` é uma instância de
  // `WindowImpl`, um objecto DIFERENTE do escopo onde `Object` resolve como
  // nome livre (lote H, `docs`/`PLAN.md` §4.H) — então `obj instanceof
  // window.Object` lia uma propriedade que a classe nunca tinha e lançava
  // `TypeError: Right-hand side of 'instanceof' is not an object`, sobre o
  // MESMO `Object` que um nome livre já resolve certo. `x instanceof
  // window.C` é a forma que o feature-detect de uma página escreve
  // (`typeof window.Promise !== 'undefined'`), então window precisa da
  // propriedade e não só do nome livre. A lista é o núcleo que o motor
  // implementa hoje (README raiz): a família `Error`, `Math`, `JSON`,
  // `Map`/`Set`, `Promise`, `Date`, `Symbol` — mais os primitivos e `RegExp`.
  alvo.Object = Object;
  alvo.Array = Array;
  alvo.Function = Function;
  alvo.String = String;
  alvo.Number = Number;
  alvo.Boolean = Boolean;
  alvo.Symbol = Symbol;
  alvo.Error = Error;
  alvo.TypeError = TypeError;
  alvo.RangeError = RangeError;
  alvo.ReferenceError = ReferenceError;
  alvo.SyntaxError = SyntaxError;
  alvo.EvalError = EvalError;
  alvo.URIError = URIError;
  alvo.Date = Date;
  alvo.RegExp = RegExp;
  alvo.Map = Map;
  alvo.Set = Set;
  alvo.Promise = Promise;
  alvo.JSON = JSON;
  alvo.Math = Math;
}
