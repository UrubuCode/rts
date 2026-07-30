// Adaptação do JS de PÁGINA à semântica que o compilador do RTS aceita — o
// pré-passo que roda em todo `<script>` antes de compilar (`runScripts`).
//
// ## Por que existe
// O corpo de um `<script>` é compilado pelo pipeline do próprio RTS (swc→HIR→JIT,
// via `new Function`). Esse pipeline é um COMPILADOR DE TS, e trata como erro
// coisas que uma página faz o tempo todo:
//
//   1. GLOBAL IMPLÍCITO — `requireLazy = function(){…}` sem `var/let/const`.
//      Em JS sloppy isso CRIA um global; para o compilador é "assignment to
//      unbound". Não afrouxamos o motor (para código RTS a recusa está certa):
//      traduzimos aqui, na borda que já tem semântica de browser, reescrevendo
//      para o SACO DE GLOBAIS compartilhado do documento (`__G`, ver window.ts).
//
//   2. SEQUÊNCIA com função — o motor aceita o operador vírgula e aceita
//      `function` como valor, mas não a combinação `x=…, y=function(){…}` numa só
//      expressão; minificadores geram isso o tempo todo.
//
// `arguments` NÃO está mais aqui: o motor materializa o objeto sozinho, também em
// função-EXPRESSÃO (`f = function(){ … arguments … }`) — `argsobj.rs` roda depois
// do lifting de closures e `HirExprKind::Arrow.is_real_arrow` distingue uma arrow
// de verdade (que não tem `arguments` próprio) de uma fn-expr (que tem). A
// reescrita textual que existia aqui foi DELETADA: reimplementar por fora o que o
// motor faz com AST é frágil e era o custo dominante de RAM (issue #2019).
//
// ## O que este módulo NÃO é
// Não é um parser de JS. É um conjunto de reescritas TEXTUAIS conservadoras que
// pulam string/comentário e só agem em padrões inequívocos. Onde não tem certeza,
// deixa como está — falha honesta do compilador em vez de resultado errado.
//
// Concatenado na mesma unidade de `dom.ts` (ver `lib.rs::DOM_TS`).

// ── GLOBAL IMPLÍCITO (`x = 1` sem var/let/const) ──────────────────────────────
//
// Em JS *sloppy mode* atribuir a um nome não declarado CRIA um global. É como o
// loader da Meta se instala (`requireLazy=function(){…}`) e como quase todo
// bundle antigo publica sua API. O compilador do RTS — corretamente, para código
// RTS — recusa: "assignment to unbound `x`". Não relaxamos isso no motor (seria
// enfraquecer a linguagem inteira por causa de uma página); resolvemos AQUI, na
// borda que já é "semântica de browser": um pré-passo reescreve o script para
// falar com o saco de globais compartilhado.
//
//   requireLazy = function(){…}   →   __G.requireLazy = function(){…}
//   requireLazy([...], cb)        →   __G.requireLazy([...], cb)
//
// `__bindGlobals` faz as duas metades: acha os nomes atribuídos sem declaração
// (`__scanImplicitGlobals`) e reescreve TODAS as ocorrências livres desses nomes
// para `__G.<nome>` (`__qualify`). Nomes já declarados no próprio script, campos
// (`a.x`), chaves de objeto (`{x:…}`) e conteúdo de string/comentário não são
// tocados.

// ── Leitura de caractere SEM ALOCAR ──────────────────────────────────────────
//
// `src.substring(i, i + 1)` aloca uma STRING NOVA por caractere: varrer 200 KB
// assim cria 200 mil handles, e o GC periódico não os recolhe (gatilho é
// `GC_LIVE_FLOOR = 500_000` handles vivos). `charCodeAt` devolve um NÚMERO e
// aloca ZERO — medido. Era o custo dominante deste pré-passo (issue #2019).
const __CH_TAB = 9;
const __CH_LF = 10;
const __CH_CR = 13;
const __CH_SPACE = 32;
const __CH_BANG = 33;
const __CH_QUOTE = 34;
const __CH_DOLLAR = 36;
const __CH_APOS = 39;
const __CH_LPAREN = 40;
const __CH_RPAREN = 41;
const __CH_STAR = 42;
const __CH_COMMA = 44;
const __CH_DOT = 46;
const __CH_SLASH = 47;
const __CH_0 = 48;
const __CH_9 = 57;
const __CH_SEMI = 59;
const __CH_LT = 60;
const __CH_EQ = 61;
const __CH_GT = 62;
const __CH_A_UP = 65;
const __CH_Z_UP = 90;
const __CH_LBRACK = 91;
const __CH_BACKSLASH = 92;
const __CH_RBRACK = 93;
const __CH_UNDER = 95;
const __CH_BACKTICK = 96;
const __CH_A_LO = 97;
const __CH_Z_LO = 122;
const __CH_LBRACE = 123;
const __CH_RBRACE = 125;

function __isIdStartCode(k: number): boolean {
  return (k >= __CH_A_LO && k <= __CH_Z_LO) || (k >= __CH_A_UP && k <= __CH_Z_UP)
    || k === __CH_UNDER || k === __CH_DOLLAR;
}
function __isIdPartCode(k: number): boolean {
  return __isIdStartCode(k) || (k >= __CH_0 && k <= __CH_9);
}
function __isSpaceCode(k: number): boolean {
  return k === __CH_SPACE || k === __CH_LF || k === __CH_TAB || k === __CH_CR;
}

function __isIdStart(c: string): boolean {
  return (c >= "a" && c <= "z") || (c >= "A" && c <= "Z") || c === "_" || c === "$";
}
function __isIdPart(c: string): boolean {
  return __isIdStart(c) || (c >= "0" && c <= "9");
}
function __isSpace(c: string): boolean {
  return c === " " || c === "\n" || c === "\t" || c === "\r";
}

// Nomes DECLARADOS em qualquer ponto do script: `var/let/const X`, `function X`,
// `class X`, parâmetros de função e `catch (X)`. Um nome daqui NUNCA é global
// implícito, mesmo que seja reatribuído adiante (`let i = 0; … i = i + 1`) — sem
// esta lista o `i` do segundo statement parece um global e o script quebra.
function __scanDeclared(src: string): string[] {
  const out: string[] = [];
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src.charCodeAt(i);
    // pula comentário e string (um `var` dentro de texto não declara nada)
    if (c === __CH_SLASH && i + 1 < n) {
      const c2 = src.charCodeAt(i + 1);
      if (c2 === __CH_SLASH) { while (i < n && src.charCodeAt(i) !== __CH_LF) i = i + 1; continue; }
      if (c2 === __CH_STAR) {
        i = i + 2;
        while (i + 1 < n && !(src.charCodeAt(i) === __CH_STAR && src.charCodeAt(i + 1) === __CH_SLASH)) i = i + 1;
        i = i + 2;
        continue;
      }
    }
    if (c === __CH_QUOTE || c === __CH_APOS || c === __CH_BACKTICK) {
      const q = c;
      i = i + 1;
      while (i < n) {
        const d = src.charCodeAt(i);
        if (d === __CH_BACKSLASH) { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      continue;
    }
    if (__isIdStartCode(c)) {
      const s0 = i;
      while (i < n && __isIdPartCode(src.charCodeAt(i))) i = i + 1;
      const w = src.substring(s0, i);
      const isDecl = w === "var" || w === "let" || w === "const";
      const isFn = w === "function" || w === "class" || w === "catch";
      if (isDecl || isFn) {
        // `var a = 1, b = 2` → captura a lista inteira até `;` ou `)`.
        let j = i;
        let guard = 0;
        while (j < n && guard < 4096) {
          while (j < n && __isSpaceCode(src.charCodeAt(j))) j = j + 1;
          if (j < n && __isIdStartCode(src.charCodeAt(j))) {
            const s2 = j;
            while (j < n && __isIdPartCode(src.charCodeAt(j))) j = j + 1;
            const nm = src.substring(s2, j);
            let dup = 0;
            let k = 0;
            while (k < out.length) { if (out[k] === nm) dup = 1; k = k + 1; }
            if (dup === 0) out.push(nm);
          }
          // parâmetros: `function f(a, b)` — anda até `)` coletando nomes
          while (j < n && __isSpaceCode(src.charCodeAt(j))) j = j + 1;
          const ch = j < n ? src.charCodeAt(j) : 0;
          if (ch === __CH_COMMA) { j = j + 1; guard = guard + 1; continue; }
          if (ch === __CH_LPAREN) { j = j + 1; guard = guard + 1; continue; }
          if (ch === __CH_RPAREN) { j = j + 1; break; }
          if (ch === __CH_EQ) {
            // pula o inicializador até `,` ou `;` de profundidade 0
            let p = 0; let b = 0; let br = 0;
            while (j < n) {
              const d = src.charCodeAt(j);
              if (d === __CH_LPAREN) p = p + 1; else if (d === __CH_RPAREN) { if (p === 0) break; p = p - 1; }
              else if (d === __CH_LBRACK) b = b + 1; else if (d === __CH_RBRACK) b = b - 1;
              else if (d === __CH_LBRACE) br = br + 1; else if (d === __CH_RBRACE) br = br - 1;
              else if ((d === __CH_COMMA || d === __CH_SEMI) && p === 0 && b === 0 && br === 0) break;
              j = j + 1;
            }
            if (j < n && src.charCodeAt(j) === __CH_COMMA) { j = j + 1; guard = guard + 1; continue; }
            break;
          }
          break;
        }
        i = j;
        continue;
      }
      continue;
    }
    i = i + 1;
  }
  return out;
}

// Varre `src` pulando comentário e string, e devolve os nomes que aparecem como
// `NOME =` (mas não `==`, `===`, `=>`, `>=`, `<=`, `!=`) sem `var`/`let`/`const`
// imediatamente antes e sem `.` antes (que seria campo).
function __scanImplicitGlobals(src: string): string[] {
  const out: string[] = [];
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src.charCodeAt(i);
    // comentário de linha / bloco
    if (c === __CH_SLASH && i + 1 < n) {
      const c2 = src.charCodeAt(i + 1);
      if (c2 === __CH_SLASH) {
        while (i < n && src.charCodeAt(i) !== __CH_LF) i = i + 1;
        continue;
      }
      if (c2 === __CH_STAR) {
        i = i + 2;
        while (i + 1 < n && !(src.charCodeAt(i) === __CH_STAR && src.charCodeAt(i + 1) === __CH_SLASH)) i = i + 1;
        i = i + 2;
        continue;
      }
    }
    // string / template
    if (c === __CH_QUOTE || c === __CH_APOS || c === __CH_BACKTICK) {
      const q = c;
      i = i + 1;
      while (i < n) {
        const d = src.charCodeAt(i);
        if (d === __CH_BACKSLASH) { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      continue;
    }
    if (__isIdStartCode(c)) {
      const start = i;
      while (i < n && __isIdPartCode(src.charCodeAt(i))) i = i + 1;
      const word = src.substring(start, i);
      // `.nome` é campo, não global
      let p = start - 1;
      while (p >= 0 && __isSpaceCode(src.charCodeAt(p))) p = p - 1;
      const prevCh = p >= 0 ? src.charCodeAt(p) : 0;
      if (prevCh === __CH_DOT) continue;
      // procura o `=` adiante
      let j = i;
      while (j < n && __isSpaceCode(src.charCodeAt(j))) j = j + 1;
      if (j < n && src.charCodeAt(j) === __CH_EQ) {
        const nx = j + 1 < n ? src.charCodeAt(j + 1) : 0;
        // `==`/`===`/`=>` não são atribuição
        if (nx !== __CH_EQ && nx !== __CH_GT && prevCh !== __CH_EQ && prevCh !== __CH_BANG
          && prevCh !== __CH_LT && prevCh !== __CH_GT) {
          // palavra-chave declarante imediatamente antes?
          let we = p + 1;
          let ws = we;
          while (ws > 0 && __isIdPartCode(src.charCodeAt(ws - 1))) ws = ws - 1;
          const kw = src.substring(ws, we);
          if (kw !== "var" && kw !== "let" && kw !== "const" && kw !== "function"
            && kw !== "class" && kw !== "case" && kw !== "return") {
            let dup = 0;
            let k = 0;
            while (k < out.length) { if (out[k] === word) dup = 1; k = k + 1; }
            if (dup === 0) out.push(word);
          }
        }
      }
      continue;
    }
    i = i + 1;
  }
  return out;
}

// Reescreve toda ocorrência LIVRE de cada nome de `names` para `__G.<nome>`,
// pulando string/comentário e não tocando em `a.nome` (campo) nem `{nome:` (chave).
function __qualify(src: string, names: string[]): string {
  if (names.length === 0) return src;
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src.substring(i, i + 1);
    if (c === "/" && i + 1 < n) {
      const c2 = src.substring(i + 1, i + 2);
      if (c2 === "/") {
        const s = i;
        while (i < n && src.substring(i, i + 1) !== "\n") i = i + 1;
        out = out + src.substring(s, i);
        continue;
      }
      if (c2 === "*") {
        const s = i;
        i = i + 2;
        while (i + 1 < n && !(src.substring(i, i + 1) === "*" && src.substring(i + 1, i + 2) === "/")) i = i + 1;
        i = i + 2;
        out = out + src.substring(s, i);
        continue;
      }
    }
    if (c === "\"" || c === "'" || c === "`") {
      const q = c;
      const s = i;
      i = i + 1;
      while (i < n) {
        const d = src.substring(i, i + 1);
        if (d === "\\") { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      out = out + src.substring(s, i);
      continue;
    }
    if (__isIdStart(c)) {
      const start = i;
      while (i < n && __isIdPart(src.substring(i, i + 1))) i = i + 1;
      const word = src.substring(start, i);
      let hit = 0;
      let k = 0;
      while (k < names.length) { if (names[k] === word) hit = 1; k = k + 1; }
      if (hit === 1) {
        // `.nome` = campo → não qualifica
        let p = start - 1;
        while (p >= 0 && __isSpace(src.substring(p, p + 1))) p = p - 1;
        const prevCh = p >= 0 ? src.substring(p, p + 1) : "";
        // `nome:` = chave de objeto ou label → não qualifica
        let j = i;
        while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
        const nextCh = j < n ? src.substring(j, j + 1) : "";
        if (prevCh === "." || nextCh === ":") out = out + word;
        else out = out + "__G." + word;
      } else {
        out = out + word;
      }
      continue;
    }
    out = out + c;
    i = i + 1;
  }
  return out;
}

// Pré-passo aplicado a todo `<script>` antes de compilar. Qualifica DUAS classes
// de nome:
//   (a) os que ESTE script cria sem declarar (`requireLazy = …`) — senão o
//       compilador recusa com "assignment to unbound";
//   (b) os que um script ANTERIOR já publicou no saco (`known`) e este apenas
//       LÊ/CHAMA (`requireLazy([...], cb)`) — senão seria "unknown function".
// (b) é o que transforma 33 ilhas num escopo só, como no browser.

// ── SEQUÊNCIA (`a=1, b=function(){}`) → statements ────────────────────────────
//
// O motor aceita o operador vírgula, e aceita `function` como valor — mas NÃO a
// combinação `x=…, y=function(){…}` numa só expressão (a extração da função
// aborta). Minificadores geram isso o tempo todo. Como no topo do script a
// sequência é só "faça isto, depois aquilo", trocar a vírgula separadora por `;`
// preserva a semântica. Só quebra vírgulas em profundidade ZERO de (), [], {} e
// fora de string — as vírgulas de argumento/array/objeto ficam intactas.
function __splitTopLevelSequences(src: string): string {
  // Saída rápida: sem vírgula NENHUMA não há sequência a quebrar, e reconstruir
  // o texto só para devolvê-lo igual seria lixo puro no heap (issue #2019).
  if (src.indexOf(",") < 0) return src;
  let out = "";
  let i = 0;
  let copiado = 0;   // tudo em [copiado, …) ainda não foi para `out`
  const n = src.length;
  let par = 0;
  let brk = 0;
  let brc = 0;
  while (i < n) {
    const c = src.charCodeAt(i);
    if (c === __CH_SLASH && i + 1 < n) {
      const c2 = src.charCodeAt(i + 1);
      if (c2 === __CH_SLASH) {
        while (i < n && src.charCodeAt(i) !== __CH_LF) i = i + 1;
        continue;
      }
      if (c2 === __CH_STAR) {
        i = i + 2;
        while (i + 1 < n && !(src.charCodeAt(i) === __CH_STAR && src.charCodeAt(i + 1) === __CH_SLASH)) i = i + 1;
        i = i + 2;
        continue;
      }
    }
    if (c === __CH_QUOTE || c === __CH_APOS || c === __CH_BACKTICK) {
      const q = c;
      i = i + 1;
      while (i < n) {
        const d = src.charCodeAt(i);
        if (d === __CH_BACKSLASH) { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      continue;
    }
    if (c === __CH_LPAREN) par = par + 1;
    else if (c === __CH_RPAREN) par = par - 1;
    else if (c === __CH_LBRACK) brk = brk + 1;
    else if (c === __CH_RBRACK) brk = brk - 1;
    else if (c === __CH_LBRACE) brc = brc + 1;
    else if (c === __CH_RBRACE) brc = brc - 1;
    // vírgula separadora de sequência no topo → `;`. Só AQUI o texto muda:
    // descarrega em BLOCO o trecho intocado e emite `;` no lugar da vírgula.
    if (c === __CH_COMMA && par === 0 && brk === 0 && brc === 0) {
      out = out + src.substring(copiado, i) + ";";
      copiado = i + 1;
    }
    i = i + 1;
  }
  return out + src.substring(copiado, n);
}

// Remove de `names` tudo que o script DECLARA (var/let/const/function/param).
function __filterDeclared(names: string[], src: string): string[] {
  if (names.length === 0) return names;
  return __filterComDeclarados(names, __scanDeclared(src));
}

// Como `__filterDeclared`, mas com a lista de declarados JÁ pronta — evita
// varrer o mesmo texto de novo.
function __filterComDeclarados(names: string[], decl: string[]): string[] {
  if (names.length === 0) return names;
  const out: string[] = [];
  let i = 0;
  while (i < names.length) {
    let hit = 0;
    let k = 0;
    while (k < decl.length) { if (decl[k] === names[i]) hit = 1; k = k + 1; }
    if (hit === 0) out.push(names[i]);
    i = i + 1;
  }
  return out;
}

// ── `x instanceof window.C` → `x instanceof C` ───────────────────────────────
//
// `instanceof` no motor resolve o lado direito por NOME de classe. Um script de
// página escreve a mesma checagem via objeto global (`e.dataset instanceof
// window.DOMStringMap` — feature-detect de classe do DOM), e a forma com membro
// não resolvia, derrubando o script inteiro.
//
// `window`/`self`/`globalThis`/`top`/`parent` denotam o objeto global, e uma
// classe global é alcançável pelo próprio nome — então as duas formas são a
// MESMA checagem, e tirar o prefixo é fiel. Quando a classe não existe no motor,
// o `instanceof` responde `false`, que é a resposta certa para um feature-detect
// (e era o que o browser daria num motor sem aquela classe).
//
// Só age imediatamente depois do token `instanceof`; qualquer outro `window.x`
// do script fica intacto.
function __unqualifyInstanceof(src: string): string {
  if (src.indexOf("instanceof") < 0) return src;
  // Só age em `instanceof <global>.Classe`. Ter um `instanceof` qualquer não
  // basta: sem o nome de um objeto global adiante, varrer reconstruiria o texto
  // para devolvê-lo IGUAL. Num bundle real `instanceof` é comum e a forma com
  // objeto global é rara (issue #2019).
  if (src.indexOf("window.") < 0 && src.indexOf("globalThis.") < 0
    && src.indexOf("self.") < 0 && src.indexOf("top.") < 0
    && src.indexOf("parent.") < 0) {
    return src;
  }
  let out = "";
  let i = 0;
  let copiado = 0;   // tudo em [copiado, …) ainda não foi para `out`
  const n = src.length;
  while (i < n) {
    // Salta direto para o próximo `instanceof`; o texto entre um e outro sai
    // depois num bloco só — copiar char-a-char era o custo dominante.
    const t = src.indexOf("instanceof", i);
    if (t < 0) break;
    i = t;
    // precisa ser o token inteiro (não sufixo de um identificador maior)
    const antes = i > 0 ? src.charCodeAt(i - 1) : 0;
    const depois = i + 10 < n ? src.charCodeAt(i + 10) : 0;
    if (!__isIdPartCode(antes) && !__isIdPartCode(depois)) {
      let j = i + 10;
      while (j < n && __isSpaceCode(src.charCodeAt(j))) j = j + 1;
      const baseStart = j;
      while (j < n && __isIdPartCode(src.charCodeAt(j))) j = j + 1;
      const base = src.substring(baseStart, j);
      const ehGlobal = base === "window" || base === "globalThis"
        || base === "self" || base === "top" || base === "parent";
      if (ehGlobal && j < n && src.charCodeAt(j) === __CH_DOT) {
        // `instanceof window.Classe` → mantém só `Classe`
        out = out + src.substring(copiado, i + 10) + " ";
        i = j + 1;
        copiado = i;
        continue;
      }
    }
    i = i + 10;
  }
  return out + src.substring(copiado, n);
}

// TODA a normalização sintática, num lugar só — o `runScripts` e o `__bindGlobals`
// precisam enxergar exatamente o mesmo texto (os nomes detectados têm de bater
// com o texto que vai ser compilado).
function __normalizeScript(code: string): string {
  return __unqualifyInstanceof(__splitTopLevelSequences(code));
}

function __bindGlobals(code: string, known: string[]): string {
  // Normaliza PRIMEIRO e só então varre: o split de sequência transforma
  // `a=1,b=2` em `a=1;b=2`, e é sobre essa forma que os nomes são detectados.
  const normalized = __normalizeScript(code);
  // `__scanDeclared` roda UMA vez e serve aos dois usos (o filtro abaixo e o
  // shadowing): varrer o mesmo texto duas vezes era custo puro em JS minificado.
  const declared = __scanDeclared(normalized);
  // Acrescenta os já publicados que aparecem neste script — MENOS os que este
  // script declara localmente (shadowing: um `const requireLazy = …` local ganha
  // do global, como no browser).
  const names = __filterComDeclarados(__scanImplicitGlobals(normalized), declared);
  let i = 0;
  while (i < known.length) {
    const kn = known[i];
    let dup = 0;
    let k = 0;
    while (k < names.length) { if (names[k] === kn) dup = 1; k = k + 1; }
    let shadow = 0;
    let s = 0;
    while (s < declared.length) { if (declared[s] === kn) shadow = 1; s = s + 1; }
    if (dup === 0 && shadow === 0) names.push(kn);
    i = i + 1;
  }
  if (names.length === 0) return normalized;
  return __qualify(normalized, names);
}
