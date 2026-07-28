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
//   2. `arguments` — o motor não tem o objeto; um ident livre desconhecido faz a
//      extração da função abortar. Mas SUPORTA rest, e para o uso dominante em
//      loaders (repassar args adiante) as duas formas são equivalentes.
//
//   3. SEQUÊNCIA com função — o motor aceita o operador vírgula e aceita
//      `function` como valor, mas não a combinação `x=…, y=function(){…}` numa só
//      expressão; minificadores geram isso o tempo todo.
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
    const c = src.substring(i, i + 1);
    // pula comentário e string (um `var` dentro de texto não declara nada)
    if (c === "/" && i + 1 < n) {
      const c2 = src.substring(i + 1, i + 2);
      if (c2 === "/") { while (i < n && src.substring(i, i + 1) !== "\n") i = i + 1; continue; }
      if (c2 === "*") {
        i = i + 2;
        while (i + 1 < n && !(src.substring(i, i + 1) === "*" && src.substring(i + 1, i + 2) === "/")) i = i + 1;
        i = i + 2;
        continue;
      }
    }
    if (c === "\"" || c === "'" || c === "`") {
      const q = c;
      i = i + 1;
      while (i < n) {
        const d = src.substring(i, i + 1);
        if (d === "\\") { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      continue;
    }
    if (__isIdStart(c)) {
      const s = i;
      while (i < n && __isIdPart(src.substring(i, i + 1))) i = i + 1;
      const w = src.substring(s, i);
      const isDecl = w === "var" || w === "let" || w === "const";
      const isFn = w === "function" || w === "class" || w === "catch";
      if (isDecl || isFn) {
        // `var a = 1, b = 2` → captura a lista inteira até `;` ou `)`.
        let j = i;
        let guard = 0;
        while (j < n && guard < 4096) {
          while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
          if (j < n && __isIdStart(src.substring(j, j + 1))) {
            const s2 = j;
            while (j < n && __isIdPart(src.substring(j, j + 1))) j = j + 1;
            const nm = src.substring(s2, j);
            let dup = 0;
            let k = 0;
            while (k < out.length) { if (out[k] === nm) dup = 1; k = k + 1; }
            if (dup === 0) out.push(nm);
          }
          // parâmetros: `function f(a, b)` — anda até `)` coletando nomes
          while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
          const ch = j < n ? src.substring(j, j + 1) : "";
          if (ch === "," ) { j = j + 1; guard = guard + 1; continue; }
          if (ch === "(") { j = j + 1; guard = guard + 1; continue; }
          if (ch === ")") { j = j + 1; break; }
          if (ch === "=") {
            // pula o inicializador até `,` ou `;` de profundidade 0
            let p = 0; let b = 0; let br = 0;
            while (j < n) {
              const d = src.substring(j, j + 1);
              if (d === "(") p = p + 1; else if (d === ")") { if (p === 0) break; p = p - 1; }
              else if (d === "[") b = b + 1; else if (d === "]") b = b - 1;
              else if (d === "{") br = br + 1; else if (d === "}") br = br - 1;
              else if ((d === "," || d === ";") && p === 0 && b === 0 && br === 0) break;
              j = j + 1;
            }
            if (j < n && src.substring(j, j + 1) === ",") { j = j + 1; guard = guard + 1; continue; }
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
    const c = src.substring(i, i + 1);
    // comentário de linha / bloco
    if (c === "/" && i + 1 < n) {
      const c2 = src.substring(i + 1, i + 2);
      if (c2 === "/") {
        while (i < n && src.substring(i, i + 1) !== "\n") i = i + 1;
        continue;
      }
      if (c2 === "*") {
        i = i + 2;
        while (i + 1 < n && !(src.substring(i, i + 1) === "*" && src.substring(i + 1, i + 2) === "/")) i = i + 1;
        i = i + 2;
        continue;
      }
    }
    // string / template
    if (c === "\"" || c === "'" || c === "`") {
      const q = c;
      i = i + 1;
      while (i < n) {
        const d = src.substring(i, i + 1);
        if (d === "\\") { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      continue;
    }
    if (__isIdStart(c)) {
      const start = i;
      while (i < n && __isIdPart(src.substring(i, i + 1))) i = i + 1;
      const word = src.substring(start, i);
      // `.nome` é campo, não global
      let p = start - 1;
      while (p >= 0 && __isSpace(src.substring(p, p + 1))) p = p - 1;
      const prevCh = p >= 0 ? src.substring(p, p + 1) : "";
      if (prevCh === ".") continue;
      // procura o `=` adiante
      let j = i;
      while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
      if (j < n && src.substring(j, j + 1) === "=") {
        const nx = j + 1 < n ? src.substring(j + 1, j + 2) : "";
        // `==`/`===`/`=>` não são atribuição
        if (nx !== "=" && nx !== ">" && prevCh !== "=" && prevCh !== "!"
          && prevCh !== "<" && prevCh !== ">") {
          // palavra-chave declarante imediatamente antes?
          let we = p + 1;
          let ws = we;
          while (ws > 0 && __isIdPart(src.substring(ws - 1, ws))) ws = ws - 1;
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
// ── `arguments` → rest param ──────────────────────────────────────────────────
//
// O motor não tem o objeto `arguments` (um ident livre desconhecido faz a
// extração da função abortar — o erro sai como "expression arrow"). Mas SUPORTA
// rest (`function(...a){}`). Para o uso dominante em loaders — repassar os args
// adiante, `f=function(){stub.push(arguments)}` — as duas formas são
// equivalentes, então reescrevemos:
//
//   function(){ … arguments … }   →   function(...__rtsargs){ … __rtsargs … }
//
// Só age em `function()` de lista de parâmetros VAZIA cujo corpo menciona
// `arguments` (com parâmetros nomeados a equivalência não vale e deixamos como
// está — falha honesta em vez de resultado errado). `arguments` é reservado como
// NOME de parâmetro, daí o `__rtsargs`.
function __rewriteArguments(src: string): string {
  if (src.indexOf("arguments") < 0) return src;
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    // `function` seguido de `(` `)` vazio
    if (src.substring(i, i + 8) === "function") {
      let j = i + 8;
      while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
      // pula um nome opcional (function foo(){})
      while (j < n && __isIdPart(src.substring(j, j + 1))) j = j + 1;
      while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
      if (j < n && src.substring(j, j + 1) === "(") {
        let k = j + 1;
        while (k < n && __isSpace(src.substring(k, k + 1))) k = k + 1;
        if (k < n && src.substring(k, k + 1) === ")") {
          // lista vazia: o corpo até a chave de fechamento menciona `arguments`?
          const bodyStart = k + 1;
          let b = bodyStart;
          while (b < n && src.substring(b, b + 1) !== "{") b = b + 1;
          let depth = 0;
          let e = b;
          while (e < n) {
            const ch = src.substring(e, e + 1);
            if (ch === "{") depth = depth + 1;
            else if (ch === "}") { depth = depth - 1; if (depth === 0) break; }
            e = e + 1;
          }
          const bodyTxt = src.substring(b, e + 1);
          if (bodyTxt.indexOf("arguments") >= 0) {
            // `src[i..j]` = "function[ nome]", depois abrimos a lista nós mesmos:
            // `(...__rtsargs)` — o `)` original está em `k` e é descartado.
            out = out + src.substring(i, j) + "(...__rtsargs)";
            out = out + __replaceIdent(bodyTxt, "arguments", "__rtsargs");
            i = e + 1;
            continue;
          }
        }
      }
    }
    out = out + src.substring(i, i + 1);
    i = i + 1;
  }
  return out;
}

// Troca ocorrências do identificador `from` por `to` (só identificador inteiro,
// não `.from` nem dentro de outro nome).
function __replaceIdent(src: string, from: string, to: string): string {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src.substring(i, i + 1);
    if (__isIdStart(c)) {
      const s = i;
      while (i < n && __isIdPart(src.substring(i, i + 1))) i = i + 1;
      const w = src.substring(s, i);
      let p = s - 1;
      while (p >= 0 && __isSpace(src.substring(p, p + 1))) p = p - 1;
      const prevCh = p >= 0 ? src.substring(p, p + 1) : "";
      out = out + (w === from && prevCh !== "." ? to : w);
      continue;
    }
    out = out + c;
    i = i + 1;
  }
  return out;
}

// ── SEQUÊNCIA (`a=1, b=function(){}`) → statements ────────────────────────────
//
// O motor aceita o operador vírgula, e aceita `function` como valor — mas NÃO a
// combinação `x=…, y=function(){…}` numa só expressão (a extração da função
// aborta). Minificadores geram isso o tempo todo. Como no topo do script a
// sequência é só "faça isto, depois aquilo", trocar a vírgula separadora por `;`
// preserva a semântica. Só quebra vírgulas em profundidade ZERO de (), [], {} e
// fora de string — as vírgulas de argumento/array/objeto ficam intactas.
function __splitTopLevelSequences(src: string): string {
  let out = "";
  let i = 0;
  const n = src.length;
  let par = 0;
  let brk = 0;
  let brc = 0;
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
    if (c === "(") par = par + 1;
    else if (c === ")") par = par - 1;
    else if (c === "[") brk = brk + 1;
    else if (c === "]") brk = brk - 1;
    else if (c === "{") brc = brc + 1;
    else if (c === "}") brc = brc - 1;
    // vírgula separadora de sequência no topo → `;`
    if (c === "," && par === 0 && brk === 0 && brc === 0) out = out + ";";
    else out = out + c;
    i = i + 1;
  }
  return out;
}

// Remove de `names` tudo que o script DECLARA (var/let/const/function/param).
function __filterDeclared(names: string[], src: string): string[] {
  if (names.length === 0) return names;
  const decl = __scanDeclared(src);
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

function __bindGlobals(code: string, known: string[]): string {
  // Normaliza PRIMEIRO e só então varre: o split de sequência transforma
  // `a=1,b=2` em `a=1;b=2`, e é sobre essa forma que os nomes são detectados.
  const normalized = __splitTopLevelSequences(__rewriteArguments(code));
  const names = __filterDeclared(__scanImplicitGlobals(normalized), normalized);
  // Acrescenta os já publicados que aparecem neste script — MENOS os que este
  // script declara localmente (shadowing: um `const requireLazy = …` local ganha
  // do global, como no browser).
  const declared = __scanDeclared(normalized);
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
