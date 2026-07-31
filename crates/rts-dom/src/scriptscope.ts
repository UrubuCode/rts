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

// ── O LEXER ───────────────────────────────────────────────────────────────────
//
// Todo pass abaixo precisa da MESMA pergunta: "este ponto do texto é CÓDIGO, ou é
// conteúdo de string/comentário/regex?". Cada um respondia isso por conta própria,
// e cada um errava de um jeito diferente:
//
//   - literal de regex NUNCA era reconhecida — `m.match(/id=(\d+)/)` virava
//     `/__G.id=(\d+)/`, e `foo(/re=1/)` declarava `re` como global implícito;
//   - `${…}` de template era tratado como conteúdo opaco — `` `${foo()}` `` nunca
//     era qualificado, e o `foo` livre estourava ReferenceError em runtime;
//   - `__unqualifyInstanceof` não pulava NADA — `var s = "a instanceof window.Foo"`
//     tinha o CONTEÚDO DA STRING reescrito.
//
// `__codeSpans` responde isso uma vez, para todos. Devolve os intervalos que são
// código, achatados em pares `[ini, fim, ini, fim, …]`:
//
//   - comentário de linha/bloco: fora;
//   - corpo de string `'…'`/`"…"`: fora;
//   - literal de regex `/…/flags`: fora (inclusive classe `[…]`, onde `/` não
//     termina a regex);
//   - template: o TEXTO fica fora, mas o miolo de cada `${…}` volta a ser código —
//     recursivamente, porque `` `a${`b${c}`}d` `` é legal.
//
// A parte que não dá para resolver com um olhar local é `/`: dividir ou começar
// regex depende do ÚLTIMO TOKEN SIGNIFICATIVO. `a / b` divide; `(/re/)` não. A
// regra padrão é: depois de identificador/número/`)`/`]`/`}` é divisão, senão é
// regex — com a exceção das palavras-chave que são identificadores mas esperam um
// valor depois (`return /re/`, `typeof /re/`, `case /re/`…).
function __regexAllowedAfter(tok: string): boolean {
  if (tok === "") return true;
  // Pontuação que sempre PRECEDE um valor.
  if (tok === "(" || tok === "," || tok === "=" || tok === ":" || tok === "["
    || tok === "!" || tok === "&" || tok === "|" || tok === "?" || tok === "{"
    || tok === "}" || tok === ";" || tok === "+" || tok === "-" || tok === "*"
    || tok === "/" || tok === "%" || tok === "^" || tok === "~" || tok === "<"
    || tok === ">") return true;
  // Palavra-chave que espera um valor: `return /re/.test(x)` é regex, não divisão.
  return tok === "return" || tok === "typeof" || tok === "instanceof"
    || tok === "in" || tok === "of" || tok === "new" || tok === "delete"
    || tok === "void" || tok === "throw" || tok === "case" || tok === "do"
    || tok === "else" || tok === "yield" || tok === "await";
}

function __codeSpans(src: string): i64[] {
  const spans: i64[] = [];
  const n = src.length;
  let i = 0;
  let runStart = 0;
  // Último token significativo, para desambiguar `/`.
  let lastTok = "";
  // Pilha de profundidade de `{` por template aberto. Vazia = fora de template.
  const tplBrace: i64[] = [];

  while (i < n) {
    const c = src.substring(i, i + 1);

    // Fecha `${…}` e volta ao TEXTO do template quando a chave casa.
    if (c === "}" && tplBrace.length > 0) {
      const top: i64 = tplBrace[tplBrace.length - 1];
      if (top === 0) {
        tplBrace.pop();
        spans.push(runStart);
        spans.push(i);
        i = i + 1;
        // Continua o texto do template de onde parou.
        let closed = 0;
        while (i < n) {
          const d = src.substring(i, i + 1);
          if (d === "\\") { i = i + 2; continue; }
          if (d === "`") { i = i + 1; closed = 1; break; }
          if (d === "$" && i + 1 < n && src.substring(i + 1, i + 2) === "{") {
            tplBrace.push(0);
            i = i + 2;
            runStart = i;
            closed = 1;
            break;
          }
          i = i + 1;
        }
        if (closed === 0) { runStart = i; break; }
        if (tplBrace.length === 0) { runStart = i; lastTok = "`"; }
        continue;
      }
      tplBrace[tplBrace.length - 1] = top - 1;
      lastTok = "}";
      i = i + 1;
      continue;
    }
    if (c === "{" && tplBrace.length > 0) {
      tplBrace[tplBrace.length - 1] = tplBrace[tplBrace.length - 1] + 1;
      lastTok = "{";
      i = i + 1;
      continue;
    }

    if (c === "/" && i + 1 < n) {
      const c2 = src.substring(i + 1, i + 2);
      if (c2 === "/") {
        spans.push(runStart); spans.push(i);
        while (i < n && src.substring(i, i + 1) !== "\n") i = i + 1;
        runStart = i;
        continue;
      }
      if (c2 === "*") {
        spans.push(runStart); spans.push(i);
        i = i + 2;
        while (i + 1 < n && !(src.substring(i, i + 1) === "*" && src.substring(i + 1, i + 2) === "/")) i = i + 1;
        i = i + 2;
        if (i > n) i = n;
        runStart = i;
        continue;
      }
      if (__regexAllowedAfter(lastTok)) {
        // Literal de regex: `/` fecha, mas não dentro de `[…]`.
        spans.push(runStart); spans.push(i);
        i = i + 1;
        let inClass = 0;
        while (i < n) {
          const d = src.substring(i, i + 1);
          if (d === "\\") { i = i + 2; continue; }
          if (d === "[") inClass = 1;
          else if (d === "]") inClass = 0;
          else if (d === "/" && inClass === 0) { i = i + 1; break; }
          else if (d === "\n") break;
          i = i + 1;
        }
        while (i < n && __isIdPart(src.substring(i, i + 1))) i = i + 1; // flags
        runStart = i;
        lastTok = "/re/";
        continue;
      }
      lastTok = "/";
      i = i + 1;
      continue;
    }

    if (c === "\"" || c === "'") {
      spans.push(runStart); spans.push(i);
      const q = c;
      i = i + 1;
      while (i < n) {
        const d = src.substring(i, i + 1);
        if (d === "\\") { i = i + 2; continue; }
        if (d === q) { i = i + 1; break; }
        i = i + 1;
      }
      runStart = i;
      lastTok = "str";
      continue;
    }

    if (c === "`") {
      spans.push(runStart); spans.push(i);
      i = i + 1;
      let done = 0;
      while (i < n) {
        const d = src.substring(i, i + 1);
        if (d === "\\") { i = i + 2; continue; }
        if (d === "`") { i = i + 1; done = 1; break; }
        if (d === "$" && i + 1 < n && src.substring(i + 1, i + 2) === "{") {
          tplBrace.push(0);
          i = i + 2;
          done = 1;
          break;
        }
        i = i + 1;
      }
      runStart = i;
      if (tplBrace.length === 0) lastTok = "`";
      if (done === 0) break;
      continue;
    }

    if (__isIdStart(c)) {
      const s = i;
      while (i < n && __isIdPart(src.substring(i, i + 1))) i = i + 1;
      lastTok = src.substring(s, i);
      continue;
    }
    if (c >= "0" && c <= "9") {
      while (i < n && (__isIdPart(src.substring(i, i + 1)) || src.substring(i, i + 1) === ".")) i = i + 1;
      lastTok = "0";
      continue;
    }
    if (!__isSpace(c)) lastTok = c;
    i = i + 1;
  }
  if (runStart < n) { spans.push(runStart); spans.push(n); }
  return spans;
}

// Nomes DECLARADOS em qualquer ponto do script: `var/let/const X`, `function X`,
// `class X`, parâmetros de função e `catch (X)`. Um nome daqui NUNCA é global
// implícito, mesmo que seja reatribuído adiante (`let i = 0; … i = i + 1`) — sem
// esta lista o `i` do segundo statement parece um global e o script quebra.
// Pula um inicializador (`= …`) até a `,` ou `;` de profundidade 0, PULANDO
// string e comentário.
//
// Sem esse cuidado, `var a = "a,b", c = 1;` terminava o inicializador na vírgula
// DENTRO da string: `b` era declarado como se fosse nome (fantasma) e o `c` real
// se perdia. Uma string com vírgula é banal em qualquer bundle.
function __skipInitializer(src: string, from: i64): i64 {
  const n = src.length;
  let j = from;
  let p = 0; let b = 0; let br = 0;
  while (j < n) {
    const d = src.substring(j, j + 1);
    if (d === "\"" || d === "'" || d === "`") {
      const q = d;
      j = j + 1;
      while (j < n) {
        const e = src.substring(j, j + 1);
        if (e === "\\") { j = j + 2; continue; }
        if (e === q) { j = j + 1; break; }
        j = j + 1;
      }
      continue;
    }
    if (d === "/" && j + 1 < n) {
      const d2 = src.substring(j + 1, j + 2);
      if (d2 === "/") { while (j < n && src.substring(j, j + 1) !== "\n") j = j + 1; continue; }
      if (d2 === "*") {
        j = j + 2;
        while (j + 1 < n && !(src.substring(j, j + 1) === "*" && src.substring(j + 1, j + 2) === "/")) j = j + 1;
        j = j + 2;
        continue;
      }
    }
    if (d === "(") p = p + 1; else if (d === ")") { if (p === 0) break; p = p - 1; }
    else if (d === "[") b = b + 1; else if (d === "]") b = b - 1;
    else if (d === "{") br = br + 1; else if (d === "}") br = br - 1;
    else if ((d === "," || d === ";") && p === 0 && b === 0 && br === 0) break;
    j = j + 1;
  }
  return j;
}

// Coleta os nomes LIGADOS por um padrão de destructuring que começa em `from`
// (`{a, b: c, d = 1, ...r}` ou `[a, , b]`), devolvendo o índice logo após o
// fechamento. Em `{chave: nome}` quem é ligado é `nome`, não `chave`; um `= …`
// dentro do padrão é valor default e é pulado.
function __collectPattern(src: string, from: i64, out: string[]): i64 {
  const n = src.length;
  let j = from;
  let depth = 0;
  while (j < n) {
    const c = src.substring(j, j + 1);
    if (c === "{" || c === "[") { depth = depth + 1; j = j + 1; continue; }
    if (c === "}" || c === "]") {
      depth = depth - 1;
      j = j + 1;
      if (depth === 0) break;
      continue;
    }
    if (c === "\"" || c === "'" || c === "`") {
      const q = c;
      j = j + 1;
      while (j < n) {
        const e = src.substring(j, j + 1);
        if (e === "\\") { j = j + 2; continue; }
        if (e === q) { j = j + 1; break; }
        j = j + 1;
      }
      continue;
    }
    if (c === "=") {
      // valor default: pula até `,` ou o fim do nível atual
      j = j + 1;
      let p = 0;
      while (j < n) {
        const d = src.substring(j, j + 1);
        if (d === "(" || d === "[" || d === "{") p = p + 1;
        else if (d === ")" ) { if (p === 0) break; p = p - 1; }
        else if (d === "]" || d === "}") { if (p === 0) break; p = p - 1; }
        else if (d === "," && p === 0) break;
        j = j + 1;
      }
      continue;
    }
    if (__isIdStart(c)) {
      const s = j;
      while (j < n && __isIdPart(src.substring(j, j + 1))) j = j + 1;
      const nm = src.substring(s, j);
      let k = j;
      while (k < n && __isSpace(src.substring(k, k + 1))) k = k + 1;
      // `{chave: nome}` — a chave não liga nada; o nome vem depois do `:`
      if (k < n && src.substring(k, k + 1) === ":") { j = k + 1; continue; }
      let dup = 0;
      let m = 0;
      while (m < out.length) { if (out[m] === nm) dup = 1; m = m + 1; }
      if (dup === 0) out.push(nm);
      continue;
    }
    j = j + 1;
  }
  return j;
}

function __scanDeclaredTs(src: string): string[] {
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
          // DESTRUCTURING (`const {c,d} = t`, `let [a,b] = xs`). Antes, um `{`/`[`
          // aqui não casava nenhum ramo e caía no `break` final: a declaração
          // inteira era abandonada, e os nomes ligados por ela ficavam invisíveis.
          // Como eles continuam sendo ATRIBUÍDOS adiante, o scanner de globais os
          // via como implícitos e o `__qualify` reescrevia LOCAIS para `__G.x` —
          // é essa a origem dos "10 falsos positivos em 3800 B" da issue, e
          // minificador destrutura o tempo todo.
          if (ch === "{" || ch === "[") {
            j = __collectPattern(src, j, out);
            while (j < n && __isSpace(src.substring(j, j + 1))) j = j + 1;
            const after = j < n ? src.substring(j, j + 1) : "";
            if (after === ",") { j = j + 1; guard = guard + 1; continue; }
            if (after === "=") {
              j = __skipInitializer(src, j);
              if (j < n && src.substring(j, j + 1) === ",") { j = j + 1; guard = guard + 1; continue; }
            }
            break;
          }
          if (ch === "," ) { j = j + 1; guard = guard + 1; continue; }
          if (ch === "(") { j = j + 1; guard = guard + 1; continue; }
          if (ch === ")") { j = j + 1; break; }
          if (ch === "=") {
            j = __skipInitializer(src, j);
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
// A varredura em si vive em RUST (`scriptscan.rs`): o laço `.ts` custava ~1 µs
// por caractere, e num bundle real da Meta (6 MB) o pré-passo levava 49 s contra
// 113 ms de compilação de verdade. A REESCRITA continua aqui; o que mudou de
// lado é só o scanner, que é o que escala com o tamanho do arquivo.
function __scanImplicitGlobals(src: string): string[] {
  const n = ScriptScan.implicitGlobals(src);
  const out: string[] = [];
  let i = 0;
  while (i < n) { out.push(ScriptScan.nameAt(i)); i = i + 1; }
  return out;
}

function __scanDeclared(src: string): string[] {
  const n = ScriptScan.declaredNames(src);
  const out: string[] = [];
  let i = 0;
  while (i < n) { out.push(ScriptScan.nameAt(i)); i = i + 1; }
  return out;
}

// Versão `.ts` de referência do scanner de globais implícitos — mantida como
// ORÁCULO do teste que compara os dois (`claude-scriptscan-paridade`), não usada
// no caminho quente.
function __scanImplicitGlobalsTs(src: string): string[] {
  const out: string[] = [];
  const spans = __codeSpans(src);
  const classRanges = __classBodyRanges(src);
  let s = 0;
  while (s < spans.length) {
    const spanEnd: i64 = spans[s + 1];
    let i: i64 = spans[s];
    s = s + 2;
    while (i < spanEnd) {
      const c = src.substring(i, i + 1);
      if (__isIdStart(c)) {
        const start = i;
        while (i < spanEnd && __isIdPart(src.substring(i, i + 1))) i = i + 1;
        const word = src.substring(start, i);
        // `.nome` é campo, não global
        let p = start - 1;
        while (p >= 0 && __isSpace(src.substring(p, p + 1))) p = p - 1;
        const prevCh = p >= 0 ? src.substring(p, p + 1) : "";
        if (prevCh === ".") continue;
        // procura o `=` adiante
        let j = i;
        while (j < spanEnd && __isSpace(src.substring(j, j + 1))) j = j + 1;
        if (j < spanEnd && src.substring(j, j + 1) === "=") {
          const nx = j + 1 < spanEnd ? src.substring(j + 1, j + 2) : "";
          // `==`/`===`/`=>` não são atribuição
          if (nx !== "=" && nx !== ">" && prevCh !== "=" && prevCh !== "!"
            && prevCh !== "<" && prevCh !== ">") {
            // palavra-chave declarante imediatamente antes?
            let we = p + 1;
            let ws = we;
            while (ws > 0 && __isIdPart(src.substring(ws - 1, ws))) ws = ws - 1;
            const kw = src.substring(ws, we);
            // `class C { nome = 1 }` é declaração de CAMPO, não global implícito —
            // qualificá-lo gerava `class C { __G.nome = 1 }`, um SyntaxError dentro
            // do código que este próprio módulo produziu.
            const inField = __inRanges(classRanges, start);
            if (kw !== "var" && kw !== "let" && kw !== "const" && kw !== "function"
              && kw !== "class" && kw !== "case" && kw !== "return" && inField === 0) {
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
  }
  return out;
}

// Intervalos que são CORPO DE CLASSE, achatados em `[ini, fim, …]`.
//
// `class C { nome = 1 }` declara um CAMPO; não é atribuição a global. Sem isso o
// scanner reportava `nome` como global implícito e o `__qualify` produzia
// `class C { __G.nome = 1 }` — SyntaxError dentro do código que este módulo
// gerou, exatamente o oposto da "falha honesta" que o cabeçalho promete.
//
// Calculado UMA vez por pass (não por candidato): a primeira versão consultava
// isto por identificador e refazia o lexer inteiro a cada consulta, o que é
// quadrático no tamanho do script — justamente o custo que este arquivo já tinha
// de sobra.
function __classBodyRanges(src: string): i64[] {
  const ranges: i64[] = [];
  const spans = __codeSpans(src);
  const openIsClass: i64[] = [];
  const openAt: i64[] = [];
  let s = 0;
  let sawClassHead = 0;
  while (s < spans.length) {
    const spanEnd: i64 = spans[s + 1];
    let i: i64 = spans[s];
    s = s + 2;
    while (i < spanEnd) {
      const c = src.substring(i, i + 1);
      if (__isIdStart(c)) {
        const st = i;
        while (i < spanEnd && __isIdPart(src.substring(i, i + 1))) i = i + 1;
        if (src.substring(st, i) === "class") sawClassHead = 1;
        continue;
      }
      if (c === "{") {
        openIsClass.push(sawClassHead);
        openAt.push(i);
        sawClassHead = 0;
      } else if (c === "}") {
        if (openIsClass.length > 0) {
          const wasClass: i64 = openIsClass[openIsClass.length - 1];
          const at: i64 = openAt[openAt.length - 1];
          openIsClass.pop();
          openAt.pop();
          // Inserção ORDENADA por posição de abertura: os pares saem na ordem de
          // FECHAMENTO (uma classe aninhada fecha antes da externa), e a busca
          // binária de `__inRanges` exige ordem de abertura. São poucos pares,
          // então a inserção linear aqui não pesa — o que pesava era consultar
          // linearmente a cada identificador do script.
          if (wasClass === 1) {
            let q = ranges.length;
            while (q >= 2 && ranges[q - 2] > at) q = q - 2;
            ranges.splice(q, 0, at);
            ranges.splice(q + 1, 0, i);
          }
        }
      } else if (c === ";") {
        sawClassHead = 0;
      }
      i = i + 1;
    }
  }
  return ranges;
}

// (Busca BINÁRIA: `__classBodyRanges` emite os pares já ordenados por posição de
// abertura e sem sobreposição — corpos aninhados fecham antes do próximo abrir.
// A varredura linear rodava a CADA identificador do script, então num bundle com
// centenas de classes o scan inteiro virava quadrático.)
function __inRanges(ranges: i64[], pos: i64): i64 {
  let lo = 0;
  let hi = ranges.length / 2 - 1;
  while (lo <= hi) {
    const mid = (lo + hi) / 2;
    const ini: i64 = ranges[mid * 2];
    const fim: i64 = ranges[mid * 2 + 1];
    if (pos <= ini) hi = mid - 1;
    else if (pos >= fim) lo = mid + 1;
    else return 1;
  }
  return 0;
}

// Reescreve toda ocorrência LIVRE de cada nome de `names` para `__G.<nome>`,
// pulando string/comentário e não tocando em `a.nome` (campo) nem `{nome:` (chave).
// A reescrita vive em RUST (`scriptscan.rs::qualify`). O laço `.ts` perdia
// ocorrências num bundle grande — 92 de 278 chamadas de `__d` qualificadas — e
// a página morria em "call to unknown function `__d`". A causa medida NÃO era o
// algoritmo (o mesmo laço fora do motor acerta 278/278): era o GC coletando o
// array `spans`, um LOCAL DE FUNÇÃO ainda vivo, durante a chamada alocadora
// seguinte — `spans.length` virava -1 e o `while` terminava calado. O motor novo
// não emite `declare_value_needs_stack_map`, então o scanner conservativo não
// enxerga locais de função (issue aberta). Em Rust o array não é um handle do
// heap do motor e o sítio deixa de depender dessa cobertura.
function __qualify(src: string, names: string[]): string {
  if (names.length === 0) return src;
  return ScriptScan.qualify(src, __juntaLinhas(names));
}

// `names.join("\n")` escrito sem literal de escape: o gerador que mantém este
// arquivo materializava o `\n` como quebra REAL, partindo a string em duas
// linhas e quebrando o parse do prelude inteiro.
function __juntaLinhas(names: string[]): string {
  let out = "";
  let i = 0;
  while (i < names.length) {
    if (i > 0) out = out + String.fromCharCode(10);
    out = out + names[i];
    i = i + 1;
  }
  return out;
}

// Versão `.ts` de referência (oráculo do teste de paridade, fora do caminho quente).
function __qualifyTs(src: string, names: string[]): string {
  if (names.length === 0) return src;
  const spans = __codeSpans(src);
  const classRanges = __classBodyRanges(src);
  const parts: string[] = [];
  // Slice preguiçoso: o output é IGUAL à entrada em toda parte menos nos sítios
  // reescritos, então nada é copiado até haver divergência de verdade. Antes cada
  // caractere passava por `out = out + c`, o que alocava uma string nova por
  // iteração; string/comentário/regex agora nem entram no laço.
  let runStart = 0;
  let sp = 0;
  while (sp < spans.length) {
    const spanEnd: i64 = spans[sp + 1];
    let i: i64 = spans[sp];
    sp = sp + 2;
    while (i < spanEnd) {
      const c = src.substring(i, i + 1);
      if (__isIdStart(c)) {
        const start = i;
        while (i < spanEnd && __isIdPart(src.substring(i, i + 1))) i = i + 1;
        const word = src.substring(start, i);
        let hit = 0;
        let k = 0;
        while (k < names.length) { if (names[k] === word) hit = 1; k = k + 1; }
        if (hit === 1) {
          // `.nome` = campo → não qualifica
          let p = start - 1;
          while (p >= 0 && __isSpace(src.substring(p, p + 1))) p = p - 1;
          const prevCh = p >= 0 ? src.substring(p, p + 1) : "";
          let j = i;
          while (j < spanEnd && __isSpace(src.substring(j, j + 1))) j = j + 1;
          const nextCh = j < spanEnd ? src.substring(j, j + 1) : "";
          // `nome:` pode ser chave de objeto, label OU `case nome:`. Nos dois
          // primeiros não se qualifica; no terceiro SIM — deixar `case foo:` livre
          // dava ReferenceError na comparação. Distingue pela palavra anterior.
          let we = p + 1;
          let ws = we;
          while (ws > 0 && __isIdPart(src.substring(ws - 1, ws))) ws = ws - 1;
          const prevWord = src.substring(ws, we);
          const isCase = prevWord === "case" ? 1 : 0;
          // `{ nome }` shorthand: qualificar direto gerava `{ __G.nome }`, um
          // SyntaxError. A forma correta é a chave explícita `nome: __G.nome`.
          const isShorthand = (prevCh === "{" || prevCh === ",")
            && (nextCh === "," || nextCh === "}") ? 1 : 0;
          if (prevCh === ".") {
            // campo: nada a fazer
          } else if (__inRanges(classRanges, start) === 1 && nextCh === "=") {
            // `class C { nome = 1 }` — declaração de campo, não uso de global
          } else if (isShorthand === 1) {
            parts.push(src.substring(runStart, start));
            parts.push(word);
            parts.push(": __G.");
            runStart = start;
          } else if (nextCh === ":" && isCase === 0) {
            // chave de objeto ou label: nada a fazer
          } else {
            parts.push(src.substring(runStart, start));
            parts.push("__G.");
            runStart = start;
          }
        }
        continue;
      }
      i = i + 1;
    }
  }
  parts.push(src.substring(runStart, src.length));
  return parts.join("");
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
  // (runStart+parts — o `out = out + src.substring(i, i+1)` por caractere era
  // O(n²); num bundle de 6 MB o pré-passo inteiro virava hora de parede.)
  const parts: string[] = [];
  let runStart = 0;
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
            parts.push(src.substring(runStart, j));
            parts.push("(...__rtsargs)");
            parts.push(__replaceIdent(bodyTxt, "arguments", "__rtsargs"));
            i = e + 1;
            runStart = i;
            continue;
          }
        }
      }
    }
    i = i + 1;
  }
  parts.push(src.substring(runStart, n));
  return parts.join("");
}

// Troca ocorrências do identificador `from` por `to` (só identificador inteiro,
// não `.from` nem dentro de outro nome).
// (runStart+parts, não `out = out + c` — concat por caractere é O(n²) e num
// bundle de 6 MB vira hora de parede; medido no pré-passo do WhatsApp.)
function __replaceIdent(src: string, from: string, to: string): string {
  const parts: string[] = [];
  let runStart = 0;
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
      if (w === from && prevCh !== ".") {
        parts.push(src.substring(runStart, s));
        parts.push(to);
        runStart = i;
      }
      continue;
    }
    i = i + 1;
  }
  parts.push(src.substring(runStart, n));
  return parts.join("");
}

// ── SEQUÊNCIA (`a=1, b=function(){}`) → statements ────────────────────────────
//
// O motor aceita o operador vírgula, e aceita `function` como valor — mas NÃO a
// combinação `x=…, y=function(){…}` numa só expressão (a extração da função
// aborta). Minificadores geram isso o tempo todo. Como no topo do script a
// sequência é só "faça isto, depois aquilo", trocar a vírgula separadora por `;`
// preserva a semântica. Só quebra vírgulas em profundidade ZERO de (), [], {} e
// fora de string — as vírgulas de argumento/array/objeto ficam intactas.
// (runStart+parts: o texto entre vírgulas trocadas é copiado em FATIAS — o
// `out = out + c` por caractere era O(n²) e dominava o load de bundle grande.)
function __splitTopLevelSequences(src: string): string {
  const parts: string[] = [];
  let runStart = 0;
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
    if (c === "(") par = par + 1;
    else if (c === ")") par = par - 1;
    else if (c === "[") brk = brk + 1;
    else if (c === "]") brk = brk - 1;
    else if (c === "{") brc = brc + 1;
    else if (c === "}") brc = brc - 1;
    // vírgula separadora de sequência no topo → `;`
    if (c === "," && par === 0 && brk === 0 && brc === 0) {
      parts.push(src.substring(runStart, i));
      parts.push(";");
      runStart = i + 1;
    }
    i = i + 1;
  }
  parts.push(src.substring(runStart, n));
  return parts.join("");
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
  // Este era o ÚNICO pass sem nenhum skip lexical: procurava o texto literal
  // "instanceof" no fonte inteiro. `var s = "a instanceof window.Foo"` tinha o
  // CONTEÚDO DA STRING reescrito para `"a instanceof Foo"` — o programa passava a
  // significar outra coisa, sem erro nenhum. Um comentário sofria o mesmo.
  const spans = __codeSpans(src);
  const parts: string[] = [];
  let runStart = 0;
  let sp = 0;
  while (sp < spans.length) {
    const spanEnd: i64 = spans[sp + 1];
    let i: i64 = spans[sp];
    sp = sp + 2;
    while (i < spanEnd) {
      if (i + 10 <= spanEnd && src.substring(i, i + 10) === "instanceof") {
        // precisa ser o token inteiro (não sufixo de um identificador maior)
        const antes = i > 0 ? src.substring(i - 1, i) : "";
        const depois = i + 10 < src.length ? src.substring(i + 10, i + 11) : "";
        if (!__isIdPart(antes) && !__isIdPart(depois)) {
          let j = i + 10;
          while (j < spanEnd && __isSpace(src.substring(j, j + 1))) j = j + 1;
          const baseStart = j;
          while (j < spanEnd && __isIdPart(src.substring(j, j + 1))) j = j + 1;
          const base = src.substring(baseStart, j);
          const ehGlobal = base === "window" || base === "globalThis"
            || base === "self" || base === "top" || base === "parent";
          if (ehGlobal && j < spanEnd && src.substring(j, j + 1) === ".") {
            // `instanceof window.Classe` → mantém só `Classe`
            parts.push(src.substring(runStart, i + 10));
            parts.push(" ");
            i = j + 1;
            runStart = i;
            continue;
          }
        }
      }
      i = i + 1;
    }
  }
  parts.push(src.substring(runStart, src.length));
  return parts.join("");
}

// TODA a normalização sintática, num lugar só — o `runScripts` e o `__bindGlobals`
// precisam enxergar exatamente o mesmo texto (os nomes detectados têm de bater
// com o texto que vai ser compilado).
function __normalizeScript(code: string): string {
  // A normalização inteira vive em RUST (`scriptscan.rs`) — os passes `.ts`
  // abaixo ficam como ORÁCULO do teste de paridade. Varrer 6 MB de bundle a
  // ~1 µs/char levava 17 s aqui contra ~1 s lá.
  return ScriptScan.normalize(code);
}

// Versão `.ts` de referência (oráculo do teste, fora do caminho quente).
function __normalizeScriptTs(code: string): string {
  return __unqualifyInstanceof(__splitTopLevelSequences(__rewriteArguments(code)));
}

function __bindGlobals(code: string, known: string[]): string {
  // Normaliza PRIMEIRO e só então varre: o split de sequência transforma
  // `a=1,b=2` em `a=1;b=2`, e é sobre essa forma que os nomes são detectados.
  const normalized = __normalizeScript(code);
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
